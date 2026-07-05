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
use std::sync::mpsc::{Receiver, TryRecvError};

use armillary::{ActorHandle, Emitter, NavGeneration, Pool, ViewportGeneration, Wake, spawn_on};
use document_canvas::{DocumentRenderPacket, DocumentStyleSheet, FontTable};
use inker::{EngineRegistry, EngineRoutePolicy};
use linked_data::GraphContribution;
use netrender::Scene;

use serval_layout::{ContentLayout, ScrollOffsets};
use serval_static_dom::{StaticDocument, StaticNodeId};

pub(crate) use content_contract::{
    ContentSceneStats, DomArenaStatsMessage as DomArenaStats,
    DomNodeKindStatsMessage as DomNodeKindStats, LayoutApplyKindMessage as LayoutApplyKind,
    LayoutBatchStatsMessage as LayoutBatchStats, LayoutDamageClassMessage as LayoutDamageClass,
    SceneTransferDecoder, SceneTransferEncoder, TextSelectionMessage, TransferBuffer,
    TransferError, scene_stats,
};

// The scripted render rung (render ladder phase 2): a scripted Serval node runs its
// page JS through pelt's `ScriptedDocument`. Boa is the base engine; `scripted-nova`
// adds the Nova variant under a second host-visible engine id. Behind the `scripted`
// feature so the base build links no JS engine (the ladder's witness discipline).
#[cfg(feature = "scripted")]
use pelt_desktop::ScriptedDocument;
#[cfg(feature = "scripted")]
use script_engine_boa::BoaEngine;
#[cfg(feature = "scripted-nova")]
use script_engine_nova::NovaEngine;

use crate::card::{
    LinkHit, RenderedContent, build_html_layout, is_serval_html_lane, render_content,
};
use crate::fetch::ContentState;
use crate::resources::{ResourceLoader, ResourceStore};
use crate::serval_render::scene_from_content_band;

pub(crate) mod script;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use document_host::{Grant, NetFetcher, Quota};
use kernel::permissions::ResolvedPermission;
use script::ScriptInstance;

#[cfg(feature = "scripted")]
pub(crate) enum HostScriptedDocument {
    Boa(ScriptedDocument<BoaEngine>),
    #[cfg(feature = "scripted-nova")]
    Nova(ScriptedDocument<NovaEngine>),
}

#[cfg(feature = "scripted")]
impl HostScriptedDocument {
    fn frame(&mut self, width: u32, height: u32) -> Scene {
        match self {
            Self::Boa(doc) => doc.frame(width, height),
            #[cfg(feature = "scripted-nova")]
            Self::Nova(doc) => doc.frame(width, height),
        }
    }

    fn links(&self) -> Vec<(String, [f32; 4])> {
        match self {
            Self::Boa(doc) => doc.links(),
            #[cfg(feature = "scripted-nova")]
            Self::Nova(doc) => doc.links(),
        }
    }

    fn click_at(&mut self, x: f32, y: f32) -> bool {
        match self {
            Self::Boa(doc) => doc.click_at(x, y),
            #[cfg(feature = "scripted-nova")]
            Self::Nova(doc) => doc.click_at(x, y),
        }
    }

    fn dom_stats(&self) -> engine_observables_api::DomArenaStats {
        match self {
            Self::Boa(doc) => doc.dom_stats(),
            #[cfg(feature = "scripted-nova")]
            Self::Nova(doc) => doc.dom_stats(),
        }
    }

    fn last_layout_batch_stats(&self) -> Option<engine_observables_api::LayoutBatchStats> {
        match self {
            Self::Boa(doc) => doc.last_layout_batch_stats(),
            #[cfg(feature = "scripted-nova")]
            Self::Nova(doc) => doc.last_layout_batch_stats(),
        }
    }

