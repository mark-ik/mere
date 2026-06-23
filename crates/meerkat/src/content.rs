/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The content actor: a focused tile's media rendered off the UI thread.
//!
//! P2 of the actor constellation plan. The kernel ships a tile's fetched document
//! to a content actor; the actor owns the serval cascade + nematic engines + a
//! per-tile subresource cache on its own thread (the cascade is confirmed
//! off-thread-safe, see the `cascade-offthread` probe), runs the existing
//! [`render_content_scene`](crate::card::render_content_scene) there, and ships a
//! `Send` [`Scene`] back. The kernel composites the latest scene and stays the sole
//! GPU owner.
//!
//! Three update kinds cross the boundary, all `Send`: the rendered [`Scene`], the
//! subresource URLs the render [`Wanted`](ContentUpdate::Wanted) (the kernel
//! fetches them through the I/O fetch actor and feeds the bytes back as
//! [`Resource`](ContentCommand::Resource)), and any linked-data
//! [`Contribution`](ContentUpdate::Contribution) harvested from the document (the
//! kernel applies it to the graph). The actor never touches the graph or the GPU.

use std::cell::RefCell;

use armillary::{ActorHandle, Emitter, NavGeneration, Pool, ViewportGeneration, Wake, spawn_on};
use document_canvas::{DocumentRenderPacket, DocumentStyleSheet, FontTable};
use inker::{EngineRegistry, EngineRoutePolicy};
use linked_data::GraphContribution;
use netrender::Scene;

use serval_layout::{ContentLayout, ScrollOffsets};
use serval_static_dom::{StaticDocument, StaticNodeId};

use crate::card::{
    build_html_layout, is_serval_html_lane, render_content, LinkHit, RenderedContent,
};
use crate::fetch::ContentState;
use crate::resources::{ResourceLoader, ResourceStore};
use crate::serval_render::scene_from_content_band;

pub(crate) mod script;
use std::path::{Path, PathBuf};

use document_host::{Grant, Quota};
use kernel::permissions::ResolvedPermission;
use script::ScriptInstance;

/// A command from the kernel to a content actor.
pub enum ContentCommand {
    /// Show `fetched` at `url` (a fresh navigation): harvest its linked data once,
    /// then render at `viewport`. The generations tag the work the kernel gets
    /// back so it can drop a scene built for a page/size the tile has left.
    Show {
        url: String,
        /// The focused card's content state (`None` for a synthesized page such as
        /// `mere://welcome`); the actor renders any state, harvesting only `Ready`.
        state: Option<ContentState>,
        viewport: (u32, u32),
        nav: NavGeneration,
        viewport_gen: ViewportGeneration,
        /// The composed document style sheet — user typography (size, line
        /// spacing, fonts, link adornment) with theme-derived colours; the actor
        /// lays the document out with it, baking the colours onto each glyph run.
        /// (Document theming P3; typography surface D1.)
        sheet: DocumentStyleSheet,
    },
    /// Re-render the current document at a new size.
    Resize {
        viewport: (u32, u32),
        viewport_gen: ViewportGeneration,
    },
    /// Re-render the current document with a new style sheet — a live theme or
    /// typography change, without a navigation or resize. Carries a bumped
    /// `viewport_gen` so the re-rendered packet clears the generation gate.
    /// (Document theming P3; typography surface D1.)
    Retheme {
        sheet: DocumentStyleSheet,
        viewport_gen: ViewportGeneration,
    },
    /// A subresource the kernel fetched on the actor's behalf has arrived: cache
    /// its bytes and re-render so the demand loader now hits.
    Resource { url: String, bytes: Vec<u8> },
    /// Re-emit the current HTML/serval document at a new scroll band (the host's
    /// windowing of the flat serval scene). `band_y` is the document scroll offset,
    /// `band_h` the band height; the actor emits only that band so a tall dense page
    /// does not overflow the GPU. The document lane never receives this. (HTML scroll.)
    Scroll {
        band_y: u32,
        band_h: u32,
        viewport_gen: ViewportGeneration,
    },
    /// Find `query` in the current HTML/serval document (find-in-page). The actor runs
    /// the search where its layout lives and ships the match rects back; the HTML lane
    /// has no host-queryable packet. An empty query clears the matches. (Find-in-page.)
    Find {
        query: String,
        viewport_gen: ViewportGeneration,
    },
    /// Attach a DocumentScript (a wasm `document-core` component) to this tile's page.
    /// The page is mirrored into a mutable `ScriptedDom` the script can edit, and the
    /// tile renders from it thereafter. HTML/serval lane only. `log` / `document` are
    /// the host-resolved permissions for the two application capabilities (the host
    /// runs the five-scope `resolve_permission`; the actor maps them to the link
    /// grant). (P2.5c, DocumentScript; §11.4 permissions seam.)
    AttachScript {
        component_path: PathBuf,
        log: ResolvedPermission,
        document: ResolvedPermission,
        viewport_gen: ViewportGeneration,
    },
    /// Deliver one event to the attached DocumentScript; the script's batch is applied
    /// to the live `ScriptedDom` and the tile re-renders. No-op if no script is
    /// attached. (P2.5c, DocumentScript.)
    DeliverEvent {
        kind: String,
        payload: String,
        viewport_gen: ViewportGeneration,
    },
    /// Detach the DocumentScript (runs its `deactivate`) and revert to the static page.
    /// (P2.5c, DocumentScript.)
    DetachScript {
        viewport_gen: ViewportGeneration,
    },
}

