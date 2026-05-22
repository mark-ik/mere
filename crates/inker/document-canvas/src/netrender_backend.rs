/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Netrender backend.
//!
//! Thin lowering shim: builds an [`crate::paint_list::InkerPaintList`]
//! from a [`crate::DocumentRenderPacket`] and lowers it to a
//! `netrender::Scene` via the shared `paint_list_render` translator —
//! the same path serval and any other engine use. All the document →
//! paint-command logic lives in [`crate::paint_list`]; this module only
//! exists to feed the result through the renderer-coupled translator
//! (behind the `netrender` feature).

use netrender::Scene;

use crate::font::FontResolver;
use crate::paint_list::paint_list_from_packet;
use crate::style::ColorVocabulary;
use crate::types::DocumentRenderPacket;

/// Build a `netrender::Scene` from a `DocumentRenderPacket`.
///
/// `resolver` supplies font face bytes for real glyph emission; pass
/// [`crate::NoFontResolver`] when no fonts are wired yet (every glyph
/// run falls back to a placeholder rect). `colors` supplies the theme
/// primitives.
pub fn scene_from_packet<R: FontResolver + ?Sized>(
    packet: &DocumentRenderPacket,
    resolver: &R,
    colors: &ColorVocabulary,
) -> Scene {
    let list = paint_list_from_packet(packet, resolver, colors);
    paint_list_render::translate_paint_list(&list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::{FontFaceData, FontRequest};
    use crate::layout::layout_document;
    use crate::style::StyleConfig;
    use crate::types::Viewport;
    use crate::NoFontResolver;
    use inker::{
        DocumentBlock, DocumentProvenance, DocumentTrustState, EngineDocument, InlineSpan,
    };

    fn doc(blocks: Vec<DocumentBlock>) -> EngineDocument {
        EngineDocument {
            address: "doc:netrender-test".into(),
            title: None,
            content_type: "text/plain".into(),
            lang: None,
            provenance: DocumentProvenance::default(),
            trust: DocumentTrustState::Unknown,
            diagnostics: Vec::new(),
            blocks,
        }
    }

    fn packet_for(blocks: Vec<DocumentBlock>) -> DocumentRenderPacket {
        layout_document(&doc(blocks), Viewport::new(640.0, 480.0), &StyleConfig::default())
    }

    struct AlwaysResolves;
    impl FontResolver for AlwaysResolves {
        fn resolve_font_data(&self, _request: FontRequest<'_>) -> Option<FontFaceData> {
            Some(FontFaceData {
                data: vec![0u8; 4],
                index: 0,
            })
        }
    }

    #[test]
    fn empty_packet_produces_empty_scene() {
        let scene = scene_from_packet(&packet_for(vec![]), &NoFontResolver, &ColorVocabulary::default());
        assert_eq!(scene.ops.len(), 0);
    }

    #[test]
    fn no_resolver_lowers_placeholder_rect_to_scene_rect() {
        let scene = scene_from_packet(
            &packet_for(vec![DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Text("hello".into())],
            }]),
            &NoFontResolver,
            &ColorVocabulary::default(),
        );
        // One placeholder DrawRect → one SceneOp::Rect.
        assert_eq!(scene.ops.len(), 1);
        assert!(matches!(scene.ops[0], netrender::SceneOp::Rect(_)));
    }

    #[test]
    fn resolver_lowers_drawtext_to_glyph_run() {
        let scene = scene_from_packet(
            &packet_for(vec![DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Text("hello".into())],
            }]),
            &AlwaysResolves,
            &ColorVocabulary::default(),
        );
        let glyph_runs = scene
            .ops
            .iter()
            .filter(|op| matches!(op, netrender::SceneOp::GlyphRun(_)))
            .count();
        assert!(glyph_runs >= 1, "expected at least one GlyphRun op");
    }

    #[test]
    fn viewport_rounds_to_u32() {
        let packet = layout_document(&doc(vec![]), Viewport::new(640.4, 480.6), &StyleConfig::default());
        let scene = scene_from_packet(&packet, &NoFontResolver, &ColorVocabulary::default());
        assert_eq!(scene.viewport_width, 640);
        assert_eq!(scene.viewport_height, 481);
    }
}
