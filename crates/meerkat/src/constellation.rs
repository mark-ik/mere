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
use std::sync::mpsc::{Receiver, TryRecvError};

use armillary::{ActorHandle, Generations, Pool, Wake};
use forme::GraphMemberId;
use frame::GraphId;
use document_canvas::{DocumentRenderPacket, FontTable};
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

impl Constellation {
    /// A new, empty pool. `wake` is cloned into every actor it spawns.
    pub fn new(wake: Wake) -> Self {
        Self {
            wake,
            active: HashMap::new(),
            pool: Pool::new(),
            cap: DEFAULT_TAB_CAP,
            touch_clock: 0,
            disabled_engines: HashSet::new(),
        }
    }

    /// Set the active-tab cap (the configurable setting; clamped to at least 1).
    pub fn set_cap(&mut self, cap: usize) {
        self.cap = cap.max(1);
    }

    /// Push the host's deactivated-engine set. New actors register only the document
    /// engines outside it. Existing actors keep their registry, so the caller reaps
    /// the affected active members (forcing a respawn) when the set changes for them.
    /// (engine-picker Phase 1b.)
    pub fn set_disabled_engines(&mut self, disabled: HashSet<String>) {
        self.disabled_engines = disabled;
    }

    /// Whether `member` currently has a live actor.
    pub fn is_active(&self, member: GraphMemberId) -> bool {
        self.active.contains_key(&member)
    }

    /// How many nodes are active.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Snapshot live operations for Steward/Apparatus-style inspection without
    /// exposing the actor handles or receivers.
    pub fn active_operations(&self) -> Vec<ActiveOperation> {
        let mut operations: Vec<_> = self
            .active
            .iter()
            .map(|(member, activation)| ActiveOperation {
                member: *member,
                url: activation.shown.as_ref().map(|(url, ..)| url.clone()),
                background: activation.background,
                recovering: activation.respawns > 0
                    && activation.scene.is_none()
                    && activation.packet.is_none(),
                scene_version: activation.scene_version,
                content_height: activation.content_height,
            })
            .collect();
        operations.sort_by_key(|operation| operation.member);
        operations
    }

    /// Reconcile the pool to the needed set: spawn an actor for any
    /// needed-but-dormant node, then **keep every active node warm** — an open tab
    /// persists after you navigate away; it is *not* reaped on blur. The only
    /// involuntary reaping is the active-tab cap: when the pool exceeds
    /// [`cap`](Self::cap), the least-recently-touched tab that is neither needed
    /// now nor `background` is evicted, until within the cap (or none remain to
    /// evict). Explicit close is [`reap`](Self::reap).
    pub fn reconcile(&mut self, needed: &[(GraphMemberId, GraphId)]) {
        // Spawn needed-but-dormant nodes, each touch-stamped so LRU is well-ordered
        // and graph-stamped (the requesting pane's graph) so its contributions route
        // back to the right orrery.
        for &(member, graph_id) in needed {
            if !self.active.contains_key(&member) {
                self.touch_clock += 1;
                let touch = self.touch_clock;
                let (handle, rx) =
                    spawn_content(&self.pool, self.wake.clone(), self.disabled_engines.clone());
                self.active.insert(
                    member,
                    Activation {
                        handle,
                        rx,
                        gens: Generations::default(),
                        shown: None,
                        scene: None,
                        packet: None,
                        fonts: FontTable::default(),
                        masks: Vec::new(),
                        content_height: 0,
                        band: (0, 0),
                        requested_band: (0, 0),
                        links: Vec::new(),
                        scene_version: 0,
                        background: false,
                        last_touched: touch,
                        respawns: 0,
                        graph_id,
                    },
                );
            }
        }
        // Enforce the cap: evict the least-recently-touched evictable tab (neither
        // needed now nor backgrounded) until within the cap, or until none remain.
        let needed_set: HashSet<GraphMemberId> = needed.iter().map(|(m, _)| *m).collect();
        while self.active.len() > self.cap {
            let victim = self
                .active
                .iter()
                .filter(|(member, a)| !needed_set.contains(member) && !a.background)
                .min_by_key(|(_, a)| a.last_touched)
                .map(|(member, _)| *member);
            match victim {
                Some(member) => {
                    self.active.remove(&member); // drop → channel closes → thread ends
                }
                None => break, // every remaining tab is needed or background
            }
        }
    }

