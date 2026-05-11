/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Netrender backend.
//!
//! Walks a [`crate::DocumentRenderPacket`] and emits a `netrender::Scene`
//! ready to feed netrender's wgpu rasterizer.
//!
//! ## Glyph emission
//!
//! For each [`crate::GlyphRun`], the backend asks the supplied
//! [`crate::FontResolver`] to map `(family, weight, style)` to a
//! `netrender::FontId`:
//!
//! - **Resolver returns `Some(id)`** — emit a real `Scene::push_glyph_run`
//!   with the resolver's font ID, the run's positioned glyphs translated
//!   into netrender's `Glyph` shape, and the appropriate text color from
//!   the [`crate::ColorVocabulary`].
//! - **Resolver returns `None`** — fall back to a placeholder filled rect
//!   at the run's bounds in `placeholder_text` color, so the text-shaped
//!   region is still visible during host bring-up.
//!
//! Image, Rule, and Group blocks emit their visual primitives through the
//! same color vocabulary (`placeholder_image`, `rule`, recursion).
//!
//! Interaction regions (`InteractionKind::Link`) are not represented in
//! the Scene — netrender renders pixels, not hit-test trees. The host
//! consumes the packet's `interactions` separately to build its hit-test
//! layer.

use netrender::{Glyph as NetGlyph, Scene, ScenePath};

use crate::font::{FontRequest, FontResolver};
use crate::style::ColorVocabulary;
use crate::types::{DocumentRenderPacket, GlyphRun, Rect, RenderedBlock, RenderedBlockKind};

const RULE_STROKE_WIDTH: f32 = 1.0;

/// Build a `netrender::Scene` from a `DocumentRenderPacket`.
///
/// `resolver` provides font IDs for real glyph emission; pass
/// [`crate::NoFontResolver`] when no fonts are wired yet (every glyph
/// run falls back to a placeholder rect).
///
/// `colors` supplies the theme primitives.
pub fn scene_from_packet<R: FontResolver + ?Sized>(
    packet: &DocumentRenderPacket,
    resolver: &R,
    colors: &ColorVocabulary,
) -> Scene {
    let viewport_w = packet.viewport.width.max(0.0).round() as u32;
    let viewport_h = packet.viewport.height.max(0.0).round() as u32;
    let mut scene = Scene::new(viewport_w, viewport_h);
    for block in &packet.blocks {
        emit_block(&mut scene, block, resolver, colors);
    }
    scene
}

fn emit_block<R: FontResolver + ?Sized>(
    scene: &mut Scene,
    block: &RenderedBlock,
    resolver: &R,
    colors: &ColorVocabulary,
) {
    match &block.kind {
        RenderedBlockKind::Text { glyph_runs } => {
            for run in glyph_runs {
                emit_glyph_run(scene, run, resolver, colors);
            }
        }
        RenderedBlockKind::Image { url: _, alt: _ } => {
            push_filled_rect(scene, block.bounds, colors.placeholder_image);
        }
        RenderedBlockKind::Rule => {
            // Hairline at vertical center of the rect.
            let mid_y = block.bounds.origin.y + block.bounds.size.height * 0.5;
            let mut path = ScenePath::new();
            path.move_to(block.bounds.origin.x, mid_y);
            path.line_to(block.bounds.max_x(), mid_y);
            scene.push_shape_stroked(path, colors.rule, RULE_STROKE_WIDTH);
        }
        RenderedBlockKind::Group { children } => {
            for child in children {
                emit_block(scene, child, resolver, colors);
            }
        }
    }
}

fn emit_glyph_run<R: FontResolver + ?Sized>(
    scene: &mut Scene,
    run: &GlyphRun,
    resolver: &R,
    colors: &ColorVocabulary,
) {
    let request = FontRequest {
        family: run.font_family.as_str(),
        weight: run.font_weight,
        style: run.font_style,
    };
    let Some(font_id) = resolver.resolve_font_id(request) else {
        // No resolver-provided font for this run — placeholder rect at
        // the run's approximate bounds so the host sees *something* lit
        // up while font wiring catches up.
        push_filled_rect(scene, glyph_run_bounds(run), colors.placeholder_text);
        return;
    };

    // Translate each PositionedGlyph (relative to run.origin) into a
    // netrender Glyph (absolute in scene space, baseline-anchored).
    let baseline_y = run.origin.y + run.baseline_y;
    let glyphs: Vec<NetGlyph> = run
        .glyphs
        .iter()
        .map(|g| NetGlyph {
            id: g.glyph_id,
            x: run.origin.x + g.x,
            y: baseline_y + g.y,
        })
        .collect();

    if glyphs.is_empty() {
        return;
    }

    scene.push_glyph_run(font_id, run.font_size, glyphs, colors.body_text);
}

fn glyph_run_bounds(run: &GlyphRun) -> Rect {
    let advance: f32 = run.glyphs.iter().map(|g| g.advance).sum();
    Rect::from_xywh(
        run.origin.x,
        run.origin.y,
        advance.max(1.0),
        // Approximate run height as font size * a generous line-gap factor.
        run.font_size * 1.4,
    )
}

