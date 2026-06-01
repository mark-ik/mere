/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Top-level document layout — dispatches per [`DocumentBlock`] variant
//! and stacks the resulting blocks vertically inside the viewport.
//!
//! v1 is a simple top-down stack with no float / no inline-image flow /
//! no scrolling. Width fills the available content width; height grows
//! to fit content (may exceed `viewport.height`).

use inker::{DocumentBlock, EngineDocument, InlineSpan};

use crate::font_table::{FontInterner, FontTable};
use crate::style::StyleConfig;
use crate::text::{
    Flattened, LaidOutText, LayoutEnvironment, TextBaseStyle, flatten_inline, layout_text_block,
};
use crate::types::{
    DocumentRenderPacket, InteractionRegion, Point, Rect, RenderedBlock, RenderedBlockKind, Size,
    Viewport,
};

/// A laid-out document: the serializable [`DocumentRenderPacket`] plus the
/// out-of-band [`FontTable`] sidecar carrying the `parley::FontData` each
/// `GlyphRun` was shaped against. The packet alone is the portable,
/// serializable shape; `fonts` holds `Arc`-cheap face handles whose owned
/// bytes materialize only at the `paint_list_api` boundary (the
/// `PaintList`'s font side-table). See [`crate::font_table`].
#[derive(Clone, Debug)]
pub struct LaidOutDocument {
    pub packet: DocumentRenderPacket,
    pub fonts: FontTable,
}

/// Layout entry point. Consumes an `EngineDocument` and a viewport,
/// produces a [`LaidOutDocument`]: a portable [`DocumentRenderPacket`]
/// downstream renderers paint, plus the font sidecar that resolves each
/// run's [`FontFaceId`](crate::FontFaceId) to the real face bytes parley
/// shaped against.
pub fn layout_document(
    document: &EngineDocument,
    viewport: Viewport,
    style: &StyleConfig,
) -> LaidOutDocument {
    let mut env = LayoutEnvironment::new();
    let mut layouter = DocumentLayouter::new(viewport, style, &mut env);

    for (idx, block) in document.blocks.iter().enumerate() {
        layouter.lay_out_block(block, idx, 0);
    }

    layouter.finish()
}

struct DocumentLayouter<'a> {
    viewport: Viewport,
    style: &'a StyleConfig,
    env: &'a mut LayoutEnvironment,
    cursor_y: f32,
    blocks: Vec<RenderedBlock>,
    interactions: Vec<InteractionRegion>,
    max_x: f32,
    /// Interns parley's chosen face per run; sealed into the
    /// [`LaidOutDocument`]'s [`FontTable`] sidecar at `finish`.
    fonts: FontInterner,
}

impl<'a> DocumentLayouter<'a> {
    fn new(viewport: Viewport, style: &'a StyleConfig, env: &'a mut LayoutEnvironment) -> Self {
        Self {
            viewport,
            style,
            env,
            cursor_y: style.vertical_padding,
            blocks: Vec::new(),
            interactions: Vec::new(),
            max_x: 0.0,
            fonts: FontInterner::new(),
        }
    }

    fn content_left(&self, indent_level: u32) -> f32 {
        self.style.horizontal_padding + (indent_level as f32) * self.style.indent_per_level
    }

    fn available_width(&self, indent_level: u32) -> f32 {
        let used = self.content_left(indent_level) + self.style.horizontal_padding;
        (self.viewport.width - used).max(0.0)
    }

    fn lay_out_block(&mut self, block: &DocumentBlock, source_index: usize, indent_level: u32) {
        let rendered = self.render_block(block, source_index, indent_level);
        if let Some(rendered) = rendered {
            self.cursor_y = rendered.bounds.max_y();
            self.max_x = self.max_x.max(rendered.bounds.max_x());
            self.blocks.push(rendered);
        }
    }

