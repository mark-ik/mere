/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The content card: the focused node's media, rendered as a floating card over
//! the orrery.
//!
//! S2.2a (synchronous): the card content is *synthesized* from the node's URL —
//! a built-in welcome page for `mere://welcome`, else a placeholder naming the
//! address. It runs the real document pipeline (inker [`EngineDocument`] →
//! [`document_canvas::layout_document`] → `scene_from_packet` →
//! `netrender::Scene`), so only the byte source is a stub. S2.2b swaps
//! [`node_document`] for a netfetcher fetch + inker engine dispatch keyed on the
//! response content-type; the layout + scene + composite path here stays.

use document_canvas::netrender_backend::scene_from_packet;
use document_canvas::{layout_document, ColorVocabulary, StyleConfig, Viewport};
use inker::{DocumentBlock, DocumentProvenance, DocumentTrustState, EngineDocument, InlineSpan};
use netrender::Scene;

use crate::fetch::{ContentState, Fetched};

/// Margin (px) between the card and the content-band edges.
const CARD_MARGIN: f32 = 24.0;

/// The document for the focused node's card, given its fetch [`ContentState`].
/// `mere://welcome` is the built-in welcome page; a fetchable URL renders its
/// loading / fetched-text / error state; anything else (an unfetched `mere://`
/// address) just names the node.
pub fn content_document(url: &str, state: Option<&ContentState>) -> EngineDocument {
    let blocks = if url == "mere://welcome" {
        welcome_blocks()
    } else {
        match state {
            Some(ContentState::Loading) => vec![heading(1, url), paragraph("Fetching…")],
            Some(ContentState::Ready(fetched)) => ready_blocks(url, fetched),
            Some(ContentState::Failed(reason)) => {
                vec![heading(1, url), paragraph(&format!("Could not load: {reason}"))]
            },
            None => vec![heading(1, url), paragraph("This node has no fetched media yet.")],
        }
    };
    document(url, blocks)
}

/// The built-in `mere://welcome` page.
fn welcome_blocks() -> Vec<DocumentBlock> {
    vec![
        heading(1, "Mere"),
        paragraph("A graph-shaped browser, hosted on serval."),
        paragraph(
            "Type a URL or a search above and press Enter. Each place you visit \
             becomes a node in the graph behind this card; Back and Forward move \
             through it.",
        ),
    ]
}

/// Fetched content as a plain document (S2.2b-i): the address, its content-type,
/// then the decoded body split into paragraphs on blank lines. Bounded so a large
/// page can't make an unbounded card. Content-type-aware engines (markdown, HTML
/// via serval, …) replace this plain split in S2.2b-ii.
fn ready_blocks(url: &str, fetched: &Fetched) -> Vec<DocumentBlock> {
    let mut blocks = vec![heading(1, url)];
    if let Some(ct) = &fetched.content_type {
        blocks.push(paragraph(&format!("({ct})")));
    }
    let text: String = fetched.body.chars().take(4000).collect();
    let mut paras = 0;
    for para in text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        blocks.push(paragraph(para));
        paras += 1;
        if paras >= 40 {
            break;
        }
    }
    if paras == 0 {
        blocks.push(paragraph("(empty response)"));
    }
    blocks
}

/// Assemble an [`EngineDocument`] over `blocks`, addressed at `url`.
fn document(url: &str, blocks: Vec<DocumentBlock>) -> EngineDocument {
    EngineDocument {
        address: url.to_string(),
        title: None,
        content_type: "text/plain".to_string(),
        lang: None,
        provenance: DocumentProvenance::default(),
        trust: DocumentTrustState::Unknown,
        diagnostics: Vec::new(),
        blocks,
    }
}

/// Lay out `doc` at `(w, h)` and lower it to a `netrender::Scene` — the proven
/// document pipeline (parley layout + the shared paint-list translator). The
/// caller composites the scene at the card rect with an opaque card background.
pub fn render_card_scene(doc: &EngineDocument, w: u32, h: u32) -> Scene {
    let laid = layout_document(doc, Viewport::new(w as f32, h as f32), &StyleConfig::default());
    scene_from_packet(&laid.packet, &laid.fonts, &ColorVocabulary::default())
}

