/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The omnibar command shell: the privileged rhai lane.
//!
//! A `>`-prefixed omnibar expression ([`nav::classify`](crate::nav::classify)
//! routes it to [`NavTarget::Command`](crate::nav::NavTarget::Command)) evaluates
//! here against a read-only [`ShellContext`] snapshot and emits a
//! [`ShellOutcome`]: the script's text result plus the [`Command`]s it called,
//! in order. The host drains those commands through the existing invoke path
//! (the same route `AgentAction::InvokeCommand` uses), so the shell is a fourth
//! driver of the one `Command` spine — alongside the palette, the agent harness,
//! and accesskit actions.
//!
//! ## The snapshot-in / commands-out shape
//!
//! rhai's `register_fn` closures are `'static`, so a binding cannot borrow the
//! live host. That constraint is the design: the shell reads a snapshot and
//! *enqueues* commands rather than mutating anything. The whole evaluator is
//! therefore pure relative to the host (snapshot in, outcome out) and unit-tested
//! without a winit loop.
//!
//! ## Trust
//!
//! This is the privileged tier. It runs only on text the local user types into
//! their own omnibar, and its privilege *is* the binding set registered below.
//! The knot-note lane ([`script_rhai::RhaiEvaluator`]) shares the sandbox
//! ([`script_rhai::base_engine`]) but registers no bindings, so a peer-authored
//! note can never reach a `Command`. See the omnibar_command_shell_plan.

use std::cell::RefCell;
use std::rc::Rc;

use script_rhai::rhai::{Array, Dynamic};

use crate::command::Command;

/// The zero-argument read-only query function names (used for the bare-call
/// sugar, so `>current_url` works like `>current_url()`).
const QUERIES: &[&str] = &[
    "current_url",
    "history",
    "can_back",
    "can_forward",
    "focused_node",
    "nodes",
];

/// The per-call operation budget: rhai's native runaway guard, generous enough
/// for an interactive command that iterates the graph, bounded so a typo like
/// `>loop {}` aborts rather than hanging the bar.
const OP_BUDGET: u64 = 5_000_000;

/// A read-only snapshot of host state a command expression may query. Built by
/// the host before each eval; the query bindings read it, nothing writes it.
#[derive(Clone, Debug, Default)]
pub struct ShellContext {
    /// The focused content's address (the omnibar's current location).
    pub current_url: String,
    /// Visited URLs, oldest first.
    pub history: Vec<String>,
    /// Whether a back step is possible.
    pub can_back: bool,
    /// Whether a forward step is possible.
    pub can_forward: bool,
    /// The focused graph node's URL, if any.
    pub focused_node: Option<String>,
    /// Every graph node's URL, in graph order.
    pub nodes: Vec<String>,
}

/// The result of evaluating a command expression.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShellOutcome {
    /// The script's return value, stringified (echoed in the omnibar). Empty for
    /// a unit result (a bare verb call).
    pub text: String,
    /// The commands the expression called, in order. The host drains these.
    pub commands: Vec<Command>,
    /// A compile / runtime error message, if the script failed. Commands called
    /// before a mid-script failure are still present in `commands`.
    pub error: Option<String>,
}

/// The privileged command-shell evaluator. Stateless: each [`eval`](Self::eval)
/// builds a fresh sandboxed engine, wires the bindings to that call's command
/// buffer and snapshot, and tears it down. (One eval per Enter press; the engine
/// build is cheap relative to an interactive keystroke.)
#[derive(Default)]
pub struct CommandShell;

impl CommandShell {
    /// A new command shell.
    pub fn new() -> Self {
        Self
    }

    /// Evaluate `source` against `ctx`, returning the text result and the
    /// commands it emitted. Never panics: a compile error, a runaway, or an
    /// unknown identifier becomes [`ShellOutcome::error`].
    pub fn eval(&self, source: &str, ctx: &ShellContext) -> ShellOutcome {
        let commands: Rc<RefCell<Vec<Command>>> = Rc::new(RefCell::new(Vec::new()));
        let snapshot = Rc::new(ctx.clone());
        let mut engine = script_rhai::base_engine();

        // Action verbs: one zero-argument function per command, derived from
        // `Command::ALL` so the omnibar's vocabulary *is* the palette's — a new
        // command is callable here the moment it has a `verb()`, with no second
        // list to maintain. Each enqueues its command.
        for cmd in Command::ALL {
            let buf = commands.clone();
            engine.register_fn(cmd.verb(), move || {
                buf.borrow_mut().push(cmd);
            });
        }

        // Read-only queries over the snapshot.
        let s = snapshot.clone();
        engine.register_fn("current_url", move || s.current_url.clone());
        let s = snapshot.clone();
        engine.register_fn("can_back", move || s.can_back);
        let s = snapshot.clone();
        engine.register_fn("can_forward", move || s.can_forward);
        let s = snapshot.clone();
        engine.register_fn("focused_node", move || {
            s.focused_node.clone().unwrap_or_default()
        });
        let s = snapshot.clone();
        engine.register_fn("history", move || to_array(&s.history));
        let s = snapshot.clone();
        engine.register_fn("nodes", move || to_array(&s.nodes));

        engine.set_max_operations(OP_BUDGET);
        let result = engine.eval::<Dynamic>(&desugar(source));
        let commands = commands.borrow().clone();
        match result {
            Ok(value) => ShellOutcome {
                text: stringify(value),
                commands,
                error: None,
            },
            Err(err) => ShellOutcome {
                text: String::new(),
                commands,
                error: Some(err.to_string()),
            },
        }
    }
}

