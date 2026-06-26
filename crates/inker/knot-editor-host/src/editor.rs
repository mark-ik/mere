/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The editor model: the host-side brain of the knot editor.
//!
//! Holds the knot source text and turns it into the two things the editor surface
//! draws: the **highlight / structure** of the source (via the portable
//! [`knot_editor`] pipe, with this crate's full pack so polyglot blocks colour),
//! and the **rendered preview** (via the same `DjotKnotEngine` the rest of the app
//! renders knots through). The host wires its text widget's buffer to
//! [`KnotEditor::set_source`] and draws [`highlights`](KnotEditor::highlights) over
//! the source plus [`rendered`](KnotEditor::rendered) in the preview pane.

use inker::{Engine, EngineDocument, EngineError, EngineInput};
use knot_editor::{Fold, InjectionRegistry, OutlineItem, Span, folds, highlight, outline};
use nematic::DjotKnotEngine;

/// A knot being edited: its source text, the injection registry it highlights
/// with, and the engine it renders the preview through. Re-derive on edit (cheap
/// at note size); the source text is the single source of truth.
pub struct KnotEditor {
    source: String,
    address: String,
    registry: InjectionRegistry,
    engine: DjotKnotEngine,
}

impl KnotEditor {
    /// Open `source` as a knot addressed at `address` (the address the preview
    /// renders against — a `mere://` / `knot://` node URL once that exists, or a
    /// placeholder for a scratch note).
    pub fn new(address: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            address: address.into(),
            registry: crate::full_pack(),
            engine: DjotKnotEngine::new(),
        }
    }

    /// The current source text (what the host's text widget edits).
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Replace the source after an edit. The host syncs its buffer here; highlight
    /// / structure / preview re-derive on the next call.
    pub fn set_source(&mut self, source: impl Into<String>) {
        self.source = source.into();
    }

    /// Highlight spans over the source: djot structure plus injected inner-language
    /// colouring for polyglot blocks. The `(range, kind)` channel the edit surface
    /// paints.
    pub fn highlights(&self) -> Vec<Span> {
        highlight(&self.source, &self.registry)
    }

    /// The heading outline (levels + text), for the gloss outline lens.
    pub fn outline(&self) -> Vec<OutlineItem> {
        outline(&self.source)
    }

    /// Collapsible regions of the source.
    pub fn folds(&self) -> Vec<Fold> {
        folds(&self.source)
    }

    /// The rendered preview: the source run through `DjotKnotEngine` into the
    /// portable block model the document-canvas pane draws. Errors only on engine
    /// failure (malformed input still yields a best-effort document).
    pub fn rendered(&self) -> Result<EngineDocument, EngineError> {
        self.engine
            .render(&EngineInput::new(self.address.clone(), self.source.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knot_editor::SyntaxKind;

    const SAMPLE: &str =
        "# Title\n\nSome _em_ and a code block:\n\n```json\n{\"a\": 1}\n```\n";

    #[test]
    fn highlights_cover_djot_and_injected_json() {
        let ed = KnotEditor::new("mere://note/test", SAMPLE);
        let hl = ed.highlights();
        assert!(
            hl.iter().any(|s| s.kind == SyntaxKind::Heading),
            "djot heading: {hl:?}"
        );
        assert!(
            hl.iter().any(|s| s.kind == SyntaxKind::StringLit),
            "injected json string: {hl:?}"
        );
        assert!(hl.iter().any(|s| s.kind == SyntaxKind::CodeBlock));
    }

    #[test]
    fn renders_a_preview_document_through_the_engine() {
        let ed = KnotEditor::new("mere://note/test", SAMPLE);
        let doc = ed.rendered().expect("engine renders");
        assert!(!doc.blocks.is_empty(), "preview should have blocks");
        assert_eq!(doc.content_type, "text/x-knot");
    }

    #[test]
    fn outline_finds_the_heading() {
        let ed = KnotEditor::new("mere://note/test", SAMPLE);
        let items = ed.outline();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "Title");
    }

    #[test]
    fn editing_updates_the_derived_views() {
        let mut ed = KnotEditor::new("mere://note/test", "# One\n");
        assert_eq!(ed.outline().len(), 1);
        ed.set_source("# One\n\n## Two\n");
        assert_eq!(ed.outline().len(), 2);
    }
}