/// The floating card rectangle within the content band (top-right, inset by
/// [`CARD_MARGIN`]). Returns `(x0, y0, x1, y1, w, h)` — window-space corners for
/// the composite plus the pixel size to rasterize at — or `None` when the band
/// is too small to host a readable card.
pub fn card_rect(w: u32, toolbar_h: u32, h: u32) -> Option<(f32, f32, f32, f32, u32, u32)> {
    let top = toolbar_h as f32 + CARD_MARGIN;
    let avail_w = w as f32 - 2.0 * CARD_MARGIN;
    let avail_h = h as f32 - top - CARD_MARGIN;
    if avail_w < 160.0 || avail_h < 100.0 {
        return None;
    }
    let cw = (w as f32 * 0.42).clamp(280.0, 460.0).min(avail_w);
    let ch = avail_h.min(560.0);
    let x1 = w as f32 - CARD_MARGIN;
    let x0 = x1 - cw;
    let y0 = top;
    let y1 = y0 + ch;
    Some((x0, y0, x1, y1, cw.round().max(1.0) as u32, ch.round().max(1.0) as u32))
}

fn heading(level: u8, text: &str) -> DocumentBlock {
    DocumentBlock::Heading { level, spans: vec![InlineSpan::Text(text.to_string())] }
}

fn paragraph(text: &str) -> DocumentBlock {
    DocumentBlock::Paragraph { spans: vec![InlineSpan::Text(text.to_string())] }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heads_with(doc: &EngineDocument, text: &str) -> bool {
        matches!(
            doc.blocks.first(),
            Some(DocumentBlock::Heading { spans, .. })
                if matches!(spans.first(), Some(InlineSpan::Text(t)) if t == text)
        )
    }

    fn body_text(doc: &EngineDocument) -> String {
        doc.blocks
            .iter()
            .filter_map(|b| match b {
                DocumentBlock::Paragraph { spans } | DocumentBlock::Heading { spans, .. } => {
                    Some(spans.iter().filter_map(|s| match s {
                        InlineSpan::Text(t) => Some(t.as_str()),
                        _ => None,
                    }))
                },
                _ => None,
            })
            .flatten()
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn welcome_document_leads_with_a_heading() {
        let doc = content_document("mere://welcome", None);
        assert_eq!(doc.address, "mere://welcome");
        assert!(matches!(doc.blocks.first(), Some(DocumentBlock::Heading { level: 1, .. })));
    }

    #[test]
    fn fetch_states_render_distinctly() {
        let url = "https://example.com";
        assert!(heads_with(&content_document(url, Some(&ContentState::Loading)), url));
        assert!(body_text(&content_document(url, Some(&ContentState::Loading))).contains("Fetching"));

        let ready = ContentState::Ready(Fetched {
            content_type: Some("text/plain".into()),
            body: "First paragraph.\n\nSecond paragraph.".into(),
        });
        let doc = content_document(url, Some(&ready));
        let text = body_text(&doc);
        assert!(text.contains("First paragraph."), "fetched body renders");
        assert!(text.contains("Second paragraph."), "blank lines split paragraphs");

        let failed = ContentState::Failed("HTTP 404".into());
        assert!(body_text(&content_document(url, Some(&failed))).contains("404"));
    }

    #[test]
    fn card_scene_lowers_text_to_glyph_runs() {
        let doc = content_document("mere://welcome", None);
        let scene = render_card_scene(&doc, 420, 360);
        let glyph_runs = scene
            .ops
            .iter()
            .filter(|op| matches!(op, netrender::SceneOp::GlyphRun(_)))
            .count();
        assert!(glyph_runs >= 1, "the welcome card lowers its text to glyph runs");
    }

    #[test]
    fn card_rect_fits_the_content_band_and_vanishes_when_tiny() {
        let (x0, y0, x1, y1, cw, ch) = card_rect(1024, 64, 600).expect("a card fits a normal window");
        assert!(x1 > x0 && y1 > y0, "non-empty rect");
        assert!(x1 <= 1024.0 && y1 <= 600.0, "within the window");
        assert!(y0 >= 64.0, "below the toolbar band");
        assert!(cw >= 1 && ch >= 1);
        assert!(card_rect(120, 64, 120).is_none(), "no card when the band is too small");
    }
}
