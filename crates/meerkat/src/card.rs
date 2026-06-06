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
use inker::{
    DocumentBlock, DocumentProvenance, DocumentTrustState, EngineDocument, EngineInput,
    EngineRegistry, InlineSpan,
};
use netrender::Scene;
use pelt_live::scene_from_layout_dom;
use serval_layout::{inline_stylesheets, linked_stylesheets_with_loader, ImageLoader, ScrollOffsets};
use serval_static_dom::StaticDocument;

use crate::fetch::{ContentState, Fetched};

/// The card's default stylesheet for fetched HTML — block defaults + readable
/// type. The page's own inline `<style>` and external `<link rel=stylesheet>`
/// CSS are layered on top of this (see [`html_scene`]), so a page styles itself
/// over these defaults.
/// The card's default stylesheet for fetched HTML — block defaults + readable
/// type. The page's own inline `<style>` and external `<link rel=stylesheet>`
/// CSS are layered on top of this (see [`html_scene`]), so a page styles itself
/// over these defaults.
///
/// These defaults stay **light**. A fetched web page renders on its own terms —
/// most assume a white canvas — so Mere does not force third-party pages dark
/// (that is a separate reader / dark-inject feature, and forcing it here would
/// drop a page's own dark text onto a dark default). Dark mode themes Mere's
/// *own* surfaces: the chrome, the orrery, and the synthesized / nematic card
/// lanes (via [`card_vocabulary`]). Web content is rendered faithfully.
const HTML_SHEET: &[&str] = &[
    "html, body, div, p, section, article, header, footer, nav, main, ul, ol, li, \
     blockquote, pre, table, tr, h1, h2, h3, h4, h5, h6 { display: block; }",
    "body { padding: 16px; background-color: rgb(250, 250, 252); color: rgb(22, 24, 32); }",
    "h1 { font-size: 30px; } h2 { font-size: 24px; } h3 { font-size: 20px; }",
    "p, li, td { font-size: 16px; }",
    "a { color: rgb(40, 80, 170); }",
    "pre, code { font-size: 14px; color: rgb(50, 54, 64); }",
];

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

/// Lay out `doc` at width `w` and lower it to a `netrender::Scene` — the proven
/// document pipeline (parley layout + the shared paint-list translator). Returns
/// the scene plus the **full content height** in px (the document grows past `h`;
/// see [`layout_document`]'s `content_bounds`), so the host can rasterize a tall
/// texture and scroll a window of it on the GPU. The caller composites the scene
/// at the card rect with an opaque card background.
pub fn render_card_scene(doc: &EngineDocument, w: u32, h: u32) -> (Scene, u32) {
    let mut laid = layout_document(doc, Viewport::new(w as f32, h as f32), &StyleConfig::default());
    let content_height = laid.packet.content_bounds.size.height.ceil().max(1.0);
    // Expand the paint-list viewport to the full content height before lowering, so
    // the rasterizer renders the *whole* document into the tall texture. The paint
    // list otherwise inherits the visible viewport (`h`) and culls everything below
    // it — which would leave a tall texture blank past `h` and nothing to scroll to.
    laid.packet.viewport = Viewport::new(w as f32, content_height);
    let scene = scene_from_packet(&laid.packet, &laid.fonts, &card_vocabulary());
    (scene, content_height as u32)
}

/// Light-on-dark text palette for the card's synthesized + nematic document
/// lanes (welcome / loading / plain / markdown / …), matching the dark
/// `CARD_BG`. The HTML lane themes through `HTML_SHEET` + the page's own CSS, so
/// it does not use this. (The document-canvas default is near-black-on-light.)
fn card_vocabulary() -> ColorVocabulary {
    ColorVocabulary {
        body_text: [0.88, 0.90, 0.94, 1.0],
        heading_text: [0.96, 0.97, 1.0, 1.0],
        link_text: [0.50, 0.70, 1.0, 1.0],
        code_text: [0.80, 0.85, 0.78, 1.0],
        badge_text: [0.66, 0.71, 0.81, 1.0],
        rule: [0.45, 0.48, 0.55, 1.0],
        placeholder_text: [0.88, 0.90, 0.94, 0.12],
        placeholder_image: [0.55, 0.60, 0.70, 0.20],
    }
}