/// An update from a content actor to the kernel. All variants are `Send`.
pub enum ContentUpdate {
    /// A document-lane render: the **retained packet** plus its font sidecar, the
    /// host windows + lowers a band of per scroll. `content_height` is the full
    /// laid-out height (px); the host scrolls the full extent and rasterizes one band
    /// at a time, so a tall page is never one giant texture. (Tiled render.)
    Document {
        nav: NavGeneration,
        viewport_gen: ViewportGeneration,
        packet: DocumentRenderPacket,
        fonts: FontTable,
        content_height: u32,
        // Link hit-testing reads the packet's own interactions
        // (`DocumentRenderPacket::link_at`), so the document lane ships no separate
        // link-rect table. (Phase 2 query API.)
    },
    /// An HTML/serval-lane render: one pre-lowered scene for a vertical BAND of the
    /// page. `content_height` is the full laid-out height; `band_y` / `band_h` are the
    /// band this scene covers (the page scrolled to `band_y`, `band_h` tall). The host
    /// composites it at that offset and requests the next band as the scroll moves
    /// (its windowing of a flat serval scene the actor emits one band of). (HTML scroll.)
    Scene {
        nav: NavGeneration,
        viewport_gen: ViewportGeneration,
        scene: Scene,
        content_height: u32,
        band_y: u32,
        band_h: u32,
        /// Content-local clickable link regions harvested from the laid-out
        /// document; the host hit-tests a click against these and navigates.
        links: Vec<LinkHit>,
        /// Blurred box-shadow mask requests the host builds (GPU) and registers
        /// before rasterizing `scene`. Empty when the page has no blurred shadows.
        masks: Vec<paint_list_render::BoxShadowMaskRequest>,
    },
    /// Subresource URLs (absolute) the last render needs but did not have cached.
    /// The kernel fetches them and feeds the bytes back as [`ContentCommand::Resource`].
    Wanted {
        nav: NavGeneration,
        urls: Vec<String>,
    },
    /// Linked data harvested from the document, for the kernel to apply.
    Contribution {
        contributions: Vec<GraphContribution>,
    },
    /// Find-in-page match rects for the current HTML document: one inner `Vec` per
    /// match (a wrapped match spans lines), in full-document px (`[x0, y0, x1, y1]`,
    /// unscrolled, the same space as the link rects). The host highlights these and
    /// scrolls to the active one. Empty when the query cleared or nothing matched.
    /// (Find-in-page.)
    FindMatches {
        nav: NavGeneration,
        viewport_gen: ViewportGeneration,
        matches: Vec<Vec<[f32; 4]>>,
    },
    /// The result of a DocumentScript attach / turn / detach, for the host to surface
    /// (diagnostics / a script console). The re-render rides the `Scene` update; this
    /// is the textual outcome alongside it. (P2.5c, DocumentScript.)
    ScriptOutcome {
        nav: NavGeneration,
        outcome: String,
    },
}