    /// Drive `member`'s actor for the current frame: a fresh document (url /
    /// fetch-state change) is a `Show` (new nav generation; blank the scene until
    /// it re-renders), a same-document size change a `Resize`. No-op if the node
    /// is not active or nothing changed.
    pub fn drive(
        &mut self,
        member: GraphMemberId,
        url: &str,
        state: Option<ContentState>,
        cw: u32,
        ch: u32,
    ) {
        let tag = ContentState::tag(state.as_ref());
        self.touch_clock += 1;
        let touch = self.touch_clock;
        let Some(activation) = self.active.get_mut(&member) else {
            return;
        };
        activation.last_touched = touch; // shown this frame → freshest against eviction
        let key = (url.to_string(), tag, cw, ch);
        if activation.shown.as_ref() == Some(&key) {
            return;
        }
        let same_doc = activation
            .shown
            .as_ref()
            .is_some_and(|(u, t, ..)| u == url && *t == tag);
        if same_doc {
            activation.gens.viewport.bump();
            activation.handle.command(ContentCommand::Resize {
                viewport: (cw, ch),
                viewport_gen: activation.gens.viewport,
            });
        } else {
            activation.gens.nav.bump();
            activation.scene = None;
            activation.links.clear();
            activation.handle.command(ContentCommand::Show {
                url: url.to_string(),
                state,
                viewport: (cw, ch),
                nav: activation.gens.nav,
                viewport_gen: activation.gens.viewport,
            });
        }
        // Both a Show and a Resize re-anchor the actor's band to the top, so the
        // last requested band is stale; clear it so the next `request_scroll` always
        // re-commands the genuinely-wanted band. (HTML scroll.)
        activation.requested_band = (0, 0);
        activation.shown = Some(key);
    }

    /// The latest composited scene for `member`, if it has rendered one.
    pub fn scene(&self, member: GraphMemberId) -> Option<&Scene> {
        self.active.get(&member).and_then(|a| a.scene.as_ref())
    }

    /// The blurred box-shadow mask requests for `member`'s latest HTML scene. The
    /// host builds + registers these GPU masks before rasterizing [`scene`](Self::scene).
    /// Empty for the document lane, an unrendered node, or a page without blurred
    /// shadows. (Box-shadow.)
    pub fn scene_masks(&self, member: GraphMemberId) -> &[paint_list_render::BoxShadowMaskRequest] {
        self.active.get(&member).map_or(&[], |a| &a.masks)
    }

    /// The member's scene version — bumped each time a new scene is accepted. The
    /// host caches a tile's rasterized texture against this so an unchanged tile is
    /// not re-rasterized every frame. `0` if the member is not active or has no scene.
    pub fn scene_version(&self, member: GraphMemberId) -> u64 {
        self.active.get(&member).map_or(0, |a| a.scene_version)
    }

    /// The full content height (px) of `member`'s latest scene — the tall texture
    /// height the host rasterizes and scrolls a window of. `0` if not active or
    /// not yet rendered.
    pub fn content_height(&self, member: GraphMemberId) -> u32 {
        self.active.get(&member).map_or(0, |a| a.content_height)
    }

    /// The vertical band `(band_y, band_h)` the member's latest HTML/serval scene
    /// covers: the page scrolled to `band_y` into a `band_h`-tall window. The host
    /// composites the flat scene at this offset (the visible UV row is
    /// `scroll − band_y`) and requests a new band via [`request_scroll`](Self::request_scroll)
    /// once the scroll leaves it. `(0, 0)` for a document-lane node (which windows
    /// its packet directly), an unrendered node, or before the first HTML scene.
    /// (HTML scroll.)
    pub fn scene_band(&self, member: GraphMemberId) -> (u32, u32) {
        self.active.get(&member).map_or((0, 0), |a| a.band)
    }

    /// Ask `member`'s actor to re-emit the HTML/serval band covering
    /// `(band_y, band_h)` — the document scrolled to `band_y` into a `band_h`-tall
    /// viewport — so a tall dense page is windowed one band at a time instead of
    /// rasterized whole (which overflows the GPU encode budget). Deduped against the
    /// last request, so holding the scroll still does not re-command (and re-lay-out)
    /// the actor every frame. No-op for a node that is not active or is on the
    /// document lane (whose packet the host windows itself). (HTML scroll.)
    pub fn request_scroll(&mut self, member: GraphMemberId, band_y: u32, band_h: u32) {
        let Some(activation) = self.active.get_mut(&member) else {
            return;
        };
        // The document lane carries a packet the host windows directly; only the
        // flat HTML/serval scene needs an actor-side re-emit.
        if activation.packet.is_some() {
            return;
        }
        if activation.requested_band == (band_y, band_h) {
            return;
        }
        activation.requested_band = (band_y, band_h);
        activation.handle.command(ContentCommand::Scroll {
            band_y,
            band_h,
            // Scroll is not a resize: the viewport generation is unchanged, so the
            // re-emitted band is accepted at the current generation.
            viewport_gen: activation.gens.viewport,
        });
    }

