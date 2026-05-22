/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! PaintList producer.
//!
//! Walks a [`crate::DocumentRenderPacket`] and emits an [`InkerPaintList`]
//! — an implementation of [`paint_list_api::PaintList`], the engine-facing
//! display-list vocabulary shared across the Mere/Serval renderer
//! ecosystem. The list lowers to a `netrender::Scene` via
//! `paint_list_render` (see [`crate::netrender_backend`]); it can equally
//! cross IPC or sit in a capture/replay fixture, since the vocabulary is
//! fully serializable and self-contained.
//!
//! This is the portable half of the rendering path: it depends only on
//! `paint_list_api` (euclid + serde, no wgpu), so it builds everywhere
//! the rest of document-canvas does. The Scene lowering lives behind the
//! `netrender` feature.
//!
//! ## Font handling
//!
//! Each glyph run resolves `(family, weight, style)` to face bytes via
//! [`crate::FontResolver::resolve_font_data`]. Resolved faces are interned
//! into the list's [`PaintList::fonts`] side-table (once per unique
//! request) and referenced from each `DrawText` by `FontInstanceKey`. A
//! run whose face the resolver can't supply falls back to a placeholder
//! rect, so the text-shaped region is still visible during host bring-up.
//!
//! Interaction regions (`InteractionKind::Link`) are not represented in
//! the paint list — it carries pixels, not hit-test trees. The host
//! consumes the packet's `interactions` separately.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use paint_list_api::{
    ColorF, CommonPlacement, DeviceIntSize, EngineId, FontInstanceKey, FontResource, GlyphInstance,
    IdNamespace, LayoutPoint, LayoutRect, LineItem, LineOrientation, LineStyle, PaintCmd, PaintList,
    RectItem, TextOptions, TextRunItem,
};

use crate::font::{FontRequest, FontResolver};
use crate::style::ColorVocabulary;
use crate::types::{DocumentRenderPacket, GlyphRun, Rect, RenderedBlock, RenderedBlockKind, TextStyle};

/// Half the hairline thickness used for [`RenderedBlockKind::Rule`]
/// (the rule fills a 1px strip centered on its mid-line).
const RULE_HALF_THICKNESS: f32 = 0.5;

/// A document-view paint list: the unit of paint output for one rendered
/// frame of an [`inker`](https://crates.io/crates/inker) document.
/// Implements [`PaintList`] so it lowers through the shared
/// `paint_list_render` translator like any other engine's output.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InkerPaintList {
    viewport: DeviceIntSize,
    commands: Vec<PaintCmd>,
    fonts: Vec<FontResource>,
    generation: u64,
}

impl PaintList for InkerPaintList {
    fn engine_id(&self) -> EngineId {
        EngineId::INKER
    }
    fn viewport(&self) -> DeviceIntSize {
        self.viewport
    }
    fn generation_id(&self) -> u64 {
        self.generation
    }
    fn commands(&self) -> &[PaintCmd] {
        &self.commands
    }
    fn fonts(&self) -> &[FontResource] {
        &self.fonts
    }
}

/// Build an [`InkerPaintList`] from a [`DocumentRenderPacket`].
///
/// `resolver` supplies font face bytes for real glyph emission; pass
/// [`crate::NoFontResolver`] when no fonts are wired yet (every glyph run
/// falls back to a placeholder rect). `colors` supplies theme primitives.
pub fn paint_list_from_packet<R: FontResolver + ?Sized>(
    packet: &DocumentRenderPacket,
    resolver: &R,
    colors: &ColorVocabulary,
) -> InkerPaintList {
    let viewport = DeviceIntSize::new(
        packet.viewport.width.max(0.0).round() as i32,
        packet.viewport.height.max(0.0).round() as i32,
    );
    let mut builder = Builder::new(resolver, colors);
    for block in &packet.blocks {
        builder.emit_block(block);
    }
    InkerPaintList {
        viewport,
        commands: builder.commands,
        fonts: builder.fonts,
        generation: 0,
    }
}

/// Walk state: accumulates `PaintCmd`s + the font side-table, interning
/// each `(family, weight, style)` request to a single `FontInstanceKey`.
struct Builder<'a, R: FontResolver + ?Sized> {
    resolver: &'a R,
    colors: &'a ColorVocabulary,
    commands: Vec<PaintCmd>,
    fonts: Vec<FontResource>,
    font_keys: HashMap<(String, u16, TextStyle), FontInstanceKey>,
    next_font_key: u32,
}

impl<'a, R: FontResolver + ?Sized> Builder<'a, R> {
    fn new(resolver: &'a R, colors: &'a ColorVocabulary) -> Self {
        Self {
            resolver,
            colors,
            commands: Vec::new(),
            fonts: Vec::new(),
            font_keys: HashMap::new(),
            next_font_key: 0,
        }
    }

