/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Per-event content handlers: script attach, event delivery, layout + render.

use super::*;

fn emit_scene(
    out: &impl ContentUpdateSink,
    nav: NavGeneration,
    viewport_gen: ViewportGeneration,
    scene: Scene,
    content_height: u32,
    band_y: u32,
    band_h: u32,
    links: Vec<LinkHit>,
    masks: Vec<paint_list_render::BoxShadowMaskRequest>,
) {
    let stats = scene_stats(&scene);
    out.emit_update(ContentUpdate::Scene {
        nav,
        viewport_gen,
        scene,
        stats,
        content_height,
        band_y,
        band_h,
        links,
        masks,
    });
}

fn emit_engine_stats(
    out: &impl ContentUpdateSink,
    nav: NavGeneration,
    viewport_gen: ViewportGeneration,
    dom: DomArenaStats,
    layout: Option<LayoutBatchStats>,
) {
    out.emit_update(ContentUpdate::EngineStats {
        nav,
        viewport_gen,
        dom,
        layout,
    });
}

/// Mirror the current HTML page into a `ScriptedDom` and attach the DocumentScript at
/// `component_path` over it under `grant` (P2.5c). HTML/serval lane only; returns a
/// human-readable outcome for the `ScriptOutcome` update. A `grant` that denies a
/// capability the component requires makes instantiation fail (reported as "attach
/// failed") — the runtime-enforced capability boundary.
#[allow(clippy::too_many_arguments)]
pub(crate) fn attach_script(
    content: &mut Content,
    component_path: &Path,
    grant: &Grant,
    fetcher: Option<Arc<dyn NetFetcher>>,
    store: &RefCell<ResourceStore>,
    registry: &EngineRegistry,
    policy: &EngineRoutePolicy,
    out: &impl ContentUpdateSink,
) -> String {
    if !is_serval_html_lane(&content.url, content.state.as_ref(), registry, policy) {
        return "not an HTML/serval page (no mirrorable DOM)".to_string();
    }
    let url = content.url.clone();
    let (w, h) = content.viewport;
    let body = match &content.state {
        Some(ContentState::Ready(fetched)) => fetched.body.clone(),
        // is_serval_html_lane above guarantees a Ready HTML body.
        _ => return "no Ready HTML body".to_string(),
    };
    // Tear down any script already attached to this tile (run its `deactivate`)
    // before replacing it, so a re-attach doesn't silently drop a live instance
    // without lifecycle teardown (C2).
    if let Some(prev) = content.script.take() {
        let _ = prev.detach();
    }
    // Same-origin `net` egress (§E1): a script may fetch only its own page's origin,
    // so a granted `net` cannot read or exfiltrate to a third-party host. A broader
    // cross-origin allowlist (a mod declaring extra net origins) is a later refinement.
    let net_origins = vec![script::host_of(&url)];
    let wanted = RefCell::new(Vec::new());
    let outcome = {
        let loader = ResourceLoader::new(store, &url, &wanted);
        match ScriptInstance::attach(
            component_path,
            &body,
            &loader,
            w,
            h,
            grant,
            Quota::default(),
            fetcher,
            net_origins,
        ) {
            Ok(inst) => {
                content.script = Some(inst);
                "attached".to_string()
            }
            Err(e) => format!("attach failed: {e}"),
        }
    };
    // Ship the subresources the mirrored page's first layout wants (#3).
    emit_fresh_wanted(content.nav, wanted, store, out);
    outcome
}

/// Deliver one event to the attached script (P2.5c). The script's batch is applied to
/// the live DOM and the layout re-laid-out on a change; returns the textual outcome.
pub(crate) fn deliver_event(
    content: &mut Content,
    kind: &str,
    payload: &str,
    store: &RefCell<ResourceStore>,
    out: &impl ContentUpdateSink,
) -> String {
    let url = content.url.clone();
    let wanted = RefCell::new(Vec::new());
    let mut trapped = false;
    let outcome = {
        let loader = ResourceLoader::new(store, &url, &wanted);
        let Some(inst) = content.script.as_mut() else {
            return "no script attached".to_string();
        };
        match inst.deliver(kind, payload, &loader) {
            Ok(outcome) => format!("{outcome:?}"),
            // An `Err` here means the guest call *trapped* (epoch-cancelled runaway
            // or memory bomb), per the DocumentScript contract — the store is
            // poisoned. Mark it so we detach below rather than re-enter the dead
            // instance on every later event (C3).
            Err(e) => {
                trapped = true;
                format!("turn trapped, detached: {e}")
            }
        }
    };
    if trapped {
        // Drop the poisoned instance (deactivate on a trapped store is futile) and
        // revert to the static page so the tile recovers instead of re-trapping.
        content.script = None;
        content.html = None;
    }
    // A script mutation may have re-laid-out (new nodes / images) and wanted new
    // subresources; ship them. (#3.)
    emit_fresh_wanted(content.nav, wanted, store, out);
    outcome
}

