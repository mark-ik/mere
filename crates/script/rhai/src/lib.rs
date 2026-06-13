/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Rhai backend for the knot block-evaluator lane.
//!
//! [`RhaiEvaluator`] implements inker's thin [`BlockEvaluator`] slice over the
//! [Rhai](https://crates.io/crates/rhai) engine: evaluate a `rhai eval` fence
//! to text, bounded so a runaway can't hang the render. Rhai fits this lane
//! better than most — it is pure Rust, sandboxed by default (the standard
//! engine exposes arithmetic, strings, arrays, and maps but no file or
//! network functions), and has a **native operation budget**
//! (`Engine::set_max_operations`), so the runaway cap is the engine's own
//! rather than a hand-rolled fuel loop.
//!
//! This is deliberately *not* serval's full DOM-shaped `ScriptEngine` seam
//! (reflectors, host promises, native callbacks). Rhai is a logic/mod
//! language with no DOM, async, or GC; the knot path needs only eval +
//! budget + to-text, which is this whole crate.
//!
//! ## Output convention
//!
//! A script's result becomes the rendered block. Return a string (or any
//! value, stringified) for plain/auto-detected output, or an object map
//! `#{ format: "gemtext", text: "## hi" }` to declare the format explicitly
//! (so the output nested-renders through that engine).

use inker::{BlockEvaluator, EvalOutput};
use rhai::{Dynamic, Engine, Map};

/// A Rhai-backed [`BlockEvaluator`] for `rhai eval` knot fences.
pub struct RhaiEvaluator {
    engine: Engine,
}

impl RhaiEvaluator {
    /// A sandboxed Rhai evaluator. The standard engine has no file or network
    /// access; this adds belt-and-braces depth/recursion caps against
    /// pathological scripts (the operation budget is per-call in
    /// [`eval_block`](BlockEvaluator::eval_block)).
    pub fn new() -> Self {
        let mut engine = Engine::new();
        engine.set_max_call_levels(64);
        engine.set_max_expr_depths(128, 64);
        // No `print`/`debug` to stdout from a rendered note.
        engine.on_print(|_| {});
        engine.on_debug(|_, _, _| {});
        Self { engine }
    }
}

impl Default for RhaiEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockEvaluator for RhaiEvaluator {
    fn language(&self) -> &str {
        "rhai"
    }

    fn eval_block(&mut self, source: &str, max_ops: u64) -> Result<EvalOutput, String> {
        // Rhai's native runaway guard: `max_operations(n)` aborts after ~n AST
        // operations with `ErrorTooManyOperations`; `0` means unlimited, so an
        // unbounded request (max_ops == 0) maps to that.
        self.engine.set_max_operations(max_ops);
        let value: Dynamic = self
            .engine
            .eval::<Dynamic>(source)
            .map_err(|e| e.to_string())?;
        Ok(to_output(value))
    }
}

