// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Engine tile — the first *live* workbench tile, proving the inker
//! engine-routing seam end to end.
//!
//! Per the composition spine
//! (`design_docs/mere_docs/technical_architecture/2026-05-21_mere_composition_spine.md`,
//! §14 v1 target: "tree-projected workbench with one live engine-backed
//! tile"). This module is that tile for the **document lane**: it routes an
//! address through inker's [`EngineRoutePolicy`], dispatches the matched
//! [`nematic`] engine over an [`EngineRegistry`], and turns the resulting
//! portable [`EngineDocument`] into Xilem/masonry views.
//!
//! Two halves live here for v1:
//!
//! - **routing + dispatch** (host-neutral) — kept inline because the host is
//!   currently the only consumer; it can hoist into a portable helper if a
//!   second consumer appears (single-consumer doctrine — no premature seam).
//! - **block → view** (Xilem-coupled) — stays here permanently: the mapping
//!   from [`DocumentBlock`] to masonry views is inherently host-layer and
//!   cannot live in framework-free `inker` / `nematic`.

use inker::{
    DocumentBlock, DocumentProvenance, DocumentTrustState, EngineDocument, EngineInput,
    EngineRegistry, EngineRoutePolicy, EngineRouteRequest, InlineSpan, WorkspaceRouteId,
    inline_text,
};
use xilem::view::{flex_col, label, prose};
use xilem::{AnyWidgetView, WidgetView};

/// A document rendered by an engine, tagged with the engine that produced it
/// — so the tile can show *which* engine handled the address (the visible
/// proof that routing actually happened, not a hardcoded render).
pub struct RenderedTile {
    /// The engine the router selected (e.g. `"nematic.markdown"`).
    pub engine_id: String,
    /// The portable document the engine produced.
    pub document: EngineDocument,
}

/// Route `address` (carrying `content_type`) to an engine and render `body`
/// through it.
///
/// Always returns a [`RenderedTile`]: on a routing or parse failure the
/// document is a synthesized error page, so a tile *shows* the failure
/// rather than the host crashing — browser-tile behavior, and it keeps the
/// view layer uniform (always render an [`EngineDocument`]).
pub fn render_address(address: &str, body: &str, content_type: Option<&str>) -> RenderedTile {
    // Register the host's available engines first, so routing can be filtered
    // to engines that actually exist here.
    let mut registry = EngineRegistry::new();
    for engine in nematic::engines() {
        registry.register(engine);
    }

    let request = EngineRouteRequest {
        workspace_id: WorkspaceRouteId::new("mere"),
        view: None,
        node: None,
        address: address.to_string(),
        content_type: content_type.map(str::to_string),
        pinned_engine: None,
    };
    let decision =
        EngineRoutePolicy::default().route_filtered(&request, |id| registry.contains(id));

    let mut input = EngineInput::new(address, body);
    if let Some(ct) = content_type {
        input = input.with_content_type(ct);
    }

    match registry.dispatch(&decision, &input) {
        Ok(document) => RenderedTile {
            engine_id: decision.engine_id,
            document,
        },
        Err(err) => RenderedTile {
            document: error_document(address, &decision.engine_id, &err.to_string()),
            engine_id: decision.engine_id,
        },
    }
}

/// Render an [`EngineDocument`]'s blocks as masonry views.
///
/// Generic over the host state `S` because the document tile is display-only
/// for v1 (no callbacks mutate state); the leaf views (`label`, `prose`) are
/// state-agnostic. Returns one view per top-level block, in document order.
pub fn document_views<S: 'static>(doc: &EngineDocument) -> Vec<Box<AnyWidgetView<S>>> {
    doc.blocks.iter().map(block_view).collect()
}