    fn render_block(
        &mut self,
        block: &DocumentBlock,
        source_index: usize,
        indent_level: u32,
    ) -> Option<RenderedBlock> {
        match block {
            DocumentBlock::Heading { level, spans } => {
                Some(self.render_heading(source_index, indent_level, *level, spans))
            }
            DocumentBlock::Paragraph { spans } => Some(self.render_paragraph(
                source_index,
                indent_level,
                spans,
                TextBaseStyle {
                    font_size: self.style.body_font_size,
                    font_family: self.style.body_font_family.clone(),
                    bold: false,
                    italic: false,
                    monospace: false,
                    line_height_ratio: self.style.line_height_ratio,
                },
                self.style.paragraph_spacing,
            )),
            DocumentBlock::CodeBlock { text, .. } => {
                Some(self.render_code_block(source_index, indent_level, text))
            }
            DocumentBlock::Preformatted { text } => {
                Some(self.render_code_block(source_index, indent_level, text))
            }
            DocumentBlock::Quote { blocks } => {
                Some(self.render_group(source_index, indent_level + 1, blocks))
            }
            DocumentBlock::List { items, .. } => {
                Some(self.render_list(source_index, indent_level + 1, items))
            }
            DocumentBlock::Image { url, alt } => {
                Some(self.render_image(source_index, indent_level, url.clone(), alt.clone()))
            }
            DocumentBlock::Rule => Some(self.render_rule(source_index, indent_level)),
            DocumentBlock::FeedHeader {
                title,
                subtitle,
                summary,
                source_url,
            } => Some(self.render_feed_header(
                source_index,
                indent_level,
                title,
                subtitle.as_deref(),
                summary.as_deref(),
                source_url.as_deref(),
            )),
            DocumentBlock::FeedEntry {
                title,
                date,
                summary,
                article_url,
                source_url,
            } => Some(self.render_feed_entry(
                source_index,
                indent_level,
                title,
                date.as_deref(),
                summary.as_deref(),
                article_url.as_deref(),
                source_url.as_deref(),
            )),
            DocumentBlock::MetadataRow { label, value } => {
                Some(self.render_metadata_row(source_index, indent_level, label, value))
            }
            DocumentBlock::Badge { text } => {
                Some(self.render_badge(source_index, indent_level, text))
            }
        }
    }

    // -------------------------------------------------------------------
    // Block renderers
    // -------------------------------------------------------------------

    fn render_heading(
        &mut self,
        source_index: usize,
        indent_level: u32,
        level: u8,
        spans: &[InlineSpan],
    ) -> RenderedBlock {
        let font_size = self.style.heading_size(level);
        let above = self.style.heading_spacing_above;
        let below = self.style.heading_spacing_below;
        let base = TextBaseStyle {
            font_size,
            font_family: self.style.body_font_family.clone(),
            bold: true,
            italic: false,
            monospace: false,
            line_height_ratio: self.style.line_height_ratio,
        };
        self.render_text_block_with_spacing(source_index, indent_level, spans, base, above, below)
    }

    fn render_paragraph(
        &mut self,
        source_index: usize,
        indent_level: u32,
        spans: &[InlineSpan],
        base: TextBaseStyle,
        spacing_below: f32,
    ) -> RenderedBlock {
        self.render_text_block_with_spacing(
            source_index,
            indent_level,
            spans,
            base,
            0.0,
            spacing_below,
        )
    }

    fn render_text_block_with_spacing(
        &mut self,
        source_index: usize,
        indent_level: u32,
        spans: &[InlineSpan],
        base: TextBaseStyle,
        spacing_above: f32,
        spacing_below: f32,
    ) -> RenderedBlock {
        let flattened = flatten_inline(spans);
        self.render_flattened_with_spacing(
            source_index,
            indent_level,
            &flattened,
            base,
            spacing_above,
            spacing_below,
        )
    }

    fn render_flattened_with_spacing(
        &mut self,
        source_index: usize,
        indent_level: u32,
        flattened: &Flattened,
        base: TextBaseStyle,
        spacing_above: f32,
        spacing_below: f32,
    ) -> RenderedBlock {
        let origin = Point::new(
            self.content_left(indent_level),
            self.cursor_y + spacing_above,
        );
        let available = self.available_width(indent_level);
        let LaidOutText {
            glyph_runs,
            total_size,
            mut interactions,
        } = layout_text_block(self.env, flattened, &base, available, origin, &mut self.fonts);

        self.interactions.append(&mut interactions);

        let bounds = Rect::new(
            origin,
            Size::new(total_size.width, total_size.height + spacing_below),
        );

        RenderedBlock {
            source_block_index: source_index,
            bounds,
            kind: RenderedBlockKind::Text { glyph_runs },
        }
    }

