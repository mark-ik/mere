/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The orrery as a reusable, **window-agnostic content-root** — the graph's
//! spatial presentation (build item 1D of the serval-as-host flip; S1 of the
//! modular integration plan).
//!
//! [`Orrery`] owns the graph, its [`gyre::Simulation`], the camera, and the
//! pre-materialized abs-pos node-children pool. It exposes:
//!
//! - [`Orrery::frame`] — advance one frame at a given viewport and return the
//!   composited `netrender::Scene` plus whether another frame is needed (the sim
//!   still settling, pan still gliding, or a node being dragged). It does **not**
//!   present — the host (a winit bin, or meerkat's content-root) rasterizes +
//!   composites the returned scene.
//! - Semantic input methods ([`pointer_down`](Orrery::pointer_down) /
//!   [`pointer_up`](Orrery::pointer_up) / [`cursor_moved`](Orrery::cursor_moved) /
//!   [`wheel`](Orrery::wheel) / [`set_ctrl`](Orrery::set_ctrl) /
//!   [`reseed`](Orrery::reseed)), each returning whether a redraw is needed. The
//!   host maps its raw events (winit, serval input, …) onto these; the orrery
//!   never sees a window.
//!
//! The three composited layers (per the §6 plan): the [`platen::orrery`]
//! scene-paint underlay (edges + demoted off-screen node rects + coupling
//! overlays) under one camera transform; the on-screen nodes as abs-pos serval
//! DOM children (laid out incrementally, moved per-frame by inline transform on
//! the `RepaintOnly` path); and a screen-space marquee rubber-band when active.
//!
//! The sample graph, simulation, node-children pool, and the small paint/DOM
//! helpers live in [`mod@build`].

use std::collections::{HashMap, HashSet, VecDeque};

use euclid::default::{Box2D, Point2D};
use gyre::{LayoutSnapshot, LayoutView};
use kernel::graph::{Graph, NodeKey};
use platen::scene_paint::{Camera, ScenePaintStyle};
use serval_layout::IncrementalLayout;
use serval_scripted_dom::{NodeId as DomNodeId, ScriptedDom};

mod build;
use build::{build_pool_dom, build_simulation, dark_scene_style, dedup_edges, sample_graph};

mod types;
pub use types::{CameraView, NodeState, PointerButton};

mod input;
mod frame;

mod physics;
use physics::Physics;

/// Force-directed settle length (frames) after a (re)seed, ~6s at 60fps.
const SETTLE_TICKS: u32 = 360;
/// Per-tick timestep handed to the gyre simulation.
const TICK_DT: f32 = 1.0 / 60.0;
/// Pixels per wheel line-notch (the host scales `LineDelta` by this before
/// calling [`Orrery::wheel`]; `wheel` divides back out to recover notches for zoom).
pub const WHEEL_PAN_SCALE: f32 = 40.0;
/// Zoom multiplier per wheel notch under Ctrl.
const ZOOM_STEP: f32 = 1.15;
/// Pan-inertia decay per frame (lower = stops sooner).
const PAN_DECAY: f32 = 0.85;
/// Clamp for the camera zoom.
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 8.0;
/// Screen-px the pointer may move before a press counts as a drag, not a click.
const CLICK_SLOP: f32 = 4.0;
/// Screen-px pick radius around an edge segment for `edge_hit_test`.
const EDGE_PICK_TOL: f32 = 6.0;
/// Node box half-extent (px) — matches the underlay's default node rect, so each
/// DOM child sits centered on the same world position.
const NODE_HALF: f32 = 18.0;


/// An in-progress left-button interaction on a node: a click until the pointer
/// passes [`CLICK_SLOP`], then a drag that pins the node to the cursor.
#[derive(Clone, Copy)]
struct Drag {
    node: NodeKey,
    /// Press position in screen px (the click/drag-slop origin).
    press: (f32, f32),
    /// Set once the pointer has moved past the slop — a real drag.
    moved: bool,
}