/// Turn a Rhai result into an [`EvalOutput`]. An object map carrying `format`
/// and `text` declares its rendering explicitly; anything else stringifies
/// and is format-detected.
fn to_output(value: Dynamic) -> EvalOutput {
    if value.is_map() {
        let map: Map = value.cast();
        let text = map
            .get("text")
            .map(|d| d.to_string())
            .unwrap_or_default();
        match map.get("format") {
            Some(format) => EvalOutput {
                format: format.to_string(),
                text,
            },
            None => EvalOutput::detect(text),
        }
    } else if value.is_unit() {
        EvalOutput::plain(String::new())
    } else {
        EvalOutput::detect(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_tag_is_rhai() {
        assert_eq!(RhaiEvaluator::new().language(), "rhai");
    }

    #[test]
    fn evaluates_arithmetic_to_plain_text() {
        let mut evaluator = RhaiEvaluator::new();
        let out = evaluator
            .eval_block("let s = 0; for i in 1..=10 { s += i } s", 1_000_000)
            .unwrap();
        assert_eq!(out.format, "plain");
        assert_eq!(out.text, "55");
    }

    #[test]
    fn a_gemtext_string_is_detected() {
        let mut evaluator = RhaiEvaluator::new();
        let out = evaluator
            .eval_block("\"## generated\\n=> gemini://x.test/ a link\"", 1_000_000)
            .unwrap();
        assert_eq!(out.format, "gemtext");
        assert!(out.text.starts_with("## generated"));
    }

    #[test]
    fn an_explicit_format_map_is_honored() {
        let mut evaluator = RhaiEvaluator::new();
        let out = evaluator
            .eval_block(r#"#{ format: "markdown", text: "**bold**" }"#, 1_000_000)
            .unwrap();
        assert_eq!(out.format, "markdown");
        assert_eq!(out.text, "**bold**");
    }

    #[test]
    fn a_runaway_is_caught_by_the_operation_budget() {
        let mut evaluator = RhaiEvaluator::new();
        let err = evaluator
            .eval_block("loop { }", 10_000)
            .unwrap_err();
        // Rhai aborts a runaway with a too-many-operations error, not a hang.
        assert!(
            err.to_lowercase().contains("operation"),
            "expected an operation-budget error, got: {err}"
        );
        // The evaluator is still usable after stopping a runaway.
        let out = evaluator.eval_block("1 + 1", 1_000_000).unwrap();
        assert_eq!(out.text, "2");
    }

    #[test]
    fn a_compile_error_is_reported_not_panicked() {
        let mut evaluator = RhaiEvaluator::new();
        assert!(evaluator.eval_block("this is not valid rhai @@@", 1_000).is_err());
    }

    #[test]
    fn the_sandbox_has_no_file_access() {
        let mut evaluator = RhaiEvaluator::new();
        // `open_file` / filesystem builtins are not registered in the standard
        // engine, so a script that reaches for one fails to resolve it.
        let err = evaluator
            .eval_block(r#"open_file("/etc/passwd")"#, 1_000_000)
            .unwrap_err();
        assert!(err.to_lowercase().contains("function") || err.to_lowercase().contains("not"));
    }

    /// The full polyglot path: a `rhai eval` fence, routed through the
    /// `BlockEvaluators` registry and inker's evaluate pass, renders its
    /// output inline. (The registry is the language menu — an unregistered
    /// tag simply isn't found.)
    #[test]
    fn a_rhai_fence_evaluates_through_the_inker_pass() {
        use inker::{
            evaluate_blocks, BlockEvaluators, DocumentBlock, DocumentProvenance,
            DocumentTrustState, EngineDocument, EngineInput, EvaluationPolicy, InlineSpan,
        };

        let mut registry = BlockEvaluators::new();
        registry.register(Box::new(RhaiEvaluator::new()));
        assert_eq!(registry.languages(), vec!["rhai"]);

        let mut document = EngineDocument {
            address: "note.knot".into(),
            title: None,
            content_type: "text/x-knot".into(),
            lang: None,
            provenance: DocumentProvenance::default(),
            trust: DocumentTrustState::Unknown,
            diagnostics: Vec::new(),
            blocks: vec![DocumentBlock::CodeBlock {
                language: Some("rhai eval".into()),
                text: "let s = 0; for i in 1..=10 { s += i } \"sum is \" + s".into(),
            }],
        };

        let mut evaluate =
            |lang: &str, source: &str| registry.evaluate(lang, source, 1_000_000);
        let mut render =
            |_input: &EngineInput| -> Result<EngineDocument, String> { unreachable!("plain output") };
        let policy = EvaluationPolicy::for_own_notes(vec!["rhai".into()]);

        let outcome = evaluate_blocks(&mut document, &mut evaluate, &mut render, &policy);
        assert_eq!(outcome.evaluated, 1);
        assert!(matches!(
            &document.blocks[0],
            DocumentBlock::Paragraph { spans }
                if spans == &vec![InlineSpan::Text("sum is 55".into())]
        ));
    }
}
