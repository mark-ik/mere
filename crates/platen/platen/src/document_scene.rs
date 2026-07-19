// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Document-canvas scene-input derivation.
//!
//! Wraps [`document_canvas`] for composition-time use, mirroring how
//! [`crate::canvas_scene`] wraps `graph-canvas`. The wrapper takes a
//! reducer-owned [`inker::EngineDocument`] plus the pane's available
//! viewport and returns a [`document_canvas::LaidOutDocument`] — the
//! render packet plus the font sidecar — ready for the host to render.
//!
//! Platen itself stays canvas-kind-independent: it composes whichever
//! canvas swatch is bound to a frame pane, without growing layout logic
//! of its own.

use document_canvas::{DocumentStyleSheet, LaidOutDocument, Viewport, layout_document};
use inker::EngineDocument;

/// Lay out a document for a single frame pane.
///
/// `viewport` is the pane's available content rectangle (already accounting
/// for chrome / scroll-bar reservation if any). `style` is supplied by the
/// host so users can tune typography per workspace; pass
/// `DocumentStyleSheet::default()` for the built-in defaults.
///
/// Returns a [`LaidOutDocument`]: the portable render packet plus the
/// font sidecar carrying the faces parley shaped against (the host needs
/// both to lower text to real glyphs).
pub fn build_document_scene(
    document: &EngineDocument,
    viewport: Viewport,
    style: &DocumentStyleSheet,
) -> LaidOutDocument {
    layout_document(document, viewport, style)
}

#[cfg(test)]
mod tests {
    use super::*;
    use document_canvas::RenderedBlockKind;
    use inker::{Block, DocumentProvenance, DocumentTrustState, InlineSpan};

    fn doc(blocks: Vec<Block>) -> EngineDocument {
        EngineDocument {
            address: "doc:platen-test".into(),
            title: None,
            content_type: "text/plain".into(),
            lang: None,
            provenance: DocumentProvenance::default(),
            trust: DocumentTrustState::Unknown,
            diagnostics: Vec::new(),
            blocks,
        }
    }

    #[test]
    fn empty_document_scene_returns_empty_packet() {
        let packet = build_document_scene(
            &doc(vec![]),
            Viewport::new(640.0, 480.0),
            &DocumentStyleSheet::default(),
        )
        .packet;
        assert!(packet.blocks.is_empty());
        assert_eq!(packet.viewport.width, 640.0);
    }

    #[test]
    fn single_paragraph_emits_one_text_block() {
        let packet = build_document_scene(
            &doc(vec![Block::Paragraph {
                spans: vec![InlineSpan::Text("Hello, platen.".into())],
            }]),
            Viewport::new(640.0, 480.0),
            &DocumentStyleSheet::default(),
        )
        .packet;
        assert_eq!(packet.blocks.len(), 1);
        assert!(matches!(
            &packet.blocks[0].kind,
            RenderedBlockKind::Text { .. }
        ));
    }

    #[test]
    fn document_scene_threads_links_through_to_interactions() {
        let packet = build_document_scene(
            &doc(vec![Block::Paragraph {
                spans: vec![InlineSpan::Link {
                    url: "mere://node/notes".into(),
                    title: None,
                    spans: vec![InlineSpan::Text("notes".into())],
                    predicate: None,
                }],
            }]),
            Viewport::new(640.0, 480.0),
            &DocumentStyleSheet::default(),
        )
        .packet;
        assert_eq!(packet.interactions.len(), 1);
    }

    #[test]
    fn viewport_width_constrains_content_bounds() {
        let style = DocumentStyleSheet::default();
        let narrow = build_document_scene(
            &doc(vec![Block::Paragraph {
                spans: vec![InlineSpan::Text(
                    "A long paragraph that should wrap across multiple lines when given little horizontal room."
                        .into(),
                )],
            }]),
            Viewport::new(160.0, 800.0),
            &style,
        )
        .packet;
        let wide = build_document_scene(
            &doc(vec![Block::Paragraph {
                spans: vec![InlineSpan::Text(
                    "A long paragraph that should wrap across multiple lines when given little horizontal room."
                        .into(),
                )],
            }]),
            Viewport::new(640.0, 800.0),
            &style,
        )
        .packet;
        // Narrower viewport produces taller content (more wrapped lines).
        assert!(
            narrow.content_bounds.size.height > wide.content_bounds.size.height,
            "narrow {}px wide should wrap to taller content than wide {}px wide",
            narrow.viewport.width,
            wide.viewport.width
        );
    }
}
