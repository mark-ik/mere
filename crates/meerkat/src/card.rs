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
    ColorVocabulary, DocumentRenderPacket, FontTable, StyleConfig, Viewport, layout_document,
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
        &StyleConfig::default(),
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
) -> RenderedContent {
    if let Some(ContentState::Ready(fetched)) = state {
        let engine_id = route_document_engine(url, fetched.content_type.as_deref(), registry, policy);
        if engine_id == inker::routing::ENGINE_SERVAL_WEB {
            let (scene, content_height, links, masks) =
                html_scene(&fetched.body, loader, w, h, band_y, band_h);
            return RenderedContent::Html { scene, content_height, links, masks };
        }
        if let Some(doc) = dispatch_document(url, fetched, &engine_id, registry) {
            return layout_document_content(&doc, w, h);
        }
    }
    layout_document_content(&content_document(url, state), w, h)
}

/// Lay out a document-lane doc into its retained packet (no lowering). The host
/// windows + lowers a band of the packet per scroll, so the full content height is
/// reachable without rasterizing the whole document into one texture. Returns the
/// packet, its font sidecar, and the full content height (px); link hit-testing reads
/// the packet's interactions directly (see [`DocumentRenderPacket::link_at`]).
fn layout_document_content(doc: &EngineDocument, w: u32, h: u32) -> RenderedContent {
    let laid = layout_document(doc, Viewport::new(w as f32, h as f32), &StyleConfig::default());
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
) -> Scene {
    let windowed = packet.window(band_y, band_h);
    scene_from_packet(&windowed, fonts, &card_vocabulary())
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
) -> (Scene, u32, Vec<LinkHit>) {
    // The snapshot/preview shows the page top: band_y = 0, one viewport tall.
    match render_content(url, state, registry, policy, loader, w, h, 0, h) {
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
            (lower_window(&packet, &fonts, 0.0, band), content_height, Vec::new())
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
    policy
        .route_filtered(&request, |id| {
            registry.contains(id) || id == inker::routing::ENGINE_SERVAL_WEB
        })
        .engine_id
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

/// The floating card rectangle within the content band (top-right, inset by
/// [`CARD_MARGIN`]). Returns `(x0, y0, x1, y1, w, h)` — window-space corners for
/// the composite plus the pixel size to rasterize at — or `None` when the band
/// is too small to host a readable card.
pub fn card_rect(band: [f32; 4]) -> Option<(f32, f32, f32, f32, u32, u32)> {
    let [bx0, by0, bx1, by1] = band;
    let bw = bx1 - bx0;
    let top = by0 + CARD_MARGIN;
    let avail_w = bw - 2.0 * CARD_MARGIN;
    let avail_h = by1 - top - CARD_MARGIN;
    if avail_w < 160.0 || avail_h < 100.0 {
        return None;
    }
    let cw = (bw * 0.42).clamp(280.0, 460.0).min(avail_w);
    let ch = avail_h.min(560.0);
    let x1 = bx1 - CARD_MARGIN;
    let x0 = x1 - cw;
    let y0 = top;
    let y1 = y0 + ch;
    Some((
        x0,
        y0,
        x1,
        y1,
        cw.round().max(1.0) as u32,
        ch.round().max(1.0) as u32,
    ))
}

/// A floating card of desired pixel size `cw`x`ch`, anchored **next to** the node
/// at window point `(nx, ny)`. Placed just right of the node, flipped to the left
/// when it would overflow; vertically centered on the node and clamped to the
/// content band. Returns window-space corners + the (clamped) size, or `None`
/// when the band is too small. The card follows the node because the caller
/// recomputes it each frame from the live node position. (Card system: anchored.)
pub fn anchored_card_rect(
    nx: f32,
    ny: f32,
    cw: u32,
    ch: u32,
    band: [f32; 4],
) -> Option<(f32, f32, f32, f32, u32, u32)> {
    let [bx0, by0, bx1, by1] = band;
    let margin = 8.0;
    let top = by0 + margin;
    let avail_w = (bx1 - bx0) - 2.0 * margin;
    let avail_h = by1 - top - margin;
    if avail_w < 120.0 || avail_h < 80.0 {
        return None;
    }
    let cwf = (cw as f32).min(avail_w);
    let chf = (ch as f32).min(avail_h);
    // Clearance from the node center so the card doesn't sit on the node.
    let gap = 28.0;
    let mut x0 = nx + gap;
    if x0 + cwf > bx1 - margin {
        x0 = nx - gap - cwf; // flip to the left of the node
    }
    let x0 = x0.clamp(bx0 + margin, (bx1 - margin - cwf).max(bx0 + margin));
    let y0 = (ny - chf * 0.5).clamp(top, (by1 - margin - chf).max(top));
    Some((
        x0,
        y0,
        x0 + cwf,
        y0 + chf,
        cwf.round().max(1.0) as u32,
        chf.round().max(1.0) as u32,
    ))
}

/// Edge length (px) of a live card's close (X) button.
pub const CLOSE_BTN: f32 = 22.0;
/// Inset (px) of the close button from the card's top-right corner.
pub const CLOSE_BTN_INSET: f32 = 6.0;

/// A standalone `size`x`size` close (X) button scene: a translucent dark backing
/// with a light "x" stroked across it. The host rasterizes this once and
/// composites it at a live card's top-right corner; a click in that rect reaps
/// the live preview. (Card system: in-card X close.)
pub fn close_button_scene(size: u32) -> Scene {
    let s = size as f32;
    let mut scene = Scene::new(size, size);
    // Translucent dark backing (premultiplied RGBA: black premultiplies to 0).
    scene.push_rect(0.0, 0.0, s, s, [0.0, 0.0, 0.0, 0.55]);
    // The "x": two diagonals inset from the edges.
    let pad = s * 0.3;
    let mut path = netrender::ScenePath::new();
    path.move_to(pad, pad).line_to(s - pad, s - pad);
    path.move_to(s - pad, pad).line_to(pad, s - pad);
    scene.push_shape_stroked(path, [0.92, 0.93, 0.97, 1.0], (s * 0.09).max(1.6));
    scene
}

/// The placeholder card for a focused node that has no snapshot yet (never
/// visited this session): a dashed outline framing a "Double-click to load"
/// hint. Double-clicking it promotes the node to a live preview, same as a
/// snapshot. Static, so the host rasterizes it once and caches it.
/// (Card system #3.)
pub fn unvisited_card_scene(w: u32, h: u32) -> Scene {
    let doc = document("mere://unvisited", vec![paragraph("Double-click to load")]);
    let mut laid = layout_document(
        &doc,
        Viewport::new(w as f32, h as f32),
        &StyleConfig::default(),
    );
    // Frame the whole card (not just the text extent) so the dashed border fits.
    laid.packet.viewport = Viewport::new(w as f32, h as f32);
    let mut scene = scene_from_packet(&laid.packet, &laid.fonts, &card_vocabulary());
    scene.push_stroke_decorated(
        2.0,
        2.0,
        w as f32 - 2.0,
        h as f32 - 2.0,
        [0.58, 0.61, 0.69, 0.75],
        1.5,
        netrender::SceneStrokeCap::default(),
        netrender::SceneStrokeJoin::default(),
        vec![6.0, 4.0],
    );
    scene
}

/// The placeholder for a tile whose actor is recovering from a crash (respawned,
/// not yet re-rendered): a centered "Reloading…" hint, shown until the respawned
/// actor delivers a scene whose texture covers it. Mirrors [`unvisited_card_scene`]
/// (the same doc → layout → scene path) but carries no dashed border — it is a
/// transient status, not an affordance. (Workbench tile decoration, re-applied on the
/// pelt surface path.)
pub fn recovering_card_scene(w: u32, h: u32) -> Scene {
    let doc = document("mere://recovering", vec![paragraph("Reloading\u{2026}")]);
    let mut laid = layout_document(
        &doc,
        Viewport::new(w as f32, h as f32),
        &StyleConfig::default(),
    );
    laid.packet.viewport = Viewport::new(w as f32, h as f32);
    scene_from_packet(&laid.packet, &laid.fonts, &card_vocabulary())
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
                }
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
        assert!(matches!(
            doc.blocks.first(),
            Some(DocumentBlock::Heading { level: 1, .. })
        ));
    }

    #[test]
    fn fetch_states_render_distinctly() {
        let url = "https://example.com";
        assert!(heads_with(
            &content_document(url, Some(&ContentState::Loading)),
            url
        ));
        assert!(
            body_text(&content_document(url, Some(&ContentState::Loading))).contains("Fetching")
        );

        let ready = ContentState::Ready(Fetched {
            content_type: Some("text/plain".into()),
            body: "First paragraph.\n\nSecond paragraph.".into(),
        });
        let doc = content_document(url, Some(&ready));
        let text = body_text(&doc);
        assert!(text.contains("First paragraph."), "fetched body renders");
        assert!(
            text.contains("Second paragraph."),
            "blank lines split paragraphs"
        );

        let failed = ContentState::Failed("HTTP 404".into());
        assert!(body_text(&content_document(url, Some(&failed))).contains("404"));
    }

    #[test]
    fn card_scene_lowers_text_to_glyph_runs() {
        let doc = content_document("mere://welcome", None);
        let (scene, _, _) = render_card_scene(&doc, 420, 360);
        let glyph_runs = scene
            .ops
            .iter()
            .filter(|op| matches!(op, netrender::SceneOp::GlyphRun(_)))
            .count();
        assert!(
            glyph_runs >= 1,
            "the welcome card lowers its text to glyph runs"
        );
    }

    #[test]
    fn card_rect_fits_the_content_band_and_vanishes_when_tiny() {
        let (x0, y0, x1, y1, cw, ch) =
            card_rect([0.0, 64.0, 1024.0, 600.0]).expect("a card fits a normal window");
        assert!(x1 > x0 && y1 > y0, "non-empty rect");
        assert!(x1 <= 1024.0 && y1 <= 600.0, "within the window");
        assert!(y0 >= 64.0, "below the toolbar band");
        assert!(cw >= 1 && ch >= 1);
        assert!(
            card_rect([0.0, 64.0, 120.0, 120.0]).is_none(),
            "no card when the band is too small"
        );
    }

    #[test]
    fn route_document_engine_maps_known_types() {
        let mut reg = inker::EngineRegistry::new();
        for engine in nematic::engines() {
            reg.register(engine);
        }
        let policy = inker::EngineRoutePolicy::default();
        // Content-type rules win over the scheme, so the url scheme doesn't matter
        // for these; pass an https url to exercise the scheme fallback in the last case.
        let route =
            |ct: Option<&str>| route_document_engine("https://example.test/x", ct, &reg, &policy);
        assert_eq!(route(Some("text/markdown")).as_str(), nematic::ENGINE_MARKDOWN);
        assert_eq!(route(Some("text/plain; charset=utf-8")).as_str(), nematic::ENGINE_TEXT);
        assert_eq!(route(Some("text/gemini")).as_str(), nematic::ENGINE_GEMTEXT);
        assert_eq!(route(Some("application/x-nex")).as_str(), nematic::ENGINE_NEX);
        assert_eq!(route(Some("application/x-guppy")).as_str(), nematic::ENGINE_GUPPY);
        assert_eq!(route(Some("application/x-titan")).as_str(), nematic::ENGINE_TITAN);
        assert_eq!(route(Some("message/x-misfin")).as_str(), nematic::ENGINE_MISFIN);
        assert_eq!(route(Some("application/gopher-menu")).as_str(), nematic::ENGINE_GOPHER);
        // HTML routes to the serval web lane by content-type, regardless of scheme.
        assert_eq!(
            route(Some("text/html")).as_str(),
            inker::routing::ENGINE_SERVAL_WEB
        );
        assert_eq!(
            route(Some("application/xhtml+xml")).as_str(),
            inker::routing::ENGINE_SERVAL_WEB
        );
        // No content-type over an https url falls to the scheme rule (serval).
        assert_eq!(route(None).as_str(), inker::routing::ENGINE_SERVAL_WEB);
    }

    fn glyph_runs(scene: &netrender::Scene) -> usize {
        scene
            .ops
            .iter()
            .filter(|op| matches!(op, netrender::SceneOp::GlyphRun(_)))
            .count()
    }

    #[test]
    fn html_with_an_unloaded_image_stays_scene_consistent() {
        // Repro for the crash Mark hit (serval -> scry -> serval, open a new node):
        // the dormant-node snapshot renders HTML through an empty image loader
        // (render.rs), so an `<img>` whose bytes are not cached has no source. The
        // lowered scene must NOT carry a `SceneImage` op whose key is absent from
        // `image_sources`, or netrender's rasterizer `.expect()`s and crashes the app
        // ("SceneImage references unknown ImageKey").
        let mut registry = EngineRegistry::new();
        for engine in nematic::engines() {
            registry.register(engine);
        }
        let ready = ContentState::Ready(Fetched {
            content_type: Some("text/html".into()),
            body: r#"<html><body><p>before</p>
                <img src="https://example.com/missing.png" width="80" height="60">
                <p>after</p></body></html>"#
                .into(),
        });
        let RenderedContent::Html { scene, .. } = render_content(
            "https://example.com/",
            Some(&ready),
            &registry,
            &inker::EngineRoutePolicy::default(),
            &NoImageLoader,
            420,
            360,
            0,
            360,
        ) else {
            panic!("text/html routes to the serval HTML lane");
        };
        let missing = unsourced_image_keys(&scene);
        assert!(
            missing.is_empty(),
            "an unloaded <img> left a SceneImage with no source (rasterizer would \
             panic): {missing:?}"
        );
    }

    #[test]
    fn html_with_an_undecodable_image_stays_scene_consistent() {
        // The decode-failure case: the loader returns bytes, but they are not a valid
        // image. If serval emits a SceneImage op without sourcing it (because the
        // decode failed), the rasterizer panics. The scene must stay consistent.
        struct GarbageLoader;
        impl serval_layout::ImageLoader for GarbageLoader {
            fn load(&self, _url: &str) -> Option<Vec<u8>> {
                Some(vec![0xDE, 0xAD, 0xBE, 0xEF, 0, 1, 2, 3, 4, 5]) // not a valid image
            }
        }
        let mut registry = EngineRegistry::new();
        for engine in nematic::engines() {
            registry.register(engine);
        }
        let ready = ContentState::Ready(Fetched {
            content_type: Some("text/html".into()),
            body: r#"<html><body><p>before</p>
                <img src="https://example.com/broken.png" width="80" height="60">
                <p>after</p></body></html>"#
                .into(),
        });
        let RenderedContent::Html { scene, .. } = render_content(
            "https://example.com/",
            Some(&ready),
            &registry,
            &inker::EngineRoutePolicy::default(),
            &GarbageLoader,
            420,
            360,
            0,
            360,
        ) else {
            panic!("text/html routes to the serval HTML lane");
        };
        let missing = unsourced_image_keys(&scene);
        assert!(
            missing.is_empty(),
            "an undecodable <img> left a SceneImage with no source (rasterizer would \
             panic): {missing:?}"
        );
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
        let (scene, _, _) = render_content_scene(
            "https://example.com",
            Some(&ready),
            &registry,
            &inker::EngineRoutePolicy::default(),
            &NoImageLoader,
            420,
            360,
        );
        assert!(
            glyph_runs(&scene) >= 1,
            "markdown renders text via the nematic document lane"
        );
    }

    #[test]
    fn document_lane_surfaces_link_hit_regions() {
        // The document lane (here markdown) lays hit-testable link regions onto the
        // retained packet; the host queries them via `DocumentRenderPacket::link_at`.
        // (Inline-link nav; Phase 2 query API.)
        let mut registry = EngineRegistry::new();
        for engine in nematic::engines() {
            registry.register(engine);
        }
        let ready = ContentState::Ready(Fetched {
            content_type: Some("text/markdown".into()),
            body: "See [the spec](https://example.test/spec) for details.".into(),
        });
        let RenderedContent::Document { packet, .. } = render_content(
            "https://example.test/",
            Some(&ready),
            &registry,
            &inker::EngineRoutePolicy::default(),
            &NoImageLoader,
            420,
            360,
            0,
            360,
        ) else {
            panic!("markdown routes to the document lane");
        };
        // The packet carries the link as a positive-area interaction region...
        let region = packet
            .interactions
            .iter()
            .find(|r| matches!(&r.kind, InteractionKind::Link { url } if url == "https://example.test/spec"))
            .expect("the markdown link is laid out as an interaction region");
        let b = region.bounds;
        assert!(b.size.width > 0.0 && b.size.height > 0.0, "positive-area link bounds: {b:?}");
        // ...and link_at at its center resolves to the URL.
        assert_eq!(
            packet.link_at(b.origin.x + b.size.width * 0.5, b.origin.y + b.size.height * 0.5),
            Some("https://example.test/spec"),
            "link_at resolves the link at its center"
        );
    }

    #[test]
    fn html_routes_through_serval_to_glyph_runs() {
        // The serval lane needs no document engine registered.
        let registry = EngineRegistry::new();
        let ready = ContentState::Ready(Fetched {
            content_type: Some("text/html".into()),
            body: "<h1>Hello</h1><p>World</p>".into(),
        });
        let (scene, _, _) = render_content_scene(
            "https://example.com",
            Some(&ready),
            &registry,
            &inker::EngineRoutePolicy::default(),
            &NoImageLoader,
            420,
            360,
        );
        assert!(
            glyph_runs(&scene) >= 1,
            "HTML renders text via the serval lane"
        );
    }

    #[test]
    fn html_lane_harvests_link_hit_regions() {
        // The HTML/serval lane has no retained packet, so it ships a parallel
        // `LinkHit` table harvested off the fragment plane; the host hit-tests a
        // click against it via `Constellation::link_at`'s HTML branch. (Inline-link
        // nav; Phase 5 lane parity.)
        let registry = EngineRegistry::new();
        let ready = ContentState::Ready(Fetched {
            content_type: Some("text/html".into()),
            body: "<html><body><p>see <a href=\"https://example.test/spec\">the spec</a> now</p></body></html>"
                .into(),
        });
        let RenderedContent::Html { links, .. } = render_content(
            "https://example.test/",
            Some(&ready),
            &registry,
            &inker::EngineRoutePolicy::default(),
            &NoImageLoader,
            420,
            360,
            0,
            360,
        ) else {
            panic!("text/html routes to the serval HTML lane");
        };
        let hit = links
            .iter()
            .find(|l| l.url == "https://example.test/spec")
            .expect("the inline <a href> is harvested into a LinkHit");
        let [x0, y0, x1, y1] = hit.rect;
        assert!(x1 > x0 && y1 > y0, "positive-area link rect: {:?}", hit.rect);
        // The link follows "see ", so it starts a little right of the content origin
        // (document-px, pre-scroll — the space the host adds the card's scroll into).
        assert!(x0 > 0.0, "link starts after the leading text, got x0={x0}");
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
        let (scene, _, _) = render_content_scene(
            "https://example.com",
            Some(&hidden),
            &registry,
            &inker::EngineRoutePolicy::default(),
            &NoImageLoader,
            420,
            360,
        );
        assert_eq!(
            glyph_runs(&scene),
            0,
            "a page `display:none` style suppresses the paragraph"
        );

        // Without the hiding style the same paragraph renders.
        let shown = ContentState::Ready(Fetched {
            content_type: Some("text/html".into()),
            body: "<p>Visible.</p>".into(),
        });
        let (scene, _, _) = render_content_scene(
            "https://example.com",
            Some(&shown),
            &registry,
            &inker::EngineRoutePolicy::default(),
            &NoImageLoader,
            420,
            360,
        );
        assert!(
            glyph_runs(&scene) >= 1,
            "without a hiding style the paragraph renders"
        );
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
        store.borrow_mut().insert(
            "https://example.com/hide.css".into(),
            b"p { display: none; }".to_vec(),
        );
        let wanted = RefCell::new(Vec::new());
        let loader = ResourceLoader::new(&store, "https://example.com/page.html", &wanted);

        let ready = ContentState::Ready(Fetched {
            content_type: Some("text/html".into()),
            body: "<head><link rel=\"stylesheet\" href=\"hide.css\"></head>\
                   <body><p>Hidden by the linked sheet.</p></body>"
                .into(),
        });
        let (scene, _, _) = render_content_scene(
            "https://example.com/page.html",
            Some(&ready),
            &registry,
            &inker::EngineRoutePolicy::default(),
            &loader,
            420,
            360,
        );
        assert_eq!(
            glyph_runs(&scene),
            0,
            "a head <link> stylesheet fetched through the loader hides the paragraph",
        );
    }
}