    /// The member's retained document-lane packet (plus font sidecar), if it is a
    /// document-lane node that has rendered. The host windows + lowers a band of this
    /// per scroll; `None` for an HTML-lane node (use [`scene`](Self::scene)) or an
    /// unrendered one. (Retained-text / tiled render.)
    pub fn packet(&self, member: GraphMemberId) -> Option<(&DocumentRenderPacket, &FontTable)> {
        self.active
            .get(&member)
            .and_then(|a| a.packet.as_ref().map(|p| (p, &a.fonts)))
    }

    /// The URL of the clickable link whose content-local rect contains `(x, y)`, if
    /// any. `(x, y)` is in the document's own coordinate space (the host subtracts
    /// the card's window origin and adds its scroll before calling). Last match wins,
    /// so a link nested in an enclosing region resolves to the innermost. `None` when
    /// the point lands on no link (the caller keeps its normal click). (Inline-link nav.)
    pub fn link_at(&self, member: GraphMemberId, x: f32, y: f32) -> Option<&str> {
        let activation = self.active.get(&member)?;
        // Document lane: hit-test the retained packet's interactions directly (the
        // query API subsumes the old parallel link-rect table). HTML lane has no
        // packet, so it walks `links` (empty until the Phase 5 lane parity).
        if let Some(packet) = &activation.packet {
            return packet.link_at(x, y);
        }
        activation
            .links
            .iter()
            .rev()
            .find(|l| x >= l.rect[0] && x <= l.rect[2] && y >= l.rect[1] && y <= l.rect[3])
            .map(|l| l.url.as_str())
    }

    /// Whether `member` is recovering from a crash: its actor was respawned and has
    /// not delivered a fresh scene yet. The host shows a placeholder for it (a tab
    /// that crashed before ever rendering would otherwise be blank). A tab that
    /// crashed *after* rendering keeps its last scene and reads as not-recovering.
    pub fn is_recovering(&self, member: GraphMemberId) -> bool {
        self.active
            .get(&member)
            .is_some_and(|a| a.respawns > 0 && a.scene.is_none() && a.packet.is_none())
    }

    /// Deactivate `member` now — its actor winds down on drop. For when a tile /
    /// live preview is closed, or the node is gone (deleted). The node's "last
    /// visit" snapshot is re-derived host-side from the durable cache, so nothing
    /// needs retaining here.
    pub fn reap(&mut self, member: GraphMemberId) {
        self.active.remove(&member);
    }

    /// Reap **every** active node, returning the pool to empty. The host calls
    /// this on a multi-graph switch so the prior session's content actors stop
    /// and don't bleed into the new graph. (Multi-graph MG2.)
    pub fn clear(&mut self) {
        self.active.clear();
        self.pool = Pool::new();
    }

    /// Reap only the active nodes belonging to `graph` — the per-graph form of
    /// [`clear`](Self::clear). A session switch parks the *outgoing* graph this way,
    /// so another live graph's actors (in this or another window's panes) survive.
    /// Unlike `clear` it does **not** reset the worker pool: the pool is shared
    /// across every live graph, so resetting it would kill the survivors' actors.
    /// (Window composition P1, multi-graph.)
    pub fn reap_graph(&mut self, graph: GraphId) {
        self.active.retain(|_, a| a.graph_id != graph);
    }

    /// Whether `member` is flagged to keep working in the background.
    pub fn is_background(&self, member: GraphMemberId) -> bool {
        self.active.get(&member).is_some_and(|a| a.background)
    }

    /// Set (or clear) `member`'s background flag. Returns whether the node was
    /// active to flag.
    pub fn set_background(&mut self, member: GraphMemberId, background: bool) -> bool {
        match self.active.get_mut(&member) {
            Some(activation) => {
                activation.background = background;
                true
            }
            None => false,
        }
    }