/// The actor-thread-local current document.
struct Content {
    url: String,
    state: Option<ContentState>,
    viewport: (u32, u32),
    nav: NavGeneration,
    viewport_gen: ViewportGeneration,
    /// The vertical band of the HTML/serval lane to emit: `band_y` is the document
    /// scroll offset, `band_h` the band height. The host requests bands as the scroll
    /// moves (its windowing of a flat serval scene, done here because only the actor
    /// holds the layout). Ignored by the document lane (the host windows its packet).
    band_y: u32,
    band_h: u32,
    /// The composed document style sheet (user typography + theme-derived
    /// colours). Kept across renders so a Resize / Scroll / Resource re-render
    /// reuses it; a `Retheme` swaps it. (Document theming P3; typography D1.)
    sheet: DocumentStyleSheet,
    /// The retained HTML/serval-lane layout: the parsed document plus its cascaded
    /// [`ContentLayout`], built ONCE and re-emitted per scroll band / find keystroke so a
    /// re-band does not re-cascade (slice 1, the single biggest per-frame content cost).
    /// `None` for the document / synthesized lanes (which keep their own retained packet),
    /// and cleared by the arms that change the body / viewport / subresources (Show builds a
    /// fresh `Content`; Resize / Resource set it back to `None`), so a present layout is
    /// fresh. A `Retheme` keeps it (the serval lane themes through HTML_SHEET + the page CSS,
    /// not the document sheet). Built lazily by [`ensure_html_layout`].
    html: Option<(StaticDocument, ContentLayout<StaticNodeId>)>,
    /// An attached DocumentScript (P2.5c). When present, the tile renders from the
    /// script's mutable `ScriptedDom` (it supersedes the static `html` path), so the
    /// script's edits are live; cleared on a fresh `Show` and by `DetachScript`.
    script: Option<ScriptInstance>,
}

