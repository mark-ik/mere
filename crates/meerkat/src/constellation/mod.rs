/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The constellation: the pool of **active nodes**.
//!
//! A graph node is persistent data (dormant). It becomes *active* when it has a
//! live content actor rendering it off the UI thread — an [`Activation`]. The
//! constellation is the kernel's pool of those, keyed by graph member (node
//! UUID). The presentation mode decides how an active node is shown: in
//! Cartography the focused node's activation is composited as a floating card; in
//! Tree the open set's activations are composited as tiles. Either way the *same*
//! pool backs them, so "active node == has an actor" is one lifecycle, not two.
//!
//! Each frame the host computes the **needed** set (the focused node, or the open
//! tiles) and [`reconcile`](Constellation::reconcile)s the pool to it: needed
//! nodes that are not active spawn an actor; active nodes that are no longer
//! needed are reaped (their actor's command channel closes, ending its thread) —
//! *unless* they are flagged [`background`](Activation::background), the
//! headless-active state for nodes doing work behind the view (a feed, a sync, a
//! compute). Reaping is dropping the [`Activation`]; the graph datum is untouched,
//! so the node simply returns to dormant.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

use armillary::{ActorHandle, Generations, Pool, Wake};
use document_canvas::{DocumentRenderPacket, DocumentStyleSheet, FontTable};
use forme::GraphMemberId;
use frame::GraphId;
use kernel::permissions::ResolvedPermission;
use linked_data::GraphContribution;
use netrender::Scene;

use crate::card::LinkHit;
use crate::content::{ContentCommand, ContentUpdate, spawn_content};
use crate::fetch::ContentState;

/// Public host-facing summary of one live content operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveOperation {
    pub member: GraphMemberId,
    pub url: Option<String>,
    pub background: bool,
    pub recovering: bool,
    pub scene_version: u64,
    pub content_height: u32,
}

/// A node brought to life: its content actor plus the per-activation
/// bookkeeping. Kept **warm** once spawned — an open tab persists after you
/// navigate away — and reaped only on explicit close ([`Constellation::reap`]) or
/// LRU eviction when the active-tab cap is exceeded, unless `background` protects
/// it.
struct Activation {
    handle: ActorHandle<ContentCommand>,
    rx: Receiver<ContentUpdate>,
    /// The generation pair stamped on `Show` / `Resize`, so a scene built for a
    /// document or size this node has left is dropped on arrival.
    gens: Generations,
    /// `(url, content-tag, w, h)` the actor was last told to show; a change
    /// drives a `Show` (new document) or `Resize` (same document, new size).
    shown: Option<(String, u8, u32, u32)>,
    /// The host-routed engine id the actor was last driven with (its pin or the
    /// policy decision). The host reads this to tell whether a tile is on the
    /// scripted rung, so a click routes to script dispatch rather than the orrery.
    /// (Render ladder phase 3.)
    engine: String,
    /// The latest generation-accepted scene, composited at the node's pane. Set by
    /// the HTML/serval lane; `None` for a document-lane node (which carries `packet`
    /// instead and the host lowers a band of it per scroll).
    scene: Option<Scene>,
    /// The retained document-lane packet (plus its font sidecar): the host windows +
    /// lowers a band of this per scroll, so a tall document is not one giant texture.
    /// `None` for an HTML-lane node (which carries `scene`). The Phase 2 query API
    /// (find / selection / hit-test) reads this. (Retained-text / tiled render.)
    packet: Option<DocumentRenderPacket>,
    fonts: FontTable,
    /// Blurred box-shadow mask requests for the latest HTML-lane `scene`: the host
    /// builds these GPU masks and registers them before rasterizing it, so the
    /// shadow image ops resolve. Empty for the document lane / no blurred shadows.
    masks: Vec<paint_list_render::BoxShadowMaskRequest>,
    /// The full laid-out content height (px) of the latest scene — the document
    /// grows past the visible card, so the host rasterizes a texture this tall and
    /// scrolls a window of it on the GPU. Defaults to 0 until the first scene.
    content_height: u32,
    /// The vertical band the latest HTML-lane `scene` covers: `(band_y, band_h)`, the
    /// document scrolled to `band_y` into a `band_h`-tall viewport. The host
    /// composites the flat scene at this offset (UV = scroll − band_y) and requests a
    /// new band as the scroll moves out of it. `(0, 0)` until the first HTML scene;
    /// meaningless for a document-lane node (which windows its packet). (HTML scroll.)
    band: (u32, u32),
    /// The band last *requested* of the actor via [`ContentCommand::Scroll`], so a
    /// repeat scroll to the same band does not re-command the actor (and re-lay-out)
    /// every frame. Distinct from `band`: this is what we asked for, `band` is what
    /// has arrived. `(0, 0)` until the first request. (HTML scroll.)
    requested_band: (u32, u32),
    /// Content-local clickable link regions from the latest scene — the host
    /// hit-tests a click on the card (offset by its scroll) against these and
    /// navigates the matching URL. Empty until the first scene; cleared on a new
    /// document (a stale link map must not survive a navigation). (Inline-link nav.)
    links: Vec<LinkHit>,
    /// Find-in-page match rects (full-document px) from the latest find query — one
    /// inner `Vec` per match (a wrapped match spans lines). The host highlights these
    /// and scrolls to the active one. Empty until a find query, on an empty query, or
    /// when a new document arrives (a stale match set must not survive). (Find-in-page.)
    find_matches: Vec<Vec<[f32; 4]>>,
    /// The query last sent to the actor via [`ContentCommand::Find`], so the host does
    /// not re-command an unchanged query every frame. (Find-in-page.)
    find_query: String,
    /// Bumped each time a new scene is accepted, so the host can cache a tile's
    /// rasterized texture and re-rasterize only when this changes (not every frame).
    scene_version: u64,
    /// Keep the actor working even when the tab is not shown (headless background
    /// work), and exempt it from cap eviction.
    background: bool,
    /// The pool clock at this tab's last spawn / drive, for LRU eviction: the
    /// least-recently-touched evictable tab is reaped first over the cap.
    last_touched: u64,
    /// How many times this tab's actor has been respawned after a fault, so a tab
    /// that panics on every load stops storming the pool. Reset when a fresh actor
    /// delivers a scene (it recovered).
    respawns: u32,
    /// Which graph this node belongs to, stamped at spawn (the requesting pane's
    /// graph, known at reconcile time). The constellation is one shared pool across
    /// every live graph — members are bare UUIDs — so this is the only graph
    /// dimension: it routes a node's harvested contributions back to *its* orrery
    /// ([`Constellation::drain`]) and scopes per-graph reaping ([`reap_graph`]).
    /// Set once at spawn; a P4 cross-graph move re-stamps it. (Window composition
    /// P1, multi-graph.)
    graph_id: GraphId,
}

