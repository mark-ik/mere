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
use kernel::geometry::PortablePoint;
use kernel::graph::{EdgeAssertion, FieldId, Graph, NodeKey, RelationSelector, SemanticSubKind};
use platen::scene_paint::{Camera, ScenePaintStyle};
use serval_layout::IncrementalLayout;
use serval_scripted_dom::{NodeId as DomNodeId, ScriptedDom};

mod build;
use build::{build_pool_dom, build_simulation, dark_scene_style, dedup_edges, sample_graph, surface_bg};
use paint_list_api::ColorF;

mod types;
pub use types::{CameraView, NodeShape, NodeState, PointerButton};

mod input;
mod frame;
mod fields;

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
    /// Whether the layout physics is paused (the user froze the graph with Space /
    /// the pause button). While paused the sim is halted and settle requests are
    /// suppressed, so the graph holds still through mutations until resumed.
    physics_paused: bool,
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
    /// The content-surface backdrop color (themed; the host pushes it via
    /// [`set_palette`](Self::set_palette)). Defaults to the dark slate.
    backdrop: ColorF,
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
    /// An in-progress field move / resize drag, if any. (Field regions.)
    field_drag: Option<fields::FieldDrag>,
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
    /// The field the cursor is over (hover) — drives box-on-interaction: a field's
    /// dashed extent box draws only while it is the active field; the soft disk well
    /// is always shown. `None` when the cursor is over no field. (Field regions.)
    active_field: Option<FieldId>,
    /// Fields the user has hidden from the canvas (the roster's hide toggle). The
    /// field pass skips these; the field and its coupling persist (hiding is
    /// display-only). In-session, mirroring `hidden_edges`. (Field regions.)
    hidden_fields: HashSet<FieldId>,
    /// Per-node activation state the host pushes for node coloring (open / closed
    /// / idle). Resolved to `NodeKey` on set; a node absent here colors as `Idle`.
    node_states: HashMap<NodeKey, NodeState>,
    /// Per-node content silhouette the host pushes for node shaping. Resolved to
    /// `NodeKey` on set; a node absent here draws as `Square` (the default).
    node_shapes: HashMap<NodeKey, NodeShape>,
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
    /// The pane's active layout strategy (a cartography adapter `projection_id`, e.g.
    /// `"phyllotaxis.default"`), or `None` for the default force-directed (gyre) layout.
    /// Persisted per pane via view-intent; the host pushes positions for it via
    /// [`apply_strategy_positions`](Orrery::apply_strategy_positions). (Layout picker.)
    active_strategy: Option<String>,
    /// Buffered positions for the active non-gyre strategy, applied into `view` each
    /// frame **after** the physics snapshot (so they win over gyre regardless of the
    /// off-thread actor's timing). `None` under force-directed. (Layout picker.)
    strategy_positions: Option<Vec<(NodeKey, PortablePoint)>>,
    /// The pane's "scope" lens: when `Some`, the orrery renders only these nodes (a
    /// curated subset), projecting through a curated forme arrangement instead of the
    /// full Identity one. `None` shows the whole graph. (Curated orrery.)
    scope: Option<Vec<NodeKey>>,
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

    /// Re-point this orrery at a different session `graph` **in place**, keeping the
    /// (possibly offloaded) physics actor and the node-children pool alive. The
    /// graph is replaced wholesale, every derived view is reconciled to it
    /// (departed nodes drop, the spring topology and node pool rebuild), per-graph
    /// interaction state (selection, hidden edges, drags, pushed node states/shapes)
    /// is cleared, and each node is restored to its committed position and halted
    /// so the switched-to session looks as it was left rather than re-scrambling.
    /// This is the Model-A graph swap the multi-graph switch drives. (Multi-graph MG2.)
    pub fn set_graph(&mut self, graph: Graph) {
        self.graph = graph;
        self.selected.clear();
        self.selected_edges.clear();
        self.hidden_edges.clear();
        self.active_field = None;
        self.hidden_fields.clear();
        self.node_states.clear();
        self.node_shapes.clear();
        self.drag = None;
        self.field_drag = None;
        self.marquee = None;
        self.middle_drag = None;
        self.reconcile_derived();
        let positions: Vec<(NodeKey, Point2D<f32>)> = self
            .graph
            .nodes()
            .map(|(key, node)| {
                let p = node.projected_position();
                (key, Point2D::new(p.x, p.y))
            })
            .collect();
        for &(key, pos) in &positions {
            self.view.set_position(key, pos);
        }
        self.physics.seed(positions);
        self.physics.halt();
        self.generation += 1;
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
            physics_paused: false,
            view,
            node_dom,
            node_layout: None,
            gnode_of,
            stage_node,
            pool_w: 0,
            pool_h: 0,
            camera: Camera::default(),
            style: dark_scene_style(),
            backdrop: surface_bg(),
            generation: 0,
            cursor: (0.0, 0.0),
            pan_velocity: (0.0, 0.0),
            middle_drag: None,
            drag: None,
            field_drag: None,
            selected: HashSet::new(),
            selected_edges: HashSet::new(),
            hidden_edges: HashSet::new(),
            active_field: None,
            hidden_fields: HashSet::new(),
            node_states: HashMap::new(),
            node_shapes: HashMap::new(),
            marquee: None,
            ctrl: false,
            shift: false,
            view_w: 1024,
            view_h: 600,
            active_strategy: None,
            strategy_positions: None,
            scope: None,
        }
    }

    /// Set the viewport the orrery culls + centers against. The host calls this on
    /// a surface resize; the next [`frame`](Orrery::frame) rebuilds the node-pool
    /// layout at the new size.
    ///
    /// Keeps whatever world point sits at the viewport center fixed across the
    /// resize by shifting the offset by half the size delta (the camera maps
    /// `screen = world * zoom + offset`, so the center moves by half the change in
    /// each axis, independent of zoom). Without this, the startup 1024->2560 grow
    /// would leave a freshly centered camera anchored to the old, smaller center
    /// and slide the graph toward a corner.
    pub fn resize(&mut self, width: u32, height: u32) {
        let (new_w, new_h) = (width.max(1), height.max(1));
        self.camera.offset.0 += (new_w as f32 - self.view_w as f32) / 2.0;
        self.camera.offset.1 += (new_h as f32 - self.view_h as f32) / 2.0;
        self.view_w = new_w;
        self.view_h = new_h;
    }

    /// Put the world origin at the viewport center **at zoom 1** — the graph is
    /// laid out around `(0, 0)`, so this frames it. Resets the zoom too (a drifted
    /// zoom would otherwise leave the graph a speck or off-screen even after the
    /// offset is centered). (A fit-to-`content_bounds` camera replaces this when a
    /// real graph is hosted.)
    pub fn recenter(&mut self) {
        self.camera.offset = (self.view_w as f32 / 2.0, self.view_h as f32 / 2.0);
        self.camera.zoom = 1.0;
    }

    /// Whether the graph is empty, or at least one node lies within the current
    /// viewport. `false` means every node is off-screen — a degenerate camera (a
    /// restored pan/zoom that no longer frames the graph), which the host recovers
    /// with [`recenter`](Self::recenter).
    pub fn graph_visible(&self) -> bool {
        self.graph.nodes().next().is_none()
            || self.view.cull_aabb(self.world_viewport()).into_iter().next().is_some()
    }

    /// Whether the graph currently holds any nodes. The host gates its one-shot
    /// camera heal on this so the heal waits for an async session load to populate
    /// the graph, rather than firing (and spending its one shot) against the empty
    /// graph that exists for the first frames after launch — at which point
    /// [`graph_visible`](Self::graph_visible) is trivially true and would suppress
    /// the recenter the restored-camera case needs.
    pub fn has_nodes(&self) -> bool {
        self.graph.nodes().next().is_some()
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
        // Re-resolve field couplings against the new node set, so a field gathers
        // nodes added after it was placed (its targets snapshot at build time).
        // (Field regions — rebuild-on-mutation / new-node capture.)
        self.rebuild_coupling_forces();
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

    /// Park this orrery's physics: stop any in-progress settle so a backgrounded
    /// graph does not keep ticking and waking the host loop. The off-thread actor
    /// then idles on its command channel (no busy-spin, no CPU) and stays warm in
    /// the pool. The host calls this when a pooled graph loses focus. No explicit
    /// unpark is needed: the layout is left at its current positions (a restored
    /// session must not re-scramble), and any later interaction resumes the settle.
    /// (Window composition P1, OQ2 park.)
    pub fn park_physics(&mut self) {
        self.physics.halt();
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
        self.settle_physics(SETTLE_TICKS);
        true
    }

    /// Stamp a fetched favicon (RGBA8 + dimensions) onto the node currently at
    /// `url`, if one exists. A metadata-only change: unlike [`ingest_graph`], it
    /// neither reconciles the derived views nor disturbs the spatial layout (no node
    /// or edge is added), so a favicon arriving mid-browse does not jostle the field.
    /// The tile reads the stamped favicon on the next frame. Returns whether a node
    /// was found and its favicon changed. (Favicon-on-tile.)
    pub fn set_node_favicon(&mut self, url: &str, rgba: Vec<u8>, width: u32, height: u32) -> bool {
        let Some(key) = self.graph.get_node_by_url(url).map(|(k, _)| k) else {
            return false;
        };
        self.graph.set_node_favicon(key, rgba, width, height)
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

    /// Toggle `member` in or out of the selection (a multi-select add), keeping the
    /// rest selected — the member-keyed twin of the canvas's Shift-click. Clears the
    /// edge selection (matching that gesture) so a mixed node+edge selection can't
    /// confuse the pairwise relate. Returns `false` if the member is not in the
    /// graph. Selection is read live at frame time, so no reconcile is needed.
    pub fn toggle_select_member(&mut self, member: uuid::Uuid) -> bool {
        let Some(key) = self.graph.get_node_key_by_id(member) else {
            return false;
        };
        if !self.selected.remove(&key) {
            self.selected.insert(key);
        }
        self.selected_edges.clear();
        true
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

    /// Assert a semantic relation of `sub_kind` between exactly two selected
    /// nodes — the user-initiated edge-creation gesture the rich kernel taxonomy
    /// always supported but the UI never reached. The pair is ordered by node
    /// UUID so a symmetric relation is reproducible; the edge is created or
    /// merged (idempotent per sub-kind) via [`Graph::assert_relation`]. Returns
    /// `true` when an edge was asserted, `false` for any selection that is not a
    /// clean pair. The springs / drawn edges refresh on the next reconcile.
    pub fn assert_selected_relation(&mut self, sub_kind: SemanticSubKind) -> bool {
        if self.selected.len() != 2 {
            return false;
        }
        let mut pair: Vec<NodeKey> = self.selected.iter().copied().collect();
        pair.sort_by_key(|k| self.graph.get_node(*k).map(|n| n.id));
        // `assert_relation` returns `None` for a no-op re-assert (the sub-kind is
        // already present), so we don't gate success on its return: for a clean
        // pair the relation is present afterwards either way, which is what
        // "relate these two" means. Reconcile rebuilds edges / springs.
        self.graph.assert_relation(
            pair[0],
            pair[1],
            EdgeAssertion::Semantic {
                sub_kind,
                label: None,
                decay_progress: None,
            },
        );
        self.reconcile_derived();
        true
    }

    /// Insert `tag` on every selected node — the user-initiated tagging gesture
    /// (the context menu's "Add tag…"). Trims the tag; an empty tag or empty
    /// selection is a no-op. Returns how many nodes newly gained the tag (an
    /// already-tagged node counts 0). Tags are node truth the host persists; they
    /// do not affect layout, so no reconcile is needed.
    pub fn tag_selected(&mut self, tag: &str) -> usize {
        let tag = tag.trim();
        if tag.is_empty() {
            return 0;
        }
        let keys: Vec<NodeKey> = self.selected.iter().copied().collect();
        let mut tagged = 0;
        for key in keys {
            if self.graph.insert_node_tag(key, tag.to_string()) {
                tagged += 1;
            }
        }
        tagged
    }

    /// Retract the user-asserted semantic relation(s) on the selected edge(s) —
    /// a true removal, not the display-only [`hide_selected_edges`]. Scoped to the
    /// `Semantic` family, so navigation / provenance history on the same edge
    /// survives; an edge left with no families is garbage-collected by the kernel.
    /// Returns how many relations were retracted, and clears the edge selection.
    pub fn retract_selected_relation(&mut self) -> usize {
        let mut removed = 0;
        // Symmetric with `assert_selected_relation`: a two-node selection retracts
        // the relation between the pair (either stored direction), so `>unrelate`
        // mirrors `>relate` on the same gesture.
        if self.selected.len() == 2 {
            let mut pair: Vec<NodeKey> = self.selected.iter().copied().collect();
            pair.sort_by_key(|k| self.graph.get_node(*k).map(|n| n.id));
            removed += self.retract_semantic_between(pair[0], pair[1]);
            removed += self.retract_semantic_between(pair[1], pair[0]);
        }
        // Also retract any directly-selected edges (the click-an-edge path).
        for (a, b) in self.selected_edges.drain().collect::<Vec<_>>() {
            removed += self.retract_semantic_between(a, b);
        }
        if removed > 0 {
            self.reconcile_derived();
        }
        removed
    }

    /// Retract every semantic relation on the directed edge `a -> b`. The `Family`
    /// selector is read-only (not retractable), so enumerate the edge's semantic
    /// sub-kinds and retract each — the user-meaning relations go, while traversal
    /// / provenance history on the same edge survives (an edge left with no
    /// families is garbage-collected by the kernel). Returns how many were removed.
    fn retract_semantic_between(&mut self, a: NodeKey, b: NodeKey) -> usize {
        let sub_kinds: Vec<SemanticSubKind> = self
            .graph
            .find_edge_key(a, b)
            .and_then(|k| self.graph.get_edge(k))
            .and_then(|p| p.semantic_data())
            .map(|s| s.sub_kinds.iter().copied().collect())
            .unwrap_or_default();
        let mut removed = 0;
        for sk in sub_kinds {
            removed += self
                .graph
                .retract_relations(a, b, RelationSelector::Semantic(sk));
        }
        removed
    }

    /// Whether any edge is currently selected (the host routes a `Delete` to edge
    /// retraction when so, else to node deletion).
    pub fn has_selected_edges(&self) -> bool {
        !self.selected_edges.is_empty()
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

    /// Graph geometry for a minimap swatch (gloss): each node's `(uuid, world
    /// position, selected)` and each visible edge as a world-space `(from, to)`
    /// segment. World coordinates — the consumer fits them into its own rect. The
    /// gloss pane draws its own swatch from this rather than rendering a second
    /// orrery (the Navigator is one surface, never a second instance).
    #[allow(clippy::type_complexity)]
    pub fn minimap_geometry(
        &self,
    ) -> (Vec<(uuid::Uuid, (f32, f32), bool)>, Vec<((f32, f32), (f32, f32))>) {
        let nodes = self
            .view
            .positions()
            .filter_map(|(key, p)| {
                self.graph
                    .get_node(key)
                    .map(|node| (node.id, (p.x, p.y), self.selected.contains(&key)))
            })
            .collect();
        let edges = self
            .view
            .edge_segments()
            .filter_map(|(a, b, pa, pb)| {
                let pair = if a <= b { (a, b) } else { (b, a) };
                (!self.hidden_edges.contains(&pair)).then_some(((pa.x, pa.y), (pb.x, pb.y)))
            })
            .collect();
        (nodes, edges)
    }

    /// The Cartography projection geometry: each member's current world position,
    /// member-keyed — the orrery's settled layout, for the host to persist as the
    /// cartography sidecar (the counterpart of the workbench's `TreeGeometry`). Reads
    /// the live gyre positions, so it captures whatever is shown (force-directed or a
    /// picked layout strategy). (Position sidecar.)
    pub fn cartography_geometry(&self) -> platen::CartographyGeometry {
        platen::CartographyGeometry::from_positions(
            self.view
                .positions()
                .filter_map(|(key, p)| self.graph.get_node(key).map(|node| (node.id, (p.x, p.y)))),
        )
    }

    /// Seed node positions from a persisted cartography sidecar, overriding the graph's
    /// load-time seed so a reloaded session shows its settled layout rather than
    /// re-scrambling. Members absent from the sidecar keep their existing seed (a node
    /// added since the last save still shows); physics halts so the restored layout
    /// holds until the user nudges it. (Position sidecar.)
    pub fn seed_cartography(&mut self, positions: impl IntoIterator<Item = (uuid::Uuid, (f32, f32))>) {
        let resolved: Vec<(NodeKey, Point2D<f32>)> = positions
            .into_iter()
            .filter_map(|(id, (x, y))| {
                self.graph.get_node_by_id(id).map(|(key, _)| (key, Point2D::new(x, y)))
            })
            .collect();
        if resolved.is_empty() {
            return;
        }
        for &(key, pos) in &resolved {
            self.view.set_position(key, pos);
        }
        self.physics.seed(resolved);
        self.physics.halt();
    }

    /// Theme the orrery's surfaces: the content-surface `backdrop` and the `edge`
    /// stroke color, as straight `[r, g, b, a]` (0..1). The host pushes these from
    /// the active theme so the graph re-themes with the chrome. Node *state* colors
    /// (open / closed / idle / selected) stay semantic and are not rethemed here.
    pub fn set_palette(&mut self, backdrop: [f32; 4], edge: [f32; 4]) {
        self.backdrop = ColorF::new(backdrop[0], backdrop[1], backdrop[2], backdrop[3]);
        self.style.edge_color = ColorF::new(edge[0], edge[1], edge[2], edge[3]);
    }

    /// Set the per-node content silhouettes the orrery shapes its on-screen nodes
    /// by, keyed by node UUID; the orrery resolves each to its `NodeKey`. The host
    /// recomputes + pushes this from each node's content type as content is
    /// fetched; a node absent from `shapes` draws as [`NodeShape::Square`].
    pub fn set_node_shapes(&mut self, shapes: HashMap<uuid::Uuid, NodeShape>) {
        self.node_shapes = shapes
            .into_iter()
            .filter_map(|(id, shape)| self.graph.get_node_by_id(id).map(|(key, _)| (key, shape)))
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

    /// The pane's active layout-strategy id, or `None` for force-directed (gyre).
    /// The host persists this as view-intent and checkmarks it in the layout picker.
    pub fn layout_strategy(&self) -> Option<&str> {
        self.active_strategy.as_deref()
    }

    /// Switch the orrery's layout strategy. `Some(id)` selects a cartography adapter
    /// (the host then pushes its positions via [`apply_strategy_positions`]) and halts
    /// gyre so the analytic layout holds still; `None` reverts to force-directed,
    /// dropping the buffered positions and re-settling the physics. (Layout picker.)
    pub fn set_layout_strategy(&mut self, id: Option<String>) {
        let reverting = id.is_none() && self.active_strategy.is_some();
        self.active_strategy = id;
        if self.active_strategy.is_some() {
            self.physics.halt();
        } else if reverting {
            self.strategy_positions = None;
            self.settle_physics(SETTLE_TICKS);
        }
    }

    /// Buffer the active strategy's node positions (host-computed through platen's
    /// cartography dispatch). They are written into the read model each frame after
    /// the physics snapshot, so they take effect regardless of the off-thread sim's
    /// timing. A no-op unless a strategy is active. (Layout picker.)
    pub fn apply_strategy_positions(&mut self, positions: &[(NodeKey, PortablePoint)]) {
        if self.active_strategy.is_some() {
            self.strategy_positions = Some(positions.to_vec());
        }
    }

    /// Overlay the buffered strategy positions onto `view` — called by
    /// [`frame`](Orrery::frame) right after the physics snapshot, so the underlay,
    /// DOM nodes, cull, and edges (all reading `view`) stay consistent in one write.
    /// A no-op under force-directed. (Layout picker.)
    fn apply_strategy_to_view(&mut self) {
        let Some(positions) = self.strategy_positions.take() else {
            return;
        };
        for &(key, p) in &positions {
            self.view.set_position(key, Point2D::new(p.x, p.y));
        }
        self.strategy_positions = Some(positions);
    }

    /// Whether a scope lens is active (the orrery is showing a curated subset, not the
    /// whole graph). The host offers "Show all" when this is true. (Curated orrery.)
    pub fn is_scoped(&self) -> bool {
        self.scope.is_some()
    }

    /// Focus the orrery on the current selection: scope it to the selected nodes plus
    /// their immediate (undirected) neighbors, so the selection shows as its own
    /// neighborhood projected through a curated arrangement. A no-op with no
    /// selection. (Curated orrery.)
    pub fn isolate_selection(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        let mut scope: HashSet<NodeKey> = self.selected.clone();
        for &key in &self.selected {
            scope.extend(self.graph.neighbors_undirected(key));
        }
        self.scope = Some(scope.into_iter().collect());
    }

    /// Drop the scope lens — show the whole graph again. (Curated orrery.)
    pub fn clear_scope(&mut self) {
        self.scope = None;
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
