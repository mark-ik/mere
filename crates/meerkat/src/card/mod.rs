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
use document_canvas::{
    ColorVocabulary, DocumentRenderPacket, FontTable, DocumentStyleSheet, Viewport, layout_document,
};
// Used only by the `#[cfg(test)]` link-lowering helper + tests; the live path queries
// `DocumentRenderPacket::link_at` directly. (Phase 2 query API.)
#[cfg(test)]
use document_canvas::{InteractionKind, InteractionRegion};
use inker::{
    DocumentBlock, DocumentProvenance, DocumentTrustState, EngineDocument, EngineInput,
    EngineRegistry, EngineRoutePolicy, EngineRouteRequest, InlineSpan, WorkspaceRouteId,
};
use netrender::Scene;
use crate::serval_render::scene_from_layout_dom;
use serval_layout::{
    ImageLoader, ScrollOffsets, inline_stylesheets, linked_stylesheets_with_loader,
};
use serval_static_dom::{StaticDocument, StaticNodeId};

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
pub(crate) const HTML_SHEET: &[&str] = &[
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
                vec![
                    heading(1, url),
                    paragraph(&format!("Could not load: {reason}")),
                ]
            }
            None => vec![
                heading(1, url),
                paragraph("This node has no fetched media yet."),
            ],
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

/// Lay out `doc` at width `w` and lower it to one full-height `netrender::Scene`
/// (the pre-windowing path: parley layout + the shared paint-list translator). The
/// live path now windows per band ([`lower_window`]); this stays as a test helper for
/// the document-lowering unit tests, returning the scene plus full content height.
#[cfg(test)]
fn render_card_scene(doc: &EngineDocument, w: u32, h: u32) -> (Scene, u32, Vec<LinkHit>) {
    let mut laid = layout_document(
        doc,
        Viewport::new(w as f32, h as f32),
        &card_sheet(card_vocabulary()),
    );
    let content_height = laid.packet.content_bounds.size.height.ceil().max(1.0);
    // The document-canvas lane lays out hit-testable link regions (content-local px,
    // full-document space) right here. Harvest them before the viewport rewrite below
    // so the host can route a click on a link to its navigation. (Inline-link nav.)
    let links = link_hits(&laid.packet.interactions);
    // Expand the paint-list viewport to the full content height before lowering, so
    // the rasterizer renders the *whole* document into the tall texture. The paint
    // list otherwise inherits the visible viewport (`h`) and culls everything below
    // it — which would leave a tall texture blank past `h` and nothing to scroll to.
    laid.packet.viewport = Viewport::new(w as f32, content_height);
    let scene = scene_from_packet(&laid.packet, &laid.fonts, &card_vocabulary());
    (scene, content_height as u32, links)
}

/// A clickable link in a rendered content card: its bounds in content-local px
/// (document space, pre-scroll) and target URL. The host hit-tests a click against
/// these (offset by the card's scroll) and navigates the URL. (Inline-link nav.)
#[derive(Clone, Debug, PartialEq)]
pub struct LinkHit {
    pub rect: [f32; 4],
    pub url: String,
}