/// Build the retained serval-lane [`ContentLayout`] into `content.html` if this is the
/// HTML/serval lane and the cache is empty (recording subresource wants through `wanted`),
/// and return whether the HTML cache is now present. The arms clear `content.html` on a
/// body / viewport / subresource change, so a present layout is fresh. (Slice 1.)
pub(crate) fn ensure_html_layout(
    content: &mut Content,
    store: &RefCell<ResourceStore>,
    registry: &EngineRegistry,
    policy: &EngineRoutePolicy,
    wanted: &RefCell<Vec<String>>,
) -> bool {
    if !is_serval_html_lane(&content.url, content.state.as_ref(), registry, policy) {
        return false;
    }
    if content.html.is_none() {
        let (w, h) = content.viewport;
        let loader = ResourceLoader::new(store, &content.url, wanted);
        let body = match &content.state {
            Some(ContentState::Ready(fetched)) => &fetched.body,
            // `is_serval_html_lane` above guarantees a Ready HTML body.
            _ => unreachable!("is_serval_html_lane implies a Ready state"),
        };
        content.html = Some(build_html_layout(body, &loader, w, h));
    }
    true
}

/// Whether `url` is a smolweb scheme. The content actor is the focused path, so a
/// smolweb capsule here renders through the serval lane; cards keep the separate
/// synchronous block path. (Smolweb host P1.)
#[cfg(feature = "smolweb")]
fn is_smolweb_lane(url: &str) -> bool {
    [
        "gemini://",
        "gopher://",
        "nex://",
        "finger://",
        "spartan://",
        "guppy://",
    ]
    .iter()
    .any(|scheme| url.starts_with(scheme))
}

/// Build the retained [`SmolwebDocument`](pelt_desktop::SmolwebDocument) into
/// `content.smolweb` if this is a smolweb capsule with a ready body, returning whether
/// it is now present. v1 themes with the per-site default; P2 maps the host's tinct
/// palette to `SmolwebTheme::App`. (Smolweb host P1.)
#[cfg(feature = "smolweb")]
fn ensure_smolweb(content: &mut Content) -> bool {
    if !is_smolweb_lane(&content.url) {
        return false;
    }
    if content.smolweb.is_none() {
        let body = match &content.state {
            Some(ContentState::Ready(fetched)) => &fetched.body,
            // Not fetched yet: fall through to the loading card until Ready.
            _ => return false,
        };
        let theme = smolweb_app_theme(&content.sheet);
        content.smolweb = Some(pelt_desktop::SmolwebDocument::parse(
            &content.url,
            body,
            theme,
        ));
    }
    true
}

/// Map the host's theme-derived document colours (+ the card background) onto a
/// smolweb `App` palette, so a native capsule matches the app chrome instead of the
/// per-site default. Rebuilt on `Retheme` (the content actor clears `smolweb`).
/// (Smolweb host P2.)
#[cfg(feature = "smolweb")]
fn smolweb_app_theme(sheet: &document_canvas::DocumentStyleSheet) -> pelt_desktop::SmolwebTheme {
    let c = sheet.colors;
    let bg = crate::theme_sheets::CARD_BG;
    // A slightly lifted background for code/pre blocks so they read against `bg`.
    let pre = wgpu::Color {
        r: (bg.r + 0.05).min(1.0),
        g: (bg.g + 0.05).min(1.0),
        b: (bg.b + 0.05).min(1.0),
        a: 1.0,
    };
    pelt_desktop::SmolwebTheme::App(pelt_desktop::SmolwebPalette {
        bg: css_wgpu(bg),
        fg: css_rgba(c.body_text),
        link: css_rgba(c.link_text),
        quote: css_rgba(c.badge_text),
        pre_bg: css_wgpu(pre),
    })
}

/// An `rgb(...)` string from a linear `[r, g, b, a]` colour (alpha dropped).
#[cfg(feature = "smolweb")]
fn css_rgba(c: [f32; 4]) -> String {
    let ch = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("rgb({}, {}, {})", ch(c[0]), ch(c[1]), ch(c[2]))
}

/// An `rgb(...)` string from a `wgpu::Color` (alpha dropped).
#[cfg(feature = "smolweb")]
fn css_wgpu(c: wgpu::Color) -> String {
    let ch = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("rgb({}, {}, {})", ch(c.r), ch(c.g), ch(c.b))
}