fn push_filled_rect(scene: &mut Scene, bounds: Rect, color: [f32; 4]) {
    if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
        return;
    }
    let mut path = ScenePath::new();
    let x0 = bounds.origin.x;
    let y0 = bounds.origin.y;
    let x1 = bounds.max_x();
    let y1 = bounds.max_y();
    path.move_to(x0, y0);
    path.line_to(x1, y0);
    path.line_to(x1, y1);
    path.line_to(x0, y1);
    path.line_to(x0, y0);
    scene.push_shape_filled(path, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_document;
    use crate::style::StyleConfig;
    use crate::types::{TextStyle, Viewport};
    use crate::{NoFontResolver, font::FontRequest};
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
        layout_document(
            &doc(blocks),
            Viewport::new(640.0, 480.0),
            &StyleConfig::default(),
        )
    }

    /// Resolver that returns a deterministic ID for any request — used
    /// to test that real glyph runs get emitted when fonts are wired.
    struct AlwaysResolves;
    impl FontResolver for AlwaysResolves {
        fn resolve_font_id(&self, _request: FontRequest<'_>) -> Option<u32> {
            Some(42)
        }
    }

    /// Resolver that returns IDs only for monospace requests — tests the
    /// per-run dispatch path.
    struct MonospaceOnlyResolver;
    impl FontResolver for MonospaceOnlyResolver {
        fn resolve_font_id(&self, request: FontRequest<'_>) -> Option<u32> {
            if request.family.eq_ignore_ascii_case("monospace") {
                Some(7)
            } else {
                None
            }
        }
    }

    #[test]
    fn empty_packet_produces_empty_scene() {
        let scene = scene_from_packet(
            &packet_for(vec![]),
            &NoFontResolver,
            &ColorVocabulary::default(),
        );
        assert_eq!(scene.ops.len(), 0);
    }

    #[test]
    fn no_resolver_falls_back_to_placeholder_rects() {
        let scene = scene_from_packet(
            &packet_for(vec![DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Text("hello".into())],
            }]),
            &NoFontResolver,
            &ColorVocabulary::default(),
        );
        // One paragraph → one glyph run → one placeholder rect (no
        // GlyphRun ops because the resolver returned None).
        assert_eq!(scene.ops.len(), 1);
        assert!(matches!(scene.ops[0], netrender::SceneOp::Shape(_)));
    }

    #[test]
    fn resolver_returning_id_emits_real_glyph_run() {
        let scene = scene_from_packet(
            &packet_for(vec![DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Text("hello".into())],
            }]),
            &AlwaysResolves,
            &ColorVocabulary::default(),
        );
        // GlyphRun op present (instead of placeholder Shape).
        let glyph_runs = scene
            .ops
            .iter()
            .filter(|op| matches!(op, netrender::SceneOp::GlyphRun(_)))
            .count();
        assert!(glyph_runs >= 1, "expected at least one GlyphRun op");
    }

    #[test]
    fn rule_uses_color_vocabulary_for_stroke() {
        let mut colors = ColorVocabulary::default();
        colors.rule = [1.0, 0.0, 0.0, 1.0]; // bright red marker
        let scene = scene_from_packet(
            &packet_for(vec![DocumentBlock::Rule]),
            &NoFontResolver,
            &colors,
        );
        assert_eq!(scene.ops.len(), 1);
        match &scene.ops[0] {
            netrender::SceneOp::Shape(shape) => {
                let stroke_color = shape.stroke.as_ref().expect("expected stroked rule");
                assert_eq!(stroke_color.color, [1.0, 0.0, 0.0, 1.0]);
            }
            other => panic!("expected Shape (stroked), got {other:?}"),
        }
    }

    #[test]
    fn group_recurses_into_children() {
        let scene = scene_from_packet(
            &packet_for(vec![DocumentBlock::List {
                ordered: false,
                items: vec![
                    vec![DocumentBlock::Paragraph {
                        spans: vec![InlineSpan::Text("first".into())],
                    }],
                    vec![DocumentBlock::Paragraph {
                        spans: vec![InlineSpan::Text("second".into())],
                    }],
                ],
            }]),
            &NoFontResolver,
            &ColorVocabulary::default(),
        );
        // Each list item is one paragraph → one placeholder Shape each.
        assert_eq!(scene.ops.len(), 2);
    }

    #[test]
    fn monospace_only_resolver_dispatches_per_run() {
        // CodeBlock → monospace family → resolver returns Some → real
        // glyph run. Paragraph in body family → resolver returns None →
        // placeholder rect.
        let scene = scene_from_packet(
            &packet_for(vec![
                DocumentBlock::CodeBlock {
                    language: None,
                    text: "fn main() {}".into(),
                },
                DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text("body text".into())],
                },
            ]),
            &MonospaceOnlyResolver,
            &ColorVocabulary::default(),
        );
        let has_glyph_run = scene
            .ops
            .iter()
            .any(|op| matches!(op, netrender::SceneOp::GlyphRun(_)));
        let has_placeholder = scene
            .ops
            .iter()
            .any(|op| matches!(op, netrender::SceneOp::Shape(_)));
        assert!(has_glyph_run, "expected GlyphRun op for code block");
        assert!(has_placeholder, "expected placeholder Shape for paragraph");
    }

    #[test]
    fn viewport_rounds_to_u32() {
        let packet = layout_document(
            &doc(vec![]),
            Viewport::new(640.4, 480.6),
            &StyleConfig::default(),
        );
        let scene = scene_from_packet(&packet, &NoFontResolver, &ColorVocabulary::default());
        assert_eq!(scene.viewport_width, 640);
        assert_eq!(scene.viewport_height, 481);
    }
}