/// The orrery content-root: the graph, its physics, the camera, and the abs-pos
/// node-children pool. Window-agnostic — see the module docs.
pub struct Orrery {
    graph: Graph,
    /// The force-directed layout backend — in-thread (tests / wasm) or an
    /// off-thread armillary actor (native always-offload). The orrery never reads
    /// it directly; it feeds positions into `view` each frame.
    physics: Physics,
    /// The rapier-free read model the frame loop and input handlers read from —
    /// positions, node/edge picks, cull. Refreshed from `sim` each frame (and on
    /// every structural / seed change). This is the seam that lets `sim` move to
    /// an off-thread physics actor (P6): swap the snapshot source feeding this
    /// view, and the read sites here are untouched.
    view: LayoutView,
    /// The pre-materialized node-children pool: a persistent serval DOM with one
    /// `.gnode` per node under a `.stage` container (built once, mutated per
    /// frame — never rebuilt structurally).
    node_dom: ScriptedDom,
    /// Incremental layout over `node_dom`; per-frame transform / class mutations
    /// go through `apply` (paint-tier → `RepaintOnly`) and paint via
    /// `emit_paint_list`. `None` until first built (or after a viewport change).
    node_layout: Option<IncrementalLayout<DomNodeId>>,
    /// node → its `.gnode` element in `node_dom`.
    gnode_of: HashMap<NodeKey, DomNodeId>,
    /// The `.stage` container element (carries the camera transform).
    stage_node: DomNodeId,
    /// Viewport `node_layout` was built at; a change rebuilds it.
    pool_w: u32,
    pool_h: u32,
    camera: Camera,
    style: ScenePaintStyle,
    /// Producer generation, bumped each rendered frame (positions / camera move).
    generation: u64,
    /// Last cursor position in screen px (zoom anchor + drag origin).
    cursor: (f32, f32),
    /// Inertial pan velocity (px/frame); decays each frame when not dragging.
    pan_velocity: (f32, f32),
    /// `Some(last_cursor)` while a middle-button pan drag is in progress.
    middle_drag: Option<(f32, f32)>,
    /// An in-progress left-button node click/drag, if any.
    drag: Option<Drag>,
    /// Currently-selected nodes (click selects one; marquee selects many).
    selected: HashSet<NodeKey>,
    /// Currently-selected edges (edge-pick, or covered by a marquee), as the
    /// `(from, to)` pairs gyre reports.
    selected_edges: HashSet<(NodeKey, NodeKey)>,
    /// Edges the user has hidden, as undirected `(from, to)` pairs (`from <= to`).
    /// The edge pass skips any relation whose pair is here; the relation and its
    /// physics spring persist (hiding is display-only). In-session for now;
    /// persistence rides view-intent's `hidden_relations`.
    hidden_edges: HashSet<(NodeKey, NodeKey)>,
    /// Per-node activation state the host pushes for node coloring (open / closed
    /// / idle). Resolved to `NodeKey` on set; a node absent here colors as `Idle`.
    node_states: HashMap<NodeKey, NodeState>,
    /// `Some(press_origin)` (screen px) while a left-drag marquee on empty space
    /// is in progress.
    marquee: Option<(f32, f32)>,
    /// Whether Ctrl is held (gates wheel-zoom vs wheel-pan).
    ctrl: bool,
    /// Whether Shift is held (a node click adds to / toggles the selection rather
    /// than replacing it — multi-select).
    shift: bool,
    /// The current viewport (px), updated by [`frame`](Orrery::frame) /
    /// [`resize`](Orrery::resize); used by `world_viewport` (cull) + `recenter`.
    view_w: u32,
    view_h: u32,
}

impl Default for Orrery {
    fn default() -> Self {
        Self::new()
    }
}

impl Orrery {
    /// A new orrery over an **empty** session graph. The host grows it with
    /// [`visit`](Orrery::visit) as the user navigates (the graph-rooted browse
    /// loop). For an isolated demo / the standalone bin, use
    /// [`with_sample_graph`](Orrery::with_sample_graph).
    pub fn new() -> Self {
        Self::from_graph(Graph::new())
    }

    /// A new orrery over a small built-in sample graph (a ring + spokes), seeded
    /// into a tight central spiral so the first settle is visible. The standalone
    /// `orrery-host` bin and the orrery tests use this; meerkat uses
    /// [`new`](Orrery::new) and drives the graph through [`visit`](Orrery::visit).
    pub fn with_sample_graph() -> Self {
        Self::from_graph(sample_graph())
    }