/// Render `content` against the cached subresources, emitting the scene and any
/// subresources the render newly wants. The HTML/serval lane rides the retained
/// [`ContentLayout`] (cascade once, emit each band off it without re-cascading); the
/// document / synthesized lanes take the one-shot [`render_content`] path.
pub(crate) fn render(
    content: &mut Content,
    store: &RefCell<ResourceStore>,
    registry: &EngineRegistry,
    policy: &EngineRoutePolicy,
    out: &impl ContentUpdateSink,
) {
    let (w, h) = content.viewport;
    // Scripted render rung: a `serval.scripted` node renders straight from its live
    // `ScriptedDocument` (page JS already ran on load). `frame()` re-lays-out the
    // mutated DOM and paints; emit it as one viewport (banding/scroll of a scripted
    // tile is a follow-up — the document scrolls internally, not via host bands).
    // (Render ladder phase 2a.)
    #[cfg(feature = "scripted")]
    if let Some(doc) = content.scripted_doc.as_mut() {
        let scene = doc.frame(w, h);
        // The link-rect table a click resolves against (ConstellationOps::link_at),
        // read off the frame's retained cascade — the same mechanism the HTML/serval
        // lane and the smolweb lane use, no per-click query into the live DOM.
        let links = doc
            .links()
            .into_iter()
            .map(|(url, rect)| LinkHit { rect, url })
            .collect();
        emit_scene(
            out,
            content.nav,
            content.viewport_gen,
            scene,
            h,
            0,
            h,
            links,
            Vec::new(),
        );
        let dom = doc.dom_stats();
        let dom = DomArenaStats {
            live_nodes: dom.live_nodes,
            node_kinds: DomNodeKindStats {
                documents: dom.node_kinds.documents,
                document_fragments: dom.node_kinds.document_fragments,
                doctypes: dom.node_kinds.doctypes,
                elements: dom.node_kinds.elements,
                text: dom.node_kinds.text,
                comments: dom.node_kinds.comments,
                processing_instructions: dom.node_kinds.processing_instructions,
            },
            attribute_count: dom.attribute_count,
            estimated_bytes: dom.estimated_bytes,
        };
        let layout = doc
            .last_layout_batch_stats()
            .map(|layout| LayoutBatchStats {
                mutations_in: layout.mutations_in,
                coalesced_invalidations: layout.coalesced_invalidations,
                restyled_elements: layout.restyled_elements,
                boxes_rebuilt: layout.boxes_rebuilt,
                fragment_count: layout.fragment_count,
                box_tree_nodes: layout.box_tree_nodes,
                ..LayoutBatchStats::default()
            });
        emit_engine_stats(out, content.nav, content.viewport_gen, dom, layout);
        return;
    }
    // Scripted page (P2.5c): render from the script's mutable `ScriptedDom`, which
    // supersedes the static `html` path so the script's edits are live. Emits one band
    // off the script's retained layout, exactly like the static serval lane.
    if let Some(inst) = content.script.as_ref() {
        let scroll = ScrollOffsets::default();
        let (scene, masks, content_height, link_rects) = scene_from_content_band(
            inst.layout(),
            inst.dom(),
            h,
            content.band_y,
            content.band_h,
            &scroll,
        );
        let links = link_rects
            .into_iter()
            .map(|(url, rect)| LinkHit { rect, url })
            .collect();
        emit_scene(
            out,
            content.nav,
            content.viewport_gen,
            scene,
            content_height,
            content.band_y,
            content.band_h,
            links,
            masks,
        );
        let dom = inst.dom().stats();
        emit_engine_stats(
            out,
            content.nav,
            content.viewport_gen,
            DomArenaStats {
                live_nodes: dom.live_nodes,
                node_kinds: DomNodeKindStats {
                    documents: dom.node_kinds.documents,
                    document_fragments: dom.node_kinds.document_fragments,
                    doctypes: dom.node_kinds.doctypes,
                    elements: dom.node_kinds.elements,
                    text: dom.node_kinds.text,
                    comments: dom.node_kinds.comments,
                    processing_instructions: dom.node_kinds.processing_instructions,
                },
                attribute_count: dom.attribute_count,
                estimated_bytes: dom.estimated_bytes,
            },
            None,
        );
        return;
    }
    // Serval smolweb lane: a focused smolweb capsule (gemini/gopher/feed) renders
    // natively through `SmolwebDocument`. Like the scripted lane it scrolls internally,
    // so it emits one viewport, not host bands (host-band scroll is P3). Falls through
    // to the loading/document lane while the body is not yet ready. (Smolweb host P1.)
    #[cfg(feature = "smolweb")]
    if ensure_smolweb(content) {
        let (w, h) = content.viewport;
        if let Some(doc) = content.smolweb.as_mut() {
            // The host requests a band once it sees `content_height` exceed the
            // viewport (`constellation::request_scroll`); scroll to the requested
            // offset before framing, and echo it back so the host's band bookkeeping
            // matches (mirrors the HTML lane's `band_y`/`band_h`, though here the
            // "band" is always one viewport tall — the session scrolls, not the frame).
            // (Smolweb host P3.)
            let content_height = doc.content_height(w, h);
            doc.scroll_to(content.band_y as f32);
            let scene = doc.frame(w, h);
            // The link-rect table a click resolves against (ConstellationOps::link_at),
            // the same mechanism the HTML/serval lane's `harvest_link_rects` populates —
            // full-document px, unscrolled, cached host-side; no per-click round trip.
            // (Smolweb host P3b.)
            let links = doc
                .links()
                .into_iter()
                .map(|(url, rect)| LinkHit { rect, url })
                .collect();
            emit_scene(
                out,
                content.nav,
                content.viewport_gen,
                scene,
                content_height,
                content.band_y,
                h,
                links,
                Vec::new(),
            );
            return;
        }
    }
    let wanted = RefCell::new(Vec::new());
    if ensure_html_layout(content, store, registry, policy, &wanted) {
        // HTML/serval lane: emit this band off the retained layout, no re-cascade.
        let (doc, layout) = content
            .html
            .as_ref()
            .expect("ensure_html_layout returned true");
        let scroll = ScrollOffsets::default();
        let (scene, masks, content_height, link_rects) =
            scene_from_content_band(layout, doc, h, content.band_y, content.band_h, &scroll);
        let links = link_rects
            .into_iter()
            .map(|(url, rect)| LinkHit { rect, url })
            .collect();
        emit_scene(
            out,
            content.nav,
            content.viewport_gen,
            scene,
            content_height,
            // Echo the band this scene represents so the host composites it at the
            // right offset and knows when to request the next band.
            content.band_y,
            content.band_h,
            links,
            masks,
        );
    } else {
        // Document / synthesized lanes: the one-shot render_content path (the document lane
        // keeps its own retained packet; a synthesized page is cheap).
        let rendered = {
            let loader = ResourceLoader::new(store, &content.url, &wanted);
            render_content(
                &content.url,
                content.state.as_ref(),
                registry,
                policy,
                &loader,
                w,
                h,
                content.band_y,
                content.band_h,
                &content.sheet,
            )
        };
        match rendered {
            RenderedContent::Document {
                packet,
                fonts,
                content_height,
            } => out.emit_update(ContentUpdate::Document {
                nav: content.nav,
                viewport_gen: content.viewport_gen,
                packet,
                fonts,
                content_height,
            }),
            RenderedContent::Html {
                scene,
                content_height,
                links,
                masks,
            } => emit_scene(
                out,
                content.nav,
                content.viewport_gen,
                scene,
                content_height,
                content.band_y,
                content.band_h,
                links,
                masks,
            ),
        }
    }
    emit_fresh_wanted(content.nav, wanted, store, out);
}