    /// Feed a fetched subresource to one node's actor (a durable-cache hit routed
    /// to the node that wanted it).
    pub fn send_resource(&self, member: GraphMemberId, url: String, bytes: Vec<u8>) {
        if let Some(activation) = self.active.get(&member) {
            activation
                .handle
                .command(ContentCommand::Resource { url, bytes });
        }
    }

    /// Feed a subresource to every active node. The fetch stream is keyed by URL,
    /// not by which node wanted it, so a network result fans out; each actor
    /// dedups via its own resource store and only the one that wanted it
    /// re-renders.
    pub fn broadcast_resource(&self, url: &str, bytes: &[u8]) {
        for activation in self.active.values() {
            activation.handle.command(ContentCommand::Resource {
                url: url.to_string(),
                bytes: bytes.to_vec(),
            });
        }
    }

    /// Drain every active node's update channel, applying generation-accepted
    /// scenes into the pool and returning the wanted subresources + harvested
    /// contributions for the host to handle. A channel that has **disconnected**
    /// means its content actor's thread died (a panic mid-render, isolated to that
    /// thread); the pool respawns it (self-healing, P4) up to [`MAX_RESPAWNS`],
    /// keeping the tab's last scene until the fresh actor renders.
    pub fn drain(&mut self) -> Drained {
        let mut out = Drained::default();
        let members: Vec<GraphMemberId> = self.active.keys().copied().collect();
        let mut dead: Vec<GraphMemberId> = Vec::new();
        for member in members {
            // Drain into an owned Vec first so the receiver borrow ends before we
            // re-borrow the activation to apply scenes. A `Disconnected` means the
            // actor thread is gone.
            let mut updates: Vec<ContentUpdate> = Vec::new();
            match self.active.get(&member) {
                Some(activation) => loop {
                    match activation.rx.try_recv() {
                        Ok(update) => updates.push(update),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            dead.push(member);
                            break;
                        }
                    }
                },
                None => continue,
            }
            for update in updates {
                match update {
                    ContentUpdate::Document {
                        nav,
                        viewport_gen,
                        packet,
                        fonts,
                        content_height,
                    } => {
                        if let Some(activation) = self.active.get_mut(&member) {
                            let stamp = Generations {
                                nav,
                                viewport: viewport_gen,
                            };
                            if activation.gens.accepts(stamp) {
                                activation.packet = Some(packet);
                                activation.fonts = fonts;
                                activation.scene = None; // forget any stale HTML scene
                                activation.masks = Vec::new(); // document lane has no shadow masks
                                activation.content_height = content_height;
                                // Document-lane hit-testing reads the packet; clear any
                                // stale HTML link table. (Phase 2 query API.)
                                activation.links = Vec::new();
                                activation.scene_version += 1;
                                activation.respawns = 0; // a fresh render = recovered
                                out.any_scene = true;
                            }
                        }
                    }
                    ContentUpdate::Scene {
                        nav,
                        viewport_gen,
                        scene,
                        content_height,
                        band_y,
                        band_h,
                        links,
                        masks,
                    } => {
                        if let Some(activation) = self.active.get_mut(&member) {
                            let stamp = Generations {
                                nav,
                                viewport: viewport_gen,
                            };
                            if activation.gens.accepts(stamp) {
                                activation.scene = Some(scene);
                                activation.packet = None; // forget any stale document packet
                                activation.masks = masks;
                                activation.content_height = content_height;
                                // Record the band this scene covers so the host composites
                                // it at the right offset; a Show/Resize that re-anchors the
                                // band to the top arrives as band_y == 0. (HTML scroll.)
                                activation.band = (band_y, band_h);
                                activation.links = links;
                                activation.scene_version += 1;
                                activation.respawns = 0; // a fresh scene = recovered
                                out.any_scene = true;
                            }
                        }
                    }
                    ContentUpdate::Wanted { nav, urls } => {
                        let current = self.active.get(&member).is_some_and(|a| a.gens.nav == nav);
                        if current {
                            out.wanted.push((member, urls));
                        }
                    }
                    ContentUpdate::Contribution { contributions } => {
                        // Pair each with the harvesting node's graph, so the host
                        // routes it to that graph's orrery.
                        if let Some(graph_id) = self.active.get(&member).map(|a| a.graph_id) {
                            out.contributions
                                .extend(contributions.into_iter().map(|c| (graph_id, c)));
                        }
                    }
                }
            }
        }
        for member in dead {
            if self.respawn(member) {
                out.respawned.push(member);
            }
        }
        out
    }

    /// Respawn a dead tab's content actor with a fresh thread (the kernel-owned
    /// spec is the tab's own `url`/size, replayed by clearing `shown` so the next
    /// [`drive`](Self::drive) re-`Show`s it). Keeps the last scene (stale-but-live)
    /// until the fresh actor renders. A no-op past [`MAX_RESPAWNS`] (the tab is left
    /// on its last scene rather than storming). Returns whether it respawned.
    fn respawn(&mut self, member: GraphMemberId) -> bool {
        let Some(activation) = self.active.get_mut(&member) else {
            return false;
        };
        if activation.respawns >= MAX_RESPAWNS {
            return false;
        }
        let (handle, rx) =
            spawn_content(&self.pool, self.wake.clone(), self.disabled_engines.clone());
        activation.handle = handle;
        activation.rx = rx;
        activation.gens = Generations::default();
        activation.shown = None; // force the next drive() to re-Show, replaying the page
        activation.respawns += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn m(n: u128) -> GraphMemberId {
        Uuid::from_u128(n)
    }

    /// A fixed graph id for the reconcile tests — they exercise spawn / warmth /
    /// LRU eviction, all graph-agnostic, so one stamp serves.
    fn g() -> GraphId {
        GraphId(Uuid::from_u128(0))
    }

    fn noop_wake() -> Wake {
        std::sync::Arc::new(|| {})
    }

    #[test]
    fn reconcile_keeps_tabs_warm() {
        let mut c = Constellation::new(noop_wake());
        c.reconcile(&[(m(1), g()), (m(2), g())]);
        assert_eq!(c.active_count(), 2, "two needed nodes spawned");
        c.reconcile(&[(m(2), g())]); // m(1) is no longer needed...
        assert!(
            c.is_active(m(1)),
            "...but stays a warm tab — no reap on blur"
        );
        assert_eq!(c.active_count(), 2);
    }

    #[test]
    fn reconcile_evicts_least_recently_touched_over_cap() {
        let mut c = Constellation::new(noop_wake());
        c.set_cap(2);
        c.reconcile(&[(m(1), g())]); // touch 1
        c.reconcile(&[(m(2), g())]); // touch 2 — m(1) is now the stalest
        c.reconcile(&[(m(3), g())]); // touch 3, over the cap of 2 → evict the stalest evictable
        assert_eq!(c.active_count(), 2, "the cap holds");
        assert!(
            !c.is_active(m(1)),
            "the least-recently-touched, non-needed tab is evicted"
        );
        assert!(c.is_active(m(2)) && c.is_active(m(3)));
    }

    #[test]
    fn a_background_tab_is_exempt_from_eviction() {
        let mut c = Constellation::new(noop_wake());
        c.set_cap(1);
        c.reconcile(&[(m(1), g())]);
        assert!(
            c.set_background(m(1), true),
            "flagging an active node succeeds"
        );
        c.reconcile(&[(m(2), g())]); // over the cap of 1, but m(1) is background → not evictable
        assert!(c.is_active(m(1)), "a background tab survives cap pressure");
        assert!(c.is_active(m(2)), "the needed node is still spawned");
        assert!(
            !c.set_background(m(3), true),
            "flagging a dormant node reports false"
        );
    }

    #[test]
    fn respawn_replays_the_tab_and_caps_the_storm() {
        let mut c = Constellation::new(noop_wake());
        c.reconcile(&[(m(1), g())]);
        c.drive(m(1), "mere://welcome", None, 100, 100); // gives it a `shown` state
        assert!(c.active.get(&m(1)).unwrap().shown.is_some());
        // A respawn replaces the actor and clears `shown` so the next drive re-Shows.
        assert!(c.respawn(m(1)));
        assert!(
            c.active.get(&m(1)).unwrap().shown.is_none(),
            "shown cleared for replay"
        );
        assert_eq!(c.active.get(&m(1)).unwrap().respawns, 1);
        // The cap stops a storm: MAX_RESPAWNS respawns, then give up.
        assert!(c.respawn(m(1))); // 2
        assert!(c.respawn(m(1))); // 3
        assert!(
            !c.respawn(m(1)),
            "past MAX_RESPAWNS the pool leaves the tab on its last scene"
        );
        // A respawn on a node that is not active is a no-op.
        assert!(!c.respawn(m(99)));
    }
}