/// Render the focused node's card scene, routing Ready content by type: HTML
/// parses + renders through serval ([`html_scene`], resolving subresources via
/// `loader`); a content-type with a matching nematic engine goes through the
/// document lane; everything else (welcome / loading / failed / unrouted)
/// renders the synthesized document.
pub fn render_content_scene(
    url: &str,
    state: Option<&ContentState>,
    registry: &EngineRegistry,
    loader: &impl ImageLoader,
    w: u32,
    h: u32,
) -> (Scene, u32) {
    if let Some(ContentState::Ready(fetched)) = state {
        if is_html(fetched.content_type.as_deref()) {
            return html_scene(&fetched.body, loader, w, h);
        }
        if let Some(doc) = routed_document(url, fetched, registry) {
            return render_card_scene(&doc, w, h);
        }
    }
    render_card_scene(&content_document(url, state), w, h)
}

/// Dispatch Ready content to the nematic engine matching its content-type, if any,
/// producing an [`EngineDocument`]. `None` when no engine matches (the caller then
/// falls back to the plain rendering).
fn routed_document(url: &str, fetched: &Fetched, registry: &EngineRegistry) -> Option<EngineDocument> {
    let id = engine_id_for(fetched.content_type.as_deref())?;
    let engine = registry.engine(id)?;
    let mut input = EngineInput::new(url, fetched.body.clone());
    if let Some(ct) = &fetched.content_type {
        input = input.with_content_type(ct.clone());
    }
    engine.render(&input).ok()
}

/// Map an HTTP content-type to a nematic document engine id (the base type, minus
/// any `; charset=…`). `None` for HTML (the serval lane) or unknown types.
fn engine_id_for(content_type: Option<&str>) -> Option<&'static str> {
    let base = base_type(content_type?);
    Some(match base.as_str() {
        "text/markdown" | "text/x-markdown" => nematic::ENGINE_MARKDOWN,
        "text/gemini" => nematic::ENGINE_GEMTEXT,
        "text/plain" => nematic::ENGINE_TEXT,
        "application/gopher-menu" => nematic::ENGINE_GOPHER,
        "text/x-finger" => nematic::ENGINE_FINGER,
        "application/x-nex" => nematic::ENGINE_NEX,
        "application/x-guppy" => nematic::ENGINE_GUPPY,
        "application/x-titan" => nematic::ENGINE_TITAN,
        "message/x-misfin" => nematic::ENGINE_MISFIN,
        "application/rss+xml" | "application/atom+xml" | "application/feed+json" => {
            nematic::ENGINE_FEED
        },
        _ => return None,
    })
}

/// Whether the content-type is HTML (handled by the serval lane).
fn is_html(content_type: Option<&str>) -> bool {
    content_type
        .map(base_type)
        .is_some_and(|b| b == "text/html" || b == "application/xhtml+xml")
}

/// The lowercased base media type, dropping parameters
/// (`text/HTML; charset=utf-8` → `text/html`).
fn base_type(content_type: &str) -> String {
    content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase()
}