/// Ship only never-requested subresources, so a re-render before the bytes arrive
/// does not re-request them (the store dedups). Shared by the static render path and
/// the scripted layout builds (attach / deliver / relayout). (Follow-on #3.)
pub(crate) fn emit_fresh_wanted(
    nav: NavGeneration,
    wanted: RefCell<Vec<String>>,
    store: &RefCell<ResourceStore>,
    out: &impl ContentUpdateSink,
) {
    let fresh: Vec<String> = wanted
        .into_inner()
        .into_iter()
        .filter(|url| store.borrow_mut().request(url.clone()))
        .collect();
    if !fresh.is_empty() {
        out.emit_update(ContentUpdate::Wanted { nav, urls: fresh });
    }
}

/// Re-lay-out the attached script's page at `(w, h)` and ship any newly-wanted
/// subresources — a resize (new viewport) or a newly-arrived subresource (re-decode).
/// No-op without an attached script. (Follow-on #3.)
pub(crate) fn relayout_script(
    content: &mut Content,
    store: &RefCell<ResourceStore>,
    out: &impl ContentUpdateSink,
    w: u32,
    h: u32,
) {
    if content.script.is_none() {
        return;
    }
    let url = content.url.clone();
    let wanted = RefCell::new(Vec::new());
    {
        let loader = ResourceLoader::new(store, &url, &wanted);
        content
            .script
            .as_mut()
            .expect("checked is_some")
            .relayout(&loader, w, h);
    }
    emit_fresh_wanted(content.nav, wanted, store, out);
}
