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

/// Margin (px) between the card and the content-band edges.
const CARD_MARGIN: f32 = 24.0;

/// The synthesized document for the node at `url` (S2.2a placeholder content):
/// the built-in welcome page for `mere://welcome`, else a card naming the
/// address until live fetching lands.
pub fn node_document(url: &str) -> EngineDocument {
    let blocks = if url == "mere://welcome" {
        vec![
            heading(1, "Mere"),
            paragraph("A graph-shaped browser, hosted on serval."),
            paragraph(
                "Type a URL or a search above and press Enter. Each place you visit \
                 becomes a node in the graph behind this card; Back and Forward move \
                 through it.",
            ),
        ]
    } else {
        vec![
            heading(1, url),
            paragraph(
                "This is the focused node's media card. Live fetching is not wired \
                 yet (S2.2b); for now the card names the node's address.",
            ),
        ]
    };
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

    #[test]
    fn welcome_document_leads_with_a_heading() {
        let doc = node_document("mere://welcome");
        assert_eq!(doc.address, "mere://welcome");
        assert!(matches!(doc.blocks.first(), Some(DocumentBlock::Heading { level: 1, .. })));
    }

    #[test]
    fn other_url_names_the_address() {
        let doc = node_document("https://example.com");
        let heads_with_url = matches!(
            doc.blocks.first(),
            Some(DocumentBlock::Heading { spans, .. })
                if matches!(spans.first(), Some(InlineSpan::Text(t)) if t == "https://example.com")
        );
        assert!(heads_with_url, "the card heads with the node's address");
    }

    #[test]
    fn card_scene_lowers_text_to_glyph_runs() {
        let doc = node_document("mere://welcome");
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