    fn render_code_block(
        &mut self,
        source_index: usize,
        indent_level: u32,
        text: &str,
    ) -> RenderedBlock {
        let base = TextBaseStyle {
            font_size: self.style.body_font_size,
            font_family: self.style.mono_font_family.clone(),
            bold: false,
            italic: false,
            monospace: true,
            line_height_ratio: self.style.line_height_ratio,
        };
        let spans = vec![InlineSpan::Text(text.to_string())];
        self.render_text_block_with_spacing(
            source_index,
            indent_level,
            &spans,
            base,
            0.0,
            self.style.paragraph_spacing,
        )
    }

    fn render_group(
        &mut self,
        source_index: usize,
        indent_level: u32,
        children: &[DocumentBlock],
    ) -> RenderedBlock {
        let group_top = self.cursor_y;
        let mut child_blocks: Vec<RenderedBlock> = Vec::new();
        for (i, child) in children.iter().enumerate() {
            // Children carry their own source indices in the parent's
            // coordinate; we project the parent's index into a synthetic
            // sub-index space (parent_index * 1000 + child_index). Crude
            // but stable enough for v1 hit-back-to-source mapping.
            let synthetic = source_index.saturating_mul(1000) + i;
            if let Some(rendered) = self.render_block(child, synthetic, indent_level) {
                self.cursor_y = rendered.bounds.max_y();
                self.max_x = self.max_x.max(rendered.bounds.max_x());
                child_blocks.push(rendered);
            }
        }
        let group_bottom = self.cursor_y;
        let group_left = self.content_left(indent_level);
        let group_right = self.max_x;
        RenderedBlock {
            source_block_index: source_index,
            bounds: Rect::from_xywh(
                group_left,
                group_top,
                (group_right - group_left).max(0.0),
                group_bottom - group_top,
            ),
            kind: RenderedBlockKind::Group {
                children: child_blocks,
            },
        }
    }

    fn render_list(
        &mut self,
        source_index: usize,
        indent_level: u32,
        items: &[Vec<DocumentBlock>],
    ) -> RenderedBlock {
        let group_top = self.cursor_y;
        let mut child_blocks: Vec<RenderedBlock> = Vec::new();
        for (i, item) in items.iter().enumerate() {
            for (j, child) in item.iter().enumerate() {
                let synthetic = source_index.saturating_mul(1000) + i.saturating_mul(100) + j;
                if let Some(rendered) = self.render_block(child, synthetic, indent_level) {
                    self.cursor_y = rendered.bounds.max_y();
                    self.max_x = self.max_x.max(rendered.bounds.max_x());
                    child_blocks.push(rendered);
                }
            }
        }
        let group_bottom = self.cursor_y;
        let group_left = self.content_left(indent_level);
        let group_right = self.max_x;
        RenderedBlock {
            source_block_index: source_index,
            bounds: Rect::from_xywh(
                group_left,
                group_top,
                (group_right - group_left).max(0.0),
                group_bottom - group_top,
            ),
            kind: RenderedBlockKind::Group {
                children: child_blocks,
            },
        }
    }

    fn render_image(
        &mut self,
        source_index: usize,
        indent_level: u32,
        url: String,
        alt: String,
    ) -> RenderedBlock {
        // v1 reserves a placeholder strip the height of one line of body
        // text. Renderer fetches + paints the actual image; document-canvas
        // doesn't load bytes.
        let line_height = self.style.line_height(self.style.body_font_size);
        let height = line_height * 6.0; // ~6 lines worth of placeholder
        let origin = Point::new(self.content_left(indent_level), self.cursor_y);
        let bounds = Rect::new(
            origin,
            Size::new(
                self.available_width(indent_level),
                height + self.style.paragraph_spacing,
            ),
        );
        RenderedBlock {
            source_block_index: source_index,
            bounds,
            kind: RenderedBlockKind::Image { url, alt },
        }
    }

    fn render_rule(&mut self, source_index: usize, indent_level: u32) -> RenderedBlock {
        let origin = Point::new(self.content_left(indent_level), self.cursor_y);
        let bounds = Rect::new(
            origin,
            Size::new(
                self.available_width(indent_level),
                self.style.paragraph_spacing,
            ),
        );
        RenderedBlock {
            source_block_index: source_index,
            bounds,
            kind: RenderedBlockKind::Rule,
        }
    }

