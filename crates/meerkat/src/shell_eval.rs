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

use script_rhai::rhai::{Array, Dynamic, Map};

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
    "inspect",
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
    /// The focused node's inspection rows (the same `(label, value)` pairs the
    /// Inspector pane shows: node metadata, fetch state, content structure). Empty
    /// when nothing is focused. Surfaced to scripts as the `inspect()` map.
    pub inspect: Vec<(String, String)>,
}

/// The result of evaluating a command expression.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShellOutcome {
    /// The script's return value, stringified (echoed in the omnibar). Empty for
    /// a unit result (a bare verb call).
    pub text: String,
    /// The commands the expression called, in order. The host drains these.
    pub commands: Vec<Command>,
    /// A relation kind chosen via `relate("cites")` — the host asserts it on the
    /// selected pair (the kind-qualified edge-creation form). `None` for the
    /// default `relate()`, which rides the `AssertEdge` command path.
    pub relation_kind: Option<String>,
    /// A SPARQL query passed to `sparql("…")` — the host runs it over the focused
    /// graph and echoes the result (`linked_data::query`). `None` when no
    /// `sparql(…)` call was made. Recorded, not run here: the shell snapshot has
    /// only node URLs, not the full RDF graph.
    pub sparql_query: Option<String>,
    /// A wasm component path passed to `attach_script("path")` — the host attaches a
    /// DocumentScript at that path to the focused tile (P2.5). `None` when not called.
    pub attach_script: Option<String>,
    /// True when `detach_script()` was called — the host detaches the focused tile's
    /// script. (P2.5.)
    pub detach_script: bool,
    /// A `(kind, payload)` passed to `script_event("kind", "payload")` — the host
    /// delivers it to the focused tile's attached script. `None` when not called. (P2.5.)
    pub script_event: Option<(String, String)>,
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
        let relation_kind: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let sparql_query: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let attach_script: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let detach_script: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let script_event: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
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
        let s = snapshot.clone();
        engine.register_fn("inspect", move || {
            // The focused node's inspection rows as a map (`inspect()["Title"]`,
            // `inspect()["Content type"]`, …) — the command-line reach into the
            // Inspector's per-node content report.
            let mut map = Map::new();
            for (label, value) in &s.inspect {
                map.insert(label.as_str().into(), Dynamic::from(value.clone()));
            }
            map
        });

        // `relate("cites")` — the kind-qualified form. The 0-arg `relate()` above
        // (from the verb loop) emits `AssertEdge` with the default kind; this 1-arg
        // overload instead records the chosen relation kind, which the host applies
        // to the selected pair. rhai dispatches the two by arity.
        let rk = relation_kind.clone();
        engine.register_fn("relate", move |kind: &str| {
            *rk.borrow_mut() = Some(kind.to_string());
        });

        // `sparql("SELECT …")` — record the query for the host to run over the
        // focused graph and echo the result. Mirrors `relate("…")`: an arg-bearing
        // binding that records into the outcome rather than mutating, since the
        // shell snapshot has only node URLs, not the full RDF graph.
        let sq = sparql_query.clone();
        engine.register_fn("sparql", move |query: &str| {
            *sq.borrow_mut() = Some(query.to_string());
        });

        // DocumentScript triggers (P2.5): `attach_script("path.wasm")` attaches a
        // wasm component to the focused tile, `script_event("kind", "payload")`
        // delivers one event to it, `detach_script()` removes it. Like `sparql` /
        // `relate`, these record into the outcome; the host applies them on the
        // focused tile (resolving the script's capability permissions there).
        let asp = attach_script.clone();
        engine.register_fn("attach_script", move |path: &str| {
            *asp.borrow_mut() = Some(path.to_string());
        });
        let dsp = detach_script.clone();
        engine.register_fn("detach_script", move || {
            *dsp.borrow_mut() = true;
        });
        let sev = script_event.clone();
        engine.register_fn("script_event", move |kind: &str, payload: &str| {
            *sev.borrow_mut() = Some((kind.to_string(), payload.to_string()));
        });

        engine.set_max_operations(OP_BUDGET);
        let result = engine.eval::<Dynamic>(&desugar(source));
        let commands = commands.borrow().clone();
        let relation_kind = relation_kind.borrow().clone();
        let sparql_query = sparql_query.borrow().clone();
        let attach_script = attach_script.borrow().clone();
        let detach_script = *detach_script.borrow();
        let script_event = script_event.borrow().clone();
        match result {
            Ok(value) => ShellOutcome {
                text: stringify(value),
                commands,
                relation_kind,
                sparql_query,
                attach_script,
                detach_script,
                script_event,
                error: None,
            },
            Err(err) => ShellOutcome {
                text: String::new(),
                commands,
                relation_kind,
                sparql_query,
                attach_script,
                detach_script,
                script_event,
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
        .chain(["sparql", "attach_script", "detach_script", "script_event"])
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
            inspect: vec![
                ("Title".into(), "Welcome".into()),
                ("Content type".into(), "text/html".into()),
            ],
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
    fn relate_with_a_kind_records_the_relation_kind() {
        // The 0-arg form rides the AssertEdge command path (default kind).
        let plain = CommandShell::new().eval("relate", &ctx());
        assert_eq!(plain.commands, vec![Command::AssertEdge]);
        assert!(plain.relation_kind.is_none());
        // The 1-arg form records the chosen kind for the host to apply, and does
        // not also emit AssertEdge.
        let kinded = CommandShell::new().eval(r#"relate("cites")"#, &ctx());
        assert_eq!(kinded.relation_kind.as_deref(), Some("cites"));
        assert!(kinded.commands.is_empty());
        assert!(kinded.error.is_none());
    }

    #[test]
    fn sparql_records_the_query_for_the_host_to_run() {
        // The arg-bearing form records the query string; the host runs it (the
        // snapshot has no RDF graph). It emits no command and is not an error.
        let out = CommandShell::new().eval(r#"sparql("SELECT ?s WHERE { ?s ?p ?o }")"#, &ctx());
        assert_eq!(
            out.sparql_query.as_deref(),
            Some("SELECT ?s WHERE { ?s ?p ?o }")
        );
        assert!(out.commands.is_empty());
        assert!(out.error.is_none());
        // `complete` ghosts the verb so a user discovers it.
        assert_eq!(complete("spa"), Some("sparql"));
    }

    #[test]
    fn inspect_exposes_the_focused_nodes_rows_as_a_map() {
        // The focused node's inspection rows are queryable by label.
        let out = CommandShell::new().eval(r#"inspect()["Title"]"#, &ctx());
        assert_eq!(out.text, "Welcome");
        assert!(out.error.is_none());
        let ct = CommandShell::new().eval(r#"inspect()["Content type"]"#, &ctx());
        assert_eq!(ct.text, "text/html");
        // The bare form sugars to a call and stringifies the whole map.
        let bare = CommandShell::new().eval("inspect", &ctx());
        assert!(bare.error.is_none());
        assert!(!bare.text.is_empty(), "the map renders");
    }

    #[test]
    fn script_triggers_record_into_the_outcome() {
        // attach_script("path") records the component path; no command, no error.
        let out = CommandShell::new().eval(r#"attach_script("mods/hi.wasm")"#, &ctx());
        assert_eq!(out.attach_script.as_deref(), Some("mods/hi.wasm"));
        assert!(out.commands.is_empty() && out.error.is_none());

        // detach_script() flips the flag.
        let out = CommandShell::new().eval("detach_script()", &ctx());
        assert!(out.detach_script);
        assert!(out.error.is_none());

        // script_event("kind", "payload") records the pair.
        let out = CommandShell::new().eval(r#"script_event("click", "btn-1")"#, &ctx());
        assert_eq!(out.script_event, Some(("click".to_string(), "btn-1".to_string())));
        assert!(out.error.is_none());

        // The verbs ghost-complete so a user discovers them.
        assert_eq!(complete("attach"), Some("attach_script"));
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