    fn emit_block(&mut self, block: &RenderedBlock) {
        match &block.kind {
            RenderedBlockKind::Text { glyph_runs } => {
                for run in glyph_runs {
                    self.emit_glyph_run(run);
                }
            },
            RenderedBlockKind::Image { .. } => {
                self.push_rect(block.bounds, self.colors.placeholder_image);
            },
            RenderedBlockKind::Rule => {
                // Hairline: a 1px-tall strip centered on the rect's
                // vertical midpoint. Lowered as a (filled) line primitive.
                let mid_y = block.bounds.origin.y + block.bounds.size.height * 0.5;
                let strip = Rect::from_xywh(
                    block.bounds.origin.x,
                    mid_y - RULE_HALF_THICKNESS,
                    block.bounds.size.width,
                    RULE_HALF_THICKNESS * 2.0,
                );
                self.commands.push(PaintCmd::DrawLine(LineItem {
                    placement: CommonPlacement::new(layout_rect(strip)),
                    color: colorf(self.colors.rule),
                    style: LineStyle::Solid,
                    orientation: LineOrientation::Horizontal,
                    wavy_thickness: 0.0,
                }));
            },
            RenderedBlockKind::Group { children } => {
                for child in children {
                    self.emit_block(child);
                }
            },
        }
    }

    fn emit_glyph_run(&mut self, run: &GlyphRun) {
        let request = FontRequest {
            family: run.font_family.as_str(),
            weight: run.font_weight,
            style: run.font_style,
        };
        let Some(font_instance) = self.intern_font(request) else {
            // No resolver-provided face for this run — placeholder rect at
            // the run's approximate bounds so the host sees something lit
            // up while font wiring catches up.
            self.push_rect(glyph_run_bounds(run), self.colors.placeholder_text);
            return;
        };

        // Translate each PositionedGlyph (relative to run.origin) into a
        // PaintList GlyphInstance (absolute in packet space, baseline-
        // anchored).
        let baseline_y = run.origin.y + run.baseline_y;
        let glyphs: Vec<GlyphInstance> = run
            .glyphs
            .iter()
            .map(|g| GlyphInstance {
                index: g.glyph_id,
                point: LayoutPoint::new(run.origin.x + g.x, baseline_y + g.y),
            })
            .collect();
        if glyphs.is_empty() {
            return;
        }

        self.commands.push(PaintCmd::DrawText(TextRunItem {
            placement: CommonPlacement::new(layout_rect(glyph_run_bounds(run))),
            font_instance,
            font_size: run.font_size,
            color: colorf(self.colors.body_text),
            glyphs,
            options: TextOptions::default(),
        }));
    }

    /// Resolve + intern a font request, returning the `FontInstanceKey`
    /// the side-table holds for it (minting + recording the face on first
    /// sight). `None` if the resolver has no face for this request.
    fn intern_font(&mut self, request: FontRequest<'_>) -> Option<FontInstanceKey> {
        let map_key = (request.family.to_string(), request.weight, request.style);
        if let Some(&key) = self.font_keys.get(&map_key) {
            return Some(key);
        }
        let face = self.resolver.resolve_font_data(request)?;
        let key = FontInstanceKey::new(IdNamespace(0), self.next_font_key);
        self.next_font_key += 1;
        self.fonts.push(FontResource {
            key,
            data: face.data,
            index: face.index,
        });
        self.font_keys.insert(map_key, key);
        Some(key)
    }

    fn push_rect(&mut self, bounds: Rect, color: [f32; 4]) {
        if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
            return;
        }
        self.commands.push(PaintCmd::DrawRect(RectItem {
            placement: CommonPlacement::new(layout_rect(bounds)),
            color: colorf(color),
        }));
    }
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

fn layout_rect(r: Rect) -> LayoutRect {
    LayoutRect::new(
        LayoutPoint::new(r.origin.x, r.origin.y),
        LayoutPoint::new(r.max_x(), r.max_y()),
    )
}

