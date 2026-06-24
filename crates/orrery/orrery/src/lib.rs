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
/// The declarative scene catalog, re-exported so hosts (and the standalone bin) can load
/// a scene by name without depending on `gyre` directly. (Physics scenes P4a.)
pub use gyre::{
    NODE_BODY_DENSITY, NodeMaterial, SceneSpec, ball_and_chain_scene, bridge_scene, chain_scene,
    cradle_scene, domino_scene, drift_scene, drop_bowl_scene, fountain_scene, funnel_scene,
    galton_scene, mixer_scene, pyramid_scene, whirlpool_scene,
};
use kernel::geometry::PortablePoint;
use kernel::graph::{EdgeAssertion, FieldId, Graph, NodeKey, RelationSelector, SemanticSubKind};
use platen::scene_paint::{Camera, ScenePaintStyle};
use serval_layout::IncrementalLayout;
use serval_scripted_dom::{NodeId as DomNodeId, ScriptedDom};

mod build;
use build::{build_pool_dom, build_simulation, dark_scene_style, dedup_edges, sample_graph, surface_bg};
use paint_list_api::ColorF;

mod types;
pub use types::{CameraView, Face, NodeShape, NodeState, PointerButton, Viewport};
pub use signals::ImportanceMetric;

mod input;
mod frame;
mod fields;

/// Ambient-sim backdrops (non-rapier liveliness painted behind the graph): Conway's
/// [`GameOfLife`] is the first. (Physics scenes P5.)
mod ambient;
pub use ambient::{AmbientSim, GameOfLife, NBody, ParticleLife, SandFall, Tincture};

mod physics;
use physics::Physics;