/// Map one [`DocumentBlock`] to a masonry view. Total over the block enum so
/// `document_views` never silently drops content.
fn block_view<S: 'static>(block: &DocumentBlock) -> Box<AnyWidgetView<S>> {
    match block {
        DocumentBlock::Heading { level, spans } => {
            label(inline_text(spans)).text_size(heading_size(*level)).boxed()
        }
        DocumentBlock::Paragraph { spans } => prose(inline_text(spans)).boxed(),
        DocumentBlock::CodeBlock { text, .. } | DocumentBlock::Preformatted { text } => {
            prose(text.clone()).boxed()
        }
        DocumentBlock::Quote { blocks } => {
            let inner: Vec<Box<AnyWidgetView<S>>> = blocks.iter().map(block_view).collect();
            flex_col(inner).boxed()
        }
        DocumentBlock::List { ordered, items } => {
            let lines: Vec<Box<AnyWidgetView<S>>> = items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let marker = if *ordered {
                        format!("{}. ", i + 1)
                    } else {
                        "• ".to_string()
                    };
                    prose(format!("{marker}{}", blocks_to_text(item))).boxed()
                })
                .collect();
            flex_col(lines).boxed()
        }
        DocumentBlock::Image { url, alt } => prose(format!("🖼 {alt} ({url})")).boxed(),
        DocumentBlock::Rule => prose("─".repeat(24)).boxed(),
        DocumentBlock::MetadataRow { label: name, value } => {
            prose(format!("{name}: {value}")).boxed()
        }
        DocumentBlock::Badge { text } => label(format!("[{text}]")).text_size(12.0).boxed(),
        DocumentBlock::FeedHeader { title, subtitle, .. } => label(match subtitle {
            Some(sub) => format!("{title} — {sub}"),
            None => title.clone(),
        })
        .text_size(18.0)
        .boxed(),
        DocumentBlock::FeedEntry { title, date, .. } => prose(match date {
            Some(d) => format!("• {title} ({d})"),
            None => format!("• {title}"),
        })
        .boxed(),
    }
}

/// Font size for a heading level (clamped 1–6).
fn heading_size(level: u8) -> f32 {
    match level {
        1 => 24.0,
        2 => 20.0,
        3 => 17.0,
        4 => 15.0,
        _ => 14.0,
    }
}

/// Flatten a list item's blocks into one line of plain text (the inline text
/// of each contained block, space-joined). Loses nested structure — fine for
/// the v1 list rendering, which is single-line bullets.
fn blocks_to_text(blocks: &[DocumentBlock]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for block in blocks {
        match block {
            DocumentBlock::Heading { spans, .. } | DocumentBlock::Paragraph { spans } => {
                parts.push(inline_text(spans));
            }
            DocumentBlock::CodeBlock { text, .. } | DocumentBlock::Preformatted { text } => {
                parts.push(text.clone());
            }
            DocumentBlock::List { items, .. } => {
                for item in items {
                    parts.push(blocks_to_text(item));
                }
            }
            _ => {}
        }
    }
    parts.join(" ")
}

/// Synthesize an error "page" so a routing / parse failure shows in the tile.
pub(crate) fn error_document(address: &str, engine_id: &str, message: &str) -> EngineDocument {
    EngineDocument {
        address: address.to_string(),
        title: Some("Engine error".to_string()),
        content_type: "text/plain".to_string(),
        lang: None,
        provenance: DocumentProvenance::for_engine(engine_id, address),
        trust: DocumentTrustState::Unknown,
        diagnostics: Vec::new(),
        blocks: vec![
            DocumentBlock::Badge {
                text: "engine error".to_string(),
            },
            DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Text(message.to_string())],
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MD: &str = "# Title\n\nA **bold** paragraph.\n\n- one\n- two\n";

    #[test]
    fn markdown_content_type_routes_to_markdown_engine() {
        let tile = render_address("mere://welcome", MD, Some("text/markdown"));
        assert_eq!(tile.engine_id, "nematic.markdown");
        assert!(!tile.document.blocks.is_empty());
        assert!(matches!(
            tile.document.blocks.first(),
            Some(DocumentBlock::Heading { level: 1, .. })
        ));
    }

    #[test]
    fn document_views_yields_one_view_per_block() {
        let tile = render_address("mere://welcome", MD, Some("text/markdown"));
        let n = tile.document.blocks.len();
        // `()` proves the renderer is genuinely state-agnostic.
        let views = document_views::<()>(&tile.document);
        assert_eq!(views.len(), n);
    }

    #[test]
    fn parse_failure_falls_back_to_error_document() {
        // The feed engine rejects truncated XML (an unclosed element on EOF)
        // with InvalidContent; the tile should surface that as an error page,
        // not panic.
        let tile = render_address("feed://bad", "<rss><channel>", Some("application/rss+xml"));
        assert_eq!(tile.engine_id, "nematic.feed");
        assert_eq!(tile.document.title.as_deref(), Some("Engine error"));
        assert!(matches!(
            tile.document.blocks.first(),
            Some(DocumentBlock::Badge { .. })
        ));
    }
}