/// Flatten the document-canvas interaction regions into a link-hit map —
/// `[x0, y0, x1, y1]` content-local bounds + URL. The live path hit-tests the
/// retained packet directly ([`DocumentRenderPacket::link_at`]); this stays for the
/// [`render_card_scene`] test helper. (Today every interaction is a link.)
#[cfg(test)]
fn link_hits(regions: &[InteractionRegion]) -> Vec<LinkHit> {
    regions
        .iter()
        .map(|r| {
            let InteractionKind::Link { url } = &r.kind;
            let b = &r.bounds;
            LinkHit {
                rect: [
                    b.origin.x,
                    b.origin.y,
                    b.origin.x + b.size.width,
                    b.origin.y + b.size.height,
                ],
                url: url.clone(),
            }
        })
        .collect()
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

/// The document style sheet for the card's synthesized + nematic lanes: the
/// built-in typography with the light-on-dark [`card_vocabulary`] palette, so
/// per-role text colors (heading / link / code / badge) resolve against the
/// dark `CARD_BG` rather than the default near-black-on-light. Layout bakes
/// these colors onto each glyph run, so the per-band [`lower_window`] lowering
/// carries them automatically.
fn card_sheet(colors: ColorVocabulary) -> DocumentStyleSheet {
    DocumentStyleSheet {
        colors,
        ..DocumentStyleSheet::default()
    }
}

/// A node's rendered content, forked by lane. The document lane (gemtext, feeds,
/// synthesized cards: most smolweb content) ships the **retained packet** the host
/// windows and lowers a band of at a time, so a tall page is not baked into one
/// capped texture. The HTML/serval lane still ships one pre-lowered scene (a
/// different pipeline; its windowing is the Phase 5 lane-parity work).
pub enum RenderedContent {
    Document {
        packet: DocumentRenderPacket,
        fonts: FontTable,
        content_height: u32,
    },
    Html {
        scene: Scene,
        content_height: u32,
        links: Vec<LinkHit>,
        /// Blurred box-shadow masks the host builds (GPU) + registers before
        /// rasterizing `scene`. Empty when the page has no blurred shadows.
        masks: Vec<paint_list_render::BoxShadowMaskRequest>,
    },
}

/// Render the focused node's content, routing Ready content through the engine
/// policy: an id of [`serval.web`](inker::routing::ENGINE_SERVAL_WEB) parses +
/// renders through serval ([`html_scene`], resolving subresources via `loader`) to a
/// scene; a registered nematic engine and everything else (welcome / loading /
/// failed / unrouted) lay out through the document lane to a retained packet.
pub fn render_content(
    url: &str,
    state: Option<&ContentState>,
    registry: &EngineRegistry,
    policy: &EngineRoutePolicy,
    loader: &impl ImageLoader,
    w: u32,
    h: u32,
    // The HTML/serval lane emits only this vertical band (`band_y`..`band_y + band_h`)
    // so a tall dense page does not overflow the GPU; the document lane ignores it (the
    // host windows its retained packet). (HTML scroll.)
    band_y: u32,
    band_h: u32,
    // The composed document style sheet (user typography + theme-derived colours);
    // the document lane lays out with it, the HTML/serval lane ignores it (it themes
    // through HTML_SHEET + the page's CSS). (Document theming P3; typography D1.)
    sheet: &DocumentStyleSheet,
) -> RenderedContent {
    if let Some(ContentState::Ready(fetched)) = state {
        let engine_id = route_document_engine(url, fetched.content_type.as_deref(), registry, policy);
        if engine_id == inker::routing::ENGINE_SERVAL_WEB {
            let (scene, content_height, links, masks) =
                html_scene(&fetched.body, loader, w, h, band_y, band_h);
            return RenderedContent::Html { scene, content_height, links, masks };
        }
        if let Some(doc) = dispatch_document(url, fetched, &engine_id, registry) {
            return layout_document_content(&doc, w, h, sheet);
        }
    }
    layout_document_content(&content_document(url, state), w, h, sheet)
}

/// Whether `(url, state)` routes to the serval HTML lane, so the content actor retains a
/// [`serval_layout::ContentLayout`] for it (the document / synthesized lanes keep their own
/// retained packet / one-shot path, so they do not). (Slice 1.)
pub fn is_serval_html_lane(
    url: &str,
    state: Option<&ContentState>,
    registry: &EngineRegistry,
    policy: &EngineRoutePolicy,
) -> bool {
    matches!(
        state,
        Some(ContentState::Ready(fetched))
            if route_document_engine(url, fetched.content_type.as_deref(), registry, policy)
                == inker::routing::ENGINE_SERVAL_WEB
    )
}

/// Lay out a document-lane doc into its retained packet (no lowering). The host
/// windows + lowers a band of the packet per scroll, so the full content height is
/// reachable without rasterizing the whole document into one texture. Returns the
/// packet, its font sidecar, and the full content height (px); link hit-testing reads
/// the packet's interactions directly (see [`DocumentRenderPacket::link_at`]).
fn layout_document_content(
    doc: &EngineDocument,
    w: u32,
    h: u32,
    sheet: &DocumentStyleSheet,
) -> RenderedContent {
    let laid = layout_document(doc, Viewport::new(w as f32, h as f32), sheet);
    let content_height = laid.packet.content_bounds.size.height.ceil().max(1.0) as u32;
    // Link hit-testing reads the retained packet's interactions directly (the host
    // queries `DocumentRenderPacket::link_at`), so the document lane no longer ships a
    // parallel link-rect table. (Inline-link nav; Phase 2 query API.)
    RenderedContent::Document {
        packet: laid.packet,
        fonts: laid.fonts,
        content_height,
    }
}

/// Lower a vertical band `[band_y, band_y + band_h]` of a retained document packet
/// into a band-tall scene the host rasterizes. The window translates the band to
/// `y = 0`, so the scene fits a `band_h`-tall texture regardless of the document's
/// full height — the heart of the tiled render. (Retained-text / tiled render.)
pub fn lower_window(
    packet: &DocumentRenderPacket,
    fonts: &FontTable,
    band_y: f32,
    band_h: f32,
    colors: ColorVocabulary,
) -> Scene {
    let windowed = packet.window(band_y, band_h);
    scene_from_packet(&windowed, fonts, &colors)
}

/// The image-op keys in `scene` that are absent from its own `image_sources`.
/// netrender's rasterizer skips exactly these — a self-consistent scene returns
/// empty. Used by the card scene-consistency tests.
#[cfg(test)]
pub(crate) fn unsourced_image_keys(scene: &Scene) -> Vec<netrender::ImageKey> {
    scene
        .ops
        .iter()
        .filter_map(|op| match op {
            netrender::SceneOp::Image(i) => Some(i.key),
            _ => None,
        })
        .filter(|key| !scene.image_sources.contains_key(key))
        .collect()
}

/// Cap (px) for a single-shot preview band. The synchronous snapshot/thumbnail
/// path ([`render_content_scene`]) lowers one band from the top of a document at
/// this height, so even a very tall dormant page rasterizes for its preview rather
/// than failing as one over-tall texture. Live cards window the full height instead.
pub(crate) const PREVIEW_BAND_PX: u32 = 6144;

/// Render content straight to one scene for the synchronous snapshot/thumbnail path
/// (dormant-node previews in the orrery) and the card unit tests. The live actor
/// path uses [`render_content`] + per-band [`lower_window`]; here the document lane
/// lowers a single band from the top, capped at [`PREVIEW_BAND_PX`], so a tall page
/// still rasterizes for its preview. The returned `content_height` is the full
/// height (the caller caps its own texture to the band).
pub fn render_content_scene(
    url: &str,
    state: Option<&ContentState>,
    registry: &EngineRegistry,
    policy: &EngineRoutePolicy,
    loader: &impl ImageLoader,
    w: u32,
    h: u32,
    sheet: &DocumentStyleSheet,
) -> (Scene, u32, Vec<LinkHit>) {
    // The snapshot/preview shows the page top: band_y = 0, one viewport tall.
    match render_content(url, state, registry, policy, loader, w, h, 0, h, sheet) {
        RenderedContent::Html {
            scene,
            content_height,
            links,
            masks: _, // the snapshot/preview path does not build shadow masks
        } => (scene, content_height, links),
        RenderedContent::Document {
            packet,
            fonts,
            content_height,
        } => {
            let band = content_height.min(PREVIEW_BAND_PX) as f32;
            (lower_window(&packet, &fonts, 0.0, band, sheet.colors), content_height, Vec::new())
        }
    }
}

/// Route Ready content to an engine id through the policy: scheme + content-type
/// over the active rules, filtered to engines this lane can serve (registered
/// document engines plus the serval html lane). Surface-engine pins are resolved
/// at nav time on the UI thread and never reach the actor, so this pass carries no
/// pin. (engine-picker Phase 0b — replaces the bespoke `engine_id_for` match.)
fn route_document_engine(
    url: &str,
    content_type: Option<&str>,
    registry: &EngineRegistry,
    policy: &EngineRoutePolicy,
) -> String {
    let request = EngineRouteRequest {
        workspace_id: WorkspaceRouteId::new("meerkat"),
        view: None,
        node: None,
        address: url.to_string(),
        content_type: content_type.map(str::to_string),
        pinned_engine: None,
    };
    let engine_id = policy
        .route_filtered(&request, |id| {
            registry.contains(id) || id == inker::routing::ENGINE_SERVAL_WEB
        })
        .engine_id;
    tracing::info!(target: "meerkat", %url, content_type = ?content_type, %engine_id, "route_document_engine TEMP probe");
    engine_id
}

/// Dispatch Ready content to the document engine `id` (a registered nematic
/// engine), producing an [`EngineDocument`]. `None` when `id` is not a registered
/// document engine (the serval / internal / external / ingest ids), so the caller
/// falls back to the synthesized document.
fn dispatch_document(
    url: &str,
    fetched: &Fetched,
    id: &str,
    registry: &EngineRegistry,
) -> Option<EngineDocument> {
    let engine = registry.engine(id)?;
    // The full body lays out: the host windows + lowers a band of the retained
    // packet per scroll (see [`lower_window`]), so an arbitrarily tall document is
    // never baked into one texture. (This is what retired the old 12 KiB body cap.)
    let mut input = EngineInput::new(url, fetched.body.clone());
    if let Some(ct) = &fetched.content_type {
        input = input.with_content_type(ct.clone());
    }
    engine.render(&input).ok()
}

/// The [`EngineDocument`] a document-family node renders to: route the Ready content
/// to its engine and dispatch. `None` for non-Ready / non-document content. The serval
/// note-tile lane uses this to get the blocks for `note_view`. (Djot reframe slice B.)
pub(crate) fn engine_document_for(
    url: &str,
    state: Option<&ContentState>,
    registry: &EngineRegistry,
    policy: &EngineRoutePolicy,
) -> Option<EngineDocument> {
    let ContentState::Ready(fetched) = state? else {
        return None;
    };
    let id = route_document_engine(url, fetched.content_type.as_deref(), registry, policy);
    dispatch_document(url, fetched, &id, registry)
}

/// Parse `body` as a full HTML document and render it through the shared content
/// core ([`crate::serval_render::scene_from_layout_dom`]) — the same cascade → image-decode
/// → layout → emit pipeline the static viewer uses. A full-document parse (not a
/// body-only fragment) keeps `<head>`, so head `<style>` / `<link>` are seen.
///
/// Author CSS layers card defaults, then the page's inline `<style>`, then
/// external `<link rel=stylesheet>` (later sheets win at equal specificity).
/// `<img>` / `background-image` and `<link>` bytes resolve through `loader`:
/// `data:` URIs decode inline; remote URLs come from the host's resource cache,
/// absent on the first frame and filled by the demand fetch that re-renders.
fn html_scene(
    body: &str,
    loader: &impl ImageLoader,
    w: u32,
    h: u32,
    band_y: u32,
    band_h: u32,
) -> (Scene, u32, Vec<LinkHit>, Vec<paint_list_render::BoxShadowMaskRequest>) {
    let doc = StaticDocument::parse(body);
    let inline = inline_stylesheets(&doc);
    let linked = linked_stylesheets_with_loader(&doc, loader);
    let mut sheets: Vec<&str> = HTML_SHEET.to_vec();
    sheets.extend(inline.iter().map(String::as_str));
    sheets.extend(linked.iter().map(String::as_str));
    let scroll = ScrollOffsets::default();
    // Lay out at the viewport (so `@media` cascades right) and emit ONE band
    // (`band_y`..`band_y + band_h`) of the page: a flat serval scene the host cannot
    // window, so emitting the whole tall dense page would overflow the GPU. The host
    // requests bands as the scroll moves. `content_height` is the full page height (the
    // scroll range); the band carries only its slice of ops. `masks` are the blurred
    // box-shadow requests the host builds + registers before rasterizing. `link_rects`
    // are every `<a href>`'s href + full-document-px hit rect, lowered to `LinkHit`s
    // for the host's click hit-test (the flat scene is not queryable). (HTML scroll;
    // box-shadow; inline-link nav.)
    let (scene, masks, content_height, link_rects) =
        scene_from_layout_dom(&doc, &sheets, loader, w, h, band_y, band_h, &scroll);
    let links = link_rects.into_iter().map(|(url, rect)| LinkHit { rect, url }).collect();
    (scene, content_height, links, masks)
}

/// Parse + cascade + lay out a fetched HTML page ONCE into a retained
/// [`serval_layout::ContentLayout`], so the content actor re-emits scroll bands / find
/// rects off it without re-cascading per band / keystroke (slice 1). Mirrors
/// [`html_scene`]'s parse + sheet assembly; the emit halves are
/// [`crate::serval_render::scene_from_content_band`] and
/// [`serval_layout::ContentLayout::find`]. Returns the parsed doc (the layout's planes are
/// keyed by its node ids and the band / find emit walk it) alongside the layout. Subresource
/// wants are recorded through `loader`, as in `html_scene`.
pub fn build_html_layout(
    body: &str,
    loader: &impl ImageLoader,
    w: u32,
    h: u32,
) -> (StaticDocument, serval_layout::ContentLayout<StaticNodeId>) {
    let doc = StaticDocument::parse(body);
    let inline = inline_stylesheets(&doc);
    let linked = linked_stylesheets_with_loader(&doc, loader);
    let mut sheets: Vec<&str> = HTML_SHEET.to_vec();
    sheets.extend(inline.iter().map(String::as_str));
    sheets.extend(linked.iter().map(String::as_str));
    let layout = serval_layout::lay_out_content(&doc, &sheets, loader, w, h);
    (doc, layout)
}

fn heading(level: u8, text: &str) -> DocumentBlock {
    DocumentBlock::Heading {
        level,
        spans: vec![InlineSpan::Text(text.to_string())],
    }
}

fn paragraph(text: &str) -> DocumentBlock {
    DocumentBlock::Paragraph {
        spans: vec![InlineSpan::Text(text.to_string())],
    }
}

mod geometry;
pub use geometry::*;

#[cfg(test)]
mod tests;