    fn render_feed_header(
        &mut self,
        source_index: usize,
        indent_level: u32,
        title: &str,
        subtitle: Option<&str>,
        summary: Option<&str>,
        source_url: Option<&str>,
    ) -> RenderedBlock {
        let mut composed: Vec<DocumentBlock> = Vec::new();
        composed.push(DocumentBlock::Heading {
            level: 1,
            spans: vec![InlineSpan::Text(title.to_string())],
        });
        if let Some(s) = subtitle {
            composed.push(DocumentBlock::Heading {
                level: 2,
                spans: vec![InlineSpan::Text(s.to_string())],
            });
        }
        if let Some(s) = summary {
            composed.push(DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Text(s.to_string())],
            });
        }
        if let Some(url) = source_url {
            composed.push(DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Link {
                    url: url.to_string(),
                    title: None,
                    spans: vec![InlineSpan::Text("Open source".to_string())],
                    predicate: None,
                }],
            });
        }
        self.render_group(source_index, indent_level, &composed)
    }

    fn render_feed_entry(
        &mut self,
        source_index: usize,
        indent_level: u32,
        title: &str,
        date: Option<&str>,
        summary: Option<&str>,
        article_url: Option<&str>,
        source_url: Option<&str>,
    ) -> RenderedBlock {
        let mut composed: Vec<DocumentBlock> = Vec::new();
        composed.push(DocumentBlock::Heading {
            level: 2,
            spans: vec![InlineSpan::Text(title.to_string())],
        });
        if let Some(d) = date {
            composed.push(DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Emphasis(vec![InlineSpan::Text(d.to_string())])],
            });
        }
        if let Some(s) = summary {
            composed.push(DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Text(s.to_string())],
            });
        }
        if let Some(url) = article_url {
            composed.push(DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Link {
                    url: url.to_string(),
                    title: None,
                    spans: vec![InlineSpan::Text("Open article".to_string())],
                    predicate: None,
                }],
            });
        }
        if let Some(url) = source_url {
            composed.push(DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Link {
                    url: url.to_string(),
                    title: None,
                    spans: vec![InlineSpan::Text("Open source".to_string())],
                    predicate: None,
                }],
            });
        }
        self.render_group(source_index, indent_level, &composed)
    }

    fn render_metadata_row(
        &mut self,
        source_index: usize,
        indent_level: u32,
        label: &str,
        value: &str,
    ) -> RenderedBlock {
        // Label in bold + value in normal. Lay out as a single paragraph.
        let spans = vec![
            InlineSpan::Strong(vec![InlineSpan::Text(format!("{label}: "))]),
            InlineSpan::Text(value.to_string()),
        ];
        let base = TextBaseStyle {
            font_size: self.style.body_font_size,
            font_family: self.style.body_font_family.clone(),
            bold: false,
            italic: false,
            monospace: false,
            line_height_ratio: self.style.line_height_ratio,
        };
        self.render_text_block_with_spacing(
            source_index,
            indent_level,
            &spans,
            base,
            0.0,
            self.style.paragraph_spacing * 0.5,
        )
    }

    fn render_badge(
        &mut self,
        source_index: usize,
        indent_level: u32,
        text: &str,
    ) -> RenderedBlock {
        // Badge as a small italic paragraph; renderer paints the pill if it
        // wants. v1 doesn't carry pill-shape metadata.
        let spans = vec![InlineSpan::Emphasis(vec![InlineSpan::Text(
            text.to_string(),
        )])];
        let base = TextBaseStyle {
            font_size: self.style.body_font_size * 0.85,
            font_family: self.style.body_font_family.clone(),
            bold: false,
            italic: true,
            monospace: false,
            line_height_ratio: self.style.line_height_ratio,
        };
        self.render_text_block_with_spacing(
            source_index,
            indent_level,
            &spans,
            base,
            0.0,
            self.style.paragraph_spacing * 0.5,
        )
    }

    fn finish(self) -> LaidOutDocument {
        let total_height = self.cursor_y + self.style.vertical_padding;
        LaidOutDocument {
            packet: DocumentRenderPacket {
                viewport: self.viewport,
                content_bounds: Rect::from_xywh(0.0, 0.0, self.viewport.width, total_height),
                blocks: self.blocks,
                interactions: self.interactions,
            },
            fonts: self.fonts.into_table(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::InteractionKind;
    use inker::{
        DocumentBlock, DocumentProvenance, DocumentTrustState, EngineDocument, InlineSpan,
    };

    fn doc(blocks: Vec<DocumentBlock>) -> EngineDocument {
        EngineDocument {
            address: "doc:test".into(),
            title: None,
            content_type: "text/plain".into(),
            lang: None,
            provenance: DocumentProvenance::default(),
            trust: DocumentTrustState::Unknown,
            diagnostics: Vec::new(),
            blocks,
        }
    }

    fn viewport() -> Viewport {
        Viewport::new(640.0, 480.0)
    }

    #[test]
    fn empty_document_lays_out_to_empty_block_list() {
        let packet = layout_document(&doc(vec![]), viewport(), &StyleConfig::default()).packet;
        assert!(packet.blocks.is_empty());
        assert!(packet.interactions.is_empty());
        assert_eq!(packet.viewport.width, 640.0);
    }

    #[test]
    fn single_paragraph_produces_one_text_block() {
        let packet = layout_document(
            &doc(vec![DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Text("Hello, world.".into())],
            }]),
            viewport(),
            &StyleConfig::default(),
        )
        .packet;
        assert_eq!(packet.blocks.len(), 1);
        let block = &packet.blocks[0];
        assert_eq!(block.source_block_index, 0);
        let RenderedBlockKind::Text { glyph_runs } = &block.kind else {
            panic!("expected Text kind, got {:?}", block.kind);
        };
        assert!(!glyph_runs.is_empty(), "expected at least one glyph run");
    }

    #[test]
    fn heading_is_taller_than_paragraph() {
        let style = StyleConfig::default();
        let packet = layout_document(
            &doc(vec![
                DocumentBlock::Heading {
                    level: 1,
                    spans: vec![InlineSpan::Text("Title".into())],
                },
                DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text("Body.".into())],
                },
            ]),
            viewport(),
            &style,
        )
        .packet;
        assert_eq!(packet.blocks.len(), 2);
        let heading = &packet.blocks[0];
        let paragraph = &packet.blocks[1];
        assert!(
            heading.bounds.size.height > paragraph.bounds.size.height,
            "heading {:?} should be taller than paragraph {:?}",
            heading.bounds,
            paragraph.bounds
        );
    }

    #[test]
    fn paragraph_with_link_emits_interaction_region() {
        let packet = layout_document(
            &doc(vec![DocumentBlock::Paragraph {
                spans: vec![
                    InlineSpan::Text("see ".into()),
                    InlineSpan::Link {
                        url: "https://x.test/".into(),
                        title: None,
                        spans: vec![InlineSpan::Text("docs".into())],
                        predicate: None,
                    },
                    InlineSpan::Text(" please".into()),
                ],
            }]),
            viewport(),
            &StyleConfig::default(),
        )
        .packet;
        assert_eq!(packet.interactions.len(), 1);
        let region = &packet.interactions[0];
        match &region.kind {
            InteractionKind::Link { url } => assert_eq!(url, "https://x.test/"),
        }
        assert!(region.bounds.size.width > 0.0);
        assert!(region.bounds.size.height > 0.0);
    }

    #[test]
    fn list_emits_group_block_with_children() {
        let packet = layout_document(
            &doc(vec![DocumentBlock::List {
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
            viewport(),
            &StyleConfig::default(),
        )
        .packet;
        let RenderedBlockKind::Group { children } = &packet.blocks[0].kind else {
            panic!("expected Group kind");
        };
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn quote_emits_group_block_with_indented_children() {
        let style = StyleConfig::default();
        let packet = layout_document(
            &doc(vec![DocumentBlock::Quote {
                blocks: vec![DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text("quoted text".into())],
                }],
            }]),
            viewport(),
            &style,
        )
        .packet;
        let RenderedBlockKind::Group { children } = &packet.blocks[0].kind else {
            panic!("expected Group kind");
        };
        assert_eq!(children.len(), 1);
        // Indent should push the child's left edge inward.
        assert!(
            children[0].bounds.origin.x >= style.horizontal_padding + style.indent_per_level,
            "quote child should be indented; got x={}",
            children[0].bounds.origin.x
        );
    }

    #[test]
    fn rule_emits_rule_block() {
        let packet = layout_document(
            &doc(vec![DocumentBlock::Rule]),
            viewport(),
            &StyleConfig::default(),
        )
        .packet;
        assert!(matches!(packet.blocks[0].kind, RenderedBlockKind::Rule));
    }

    #[test]
    fn image_emits_image_block_with_url_and_alt() {
        let packet = layout_document(
            &doc(vec![DocumentBlock::Image {
                url: "https://x.test/pic.png".into(),
                alt: "a picture".into(),
            }]),
            viewport(),
            &StyleConfig::default(),
        )
        .packet;
        let RenderedBlockKind::Image { url, alt } = &packet.blocks[0].kind else {
            panic!("expected Image kind");
        };
        assert_eq!(url, "https://x.test/pic.png");
        assert_eq!(alt, "a picture");
    }

    #[test]
    fn feed_entry_composes_into_group_with_h2_summary_link() {
        let packet = layout_document(
            &doc(vec![DocumentBlock::FeedEntry {
                title: "Article".into(),
                date: Some("2026-05-09".into()),
                summary: Some("Summary text.".into()),
                article_url: Some("https://feed.test/x".into()),
                source_url: None,
            }]),
            viewport(),
            &StyleConfig::default(),
        )
        .packet;
        let RenderedBlockKind::Group { children } = &packet.blocks[0].kind else {
            panic!("expected Group kind");
        };
        // Heading + date + summary + article link = 4 children.
        assert_eq!(children.len(), 4);

        // Article URL surfaces as an interaction region.
        assert!(packet.interactions.iter().any(
            |r| matches!(&r.kind, InteractionKind::Link { url } if url == "https://feed.test/x")
        ));
    }

    #[test]
    fn metadata_row_lays_out_label_and_value() {
        let packet = layout_document(
            &doc(vec![DocumentBlock::MetadataRow {
                label: "Login".into(),
                value: "alice".into(),
            }]),
            viewport(),
            &StyleConfig::default(),
        )
        .packet;
        assert_eq!(packet.blocks.len(), 1);
        let RenderedBlockKind::Text { glyph_runs } = &packet.blocks[0].kind else {
            panic!("expected Text kind");
        };
        assert!(!glyph_runs.is_empty());
    }

    #[test]
    fn content_bounds_grow_with_blocks() {
        let style = StyleConfig::default();
        let single = layout_document(
            &doc(vec![DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Text("one".into())],
            }]),
            viewport(),
            &style,
        )
        .packet;
        let several = layout_document(
            &doc(vec![
                DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text("one".into())],
                },
                DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text("two".into())],
                },
                DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text("three".into())],
                },
            ]),
            viewport(),
            &style,
        )
        .packet;
        assert!(several.content_bounds.size.height > single.content_bounds.size.height);
    }

    #[test]
    fn document_dedups_shared_face() {
        // Two body paragraphs shape against the same family/weight, so
        // parley returns the same face for both runs → one entry in the
        // sidecar, and both runs carry the same FontFaceId.
        let laid = layout_document(
            &doc(vec![
                DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text("first".into())],
                },
                DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text("second".into())],
                },
            ]),
            viewport(),
            &StyleConfig::default(),
        );
        assert_eq!(laid.fonts.len(), 1, "shared body face should intern once");
        let faces: Vec<_> = laid
            .packet
            .blocks
            .iter()
            .filter_map(|b| match &b.kind {
                RenderedBlockKind::Text { glyph_runs } => glyph_runs.first(),
                _ => None,
            })
            .map(|r| r.font_face)
            .collect();
        assert!(faces.len() >= 2, "expected a run per paragraph");
        assert!(faces.iter().all(|f| *f == faces[0]), "runs share one face id");
    }

    #[test]
    fn text_populates_font_sidecar() {
        // Mirrors serval's `emit_with_layouts_populates_font_table`: real
        // text yields a non-empty sidecar, every run's face resolves in
        // it, and the resolved face carries real bytes.
        let laid = layout_document(
            &doc(vec![DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Text("hello".into())],
            }]),
            viewport(),
            &StyleConfig::default(),
        );
        assert!(!laid.fonts.is_empty(), "text should populate the font sidecar");
        for block in &laid.packet.blocks {
            if let RenderedBlockKind::Text { glyph_runs } = &block.kind {
                for run in glyph_runs {
                    let face = laid
                        .fonts
                        .get(run.font_face)
                        .expect("run face resolves in sidecar");
                    assert!(!face.data.data().is_empty(), "face carries real bytes");
                }
            }
        }
    }
}