    /// Build an orrery over a **restored** session `graph`: keep each node at its
    /// saved (committed) position and do not auto-settle, so a reloaded session
    /// looks as it was left rather than re-scrambling into a fresh spiral.
    /// (Persistence host seam, S3.)
    pub fn with_graph(graph: Graph) -> Self {
        let mut orrery = Self::from_graph(graph);
        let positions: Vec<(NodeKey, Point2D<f32>)> = orrery
            .graph
            .nodes()
            .map(|(key, node)| {
                let p = node.projected_position();
                (key, Point2D::new(p.x, p.y))
            })
            .collect();
        for &(key, pos) in &positions {
            orrery.view.set_position(key, pos);
        }
        orrery.physics.seed(positions);
        orrery.physics.halt();
        orrery
    }

    /// Build an orrery over `graph`: its [`build_simulation`], the node-children
    /// pool, and a default camera. Shared by [`new`](Orrery::new) (empty),
    /// [`with_sample_graph`](Orrery::with_sample_graph), and
    /// [`with_graph`](Orrery::with_graph).
    fn from_graph(graph: Graph) -> Self {
        let sim = build_simulation(&graph);
        let view = sim.view();
        let physics = Physics::inline(sim, SETTLE_TICKS);
        let (node_dom, gnode_of, stage_node) = build_pool_dom(&graph);
        Self {
            graph,
            physics,
            view,
            node_dom,
            node_layout: None,
            gnode_of,
            stage_node,
            pool_w: 0,
            pool_h: 0,
            camera: Camera::default(),
            style: dark_scene_style(),
            generation: 0,
            cursor: (0.0, 0.0),
            pan_velocity: (0.0, 0.0),
            middle_drag: None,
            drag: None,
            selected: HashSet::new(),
            selected_edges: HashSet::new(),
            hidden_edges: HashSet::new(),
            node_states: HashMap::new(),
            marquee: None,
            ctrl: false,
            shift: false,
            view_w: 1024,
            view_h: 600,
        }
    }