/// The best inline-autocomplete completion for a partial command-mode token:
/// the first callable name (a [`Command`] verb or a read-only query) that
/// `prefix` is a *proper* prefix of. `None` when `prefix` is empty, not an
/// identifier fragment, already a complete name, or matches nothing.
///
/// The candidates are exactly the omnibar's vocabulary — `Command::ALL` verbs
/// then the queries — so the ghost a user sees and the palette's command set are
/// one surface. The caller derives the ghost suffix as `full[prefix.len()..]`.
pub fn complete(prefix: &str) -> Option<&'static str> {
    if prefix.is_empty()
        || !prefix
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return None;
    }
    Command::ALL
        .iter()
        .map(|c| c.verb())
        .chain(QUERIES.iter().copied())
        .find(|name| name.len() > prefix.len() && name.starts_with(prefix))
}

/// Bare-call sugar: a lone identifier naming a zero-arg verb or query becomes a
/// call, so `>back` runs `back()` and `>current_url` echoes the location.
/// Anything else (a call, an expression, a statement block) passes through to be
/// evaluated as ordinary rhai.
fn desugar(source: &str) -> String {
    let trimmed = source.trim();
    let is_bare =
        Command::ALL.iter().any(|c| c.verb() == trimmed) || QUERIES.contains(&trimmed);
    if is_bare {
        format!("{trimmed}()")
    } else {
        source.to_string()
    }
}

/// A rhai array of strings from a slice of URLs.
fn to_array(items: &[String]) -> Array {
    items.iter().cloned().map(Dynamic::from).collect()
}

/// Stringify a rhai result for the omnibar echo: a unit result (a bare verb
/// call) is empty; anything else is its display form.
fn stringify(value: Dynamic) -> String {
    if value.is_unit() {
        String::new()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ShellContext {
        ShellContext {
            current_url: "mere://welcome".into(),
            history: vec!["mere://welcome".into(), "https://servo.org".into()],
            can_back: true,
            can_forward: false,
            focused_node: Some("mere://welcome".into()),
            nodes: vec!["mere://welcome".into(), "https://servo.org".into()],
        }
    }

    #[test]
    fn bare_verb_is_sugared_into_a_command() {
        let out = CommandShell::new().eval("back", &ctx());
        assert_eq!(out.commands, vec![Command::Back]);
        assert!(out.error.is_none());
        assert_eq!(out.text, "", "a verb call returns unit, echoes nothing");
    }

    #[test]
    fn explicit_call_and_sequence_emit_in_order() {
        let out = CommandShell::new().eval("back(); forward()", &ctx());
        assert_eq!(out.commands, vec![Command::Back, Command::Forward]);
        assert!(out.error.is_none());
    }

    #[test]
    fn a_query_echoes_text_without_emitting_a_command() {
        let out = CommandShell::new().eval("current_url()", &ctx());
        assert_eq!(out.text, "mere://welcome");
        assert!(out.commands.is_empty());
        // The bare form sugars too.
        let bare = CommandShell::new().eval("current_url", &ctx());
        assert_eq!(bare.text, "mere://welcome");
    }

    #[test]
    fn a_conditional_over_a_query_gates_the_command() {
        // can_forward is false in the fixture, so the branch does not fire.
        let out = CommandShell::new().eval("if can_forward() { forward() }", &ctx());
        assert!(out.commands.is_empty(), "the guarded command did not run");
        assert!(out.error.is_none());

        // can_back is true, so this one does.
        let out = CommandShell::new().eval("if can_back() { back() }", &ctx());
        assert_eq!(out.commands, vec![Command::Back]);
    }

    #[test]
    fn a_loop_over_the_graph_reaches_every_node() {
        // Iterating nodes() and acting per-node: the cross-to-orrery reach. Here
        // each node bumps a counter; the script returns the count it walked.
        let out = CommandShell::new().eval("let n = 0; for u in nodes() { n += 1 } n", &ctx());
        assert_eq!(out.text, "2");
        assert!(out.error.is_none());
    }

    #[test]
    fn a_runaway_is_caught_by_the_operation_budget() {
        let out = CommandShell::new().eval("loop { }", &ctx());
        let err = out.error.expect("a runaway reports an error, not a hang");
        assert!(
            err.to_lowercase().contains("operation"),
            "expected an operation-budget error, got: {err}"
        );
    }

    #[test]
    fn an_unknown_identifier_errors_without_panicking() {
        let out = CommandShell::new().eval("nonsense_verb()", &ctx());
        assert!(out.error.is_some());
        assert!(out.commands.is_empty());
    }

    #[test]
    fn an_empty_expression_is_a_no_op() {
        let out = CommandShell::new().eval("", &ctx());
        assert_eq!(out, ShellOutcome::default());
    }

    #[test]
    fn complete_resolves_a_partial_token_to_a_callable() {
        // A partial verb completes to its full name (the caller takes the suffix).
        assert_eq!(complete("ros"), Some("roster"));
        assert_eq!(complete("back"[..2].as_ref()), Some("back")); // "ba" -> back
        // Queries are in the same vocabulary.
        assert_eq!(complete("cur"), Some("current_url"));
        // A complete name has no further completion (no ghost when already done).
        assert_eq!(complete("roster"), None);
        // Non-identifier fragments and empties never complete (mid-expression).
        assert_eq!(complete(""), None);
        assert_eq!(complete("for n in"), None);
        assert_eq!(complete("zzz"), None);
    }
}