/// Parse `body` as a full HTML document and render it through the shared content
/// core ([`pelt_live::scene_from_layout_dom`]) — the same cascade → image-decode
/// → layout → emit pipeline the static viewer uses. A full-document parse (not a
/// body-only fragment) keeps `<head>`, so head `<style>` / `<link>` are seen.
///
/// Author CSS layers card defaults, then the page's inline `<style>`, then
/// external `<link rel=stylesheet>` (later sheets win at equal specificity).
/// `<img>` / `background-image` and `<link>` bytes resolve through `loader`:
/// `data:` URIs decode inline; remote URLs come from the host's resource cache,
/// absent on the first frame and filled by the demand fetch that re-renders.
fn html_scene(body: &str, loader: &impl ImageLoader, w: u32, h: u32) -> (Scene, u32) {
    let doc = StaticDocument::parse(body);
    let inline = inline_stylesheets(&doc);
    let linked = linked_stylesheets_with_loader(&doc, loader);
    let mut sheets: Vec<&str> = HTML_SHEET.to_vec();
    sheets.extend(inline.iter().map(String::as_str));
    sheets.extend(linked.iter().map(String::as_str));
    let scroll = ScrollOffsets::default();
    let scene = scene_from_layout_dom(&doc, &sheets, loader, w, h, &scroll);
    // The serval lane lays out to the viewport height and does not yet report a
    // taller content extent, so HTML cards report `h` and simply do not scroll
    // until that serval-side change lands. The document lane (most smolweb content)
    // reports its true content height and scrolls.
    (scene, h.max(1))
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
    use serval_layout::NoImageLoader;

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
        let (scene, _) = render_card_scene(&doc, 420, 360);
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

    #[test]
    fn engine_id_for_maps_known_types() {
        assert_eq!(engine_id_for(Some("text/markdown")), Some(nematic::ENGINE_MARKDOWN));
        assert_eq!(engine_id_for(Some("text/plain; charset=utf-8")), Some(nematic::ENGINE_TEXT));
        assert_eq!(engine_id_for(Some("text/gemini")), Some(nematic::ENGINE_GEMTEXT));
        assert_eq!(engine_id_for(Some("application/x-nex")), Some(nematic::ENGINE_NEX));
        assert_eq!(engine_id_for(Some("application/x-guppy")), Some(nematic::ENGINE_GUPPY));
        assert_eq!(engine_id_for(Some("application/x-titan")), Some(nematic::ENGINE_TITAN));
        assert_eq!(engine_id_for(Some("message/x-misfin")), Some(nematic::ENGINE_MISFIN));
        assert_eq!(engine_id_for(Some("text/html")), None, "HTML uses the serval lane");
        assert_eq!(engine_id_for(None), None);
    }

    #[test]
    fn is_html_detects_html_types() {
        assert!(is_html(Some("text/html; charset=utf-8")));
        assert!(is_html(Some("application/xhtml+xml")));
        assert!(!is_html(Some("text/markdown")));
        assert!(!is_html(None));
    }

    fn glyph_runs(scene: &netrender::Scene) -> usize {
        scene.ops.iter().filter(|op| matches!(op, netrender::SceneOp::GlyphRun(_))).count()
    }

    #[test]
    fn markdown_routes_through_nematic_to_glyph_runs() {
        let mut registry = EngineRegistry::new();
        for engine in nematic::engines() {
            registry.register(engine);
        }
        let ready = ContentState::Ready(Fetched {
            content_type: Some("text/markdown".into()),
            body: "# Heading\n\nA paragraph.".into(),
        });
        let (scene, _) = render_content_scene("https://example.com", Some(&ready), &registry, &NoImageLoader, 420, 360);
        assert!(glyph_runs(&scene) >= 1, "markdown renders text via the nematic document lane");
    }

    #[test]
    fn html_routes_through_serval_to_glyph_runs() {
        // The serval lane needs no document engine registered.
        let registry = EngineRegistry::new();
        let ready = ContentState::Ready(Fetched {
            content_type: Some("text/html".into()),
            body: "<h1>Hello</h1><p>World</p>".into(),
        });
        let (scene, _) = render_content_scene("https://example.com", Some(&ready), &registry, &NoImageLoader, 420, 360);
        assert!(glyph_runs(&scene) >= 1, "HTML renders text via the serval lane");
    }

    #[test]
    fn html_lane_applies_page_supplied_style() {
        let registry = EngineRegistry::new();
        // A page `<style>` that hides the only paragraph suppresses its text —
        // proof the page's own CSS reaches the cascade (this rule is not in
        // HTML_SHEET, which only sets `p`'s font-size).
        let hidden = ContentState::Ready(Fetched {
            content_type: Some("text/html".into()),
            body: "<style>p { display: none; }</style><p>Hidden by the page.</p>".into(),
        });
        let (scene, _) = render_content_scene("https://example.com", Some(&hidden), &registry, &NoImageLoader, 420, 360);
        assert_eq!(glyph_runs(&scene), 0, "a page `display:none` style suppresses the paragraph");

        // Without the hiding style the same paragraph renders.
        let shown = ContentState::Ready(Fetched {
            content_type: Some("text/html".into()),
            body: "<p>Visible.</p>".into(),
        });
        let (scene, _) = render_content_scene("https://example.com", Some(&shown), &registry, &NoImageLoader, 420, 360);
        assert!(glyph_runs(&scene) >= 1, "without a hiding style the paragraph renders");
    }

    #[test]
    fn html_lane_applies_head_linked_stylesheet_through_the_loader() {
        use std::cell::RefCell;

        use crate::resources::{ResourceLoader, ResourceStore};

        let registry = EngineRegistry::new();
        // A `<head>` `<link>` to a sheet that hides the paragraph, with its bytes
        // already cached. The link must be seen despite living in `<head>` (full
        // document parse) and apply through the loader seam.
        let store = RefCell::new(ResourceStore::default());
        store
            .borrow_mut()
            .insert("https://example.com/hide.css".into(), b"p { display: none; }".to_vec());
        let wanted = RefCell::new(Vec::new());
        let loader = ResourceLoader::new(&store, "https://example.com/page.html", &wanted);

        let ready = ContentState::Ready(Fetched {
            content_type: Some("text/html".into()),
            body: "<head><link rel=\"stylesheet\" href=\"hide.css\"></head>\
                   <body><p>Hidden by the linked sheet.</p></body>"
                .into(),
        });
        let (scene, _) =
            render_content_scene("https://example.com/page.html", Some(&ready), &registry, &loader, 420, 360);
        assert_eq!(
            glyph_runs(&scene),
            0,
            "a head <link> stylesheet fetched through the loader hides the paragraph",
        );
    }
}
