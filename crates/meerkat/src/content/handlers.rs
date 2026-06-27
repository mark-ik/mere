/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Per-event content handlers: script attach, event delivery, layout + render.

use super::*;

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
    out: &Emitter<ContentUpdate>,
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
    out: &Emitter<ContentUpdate>,
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

/// Render `content` against the cached subresources, emitting the scene and any
/// subresources the render newly wants. The HTML/serval lane rides the retained
/// [`ContentLayout`] (cascade once, emit each band off it without re-cascading); the
/// document / synthesized lanes take the one-shot [`render_content`] path.
pub(crate) fn render(
    content: &mut Content,
    store: &RefCell<ResourceStore>,
    registry: &EngineRegistry,
    policy: &EngineRoutePolicy,
    out: &Emitter<ContentUpdate>,
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
        out.emit(ContentUpdate::Scene {
            nav: content.nav,
            viewport_gen: content.viewport_gen,
            scene,
            content_height: h,
            masks: Vec::new(),
            links: Vec::new(),
            band_y: 0,
            band_h: h,
        });
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
        let links = link_rects.into_iter().map(|(url, rect)| LinkHit { rect, url }).collect();
        out.emit(ContentUpdate::Scene {
            nav: content.nav,
            viewport_gen: content.viewport_gen,
            scene,
            content_height,
            masks,
            links,
            band_y: content.band_y,
            band_h: content.band_h,
        });
        return;
    }
    let wanted = RefCell::new(Vec::new());
    if ensure_html_layout(content, store, registry, policy, &wanted) {
        // HTML/serval lane: emit this band off the retained layout, no re-cascade.
        let (doc, layout) = content.html.as_ref().expect("ensure_html_layout returned true");
        let scroll = ScrollOffsets::default();
        let (scene, masks, content_height, link_rects) =
            scene_from_content_band(layout, doc, h, content.band_y, content.band_h, &scroll);
        let links = link_rects
            .into_iter()
            .map(|(url, rect)| LinkHit { rect, url })
            .collect();
        out.emit(ContentUpdate::Scene {
            nav: content.nav,
            viewport_gen: content.viewport_gen,
            scene,
            content_height,
            masks,
            links,
            // Echo the band this scene represents so the host composites it at the
            // right offset and knows when to request the next band.
            band_y: content.band_y,
            band_h: content.band_h,
        });
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
            } => out.emit(ContentUpdate::Document {
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
            } => out.emit(ContentUpdate::Scene {
                nav: content.nav,
                viewport_gen: content.viewport_gen,
                scene,
                content_height,
                masks,
                links,
                band_y: content.band_y,
                band_h: content.band_h,
            }),
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
    out: &Emitter<ContentUpdate>,
) {
    let fresh: Vec<String> = wanted
        .into_inner()
        .into_iter()
        .filter(|url| store.borrow_mut().request(url.clone()))
        .collect();
    if !fresh.is_empty() {
        out.emit(ContentUpdate::Wanted { nav, urls: fresh });
    }
}

/// Re-lay-out the attached script's page at `(w, h)` and ship any newly-wanted
/// subresources — a resize (new viewport) or a newly-arrived subresource (re-decode).
/// No-op without an attached script. (Follow-on #3.)
pub(crate) fn relayout_script(
    content: &mut Content,
    store: &RefCell<ResourceStore>,
    out: &Emitter<ContentUpdate>,
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
        content.script.as_mut().expect("checked is_some").relayout(&loader, w, h);
    }
    emit_fresh_wanted(content.nav, wanted, store, out);
}