/// The most times a faulted tab's actor is respawned before the pool gives up (and
/// leaves the tab on its last scene). Guards against a respawn storm from content
/// that panics on every load.
const MAX_RESPAWNS: u32 = 3;

/// Default cap on warm tabs (active actors) before LRU eviction kicks in. A
/// configurable setting later; the per-tab resource cost keeps real tab counts
/// well under it in practice.
pub const DEFAULT_TAB_CAP: usize = 12;

/// The pool of active nodes (the live half of the graph).
pub struct Constellation {
    /// The wake every spawned actor pokes to drive the host's event loop.
    wake: Wake,
    active: HashMap<GraphMemberId, Activation>,
    /// The pool every content actor runs on. Workers are reused across tab
    /// lifetimes, so OS threads (and the leaked Stylo thread-local per thread) are
    /// bounded by peak concurrent tabs, not the total ever opened / respawned.
    pool: Pool,
    /// The most warm tabs to keep; over this, the least-recently-touched
    /// evictable tab is reaped on reconcile.
    cap: usize,
    /// Monotonic clock, bumped on each spawn / drive and stamped into a tab's
    /// `last_touched`, so eviction picks the genuinely-stalest tab.
    touch_clock: u64,
    /// Engine ids deactivated this session, pushed from the host's activation set.
    /// A spawned actor registers only the document engines NOT in this set, so a
    /// deactivated nematic engine routes to the fallback (synthesized) card instead
    /// of rendering. A snapshot at spawn time — the host respawns affected actors
    /// when the set changes. (engine-picker Phase 1b.)
    disabled_engines: HashSet<String>,
    /// Whether new content actors auto-harvest each loaded document's embedded
    /// linked data (JSON-LD/RDFa) into graph contributions. Off by default: a page's
    /// own structured data (e.g. a Wikipedia article's) would otherwise flood the
    /// graph on every visit. Opt-in (a future setting / explicit "ingest" action
    /// flips it; actors snapshot it at spawn, like `disabled_engines`). (Linked-data
    /// ingest.)
    auto_ingest_linked_data: bool,
    /// Installed DocumentScript origin bindings (§11.4 follow-on #2): on a fresh
    /// navigation whose origin matches one, the actor auto-attaches that script over
    /// the page. The host resolves these (origin + component + permissions) from
    /// `script-bindings.json` and pushes them via [`set_script_bindings`](Self::set_script_bindings).
    script_bindings: Vec<crate::content::script::ResolvedScriptBinding>,
    /// The display device-pixel-ratio the host pushes each frame (winit
    /// `scale_factor`). Content actors lay out in **logical** (DIP) coordinates, so
    /// the viewport / band requests sent to them are divided by this and the heights /
    /// bands / link rects they report back are multiplied by it — the host's content
    /// coordinate space stays physical. 1.0 = no scaling. The host rasterizes the
    /// (logical) scene at physical res via `rasterize_scaled`. (Auto-DPI D2.)
    dpr: f32,
}

/// What a [`Constellation::drain`] surfaced for the host to act on. Scenes are
/// applied inside the pool; these are the cross-cutting effects the host owns.
#[derive(Default)]
pub struct Drained {
    /// A generation-accepted scene landed on at least one activation (redraw).
    pub any_scene: bool,
    /// Subresources a node's render wants, per node. The host fetches each (a
    /// durable-cache hit feeds the node directly via [`Constellation::send_resource`];
    /// a miss spawns a network fetch whose bytes return as a broadcast).
    pub wanted: Vec<(GraphMemberId, Vec<String>)>,
    /// Linked data harvested from active documents, each paired with the graph the
    /// harvesting node belongs to so the host applies it to *that* graph's orrery
    /// (not always the focused one — a background graph's node can contribute).
    pub contributions: Vec<(GraphId, GraphContribution)>,
    /// Tabs whose content actor died (its thread panicked or exited) and was
    /// respawned this drain. The host redraws so the next frame re-`Show`s them.
    pub respawned: Vec<GraphMemberId>,
}

mod drain;
mod ops;

#[cfg(test)]
mod tests;