/// Force-directed settle length (frames) after a (re)seed, ~6s at 60fps.
const SETTLE_TICKS: u32 = 360;
/// A gentle re-separation burst (~1.5s at 60fps) after node colliders resize, so
/// neighbors ease away from a grown node without a full re-layout. (P0/P5 collider.)
const SIZE_RESETTLE_TICKS: u32 = 90;
/// The five face-size presets (px) the on-graph size editor steps between — the notch
/// points of the snapshot-card resize control. The default (36) is tier 1 (0-indexed), so
/// an un-sized node reads as the second notch; the ends span dense-small to big-hub, inside
/// the `set_node_size` 16..160 clamp. (Node-rep — size tiers.)
pub const SIZE_TIERS: [f32; 5] = [24.0, 36.0, 56.0, 84.0, 120.0];
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
/// Orbit-drag sensitivity: yaw radians per screen-px of horizontal Alt+drag (a ~30px drag ≈ the
/// `q`/`e` keystep). (Isometric camera — orbit gesture.)
const ORBIT_YAW_PER_PX: f32 = 0.005;
/// Orbit-drag sensitivity: tilt change per screen-px of vertical Alt+drag (`set_tilt` clamps).
const ORBIT_TILT_PER_PX: f32 = 0.004;
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
    /// `Some(last_cursor)` while an Alt+left-button **orbit** drag is in progress (horizontal =
    /// yaw, vertical = tilt). Sibling to `middle_drag`'s pan. (Isometric camera — orbit gesture.)
    orbit_drag: Option<(f32, f32)>,
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
    /// Per-node **face** overrides (the user's per-node texture choice on the Face axis). A
    /// node absent here defaults to [`Face::Favicon`], so this holds only explicit overrides,
    /// never the whole graph. Independent of the body (the collider hull). (Node body & face.)
    node_faces: HashMap<NodeKey, Face>,
    /// Per-node face footprint overrides (px). A node absent here takes size-by-degree
    /// (when on) or the uniform default, so this holds only explicit resizes. (P0 resize.)
    node_sizes: HashMap<NodeKey, f32>,
    /// Per-node custom sprite faces: the imported image as a PNG data-URI. A node here
    /// renders with [`Face::Sprite`] (set together via [`set_node_sprite`]); absent means no
    /// sprite. Persisted in the cartography sidecar. (Node body & face — sprite face.)
    node_sprites: HashMap<NodeKey, String>,
    /// Per-node sprite collider hulls: the sprite's opaque region as a convex polygon, in
    /// face-normalized coords ([-0.5, 0.5], scaled to `node_size` at collider time). The host
    /// traces it from the image at import, or the user authors it in the shape editor; a node
    /// with one collides at its hull rather than its silhouette, regardless of its face.
    /// Persisted in the cartography sidecar. (Node body & face — the Body axis.)
    node_sprite_hulls: HashMap<NodeKey, Vec<(f32, f32)>>,
    /// Scene-prop sprite textures, keyed by the opaque handle a [`gyre::SceneBodySpec`] carries:
    /// raw RGBA8 (straight alpha) plus dimensions, the [`PaintCmd::DrawImage`] shape (as favicons
    /// use). A scene prop whose `sprite` handle resolves here paints as a textured billboard instead
    /// of the abstract orb / polygon; unset handles fall back. A registry (persists across scenes,
    /// not cleared on `clear_scene`), populated via [`register_scene_sprite`](Self::register_scene_sprite).
    /// (Physics scenes — scene-prop sprites.)
    scene_sprite_textures: HashMap<String, (Vec<u8>, u32, u32)>,
    /// An optional ambient-sim backdrop (a non-rapier sim painted behind the graph for atmosphere):
    /// any [`AmbientSim`] (Game of Life, n-body drift, particle-life), advanced + painted as the
    /// bottom layer each [`frame`](Self::frame). `None` until one is loaded. (Physics scenes P5.)
    ambient: Option<Box<dyn AmbientSim>>,
    /// The base colour (tincture) the active ambient backdrop is painted in. Set from the sim's
    /// default on load; overridable via [`set_ambient_tincture`](Self::set_ambient_tincture). (P5.)
    ambient_tincture: Tincture,
    /// Per-node physical **material** overrides (restitution / friction / density) on the Body
    /// axis. A node absent here takes the default [`gyre::NodeMaterial`] (the spawn values), so
    /// this holds only deliberate overrides. Pushed to physics and persisted in the cartography
    /// sidecar. (Node body & face — material.)
    node_materials: HashMap<NodeKey, gyre::NodeMaterial>,
    /// Scene toggle: when on, a node's face grows with its undirected degree (capped),
    /// so the spatial map reads connection weight at a glance. Default off (uniform).
    /// (P0 resize — size-by-degree.)
    size_by_degree: bool,
    /// Scene toggle: when on, a node's face grows with its **importance signal** (the
    /// graph-signals layer — degree-based now, betweenness later), normalized so the most
    /// important node hits the cap. The signal-driven size channel; wins over size-by-degree,
    /// loses to a manual override. Default off. (Graph signals — importance encoding.)
    size_by_importance: bool,
    /// Which metric [`size_by_importance`](Self::size_by_importance) reads: degree (cheap default)
    /// or betweenness (structural brokerage). A per-scene choice. (Graph signals — importance metric.)
    importance_metric: signals::ImportanceMetric,
    /// The cached per-node importance (normalized `0..=1`), refreshed from `signals` whenever
    /// geometry is pushed while [`size_by_importance`](Self::size_by_importance) is on (the
    /// normalization needs all nodes, so it is computed once per change, not per `node_size`
    /// call). Empty when the mode is off. (Graph signals — importance encoding.)
    node_importance: HashMap<NodeKey, f32>,
    /// Whether [`node_importance`](Self::node_importance) is stale and must be recomputed before
    /// the next read. Set by [`reconcile_derived`](Self::reconcile_derived) (the topology-change
    /// hook) and on enabling the mode; cleared after a recompute. The cheap-signal cache: degree
    /// importance recomputes only on a topology change, not on every geometry push (a size-only
    /// change does not dirty it). The generation + per-signal-dirty-bit + background-lane cache
    /// the plan describes is the *expensive*-signal substrate (betweenness / communities), built
    /// when those land. (Graph signals — the cheap-signal cache.)
    importance_dirty: bool,
    /// Scene toggle: when on, a node floats above the ground by its undirected degree
    /// (hubs highest), with a stem to its ground anchor — the isometric "fake height".
    /// Purely visual (the gyre body does not move). Default off. (Isometric camera P3.)
    height_by_degree: bool,
    /// `Some(press_origin)` (screen px) while a left-drag marquee on empty space
    /// is in progress.
    marquee: Option<(f32, f32)>,
    /// Whether Ctrl is held (gates wheel-zoom vs wheel-pan).
    ctrl: bool,
    /// Whether Shift is held (a node click adds to / toggles the selection rather
    /// than replacing it — multi-select).
    shift: bool,
    /// Whether Alt is held (a left-drag orbits the camera instead of picking / marqueeing).
    /// (Isometric camera — orbit gesture.)
    alt: bool,
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
    /// When set, the scene omits the on-screen gnode + favicon layers: the host renders
    /// those nodes as DOM cards in the shell document instead (the focused orrery only;
    /// secondary panes keep their gnodes). Edges + demoted dots stay as the underlay.
    /// (Orrery-as-element — Phase 2.)
    render_as_cards: bool,
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
        self.node_faces.clear();
        self.node_sizes.clear();
        self.node_sprites.clear();
        self.node_sprite_hulls.clear();
        self.node_materials.clear();
        self.node_importance.clear();
        self.importance_dirty = true;
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
            orbit_drag: None,
            drag: None,
            field_drag: None,
            selected: HashSet::new(),
            selected_edges: HashSet::new(),
            hidden_edges: HashSet::new(),
            active_field: None,
            hidden_fields: HashSet::new(),
            node_states: HashMap::new(),
            node_shapes: HashMap::new(),
            node_faces: HashMap::new(),
            node_sizes: HashMap::new(),
            node_sprites: HashMap::new(),
            node_sprite_hulls: HashMap::new(),
            scene_sprite_textures: HashMap::new(),
            ambient: None,
            ambient_tincture: ColorF::new(0.0, 0.0, 0.0, 0.0),
            node_materials: HashMap::new(),
            size_by_degree: false,
            size_by_importance: false,
            importance_metric: signals::ImportanceMetric::Degree,
            node_importance: HashMap::new(),
            importance_dirty: true,
            height_by_degree: false,
            marquee: None,
            ctrl: false,
            shift: false,
            alt: false,
            view_w: 1024,
            view_h: 600,
            active_strategy: None,
            strategy_positions: None,
            scope: None,
            render_as_cards: false,
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
        // The graph topology changed (this is the topology-change hook), so any degree-derived
        // signal is stale: mark the importance cache for recompute on the next push. (Graph signals.)
        self.importance_dirty = true;
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
        // Degree-based sizes shift when the topology changes, so re-push radii (also
        // seeds them for newly-spawned bodies). No re-settle here: the structural
        // change's own settle covers it.
        self.push_node_geometry();
    }

    /// Push every node's collider/pick radius (`node_size / 2`) to the read-model view
    /// and the physics bodies, so a node sized in the view — size-by-degree or a per-node
    /// footprint — picks and collides at its true face (Decision 5). A no-op visually when
    /// every node is the uniform default. Does not settle; callers that change a size knob
    /// follow with [`resettle_for_size`](Self::resettle_for_size) so neighbors re-separate.
    /// (P0/P5 collider.)
    fn push_node_geometry(&mut self) {
        // Refresh the cached importance before sizing, so size-by-importance reads a current
        // snapshot (the normalization needs all nodes; do it once here, not per `node_size`).
        if self.size_by_importance {
            self.recompute_importance();
        }
        let radii: Vec<(NodeKey, f32)> = self
            .graph
            .nodes()
            .map(|(key, _)| (key, self.node_size(key) / 2.0))
            .collect();
        self.view.set_radii(radii.iter().copied());
        // The physics collider matches the *shape*, not just the size: a square node collides
        // square, a circle round (Decision 1 — the face is the collider). The view keeps the
        // bounding radius above for the pick + edge-trim. (Node-rep — collider matches shape.)
        let colliders: Vec<(NodeKey, gyre::NodeCollider)> =
            self.graph.nodes().map(|(key, _)| (key, self.node_collider(key))).collect();
        self.physics.set_node_colliders(colliders);
    }

    /// The collider shape for a node: the node's **body**. A node with a custom hull (the
    /// Body axis: traced from a sprite or hand-authored in the shape editor) collides at that
    /// hull, scaled from face-normalized coords to the current [`node_size`](Self::node_size);
    /// otherwise its content silhouette ([`node_shape`](Self::node_shape)) at its footprint.
    /// The hull applies **regardless of the face** ([`node_face`](Self::node_face)), so a node
    /// can wear a favicon over a custom-shaped body. So physics tracks the body, not just a
    /// bounding box. (Node body & face — the Body axis.)
    fn node_collider(&self, key: NodeKey) -> gyre::NodeCollider {
        let size = self.node_size(key);
        let half = size / 2.0;
        // A custom hull (sprite-traced or hand-authored) collides at that hull, scaled to the
        // face. Independent of the face texture — the body is its own axis.
        if let Some(hull) = self.node_sprite_hulls.get(&key).filter(|h| h.len() >= 3) {
            let points = hull.iter().map(|&(nx, ny)| (nx * size, ny * size)).collect();
            return gyre::NodeCollider::Hull { points, fallback: half };
        }
        match self.node_shape(key) {
            NodeShape::Circle => gyre::NodeCollider::Ball { radius: half },
            NodeShape::Square => gyre::NodeCollider::Square { half },
            NodeShape::Rounded => gyre::NodeCollider::RoundedSquare { half, border: half * 0.3 },
        }
    }

    /// Re-push radii and kick a gentle re-separation burst — the response to a size knob
    /// (a per-node footprint, the size-by-degree toggle) moving on an otherwise idle graph,
    /// so grown nodes actually push their neighbors apart. (P0/P5 collider.)
    fn resettle_for_size(&mut self) {
        self.push_node_geometry();
        self.physics.settle(SIZE_RESETTLE_TICKS);
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
        // A node-position-only refresh (no scene bodies / fluid here); the actor's per-tick
        // snapshots carry the live scene + fluid. (Physics scenes P1 / P4c.)
        self.view.apply_snapshot(&LayoutSnapshot {
            positions,
            scene: Vec::new(),
            fluid: Vec::new(),
            fluid_radius: 0.0,
            generation: self.generation,
        });
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

    /// The single focused node key, for layout strategies that center on a selection
    /// (radial). `None` when zero or many nodes are selected, so a focus-driven layout
    /// stays well-defined (it no-ops rather than picking an arbitrary node from a
    /// multi-selection). (Layout picker — radial.)
    pub fn focused_key(&self) -> Option<NodeKey> {
        match self.selected.len() {
            1 => self.selected.iter().copied().next(),
            _ => None,
        }
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
        // Persist the deliberate per-node size overrides + the size-by-degree scene flag
        // alongside the positions, so a sized graph re-opens sized. Only explicit overrides
        // travel; degree-derived sizes recompute from the flag. (Node-rep — size persistence.)
        .with_sizes(
            self.node_sizes
                .iter()
                .filter_map(|(&key, &size)| self.graph.get_node(key).map(|node| (node.id, size))),
        )
        .with_size_by_degree(self.size_by_degree)
        .with_size_by_importance(self.size_by_importance)
        .with_importance_metric(self.importance_metric.as_code())
        // Persist the custom sprite faces (the imported image as a data-URI) so a node textured
        // with one re-opens textured, not reverted to its default face. (Node-rep — sprite persistence.)
        .with_sprites(self.node_sprites.iter().filter_map(|(&key, uri)| {
            self.graph.get_node(key).map(|node| (node.id, uri.clone()))
        }))
        // Persist each sprite's collider hull alongside it, so the traced-to-image collider
        // survives a reload without re-decoding the image. (Node-rep — sprite hull persistence.)
        .with_sprite_hulls(self.node_sprite_hulls.iter().filter_map(|(&key, hull)| {
            self.graph.get_node(key).map(|node| (node.id, hull.clone()))
        }))
        // Persist the per-node physical material overrides (restitution / friction / density),
        // so a node tuned heavier / bouncier re-opens that way. (Node body & face — material.)
        .with_materials(self.node_materials.iter().filter_map(|(&key, mat)| {
            self.graph
                .get_node(key)
                .map(|node| (node.id, (mat.restitution, mat.friction, mat.density)))
        }))
        // Persist the per-node face overrides (favicon / sprite / bare), so a node's chosen
        // texture re-opens that way (the body persists separately). (Node body & face — face.)
        .with_faces(self.node_faces.iter().filter_map(|(&key, &face)| {
            self.graph.get_node(key).map(|node| (node.id, face.as_code().to_string()))
        }))
    }

    /// One node's current world position (the live gyre position), so the host can
    /// draw a switcher thumbnail from the orrery rather than the now position-free
    /// graph. `None` for a node the view has not placed. (Position gut.)
    pub fn node_position(&self, key: NodeKey) -> Option<PortablePoint> {
        self.view.position_of(key).map(|p| PortablePoint::new(p.x, p.y))
    }

    /// A node's render color, matching the on-screen tile's class palette: orange when
    /// selected, else green open / red closed / blue idle. The host colors the orrery
    /// element's DOM node-cards with it so they carry node identity without the gnode
    /// layer. (Orrery-as-element — Phase 2.)
    pub fn node_color(&self, key: NodeKey) -> &'static str {
        if self.selected.contains(&key) {
            return "#f7a440";
        }
        match self.node_states.get(&key) {
            Some(NodeState::Open) => "#5fb878",
            Some(NodeState::Closed) => "#cc5a54",
            _ => "#5a8fc8",
        }
    }

    /// A node's activation-state color WITHOUT the selection override — green open /
    /// red closed / blue idle. The card form colors its face with this and shows
    /// selection as a ring + lift instead, so the color channel stays free to carry
    /// activation state. The in-scene tile path still uses [`node_color`](Self::node_color).
    pub fn node_state_color(&self, key: NodeKey) -> &'static str {
        match self.node_states.get(&key) {
            Some(NodeState::Open) => "#5fb878",
            Some(NodeState::Closed) => "#cc5a54",
            _ => "#5a8fc8",
        }
    }

    /// Whether `key` is in the current selection set, so a node's representation can show
    /// selection through geometry (a ring + lift) rather than recoloring its face.
    pub fn node_selected(&self, key: NodeKey) -> bool {
        self.selected.contains(&key)
    }

    /// A node's content-type silhouette (square document / rounded menu / circle feed),
    /// for the card form to shape its face. `Square` (the default) for an unmapped node.
    pub fn node_shape(&self, key: NodeKey) -> NodeShape {
        self.node_shapes.get(&key).copied().unwrap_or_default()
    }

    /// A node's **face** (the texture on its body): a per-node override if the user set one
    /// (via [`set_node_face`](Self::set_node_face) or [`set_node_sprite`](Self::set_node_sprite)),
    /// otherwise the default [`Favicon`](Face::Favicon). Independent of the body (the collider
    /// hull). A future scene pane (Decision 6) can diversify the default by content type.
    /// (Node body & face — the Face axis.)
    pub fn node_face(&self, key: NodeKey) -> Face {
        self.node_faces.get(&key).copied().unwrap_or_default()
    }

    /// Render the on-screen nodes as host DOM cards instead of in-scene gnodes: the
    /// next [`frame`](Orrery::frame) drops the gnode + favicon layers, keeping edges +
    /// demoted dots as the underlay. The host sets this on the focused orrery (whose
    /// cards it snapshots) and leaves it off on secondary panes. (Orrery-as-element.)
    pub fn set_render_as_cards(&mut self, on: bool) {
        self.render_as_cards = on;
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

    /// Restore the per-node size overrides + the size-by-degree scene flag from a persisted
    /// cartography sidecar (the sizing counterpart of [`seed_cartography`]). Pushes the
    /// resulting collider/pick radii to the view + bodies but does **not** settle, so the
    /// restored layout holds (the position seed already halted physics). Sizes are clamped
    /// like [`set_node_size`]; an unknown member id is skipped. (Node-rep — size persistence.)
    pub fn apply_cartography_sizing(
        &mut self,
        sizes: impl IntoIterator<Item = (uuid::Uuid, f32)>,
        size_by_degree: bool,
        size_by_importance: bool,
    ) {
        self.size_by_degree = size_by_degree;
        // Restore size-by-importance before `push_node_geometry` below, and force the cache stale
        // so the push actually recomputes: on a reused orrery (a session switch) `importance_dirty`
        // may already be clean, which would otherwise leave the restored mode with an empty map and
        // every node at the default size. (Graph signals — restore the importance encoding.)
        self.size_by_importance = size_by_importance;
        if size_by_importance {
            self.importance_dirty = true;
        }
        for (id, size) in sizes {
            if let Some((key, _)) = self.graph.get_node_by_id(id) {
                self.node_sizes.insert(key, size.clamp(16.0, 160.0));
            }
        }
        self.push_node_geometry();
    }

    /// Restore the per-node sprite faces from a persisted cartography sidecar (the sprite
    /// counterpart of [`apply_cartography_sizing`]). Each `(member, data-URI)` is applied via
    /// [`set_node_sprite`], so the node re-opens textured and back at [`Face::Sprite`].
    /// An unknown member id is skipped; does not settle. (Node-rep — sprite persistence.)
    pub fn apply_cartography_sprites<'a>(
        &mut self,
        sprites: impl IntoIterator<Item = (uuid::Uuid, &'a str)>,
    ) {
        for (id, uri) in sprites {
            self.set_node_sprite(id, uri.to_string());
        }
    }

    /// Restore the per-node sprite collider hulls from a persisted cartography sidecar (the
    /// companion of [`apply_cartography_sprites`] — call it after, once the sprites exist).
    /// Each `(member, hull)` is set via [`set_node_sprite_hull`], so the node re-opens
    /// colliding at its traced outline. An unknown member id is skipped; does not settle.
    /// (Node-rep — sprite hull persistence.)
    pub fn apply_cartography_sprite_hulls(
        &mut self,
        hulls: impl IntoIterator<Item = (uuid::Uuid, Vec<(f32, f32)>)>,
    ) {
        for (id, hull) in hulls {
            self.set_node_sprite_hull(id, hull);
        }
        self.push_node_geometry();
    }

    /// Restore the per-node physical materials from a persisted cartography sidecar. Each
    /// `(member, (restitution, friction, density))` is applied via [`set_node_material`], so a
    /// node re-opens with its tuned weight / bounce / grip. An unknown member id is skipped.
    /// (Node body & face — material persistence.)
    pub fn apply_cartography_materials(
        &mut self,
        materials: impl IntoIterator<Item = (uuid::Uuid, (f32, f32, f32))>,
    ) {
        for (id, (restitution, friction, density)) in materials {
            self.set_node_material(id, gyre::NodeMaterial { restitution, friction, density });
        }
    }

    /// Restore the per-node face overrides from a persisted cartography sidecar (call it AFTER
    /// [`apply_cartography_sprites`], so a node the user switched off its sprite face re-opens on
    /// the chosen face rather than back on `Sprite`). Each `(member, code)` is applied via
    /// [`set_node_face`]. An unknown member id is skipped. (Node body & face — face persistence.)
    pub fn apply_cartography_faces<'a>(
        &mut self,
        faces: impl IntoIterator<Item = (uuid::Uuid, &'a str)>,
    ) {
        for (id, code) in faces {
            self.set_node_face(id, Face::from_code(code));
        }
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
        // The collider follows the silhouette, so a content-type shape change reshapes the
        // physics body too, not just the drawn face. (Node-rep — collider matches shape.)
        self.push_node_geometry();
    }

    /// Override a single node's **face** (the texture on its body), keyed by node UUID. Wins
    /// over the default [`Favicon`](Face::Favicon) until [`clear_node_face`](Self::clear_node_face).
    /// Sets only the face: the body (the collider hull) and any stored sprite image are left
    /// intact, so a face switch never reshapes the node or discards an imported sprite. The
    /// override is held on the orrery; persisting it is a follow-up (it joins the cartography
    /// sidecar). A no-op for an unknown id. (Node body & face — the Face axis.)
    pub fn set_node_face(&mut self, id: uuid::Uuid, face: Face) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            self.node_faces.insert(key, face);
        }
    }

    /// Clear a node's per-node face override, reverting it to the default
    /// [`Favicon`](Face::Favicon). Leaves the body and any sprite image intact. Keyed by node
    /// UUID; a no-op for an unknown id. (Node body & face — the Face axis.)
    pub fn clear_node_face(&mut self, id: uuid::Uuid) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            self.node_faces.remove(&key);
        }
    }

    /// Reset a node's **body** to its content-type silhouette: drop any custom hull (sprite-traced
    /// or hand-authored), so the collider falls back to the silhouette primitive. Leaves the face
    /// untouched. Pushes the new geometry to physics. Keyed by node UUID; a no-op for an unknown
    /// id. (Node body & face — the Body axis.)
    pub fn clear_node_body(&mut self, id: uuid::Uuid) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            if self.node_sprite_hulls.remove(&key).is_some() {
                self.push_node_geometry();
            }
        }
    }

    /// Give a node a custom sprite face: store the imported image (a PNG data-URI) and set its
    /// face to [`Sprite`](Face::Sprite) in one step. Keyed by node UUID; a no-op for an unknown
    /// id. Held on the orrery and persisted in the cartography sidecar. The host follows with
    /// [`set_node_sprite_hull`] to trace the body hull; this backs the drag-and-drop image
    /// import. (Node body & face — sprite face.)
    pub fn set_node_sprite(&mut self, id: uuid::Uuid, data_uri: String) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            self.node_sprites.insert(key, data_uri);
            self.node_faces.insert(key, Face::Sprite);
        }
    }

    /// Remove a node's stored sprite image; if its face was [`Sprite`](Face::Sprite), revert the
    /// face to [`Favicon`](Face::Favicon). Leaves the body (the collider hull) intact, since the
    /// hull is the node's own shape once traced. Keyed by node UUID; a no-op for an unknown id.
    /// (Node body & face — sprite face.)
    pub fn clear_node_sprite(&mut self, id: uuid::Uuid) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            self.node_sprites.remove(&key);
            if self.node_faces.get(&key) == Some(&Face::Sprite) {
                self.node_faces.insert(key, Face::Favicon);
            }
        }
    }

    /// A node's custom sprite face (a PNG data-URI), if it has one. The card uses this as
    /// the face image when [`node_face`](Self::node_face) is [`Sprite`](Face::Sprite).
    /// (Node body & face — sprite face.)
    pub fn node_sprite(&self, key: NodeKey) -> Option<&str> {
        self.node_sprites.get(&key).map(String::as_str)
    }

    /// Set a node's sprite collider hull: the sprite's opaque region as a convex polygon in
    /// face-normalized coords ([-0.5, 0.5], scaled to `node_size` when the collider is built).
    /// The host traces it from the image at import. A hull of fewer than 3 points clears it
    /// (the node falls back to its silhouette collider). Pushes the new geometry to physics.
    /// Keyed by node UUID; a no-op for an unknown id. (Node representation P2 — sprite hull.)
    pub fn set_node_sprite_hull(&mut self, id: uuid::Uuid, hull: Vec<(f32, f32)>) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            if hull.len() >= 3 {
                self.node_sprite_hulls.insert(key, hull);
            } else {
                self.node_sprite_hulls.remove(&key);
            }
            self.push_node_geometry();
        }
    }

    /// A node's sprite collider hull (face-normalized convex polygon), if it has one.
    /// (Node representation P2 — sprite hull.)
    pub fn node_sprite_hull(&self, key: NodeKey) -> Option<&[(f32, f32)]> {
        self.node_sprite_hulls.get(&key).map(Vec::as_slice)
    }

    /// Seed a node with a default editable body hull — a square covering most of the face — so a
    /// node with no sprite can be given a custom shape from scratch in the editor (authoring a
    /// body beyond tracing a sprite). A no-op if it already has a hull. Pushes geometry to
    /// physics. Keyed by node UUID; a no-op for an unknown id. (Node body & face — the Body axis.)
    pub fn seed_node_body(&mut self, id: uuid::Uuid) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            if self.node_sprite_hulls.contains_key(&key) {
                return;
            }
        }
        self.set_node_sprite_hull(
            id,
            vec![(-0.35, -0.35), (0.35, -0.35), (0.35, 0.35), (-0.35, 0.35)],
        );
    }

    /// A node's physical material (the Body axis): its per-node override if set, else the
    /// default [`gyre::NodeMaterial`] (the spawn restitution / friction / density).
    /// (Node body & face — material.)
    pub fn node_material(&self, key: NodeKey) -> gyre::NodeMaterial {
        self.node_materials.get(&key).copied().unwrap_or_default()
    }

    /// Set a node's physical material (restitution / friction / density) and push it to the
    /// live body, so the node feels heavier / bouncier / grippier at once. A material equal to
    /// the default is stored too (an explicit override), so the facet shows it as set. Keyed by
    /// node UUID; a no-op for an unknown id. Persisted in the cartography sidecar. (Node body
    /// & face — material.)
    pub fn set_node_material(&mut self, id: uuid::Uuid, material: gyre::NodeMaterial) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            self.node_materials.insert(key, material);
            self.physics.set_node_materials(vec![(key, material)]);
        }
    }

    /// Reset a node's physical material to the default (drop its override) and push the default
    /// back to the live body. Keyed by node UUID; a no-op for an unknown id. (Node body & face.)
    pub fn clear_node_material(&mut self, id: uuid::Uuid) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            if self.node_materials.remove(&key).is_some() {
                self.physics.set_node_materials(vec![(key, gyre::NodeMaterial::default())]);
            }
        }
    }

    /// A node's face footprint (px): a per-node override if set, else size-by-degree
    /// (the face grows with the node's undirected degree, capped) when that mode is on,
    /// else the uniform default. The card applies the selection lift on top, and uses the
    /// same value to center the face on the gyre collider. (P0 resize.)
    pub fn node_size(&self, key: NodeKey) -> f32 {
        const DEFAULT: f32 = 36.0;
        const MAX: f32 = 88.0;
        if let Some(&s) = self.node_sizes.get(&key) {
            return s;
        }
        // Size by importance (the signal-driven channel) wins over size-by-degree: normalized
        // importance (0..=1) maps to DEFAULT..=MAX, so the most important node hits the cap and
        // the rest scale relative to it. An un-scored node reads 0 -> DEFAULT. (Graph signals.)
        if self.size_by_importance {
            let importance = self.node_importance.get(&key).copied().unwrap_or(0.0);
            return DEFAULT + importance * (MAX - DEFAULT);
        }
        if self.size_by_degree {
            let degree = self.graph.neighbors_undirected(key).count();
            return (DEFAULT + 8.0 * degree as f32).min(MAX);
        }
        DEFAULT
    }

    /// Override a single node's face footprint (px), keyed by node UUID; clamped to a sane
    /// range. Wins over size-by-degree / the default until [`clear_node_size`]; a no-op for
    /// an unknown id. (P0 resize.)
    pub fn set_node_size(&mut self, id: uuid::Uuid, size: f32) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            self.node_sizes.insert(key, size.clamp(16.0, 160.0));
            self.resettle_for_size();
        }
    }

    /// Clear a node's footprint override, reverting it to size-by-degree / the default.
    /// Keyed by node UUID; a no-op for an unknown id. (P0 resize.)
    pub fn clear_node_size(&mut self, id: uuid::Uuid) {
        if let Some((key, _)) = self.graph.get_node_by_id(id) {
            self.node_sizes.remove(&key);
            self.resettle_for_size();
        }
    }

    /// Toggle size-by-degree: when on, node faces grow with their undirected degree. A
    /// scene-level presentation choice (the user's opt-in, off by default). (P0 resize.)
    pub fn set_size_by_degree(&mut self, on: bool) {
        self.size_by_degree = on;
        self.resettle_for_size();
    }

    /// Whether size-by-degree is on. (P0 resize.)
    pub fn size_by_degree(&self) -> bool {
        self.size_by_degree
    }

    /// Toggle **size-by-importance**: when on, node faces grow with their graph-signals
    /// importance (degree-based now, betweenness later), normalized so the most important node
    /// hits the cap. A scene-level choice (off by default); wins over size-by-degree, loses to a
    /// manual per-node override. Refreshes the cached importance + re-separates neighbours.
    /// (Graph signals — importance encoding.)
    pub fn set_size_by_importance(&mut self, on: bool) {
        self.size_by_importance = on;
        if on {
            self.importance_dirty = true; // force a fresh compute on enable
            self.recompute_importance();
        } else {
            self.node_importance.clear();
        }
        self.resettle_for_size();
    }

    /// Whether size-by-importance is on. (Graph signals — importance encoding.)
    pub fn size_by_importance(&self) -> bool {
        self.size_by_importance
    }

    /// Choose the importance metric (degree or betweenness) size-by-importance reads. Dirties the
    /// cache + re-separates so the change takes effect immediately when the mode is on; a no-op
    /// effect when off. (Graph signals — importance metric.)
    pub fn set_importance_metric(&mut self, metric: ImportanceMetric) {
        self.importance_metric = metric;
        self.importance_dirty = true;
        if self.size_by_importance {
            self.recompute_importance();
            self.resettle_for_size();
        }
    }

    /// The active importance metric. (Graph signals — importance metric.)
    pub fn importance_metric(&self) -> ImportanceMetric {
        self.importance_metric
    }

    /// Restore the importance metric from a persisted sidecar code. Call this **before**
    /// [`apply_cartography_sizing`](Self::apply_cartography_sizing), so its size recompute uses the
    /// restored metric: this only records the choice + dirties the cache, leaving the geometry push
    /// to the sizing restore. An empty / unknown code restores degree. (Graph signals — metric
    /// persistence.)
    pub fn apply_cartography_importance_metric(&mut self, code: &str) {
        self.importance_metric = ImportanceMetric::from_code(code);
        self.importance_dirty = true;
    }

    /// Recompute the cached per-node importance from `signals` (degree-based, normalized
    /// `0..=1`). Called when geometry is pushed under size-by-importance; the generation +
    /// dirty-bit cache that gates this is a later graph-signals slice. (Graph signals.)
    fn recompute_importance(&mut self) {
        // The cheap-signal cache: only recompute when the graph topology changed since the last
        // compute (the dirty flag), so a size-only geometry push does not redo the O(N) degree
        // pass. (Graph signals — the cheap-signal cache.)
        if !self.importance_dirty {
            return;
        }
        self.node_importance =
            signals::importance(&self.graph, self.importance_metric).weights.into_iter().collect();
        self.importance_dirty = false;
    }

    /// A node's render height (px above the ground plane) for the isometric float: `0`
    /// (flat) unless height-by-degree is on, where a node rises with its undirected
    /// degree (capped) so hubs stand tallest. Purely visual: it does not move the gyre
    /// body. The card is raised in screen-y by this (times zoom) and a stem drops to its
    /// ground anchor, where its edges meet. (Isometric camera P3 — fake height.)
    pub fn node_height(&self, key: NodeKey) -> f32 {
        if !self.height_by_degree {
            return 0.0;
        }
        let degree = self.graph.neighbors_undirected(key).count();
        (16.0 * degree as f32).min(80.0)
    }

    /// Toggle height-by-degree: when on, nodes float above the ground by their degree
    /// (hubs highest), each with a stem to its ground anchor. Off by default; pairs with
    /// the isometric tilt. (Isometric camera P3.)
    pub fn set_height_by_degree(&mut self, on: bool) {
        self.height_by_degree = on;
    }

    /// Whether height-by-degree is on. (Isometric camera P3.)
    pub fn height_by_degree(&self) -> bool {
        self.height_by_degree
    }

    /// Add a drifting scene-decoration body to the orrery's world — a "living backdrop"
    /// element, intangible to the graph by default (it never perturbs the layout). A ball
    /// of `radius` at world `position`, with an initial `velocity` (px/s). Best called
    /// before [`offload_physics`](Self::offload_physics) so it rides onto the actor with
    /// the rest of the world. (Physics scenes P1.)
    pub fn add_scene_body(&mut self, position: (f32, f32), radius: f32, velocity: (f32, f32)) {
        self.physics.add_scene_body(
            gyre::NodeCollider::Ball { radius },
            Point2D::new(position.0, position.1),
            velocity,
        );
    }

    /// Set every node's tangibility to the scene bodies: `true` lets the graph collide with
    /// the scene (push it / be pushed), `false` (the default) passes through. The scene-wide
    /// tangibility lever; applies to the current nodes. (Physics scenes P2.)
    pub fn set_nodes_tangible(&mut self, tangible: bool) {
        self.physics.set_nodes_tangible(tangible);
    }

    /// Register a scene-prop sprite texture under an opaque `handle`: raw RGBA8 (straight alpha) plus
    /// its pixel dimensions. A scene prop whose [`SceneBodySpec::sprite`](gyre::SceneBodySpec) carries
    /// the same handle then paints as a textured billboard over its footprint (otherwise it falls back
    /// to the abstract orb / polygon). The registry persists across scene loads, so a host registers
    /// its props' textures once at startup. A re-register under the same handle replaces it. (Physics
    /// scenes — scene-prop sprites.)
    pub fn register_scene_sprite(
        &mut self,
        handle: impl Into<String>,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    ) {
        self.scene_sprite_textures.insert(handle.into(), (rgba, width, height));
    }

    /// Load Conway's Game of Life as the ambient backdrop: a wrapped grid of cells, seeded with a
    /// random soup, that the frame loop steps a few generations a second and paints behind the graph.
    /// Replaces any prior ambient sim; the orrery keeps redrawing while one is active so it animates.
    /// Clear with [`clear_ambient`](Self::clear_ambient). (Physics scenes P5.)
    pub fn load_game_of_life(&mut self) {
        let sim = GameOfLife::seeded(100, 64, 0x5EED_1234);
        self.ambient_tincture = sim.default_tincture();
        self.ambient = Some(Box::new(sim));
    }

    /// Load the n-body drift as the ambient backdrop: a cloud of bodies orbiting a central well and
    /// tugging on one another, a slow galaxy-like swirl behind the graph. (Physics scenes P5.)
    pub fn load_nbody(&mut self) {
        let sim = NBody::seeded(200, 0x0B17_2024);
        self.ambient_tincture = sim.default_tincture();
        self.ambient = Some(Box::new(sim));
    }

    /// Load particle-life as the ambient backdrop: particles of a few species self-organising under
    /// asymmetric attraction into drifting clusters and chains, each species a hue rotated from the
    /// tincture. (Physics scenes P5.)
    pub fn load_particle_life(&mut self) {
        let sim = ParticleLife::seeded(320, 0x9A17_2024);
        self.ambient_tincture = sim.default_tincture();
        self.ambient = Some(Box::new(sim));
    }

    /// Load falling-sand as the ambient backdrop: grains pour from the top, pile into drifting dunes,
    /// and drain at the bottom - a gravity-shaped CA flowing behind the graph. (Physics scenes P5.)
    pub fn load_sand(&mut self) {
        let sim = SandFall::new(120, 76, 0x5A11_2024);
        self.ambient_tincture = sim.default_tincture();
        self.ambient = Some(Box::new(sim));
    }

    /// Remove the ambient backdrop sim. (Physics scenes P5.)
    pub fn clear_ambient(&mut self) {
        self.ambient = None;
    }

    /// Override the active ambient backdrop's [`Tincture`] (its base paint colour). Takes effect on
    /// the next frame; persists until the next backdrop is loaded (which resets it to that sim's
    /// default). A no-op visually when no backdrop is loaded. (Physics scenes P5 - tincture.)
    pub fn set_ambient_tincture(&mut self, tincture: Tincture) {
        self.ambient_tincture = tincture;
    }

    /// Load a declarative [`SceneSpec`] into the world (clearing any prior scene), then kick
    /// a settle so it falls / arranges into place. A perpetual scene (a drift) then keeps
    /// ticking on its own. The graph is intangible to the scene by default
    /// ([`set_nodes_tangible`] makes it interactive). Pair with the re-exported catalog
    /// constructors ([`pyramid_scene`], [`domino_scene`], ...). (Physics scenes P4a.)
    pub fn load_scene(&mut self, spec: SceneSpec) {
        self.physics.load_scene(spec);
        self.settle_physics(SETTLE_TICKS);
    }

    /// Load the demo "drop bowl" interactive scene (a bumpy fixed floor + dynamic balls
    /// falling under gravity). (Physics scenes P3.)
    pub fn load_demo_scene(&mut self) {
        self.load_scene(drop_bowl_scene());
    }

    /// Remove every scene body (the living backdrop / loaded scene). (Physics scenes P3.)
    pub fn clear_scene(&mut self) {
        self.physics.clear_scene();
    }

    /// Load a demo liquid pool: a block of PBF particles dropped into a basin behind the graph; the
    /// physics actor keeps ticking while it flows. Clear with [`clear_fluid`]. (Physics scenes P4c.)
    pub fn load_demo_fluid(&mut self) {
        let basin = gyre::Basin { min_x: -250.0, max_x: 250.0, floor_y: 250.0 };
        self.physics.load_fluid(
            gyre::FluidParams::default(),
            basin,
            Point2D::new(-150.0, -170.0),
            16,
            10,
            18.0,
        );
        self.settle_physics(SETTLE_TICKS);
    }

    /// Remove the liquid pool. (Physics scenes P4c.)
    pub fn clear_fluid(&mut self) {
        self.physics.clear_fluid();
    }

    /// Load the demo whirlpool: a ring of loose balls plus a centred vortex force-field that swirls
    /// them into an orbiting gyre. The field keeps the actor ticking; `clear_scene` (key `0`) drops
    /// both. (Physics scenes P4 — force-field tier.)
    pub fn load_whirlpool(&mut self) {
        self.physics.load_scene(whirlpool_scene());
        self.physics.set_scene_field(Some(gyre::SceneField::Vortex {
            center: (0.0, 0.0),
            strength: 90.0,
            inward: 30.0,
        }));
        self.settle_physics(SETTLE_TICKS);
    }

    /// Load the demo fountain: a catch basin plus an upward emitter whose droplets spray up, arc,
    /// and rain back down (ageing out so the jet is perpetual). The emitter keeps the actor ticking;
    /// `clear_scene` (key `0`) drops both. (Physics scenes — emitters.)
    pub fn load_fountain(&mut self) {
        self.physics.load_scene(fountain_scene());
        self.physics.add_emitter(gyre::SceneEmitter {
            collider: gyre::NodeCollider::Ball { radius: 7.0 },
            position: (0.0, 260.0),
            position_jitter: (10.0, 0.0),
            velocity: (0.0, -430.0),
            velocity_jitter: (70.0, 40.0),
            rate_per_sec: 45.0,
            lifetime_secs: 2.2,
            max_alive: 130,
        });
        self.settle_physics(SETTLE_TICKS);
    }

    /// The size-tier index (0..[`SIZE_TIERS`]`.len()`) nearest a node's current resolved
    /// size — where the resize control's filled notches stop. A size-by-degree or default
    /// size snaps to its nearest tier for display. (Node-rep — size tiers.)
    pub fn node_size_tier(&self, key: NodeKey) -> usize {
        let size = self.node_size(key);
        SIZE_TIERS
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (**a - size).abs().total_cmp(&(**b - size).abs()))
            .map(|(i, _)| i)
            .unwrap_or(1)
    }

    /// Step a node's size by `delta` tiers (−1 / +1) from its current nearest tier, set the
    /// per-node override to that preset, and return the new tier index. The first step snaps
    /// a size-by-degree / default size onto the tier ladder. Keyed by node UUID; returns the
    /// unchanged nearest tier for an unknown id. (Node-rep — size tiers.)
    pub fn step_node_size_tier(&mut self, id: uuid::Uuid, delta: i32) -> usize {
        let Some((key, _)) = self.graph.get_node_by_id(id) else {
            return 1;
        };
        let current = self.node_size_tier(key) as i32;
        let next = (current + delta).clamp(0, SIZE_TIERS.len() as i32 - 1) as usize;
        self.set_node_size(id, SIZE_TIERS[next]);
        next
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
        Some(self.camera.to_screen(kernel::geometry::PortablePoint::new(world.x, world.y)))
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

    /// The full per-pane [`Viewport`] (camera + pan inertia), for the host to stash on
    /// the owning (window, graph) pane and reinstall before the next render / input
    /// pass via [`set_viewport`](Self::set_viewport). Carries yaw / tilt too (unlike
    /// [`camera`](Self::camera)), so an isometric pane round-trips losslessly. This is
    /// the seam that moves the camera off the shared authority onto the view: the
    /// orrery's own `camera` / `pan_velocity` become per-pass working state the host
    /// drives, so two windows on one graph hold distinct viewports, not a mirror.
    pub fn viewport(&self) -> Viewport {
        Viewport {
            offset: self.camera.offset,
            zoom: self.camera.zoom,
            yaw: self.camera.yaw,
            tilt: self.camera.tilt,
            pan_velocity: self.pan_velocity,
        }
    }

    /// Install a per-pane [`Viewport`] before rendering or handling input for the pane
    /// that owns it. Clamps as [`set_camera`](Self::set_camera) (zoom) and
    /// [`set_tilt`](Self::set_tilt) (tilt) do, so a host-stashed value is sanitized on
    /// the way back in.
    pub fn set_viewport(&mut self, v: Viewport) {
        self.camera.offset = v.offset;
        self.camera.zoom = if v.zoom.is_finite() && v.zoom > 0.0 {
            v.zoom.clamp(MIN_ZOOM, MAX_ZOOM)
        } else {
            1.0
        };
        self.camera.yaw = v.yaw;
        self.camera.tilt = v.tilt.clamp(0.05, 1.0);
        self.pan_velocity = v.pan_velocity;
    }

    /// Toggle the isometric (foreshortened-ground) projection. `on` reclines the
    /// ground by foreshortening the vertical, so the graph reads as a tilted floor
    /// while the node cards stay upright billboards; `off` restores the plain
    /// top-down view. The free-cam yaw orbit + persistence are P2. (Isometric camera P1.)
    pub fn set_isometric(&mut self, on: bool) {
        /// Vertical foreshorten for the isometric preset (a dimetric squash); becomes
        /// a setting once the projection picker lands (P2).
        const ISO_TILT: f32 = 0.55;
        self.camera.tilt = if on { ISO_TILT } else { 1.0 };
    }

    /// Whether the isometric projection is active (the ground is foreshortened).
    pub fn is_isometric(&self) -> bool {
        self.camera.tilt < 1.0
    }

    /// Orbit the view by `d_radians` about the vertical (the free-cam yaw). The
    /// ground rotates while the node cards stay upright billboards; pair with the
    /// isometric `tilt` for the 2.5D orbit (at `tilt == 1` it spins the flat layout).
    /// The host's orbit gesture / projection picker drives this. (Isometric camera P2.)
    pub fn orbit_by(&mut self, d_radians: f32) {
        self.camera.yaw += d_radians;
    }

    /// Set the orbit yaw (radians) directly. (Isometric camera P2.)
    pub fn set_yaw(&mut self, yaw: f32) {
        self.camera.yaw = yaw;
    }

    /// Set the vertical foreshorten directly, clamped to a sane `(0, 1]`. `1.0` is
    /// top-down; lower reclines the ground further. (Isometric camera P2.)
    pub fn set_tilt(&mut self, tilt: f32) {
        self.camera.tilt = tilt.clamp(0.05, 1.0);
    }

    /// The current orbit yaw (radians), for a host control to display / persist.
    pub fn yaw(&self) -> f32 {
        self.camera.yaw
    }

    /// The current vertical foreshorten (`tilt`), for a host control to display / persist.
    pub fn tilt(&self) -> f32 {
        self.camera.tilt
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

    /// Scope the orrery to a host-supplied member set (by UUID), e.g. the workbench's
    /// open tiles, so the *same* arrangement renders as both a tiled workbench and a
    /// spatial map (the spine's "two projections of one arrangement"). Members absent
    /// from the graph are skipped; an empty set clears the lens (shows the whole
    /// graph). (Curated orrery — workbench mirror.)
    pub fn scope_to_members(&mut self, members: impl IntoIterator<Item = uuid::Uuid>) {
        let keys: Vec<NodeKey> = members
            .into_iter()
            .filter_map(|id| self.graph.get_node_by_id(id).map(|(key, _)| key))
            .collect();
        self.scope = (!keys.is_empty()).then_some(keys);
    }

    /// Drop the scope lens — show the whole graph again. (Curated orrery.)
    pub fn clear_scope(&mut self) {
        self.scope = None;
    }

    /// Zoom by `factor`, keeping the world point under `anchor` (screen px) fixed.
    fn zoom_at(&mut self, anchor: (f32, f32), factor: f32) {
        // Keep the world point currently under `anchor` fixed across the zoom: read it
        // before, then shift `offset` so it lands back under `anchor` after. Correct for
        // any projection (top-down or isometric), and identical to the old
        // `world*zoom+offset` formula at the default camera. (Isometric camera P0.)
        let world_under = self.camera.to_world(anchor);
        self.camera.zoom = (self.camera.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let landed = self.camera.to_screen(world_under);
        self.camera.offset.0 += anchor.0 - landed.0;
        self.camera.offset.1 += anchor.1 - landed.1;
    }

    /// Map a screen-px point back to world space through the camera projector
    /// (the inverse of `Camera::to_screen`; at the default camera this is
    /// `world = (screen - offset) / zoom`).
    fn screen_to_world(&self, screen: (f32, f32)) -> Point2D<f32> {
        let w = self.camera.to_world(screen);
        Point2D::new(w.x, w.y)
    }

    /// The screen viewport mapped to world space — the region gyre culls against
    /// to decide which nodes are on screen.
    fn world_viewport(&self) -> Box2D<f32> {
        // Bound all four screen corners in world space: under an isometric yaw the
        // screen rectangle maps to a rotated world quad, so two corners under-cover.
        // At the default top-down camera this is the same box as before. (Isometric P0.)
        let (w, h) = (self.view_w as f32, self.view_h as f32);
        let corners = [
            self.screen_to_world((0.0, 0.0)),
            self.screen_to_world((w, 0.0)),
            self.screen_to_world((0.0, h)),
            self.screen_to_world((w, h)),
        ];
        let mut min = corners[0];
        let mut max = corners[0];
        for p in &corners[1..] {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
        Box2D::new(min, max)
    }
}

#[cfg(test)]
mod tests;