fn colorf(c: [f32; 4]) -> ColorF {
    ColorF::new(c[0], c[1], c[2], c[3])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::FontFaceData;
    use crate::layout::layout_document;
    use crate::style::StyleConfig;
    use crate::types::Viewport;
    use crate::NoFontResolver;
    use inker::{
        DocumentBlock, DocumentProvenance, DocumentTrustState, EngineDocument, InlineSpan,
    };

    fn doc(blocks: Vec<DocumentBlock>) -> EngineDocument {
        EngineDocument {
            address: "doc:paint-list-test".into(),
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

    /// Resolver that hands back a dummy face for any request — used to
    /// test that real `DrawText` commands + a font side-table get emitted.
    struct AlwaysResolves;
    impl FontResolver for AlwaysResolves {
        fn resolve_font_data(&self, _request: FontRequest<'_>) -> Option<FontFaceData> {
            Some(FontFaceData {
                data: vec![0u8; 4],
                index: 0,
            })
        }
    }

    /// Resolver that hands back a face only for monospace requests.
    struct MonospaceOnlyResolver;
    impl FontResolver for MonospaceOnlyResolver {
        fn resolve_font_data(&self, request: FontRequest<'_>) -> Option<FontFaceData> {
            if request.family.eq_ignore_ascii_case("monospace") {
                Some(FontFaceData {
                    data: vec![0u8; 4],
                    index: 0,
                })
            } else {
                None
            }
        }
    }

    fn count<F: Fn(&PaintCmd) -> bool>(list: &InkerPaintList, pred: F) -> usize {
        list.commands().iter().filter(|c| pred(c)).count()
    }

    #[test]
    fn empty_packet_produces_no_commands() {
        let list = paint_list_from_packet(&packet_for(vec![]), &NoFontResolver, &ColorVocabulary::default());
        assert_eq!(list.engine_id(), EngineId::INKER);
        assert!(list.commands().is_empty());
        assert!(list.fonts().is_empty());
    }

    #[test]
    fn no_resolver_falls_back_to_placeholder_rect() {
        let list = paint_list_from_packet(
            &packet_for(vec![DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Text("hello".into())],
            }]),
            &NoFontResolver,
            &ColorVocabulary::default(),
        );
        // One paragraph → one glyph run → one placeholder DrawRect (no
        // DrawText, no fonts, because the resolver returned None).
        assert_eq!(count(&list, |c| matches!(c, PaintCmd::DrawRect(_))), 1);
        assert_eq!(count(&list, |c| matches!(c, PaintCmd::DrawText(_))), 0);
        assert!(list.fonts().is_empty());
    }

    #[test]
    fn resolver_emits_drawtext_and_populates_font_side_table() {
        let list = paint_list_from_packet(
            &packet_for(vec![DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Text("hello".into())],
            }]),
            &AlwaysResolves,
            &ColorVocabulary::default(),
        );
        assert!(count(&list, |c| matches!(c, PaintCmd::DrawText(_))) >= 1);
        // The referenced face is carried in the side-table exactly once.
        assert_eq!(list.fonts().len(), 1);
    }

    #[test]
    fn rule_emits_line_command_in_rule_color() {
        let mut colors = ColorVocabulary::default();
        colors.rule = [1.0, 0.0, 0.0, 1.0];
        let list = paint_list_from_packet(&packet_for(vec![DocumentBlock::Rule]), &NoFontResolver, &colors);
        let line = list
            .commands()
            .iter()
            .find_map(|c| match c {
                PaintCmd::DrawLine(l) => Some(l),
                _ => None,
            })
            .expect("expected a DrawLine for the rule");
        assert_eq!(line.color, colorf([1.0, 0.0, 0.0, 1.0]));
    }

    #[test]
    fn group_recurses_into_children() {
        let list = paint_list_from_packet(
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
        // Each list item is one paragraph → one placeholder rect each.
        assert_eq!(count(&list, |c| matches!(c, PaintCmd::DrawRect(_))), 2);
    }

    #[test]
    fn monospace_only_resolver_dispatches_per_run() {
        // CodeBlock → monospace family → face resolves → DrawText.
        // Paragraph → body family → no face → placeholder DrawRect.
        let list = paint_list_from_packet(
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
        assert!(count(&list, |c| matches!(c, PaintCmd::DrawText(_))) >= 1);
        assert!(count(&list, |c| matches!(c, PaintCmd::DrawRect(_))) >= 1);
        // Exactly one unique face interned (the monospace one).
        assert_eq!(list.fonts().len(), 1);
    }

    #[test]
    fn repeated_runs_share_one_font_side_table_entry() {
        // Two paragraphs in the same body family → the face is interned
        // once, both runs reference the same FontInstanceKey.
        let list = paint_list_from_packet(
            &packet_for(vec![
                DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text("first".into())],
                },
                DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text("second".into())],
                },
            ]),
            &AlwaysResolves,
            &ColorVocabulary::default(),
        );
        assert_eq!(list.fonts().len(), 1, "shared face should intern once");
        let keys: Vec<_> = list
            .commands()
            .iter()
            .filter_map(|c| match c {
                PaintCmd::DrawText(t) => Some(t.font_instance),
                _ => None,
            })
            .collect();
        assert!(keys.len() >= 2);
        assert!(keys.iter().all(|k| *k == keys[0]), "all runs share one key");
    }

    #[test]
    fn viewport_rounds_to_device_int_size() {
        let packet = layout_document(&doc(vec![]), Viewport::new(640.4, 480.6), &StyleConfig::default());
        let list = paint_list_from_packet(&packet, &NoFontResolver, &ColorVocabulary::default());
        assert_eq!(list.viewport().width, 640);
        assert_eq!(list.viewport().height, 481);
    }
}