/// Spawn a content actor on its own thread (armillary harness). It builds the
/// nematic engine registry and an empty subresource cache on that thread (neither
/// crosses the boundary), then renders on each command. Returns the kernel's
/// command handle plus the receiver of [`ContentUpdate`]s to drain.
pub fn spawn_content(
    pool: &Pool,
    wake: Wake,
    disabled: std::collections::HashSet<String>,
    auto_ingest: bool,
) -> (
    ActorHandle<ContentCommand>,
    std::sync::mpsc::Receiver<ContentUpdate>,
) {
    spawn_on(pool, wake, move |commands, out: Emitter<ContentUpdate>| {
        let mut registry = EngineRegistry::new();
        for engine in nematic::engines() {
            // Skip engines deactivated this session: an unregistered engine is
            // routed past by the policy (`route_document_engine`), so its content
            // falls to the synthesized card. (engine-picker Phase 1b.)
            if !disabled.contains(engine.engine_id()) {
                registry.register(engine);
            }
        }
        // The actor's own copy of the default routing policy (it owns its registry
        // too); content-type routing for this node's document runs against it.
        let policy = EngineRoutePolicy::default();
        let store = RefCell::new(ResourceStore::default());
        let mut current: Option<Content> = None;

        while let Ok(command) = commands.recv() {
            match command {
                ContentCommand::Show {
                    url,
                    state,
                    viewport,
                    nav,
                    viewport_gen,
                    sheet,
                } => {
                    // Harvest the document's linked data once, on load (Ready only).
                    // Off by default: auto-ingesting every page's embedded JSON-LD/RDFa
                    // floods the graph with the page's own entities (e.g. a Wikipedia
                    // article's structured data on each visit), so this is opt-in. The
                    // host passes the flag; default false. (Linked-data ingest.)
                    if auto_ingest {
                        if let Some(ContentState::Ready(fetched)) = &state {
                            let contributions = meerkat::ingest::harvest_contributions(
                                fetched.content_type.as_deref(),
                                &fetched.body,
                            );
                            if !contributions.is_empty() {
                                out.emit(ContentUpdate::Contribution { contributions });
                            }
                        }
                    }
                    // A fresh navigation resets the band to the top, one viewport tall
                    // (the host requests a taller scroll band once it has the content).
                    let mut content = Content {
                        url,
                        state,
                        viewport,
                        nav,
                        viewport_gen,
                        band_y: 0,
                        band_h: viewport.1,
                        sheet,
                        html: None,
                        script: None,
                    };
                    render(&mut content, &store, &registry, &policy, &out);
                    current = Some(content);
                }
                ContentCommand::Resize {
                    viewport,
                    viewport_gen,
                } => {
                    if let Some(content) = current.as_mut() {
                        content.viewport = viewport;
                        content.viewport_gen = viewport_gen;
                        content.band_y = 0; // a resize relays out; re-anchor the band
                        content.html = None; // the viewport changed; rebuild the retained layout
                        // The scripted lane re-lays-out its own retained layout at the
                        // new viewport (the static lane rebuilds lazily in render). (#3.)
                        relayout_script(content, &store, &out, viewport.0, viewport.1);
                        render(content, &store, &registry, &policy, &out);
                    }
                }
                ContentCommand::Retheme {
                    sheet,
                    viewport_gen,
                } => {
                    if let Some(content) = current.as_mut() {
                        content.sheet = sheet;
                        // A bumped gen so the re-baked packet clears the generation gate.
                        content.viewport_gen = viewport_gen;
                        render(content, &store, &registry, &policy, &out);
                    }
                }
                ContentCommand::Resource { url, bytes } => {
                    store.borrow_mut().insert(url, bytes);
                    if let Some(content) = current.as_mut() {
                        content.html = None; // a subresource arrived; rebuild so images decode
                        // The scripted lane re-lays-out so the newly-arrived bytes decode
                        // into its retained layout too (same viewport). (#3.)
                        let (w, h) = content.viewport;
                        relayout_script(content, &store, &out, w, h);
                        render(content, &store, &registry, &policy, &out);
                    }
                }
                ContentCommand::Scroll {
                    band_y,
                    band_h,
                    viewport_gen,
                } => {
                    if let Some(content) = current.as_mut() {
                        content.band_y = band_y;
                        content.band_h = band_h;
                        content.viewport_gen = viewport_gen;
                        render(content, &store, &registry, &policy, &out);
                    }
                }
                ContentCommand::Find {
                    query,
                    viewport_gen,
                } => {
                    if let Some(content) = current.as_mut() {
                        // Find off the retained serval layout (no re-cascade per keystroke). A
                        // render runs before find in practice and builds the layout; if a find
                        // lands first, `ensure_html_layout` builds it. Non-HTML lanes find
                        // nothing here (their find rides the retained packet, a separate path).
                        let wanted = RefCell::new(Vec::new());
                        let matches =
                            if ensure_html_layout(content, &store, &registry, &policy, &wanted) {
                                let (doc, layout) = content.html.as_ref().expect("ensured");
                                layout.find(doc, &query)
                            } else {
                                Vec::new()
                            };
                        out.emit(ContentUpdate::FindMatches {
                            nav: content.nav,
                            viewport_gen,
                            matches,
                        });
                    }
                }
                ContentCommand::AttachScript {
                    component_path,
                    log,
                    document,
                    viewport_gen,
                } => {
                    if let Some(content) = current.as_mut() {
                        content.viewport_gen = viewport_gen;
                        // Map the host-resolved permissions to the link grant (§11.4).
                        let grant = script::grant_from_resolved(log, document);
                        let outcome = attach_script(
                            content,
                            &component_path,
                            &grant,
                            &store,
                            &registry,
                            &policy,
                            &out,
                        );
                        out.emit(ContentUpdate::ScriptOutcome { nav: content.nav, outcome });
                        // Render from the script's DOM once attached.
                        if content.script.is_some() {
                            render(content, &store, &registry, &policy, &out);
                        }
                    }
                }
                ContentCommand::DeliverEvent {
                    kind,
                    payload,
                    viewport_gen,
                } => {
                    if let Some(content) = current.as_mut() {
                        if content.script.is_some() {
                            content.viewport_gen = viewport_gen;
                            let outcome = deliver_event(content, &kind, &payload, &store, &out);
                            out.emit(ContentUpdate::ScriptOutcome { nav: content.nav, outcome });
                            render(content, &store, &registry, &policy, &out);
                        }
                    }
                }
                ContentCommand::DetachScript { viewport_gen } => {
                    if let Some(content) = current.as_mut() {
                        let outcome = match content.script.take() {
                            Some(inst) => match inst.detach() {
                                Ok(_) => "detached".to_string(),
                                Err(e) => format!("detach error: {e}"),
                            },
                            None => "no script attached".to_string(),
                        };
                        content.html = None; // revert to the static page path
                        content.viewport_gen = viewport_gen;
                        out.emit(ContentUpdate::ScriptOutcome { nav: content.nav, outcome });
                        render(content, &store, &registry, &policy, &out);
                    }
                }
            }
        }
    })
}