    fn extract(&self) -> pelt_desktop::PageExtract {
        match self {
            Self::Boa(doc) => doc.extract(),
            #[cfg(feature = "scripted-nova")]
            Self::Nova(doc) => doc.extract(),
        }
    }
}

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
        /// The host-routed engine id for this node (its pin or the policy decision).
        /// The actor renders the static serval lane for `serval.web`; the `scripted`
        /// feature additionally drives `ScriptedDocument` for the scripted Serval
        /// engine ids. Other ids fall to the content-type routing the actor runs
        /// itself. (Render ladder.)
        engine: String,
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
    /// Page Visibility / Page Lifecycle for the card this actor renders (W3C
    /// adoption plan P1). `hidden` throttles the scripted document's timer
    /// pump to the spec clamp and fires `visibilitychange`; `frozen` stops
    /// tasks entirely (`freeze`/`resume` events). No-op for non-scripted
    /// lanes. Becoming visible re-renders once so a stale band refreshes.
    SetLifecycle { hidden: bool, frozen: bool },
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
    /// Re-point the active find match (Enter / Shift+Enter stepping): the actor
    /// re-registers the `find-active` engine highlight over the `index`-th match
    /// from the last `Find` and re-emits the band, so the stronger tint paints
    /// engine-side with the content. No-op when no find ranges are held.
    /// (Overlay-roots P2 — find highlights via the custom-highlight registry.)
    FindActive {
        index: usize,
        viewport_gen: ViewportGeneration,
    },
    /// Resolve a point-drag text selection in the current HTML/serval document. The
    /// points are content-local document coords in device px (the host subtracts the
    /// card origin and adds scroll before calling); the actor maps them through its
    /// retained layout and ships back rects + plain text for copy. No-op on the
    /// document lane. (HTML page selection.)
    SelectText {
        anchor: (f32, f32),
        focus: (f32, f32),
        viewport_gen: ViewportGeneration,
    },
    /// Attach a DocumentScript (a wasm `document-core` component) to this tile's page.
    /// The page is mirrored into a mutable `ScriptedDom` the script can edit, and the
    /// tile renders from it thereafter. HTML/serval lane only. `log` / `document` are
    /// the host-resolved permissions for the application capabilities (`log` /
    /// `document` / `net`; the host runs the five-scope `resolve_permission`, the actor
    /// maps them to the link grant). A script attaches only where the caps it imports
    /// resolved to Allow (so `net` egress needs an explicit grant). (P2.5c +
    /// net-permission loop; §11.4 permissions seam.)
    AttachScript {
        component_path: PathBuf,
        log: ResolvedPermission,
        document: ResolvedPermission,
        net: ResolvedPermission,
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
    DetachScript { viewport_gen: ViewportGeneration },
    /// Materialize the current page's outbound-link neighborhood as graph nodes +
    /// `Semantic:Hyperlink` edges (relational-browse V1), emitted as a Contribution.
    /// A render-free parse of the already-fetched body — no target fetch, no new
    /// actor. No-op if the node has no fetched HTML body. (Extraction lane / crawl
    /// frontier, single-hop.)
    MaterializeLinks { viewport_gen: ViewportGeneration },
    /// Forward a pointer click at card-local scene point `(x, y)` (device px) to the
    /// scripted render rung: the live `ScriptedDocument` hit-tests the point and
    /// dispatches a `click` (so the page's listeners run and may mutate the DOM),
    /// then re-renders. No-op for a node not on the scripted rung. The input → event
    /// bridge that makes the scripted rung interactive. (Render ladder phase 3.)
    #[cfg(feature = "scripted")]
    ScriptedClick {
        x: f32,
        y: f32,
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
        stats: ContentSceneStats,
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
    /// The current HTML page-text selection for a drag query, if any: full-document
    /// rects in device px plus plain text for copy. `None` when either endpoint
    /// misses laid-out text or the range collapses. (HTML page selection.)
    TextSelection {
        nav: NavGeneration,
        viewport_gen: ViewportGeneration,
        selection: Option<TextSelectionMessage>,
    },
    /// Focused-document engine observables for the current render lane, when the
    /// actor owns a real Serval DOM/layout surface.
    EngineStats {
        nav: NavGeneration,
        viewport_gen: ViewportGeneration,
        dom: DomArenaStats,
        layout: Option<LayoutBatchStats>,
    },
    /// The result of a DocumentScript attach / turn / detach, for the host to surface
    /// (diagnostics / a script console). The re-render rides the `Scene` update; this
    /// is the textual outcome alongside it. (P2.5c, DocumentScript.)
    ScriptOutcome { nav: NavGeneration, outcome: String },
    /// The transfer transport could not encode or deliver a normal content update.
    /// This is emitted as a best-effort explicit diagnostic instead of silently
    /// dropping the failed update. Native transport normally never emits it.
    TransportError { reason: String },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContentEngineStats {
    pub dom: DomArenaStats,
    pub layout: Option<LayoutBatchStats>,
}

pub(crate) trait ContentUpdateSink {
    fn emit_update(&self, update: ContentUpdate);
}

pub(crate) enum ContentHandle {
    Native(ActorHandle<ContentCommand>),
    #[cfg(target_arch = "wasm32")]
    Worker(worker::WorkerContentHandle),
}

impl ContentHandle {
    pub(crate) fn command(&self, command: ContentCommand) -> bool {
        match self {
            Self::Native(handle) => handle.command(command),
            #[cfg(target_arch = "wasm32")]
            Self::Worker(handle) => handle.command(command),
        }
    }
}

impl ContentUpdateSink for Emitter<ContentUpdate> {
    fn emit_update(&self, update: ContentUpdate) {
        self.emit(update);
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContentUpdateTransport {
    Native,
    Transfer,
}

#[allow(dead_code)]
pub(crate) enum ContentUpdateStream {
    Native(Receiver<ContentUpdate>),
    Transfer {
        updates: Receiver<TransferBuffer>,
        decoder: SceneTransferDecoder,
    },
}

#[allow(dead_code)]
pub(crate) enum ContentUpdatePoll {
    Update(ContentUpdate),
    Empty,
    Disconnected,
}

#[allow(dead_code)]
impl ContentUpdateStream {
    pub(crate) fn try_recv_update(&mut self) -> Result<ContentUpdatePoll, TransferError> {
        match self {
            Self::Native(updates) => match updates.try_recv() {
                Ok(update) => Ok(ContentUpdatePoll::Update(update)),
                Err(TryRecvError::Empty) => Ok(ContentUpdatePoll::Empty),
                Err(TryRecvError::Disconnected) => Ok(ContentUpdatePoll::Disconnected),
            },
            Self::Transfer { updates, decoder } => match updates.try_recv() {
                Ok(update) => ContentUpdate::from_transfer_buffer(update.as_bytes(), decoder)
                    .map(ContentUpdatePoll::Update),
                Err(TryRecvError::Empty) => Ok(ContentUpdatePoll::Empty),
                Err(TryRecvError::Disconnected) => Ok(ContentUpdatePoll::Disconnected),
            },
        }
    }
}

/// The actor-thread-local current document.
pub(crate) struct Content {
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
    /// The last `Find`'s match ranges (HTML/serval lane), retained so
    /// `FindActive` can re-register the active-match engine highlight without
    /// re-searching. Cleared with `html` (a fresh layout invalidates leaf ids)
    /// and on an empty query. (Overlay-roots P2.)
    find_ranges: Vec<serval_layout::HighlightRange<StaticNodeId>>,
    /// An attached DocumentScript (P2.5c). When present, the tile renders from the
    /// script's mutable `ScriptedDom` (it supersedes the static `html` path), so the
    /// script's edits are live; cleared on a fresh `Show` and by `DetachScript`.
    script: Option<ScriptInstance>,
    /// The scripted render rung: a live `ScriptedDocument` whose page JS ran on load
    /// and whose mutated DOM renders each frame. `Some` only for a scripted Serval
    /// rung (`serval.scripted`, or `serval.scripted.nova` when that feature is on),
    /// built on `Show` and only in the `scripted` build. Supersedes every other lane
    /// in `render`. (Render ladder phase 2a.)
    #[cfg(feature = "scripted")]
    scripted_doc: Option<HostScriptedDocument>,
    /// The serval smolweb lane: a focused smolweb capsule (gemini/gopher/feed) rendered
    /// natively through pelt's `SmolwebDocument` (errand parse -> smolweb-views ->
    /// ScriptedDom -> serval-layout). `Some` once a smolweb-scheme node's body is ready
    /// (built lazily in `render`). It scrolls internally (like the scripted lane), so it
    /// emits one viewport, not host bands; `frame(w, h)` re-lays-out on a viewport change,
    /// so a resize needs no explicit clear. (Smolweb host lane P1.)
    #[cfg(feature = "smolweb")]
    smolweb: Option<pelt_desktop::SmolwebDocument>,
    /// Fingerprint of the last band scene shipped (gens + band + content
    /// height + the serialized scene), so a re-render that converges to an
    /// IDENTICAL band is not re-shipped: no version bump host-side, no wake,
    /// no re-raster. The scripted Wikipedia card re-emitted an unchanged band
    /// every ~7s cycle forever, costing 90-260ms of raster each (shell paint
    /// plan, focused-card churn). Real changes always differ and ship.
    last_scene_sig: Option<u64>,
}

/// The scripted rung's external `<script src>` fetcher
/// ([`pelt_desktop::ScriptResourceFetcher`], the byte seam `ScriptedDocument::from_body`
/// takes — not `pelt_core::ResourceFetcher`, a distinct shell-level trait), over the
/// content actor's blocking [`script::ContentNetFetcher`] (a tokio `block_on` of the
/// routing fetch — so scripts ride the session jar and the same SSRF / scheme floors as
/// `net.fetch`). One per scripted document; built on `Show`. (Render ladder 2b.)
#[cfg(feature = "scripted")]
struct ScriptFetcher(script::ContentNetFetcher);

#[cfg(feature = "scripted")]
impl ScriptFetcher {
    fn new() -> Option<Self> {
        script::ContentNetFetcher::new().ok().map(Self)
    }
}

#[cfg(feature = "scripted")]
impl pelt_desktop::ScriptResourceFetcher for ScriptFetcher {
    fn fetch(&self, url: &str) -> Option<Vec<u8>> {
        self.0
            .fetch(url)
            .ok()
            .map(|response| response.body.into_bytes())
    }
}

/// `document.cookie` over the process session jar, scoped to the node's origin. The
/// scripted rung installs one so a page's JS reads / writes the same cookies HTTP does
/// (the native session store). HttpOnly cookies are hidden from script (the spec). A
/// write marks the jar dirty so it persists like an HTTP `Set-Cookie`. (Render ladder 2c.)
#[cfg(feature = "scripted")]
struct JarCookieProvider {
    url: url::Url,
}

#[cfg(feature = "scripted")]
impl pelt_desktop::CookieProvider for JarCookieProvider {
    fn get_cookies(&self) -> String {
        use netfetcher::{CookieStore, SameSiteContext};
        crate::fetch::session_jar()
            .records_for(&self.url, SameSiteContext::same_site())
            .into_iter()
            .filter(|record| !record.http_only)
            .map(|record| format!("{}={}", record.name, record.value))
            .collect::<Vec<_>>()
            .join("; ")
    }

    fn set_cookie(&self, cookie: &str) {
        use netfetcher::CookieStore;
        crate::fetch::session_jar().set_cookie(&self.url, cookie);
        crate::fetch::mark_cookies_dirty();
    }
}

mod actor;
mod handlers;
mod transfer;
mod worker;
pub(crate) use actor::*;
pub(crate) use handlers::*;
#[allow(unused_imports)]
pub(crate) use worker::*;

#[cfg(test)]
mod tests;