    /// Set the viewport the orrery culls + centers against. The host calls this on
    /// a surface resize; the next [`frame`](Orrery::frame) rebuilds the node-pool
    /// layout at the new size.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.view_w = width.max(1);
        self.view_h = height.max(1);
    }

    /// Put the world origin at the viewport center — the sample graph is laid out
    /// around `(0, 0)`, so this frames it at zoom 1. (A fit-to-`content_bounds`
    /// camera replaces this when a real graph is hosted.)
    pub fn recenter(&mut self) {
        self.camera.offset = (self.view_w as f32 / 2.0, self.view_h as f32 / 2.0);
    }

    /// Re-sync the physics bodies, edge topology, and node-children pool to the
    /// current graph after a structural change. Does not seed positions, alter the
    /// selection, or restart the settle; callers do that as they need. The pool
    /// is structural, so it is rebuilt, not grown incrementally.
    fn reconcile_derived(&mut self) {
        let nodes: Vec<(NodeKey, Point2D<f32>)> = self
            .graph
            .nodes()
            .map(|(key, node)| {
                let p = node.projected_position();
                (key, Point2D::new(p.x, p.y))
            })
            .collect();
        self.physics.sync_nodes(nodes);
        self.physics.sync_edges(dedup_edges(&self.graph));
        let (node_dom, gnode_of, stage_node) = build_pool_dom(&self.graph);
        self.node_dom = node_dom;
        self.gnode_of = gnode_of;
        self.stage_node = stage_node;
        self.node_layout = None;
        self.resync_view_to_graph();
    }

    /// Make the read model's node set match the graph after a structural change,
    /// **backend-free**: keep the live position of every node still present, fall
    /// back to its committed position for a node the view has not placed yet, drop
    /// departed nodes, and refresh the edge topology. This gives the next input
    /// read (hit-test, edge-pick) a correct node set synchronously, without
    /// waiting for the physics backend's next snapshot; the per-frame
    /// [`Physics::advance_frame`] then overwrites positions with authoritative
    /// ones. A subsequent seed overrides newly-added nodes' positions.
    fn resync_view_to_graph(&mut self) {
        let positions: Vec<(NodeKey, Point2D<f32>)> = self
            .graph
            .nodes()
            .map(|(key, node)| {
                let pos = self.view.position_of(key).unwrap_or_else(|| {
                    let p = node.projected_position();
                    Point2D::new(p.x, p.y)
                });
                (key, pos)
            })
            .collect();
        self.view.apply_snapshot(&LayoutSnapshot { positions, generation: self.generation });
        self.view.set_edges(dedup_edges(&self.graph));
    }

    /// Whether the layout is still settling or a node is being dragged — the host
    /// chains another frame while true.
    pub fn is_settling(&self) -> bool {
        self.physics.is_settling()
    }

    /// Move physics onto an off-thread actor (the native always-offload path).
    /// The host calls this once, just after construction, with a [`Wake`] that
    /// pokes its event loop when a layout snapshot is ready. Left uncalled, the
    /// orrery keeps ticking in-thread (tests; the no-threads wasm profile).
    ///
    /// [`Wake`]: armillary::Wake
    pub fn offload_physics(&mut self, wake: armillary::Wake) {
        self.physics.offload(wake);
    }

    /// Apply a structural mutation to the session graph and reconcile every derived
    /// view, so externally-ingested nodes/edges (e.g. a linked-data merge) join the
    /// spatial field. `mutate` returns whether it changed the graph; on a change,
    /// the newly-added nodes are fanned out around the current selection (they are
    /// minted at the origin, which the force sim must not stack), and the settle
    /// restarts. Returns whether the graph changed.
    ///
    /// orrery-host stays free of the linked-data bridge: the host passes the merge
    /// in as a closure over [`Graph`].
    pub fn ingest_graph<F: FnOnce(&mut Graph) -> bool>(&mut self, mutate: F) -> bool {
        let before: HashSet<NodeKey> = self.graph.nodes().map(|(k, _)| k).collect();
        if !mutate(&mut self.graph) {
            return false;
        }
        self.reconcile_derived();
        let anchor = self
            .selected
            .iter()
            .copied()
            .next()
            .and_then(|k| self.view.position_of(k))
            .unwrap_or(Point2D::new(0.0, 0.0));
        let seeds: Vec<_> = self
            .graph
            .nodes()
            .map(|(k, _)| k)
            .filter(|k| !before.contains(k))
            .enumerate()
            .map(|(i, k)| {
                let col = (i % 6) as f32;
                let row = (i / 6) as f32;
                (k, Point2D::new(anchor.x + 12.0 + col * 16.0, anchor.y + 12.0 + row * 16.0))
            })
            .collect();
        for &(key, pos) in &seeds {
            self.view.set_position(key, pos);
        }
        self.physics.seed(seeds);
        self.physics.settle(SETTLE_TICKS);
        true
    }

    /// Replace the selection with just `key` (clearing any selected nodes/edges).
    fn select_only(&mut self, key: NodeKey) {
        self.selected.clear();
        self.selected_edges.clear();
        self.selected.insert(key);
    }

    /// Select the existing node with `url` (URL identity), if present, without
    /// adding one. Returns whether a node was found and focused. The host calls
    /// this to restore the focused node from persisted view-intent.
    pub fn select_by_url(&mut self, url: &str) -> bool {
        if let Some((key, _)) = self.graph.get_node_by_url(url) {
            self.select_only(key);
            true
        } else {
            false
        }
    }

    /// Remove the single focused node from the session graph (if exactly one is
    /// selected), returning its member id (the kernel node UUID) so the host can
    /// reap any activation for it. Clears the selection and reconciles the physics
    /// + node pool to the smaller graph. Returns `None` when zero or many nodes
    /// are selected, leaving the graph untouched. The host calls this on a
    /// delete-node gesture; deactivation (reaping the actor) is the host's job.
    pub fn remove_focused(&mut self) -> Option<uuid::Uuid> {
        if self.selected.len() != 1 {
            return None;
        }
        let key = *self.selected.iter().next()?;
        let id = self.graph.get_node(key)?.id;
        self.graph.remove_node(key);
        self.selected.clear();
        self.selected_edges.clear();
        self.reconcile_derived();
        Some(id)
    }

    /// Clear the node + edge selection (focus) without removing anything from
    /// the graph. The host calls this to drop focus — e.g. closing the last
    /// workbench tile returns to the graph with nothing focused, so the node
    /// deactivates instead of its Cartography preview re-activating it.
    /// (Card-system plan, Phase 1.)
    pub fn clear_selection(&mut self) {
        if self.selected.is_empty() && self.selected_edges.is_empty() {
            return;
        }
        self.selected.clear();
        self.selected_edges.clear();
        self.reconcile_derived();
    }

    /// Hide the currently-selected edges: move them into the hidden set (as
    /// undirected pairs) so the edge pass skips them, and clear the selection.
    /// Returns how many were hidden. The relations and their physics springs
    /// persist — this hides the drawn lines, it does not delete the relations.
    pub fn hide_selected_edges(&mut self) -> usize {
        let count = self.selected_edges.len();
        for (a, b) in self.selected_edges.drain() {
            let pair = if a <= b { (a, b) } else { (b, a) };
            self.hidden_edges.insert(pair);
        }
        count
    }

    /// Reveal every hidden edge. Returns how many were shown.
    pub fn show_all_edges(&mut self) -> usize {
        let count = self.hidden_edges.len();
        self.hidden_edges.clear();
        count
    }

    /// Set the per-node activation states the orrery colors its on-screen nodes
    /// by, keyed by node UUID (the host's member id); the orrery resolves each to
    /// its `NodeKey`. The host recomputes + pushes this as the actor pool / content
    /// cache change; a node absent from `states` colors as [`NodeState::Idle`].
    pub fn set_node_states(&mut self, states: HashMap<uuid::Uuid, NodeState>) {
        self.node_states = states
            .into_iter()
            .filter_map(|(id, state)| self.graph.get_node_by_id(id).map(|(key, _)| (key, state)))
            .collect();
    }

    /// The URL of the single focused (selected) node, if exactly one node is
    /// selected. The host reads this to project the focused node's media — e.g.
    /// meerkat's floating content card. `None` when zero or many are selected.
    pub fn focused_url(&self) -> Option<&str> {
        if self.selected.len() != 1 {
            return None;
        }
        let key = *self.selected.iter().next()?;
        self.graph.get_node(key).map(|n| n.url())
    }

    /// The focused node's center in **content-band** screen px (its world
    /// position through the camera, `screen = world * zoom + offset`), if exactly
    /// one node is selected. The host anchors the floating card next to this; add
    /// the toolbar height for window coords. `None` when zero or many are
    /// selected, or the node has no position yet. (Card system: anchored card.)
    pub fn focused_node_screen(&self) -> Option<(f32, f32)> {
        if self.selected.len() != 1 {
            return None;
        }
        let key = *self.selected.iter().next()?;
        let world = self.view.position_of(key)?;
        let (ox, oy) = self.camera.offset;
        let z = self.camera.zoom;
        Some((world.x * z + ox, world.y * z + oy))
    }

    /// The member id (node UUID) of the single focused node, if exactly one is
    /// selected. The host targets per-node navigation (omnibar, back/forward) at
    /// it in Cartography; in Tree the host uses the focused tile's member.
    pub fn focused_member(&self) -> Option<uuid::Uuid> {
        if self.selected.len() != 1 {
            return None;
        }
        let key = *self.selected.iter().next()?;
        self.graph.get_node(key).map(|n| n.id)
    }

    /// Navigate `member` in place to `url`: the node is a browsing surface whose
    /// content changes; its position does not (no new node, no edge, no
    /// re-settle). The within-node history grows. Returns false if `member` is
    /// unknown. Per-node navigation (the node-lineage model).
    pub fn navigate_member(&mut self, member: uuid::Uuid, url: &str) -> bool {
        let Some((key, _)) = self.graph.get_node_by_id(member) else {
            return false;
        };
        self.graph.navigate_node(key, url);
        true
    }

    /// Step `member` back one visit in its own browse history, returning the
    /// revealed URL (the host re-fetches / re-renders it). `None` at the root.
    pub fn member_history_back(&mut self, member: uuid::Uuid) -> Option<String> {
        let (key, _) = self.graph.get_node_by_id(member)?;
        self.graph.node_history_back(key)
    }

    /// Step `member` forward one visit in its own history. `None` at the tip.
    pub fn member_history_forward(&mut self, member: uuid::Uuid) -> Option<String> {
        let (key, _) = self.graph.get_node_by_id(member)?;
        self.graph.node_history_forward(key)
    }

    /// Whether `member`'s history can step back (toolbar enablement).
    pub fn member_can_back(&self, member: uuid::Uuid) -> bool {
        match self.graph.get_node_by_id(member) {
            Some((key, _)) => self.graph.node_can_back(key),
            None => false,
        }
    }

    /// Whether `member` has ever been visited — it has a current entry in the
    /// shared navigation memory. The host shows a "last visit" snapshot for a
    /// visited node and the "unvisited" placeholder otherwise. (Card system #4.)
    pub fn member_visited(&self, member: uuid::Uuid) -> bool {
        match self.graph.get_node_by_id(member) {
            Some((key, _)) => self.graph.node_current_url(key).is_some(),
            None => false,
        }
    }

    /// Whether `member`'s history can step forward (toolbar enablement).
    pub fn member_can_forward(&self, member: uuid::Uuid) -> bool {
        match self.graph.get_node_by_id(member) {
            Some((key, _)) => self.graph.node_can_forward(key),
            None => false,
        }
    }

    /// The graph members (node UUIDs) of the currently-selected nodes. The host
    /// reads this for a selection-driven open: a single selection opens that
    /// node's graphlet, a multi-selection opens the selected nodes.
    pub fn selected_members(&self) -> Vec<uuid::Uuid> {
        self.selected.iter().filter_map(|&k| self.graph.get_node(k).map(|n| n.id)).collect()
    }

    /// The members in `member`'s connected component — `member` plus every node
    /// reachable from it through relations (undirected), breadth-first from the
    /// queried node. Empty if `member` is not in the graph. This is the node's
    /// "graphlet"; the host intersects it with the warm-tab set to decide what to
    /// tile.
    pub fn connected_members(&self, member: uuid::Uuid) -> Vec<uuid::Uuid> {
        let Some((start, _)) = self.graph.get_node_by_id(member) else {
            return Vec::new();
        };
        let mut seen = HashSet::new();
        let mut order = Vec::new();
        let mut queue = VecDeque::new();
        seen.insert(start);
        queue.push_back(start);
        while let Some(key) = queue.pop_front() {
            if let Some(node) = self.graph.get_node(key) {
                order.push(node.id);
            }
            for neighbor in self.graph.neighbors_undirected_sorted(key) {
                if seen.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
        order
    }

    /// The session graph, for the host to persist (`to_snapshot` → `graph.json`).
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// The current camera (pan + zoom), for the host to persist as view-intent.
    pub fn camera(&self) -> CameraView {
        CameraView { offset: self.camera.offset, zoom: self.camera.zoom }
    }

    /// Restore the camera from persisted view-intent. A non-finite or
    /// non-positive zoom falls back to `1.0`; the zoom is clamped to the orrery's
    /// range. The host suppresses its own first-frame recenter when it restores a
    /// camera, so this value is not immediately overwritten.
    pub fn set_camera(&mut self, view: CameraView) {
        self.camera.zoom = if view.zoom.is_finite() && view.zoom > 0.0 {
            view.zoom.clamp(MIN_ZOOM, MAX_ZOOM)
        } else {
            1.0
        };
        self.camera.offset = view.offset;
    }

    /// Zoom by `factor`, keeping the world point under `anchor` (screen px) fixed.
    fn zoom_at(&mut self, anchor: (f32, f32), factor: f32) {
        let new_zoom = (self.camera.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let applied = new_zoom / self.camera.zoom;
        self.camera.offset.0 = anchor.0 - (anchor.0 - self.camera.offset.0) * applied;
        self.camera.offset.1 = anchor.1 - (anchor.1 - self.camera.offset.1) * applied;
        self.camera.zoom = new_zoom;
    }

    /// Map a screen-px point back to world space through the camera
    /// (`world = (screen - offset) / zoom`).
    fn screen_to_world(&self, (sx, sy): (f32, f32)) -> Point2D<f32> {
        let (ox, oy) = self.camera.offset;
        let z = self.camera.zoom.max(f32::EPSILON);
        Point2D::new((sx - ox) / z, (sy - oy) / z)
    }

    /// The screen viewport mapped to world space — the region gyre culls against
    /// to decide which nodes are on screen.
    fn world_viewport(&self) -> Box2D<f32> {
        Box2D::new(
            self.screen_to_world((0.0, 0.0)),
            self.screen_to_world((self.view_w as f32, self.view_h as f32)),
        )
    }
}

#[cfg(test)]
mod tests;