/// Mirror the current HTML page into a `ScriptedDom` and attach the DocumentScript at
/// `component_path` over it under `grant` (P2.5c). HTML/serval lane only; returns a
/// human-readable outcome for the `ScriptOutcome` update. A `grant` that denies a
/// capability the component requires makes instantiation fail (reported as "attach
/// failed") — the runtime-enforced capability boundary.
fn attach_script(
    content: &mut Content,
    component_path: &Path,
    grant: &Grant,
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
    let wanted = RefCell::new(Vec::new());
    let outcome = {
        let loader = ResourceLoader::new(store, &url, &wanted);
        match ScriptInstance::attach(component_path, &body, &loader, w, h, grant, Quota::default()) {
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
fn deliver_event(
    content: &mut Content,
    kind: &str,
    payload: &str,
    store: &RefCell<ResourceStore>,
    out: &Emitter<ContentUpdate>,
) -> String {
    let url = content.url.clone();
    let wanted = RefCell::new(Vec::new());
    let outcome = {
        let loader = ResourceLoader::new(store, &url, &wanted);
        let Some(inst) = content.script.as_mut() else {
            return "no script attached".to_string();
        };
        match inst.deliver(kind, payload, &loader) {
            Ok(outcome) => format!("{outcome:?}"),
            Err(e) => format!("turn error: {e}"),
        }
    };
    // A script mutation may have re-laid-out (new nodes / images) and wanted new
    // subresources; ship them. (#3.)
    emit_fresh_wanted(content.nav, wanted, store, out);
    outcome
}

/// Build the retained serval-lane [`ContentLayout`] into `content.html` if this is the
/// HTML/serval lane and the cache is empty (recording subresource wants through `wanted`),
/// and return whether the HTML cache is now present. The arms clear `content.html` on a
/// body / viewport / subresource change, so a present layout is fresh. (Slice 1.)
fn ensure_html_layout(
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
fn render(
    content: &mut Content,
    store: &RefCell<ResourceStore>,
    registry: &EngineRegistry,
    policy: &EngineRoutePolicy,
    out: &Emitter<ContentUpdate>,
) {
    let (w, h) = content.viewport;
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
fn emit_fresh_wanted(
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
fn relayout_script(
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::fetch::Fetched;

    fn noop_wake() -> Wake {
        Arc::new(|| {})
    }

    fn show(url: &str, content_type: &str, body: &str) -> ContentCommand {
        ContentCommand::Show {
            url: url.to_string(),
            state: Some(ContentState::Ready(Fetched {
                content_type: Some(content_type.to_string()),
                body: body.to_string(),
            })),
            viewport: (420, 360),
            nav: NavGeneration::default(),
            viewport_gen: ViewportGeneration::default(),
            sheet: DocumentStyleSheet::default(),
        }
    }

    fn glyph_runs(scene: &Scene) -> usize {
        scene
            .ops
            .iter()
            .filter(|op| matches!(op, netrender::SceneOp::GlyphRun(_)))
            .count()
    }

    #[test]
    fn show_renders_a_scene_off_thread() {
        let (handle, updates) =
            spawn_content(&Pool::new(), noop_wake(), std::collections::HashSet::new(), false);
        handle.command(show(
            "https://example.com/",
            "text/html",
            "<h1>Hi</h1><p>There</p>",
        ));
        handle.join();

        let scene = updates
            .iter()
            .find_map(|u| match u {
                ContentUpdate::Scene { scene, .. } => Some(scene),
                _ => None,
            })
            .expect("a scene update");
        assert!(
            glyph_runs(&scene) >= 1,
            "the off-thread render lowered text to glyph runs"
        );
    }

    #[test]
    fn show_harvests_embedded_jsonld_into_a_contribution() {
        let (handle, updates) =
            spawn_content(&Pool::new(), noop_wake(), std::collections::HashSet::new(), true);
        handle.command(show(
            "https://example.com/",
            "text/html",
            r#"<script type="application/ld+json">
               {"@context":{"name":"https://schema.org/name"},"@id":"mere://z","name":"Z"}
               </script><p>body</p>"#,
        ));
        handle.join();

        let harvested = updates.iter().any(|u| match u {
            ContentUpdate::Contribution { contributions } => contributions
                .iter()
                .any(|c| c.nodes.iter().any(|n| n.id == "mere://z")),
            _ => false,
        });
        assert!(harvested, "embedded JSON-LD harvested into a Contribution");
    }
}
