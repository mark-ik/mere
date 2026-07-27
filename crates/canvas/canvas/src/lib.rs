// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The canvas as a reusable, **window-agnostic content-root** — the graph's
//! spatial presentation (build item 1D of the genet-as-host flip; S1 of the
//! modular integration plan).
//!
//! [`Canvas`] owns the graph, its [`seiche::Simulation`], the camera, and the
//! pre-materialized abs-pos node-children pool. It exposes:
//!
//! - [`Canvas::frame`] — advance one frame at a given viewport and return the
//!   composited `netrender::Scene` plus whether another frame is needed (the sim
//!   still settling, pan still gliding, or a node being dragged). It does **not**
//!   present — the host (a winit bin, or meerkat's content-root) rasterizes +
//!   composites the returned scene.
//! - Semantic input methods ([`pointer_down`](Canvas::pointer_down) /
//!   [`pointer_up`](Canvas::pointer_up) / [`cursor_moved`](Canvas::cursor_moved) /
//!   [`wheel`](Canvas::wheel) / [`set_ctrl`](Canvas::set_ctrl) /
//!   [`reseed`](Canvas::reseed)), each returning whether a redraw is needed. The
//!   host maps its raw events (winit, genet input, …) onto these; the canvas
//!   never sees a window.
//!
//! The three composited layers (per the §6 plan): the [`crate::underlay`]
//! scene-paint underlay (edges + demoted off-screen node rects + coupling
//! overlays) under one camera transform; the on-screen nodes as abs-pos genet
//! DOM children (laid out incrementally, moved per-frame by inline transform on
//! the `RepaintOnly` path); and a screen-space marquee rubber-band when active.
//!
//! The sample graph, simulation, node-children pool, and the small paint/DOM
//! helpers live in [`mod@build`].

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use crate::scene_paint::{Camera, ScenePaintStyle};
use euclid::default::{Box2D, Point2D};
use genet_layout::IncrementalLayout;
use genet_scripted_dom::{NodeId as DomNodeId, ScriptedDom};
use kernel::geometry::PortablePoint;
use kernel::graph::{EdgeAssertion, FieldId, Graph, NodeKey, RelationSelector, SemanticSubKind};
use seiche::{AffinitySpring, LayoutSnapshot, LayoutView};
/// The declarative scene catalog, re-exported so hosts (and the standalone bin) can load
/// a scene by name without depending on `seiche` directly. (Physics scenes P4a.)
pub use seiche::{
    NODE_BODY_DENSITY, NodeMaterial, SceneSpec, ball_and_chain_scene, bridge_scene, chain_scene,
    cradle_scene, domino_scene, drift_scene, drop_bowl_scene, fountain_scene, funnel_scene,
    galton_scene, mixer_scene, pyramid_scene, whirlpool_scene,
};

mod build;
#[cfg(test)]
mod build_tests;
mod seiche_bridge;
use build::{
    build_pool_dom, cluster_color, dark_scene_style, dedup_edges, dedup_edges_weighted,
    sample_graph, surface_bg,
};
use paint_list_api::ColorF;
use seiche_bridge::{build_simulation, visible_relation_edges};

/// Build the seiche [`AffinitySpring`] from a cartography [`AffinityScores`](signals::AffinityScores)
/// signal: flatten the `((a, b), weight)` pairs into the force's `(a, b, weight)` triples at the
/// default stiffness / rest length. (Graph signals — P4.)
fn build_affinity_spring(scores: &signals::AffinityScores) -> AffinitySpring {
    AffinitySpring::new(scores.pairs.iter().map(|&((a, b), w)| (a, b, w)))
}

/// How the affinity force combines its two signals — **structural** (Jaccard over topology) and
/// **content** (cosine over node embeddings) — when both are available under the
/// [`cluster_by_affinity`](Canvas::set_cluster_by_affinity) toggle. (burn brief Lane 5 — P6,
/// blended affinity.)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AffinityBlend {
    /// Draw a pair together if *either* signal likes it, harder if both — a noisy-OR of the two
    /// weights (`1 − (1−s)(1−c)`, bounded to `0..=1`). Topology and meaning as complementary
    /// clustering forces; degrades to whichever signal is present (structural alone when no content
    /// is injected). The default.
    #[default]
    Blend,
    /// The injected content signal supersedes structural whenever present (the original P5
    /// behavior); structural is the fallback when no content is injected.
    ContentOnly,
    /// Structural Jaccard only; ignore any injected content signal.
    StructuralOnly,
}

/// Merge the structural and content affinity signals into one `(a, b, weight)` pair list by a
/// **noisy-OR** of their weights: a pair present in either signal is emitted, and one present in
/// both is boosted (`w = 1 − (1−s)(1−c)`, so 0.8 with 0.8 → 0.96). Either `None` means that source
/// does not contribute. Weights are clamped to `0..=1` first. (burn brief Lane 5 — P6.)
fn blend_affinity_pairs(
    structural: Option<&signals::AffinityScores>,
    content: Option<&[(NodeKey, NodeKey, f32)]>,
) -> Vec<(NodeKey, NodeKey, f32)> {
    let canon = |a: NodeKey, b: NodeKey| if a <= b { (a, b) } else { (b, a) };
    let mut weights: std::collections::HashMap<(NodeKey, NodeKey), f32> =
        std::collections::HashMap::new();
    if let Some(s) = structural {
        for &((a, b), w) in &s.pairs {
            weights.insert(canon(a, b), w.clamp(0.0, 1.0));
        }
    }
    if let Some(c) = content {
        for &(a, b, w) in c {
            let slot = weights.entry(canon(a, b)).or_insert(0.0);
            *slot = 1.0 - (1.0 - *slot) * (1.0 - w.clamp(0.0, 1.0));
        }
    }
    weights.into_iter().map(|((a, b), w)| (a, b, w)).collect()
}

mod types;
pub use signals::{BridgeMetric, ImportanceMetric};
pub use types::{CameraView, EdgeCell, Face, NodeShape, NodeState, PointerButton, Viewport};

// The graph-scene paint lane, merged from platen in the 2026-07-09
// decomposition (platen is the pane home now; the canvas is the graph-truth
// presentation library):
/// Cartography projection-request derivation + the layout-strategy catalog.
pub mod cartography_scene;
/// Visual couplings → paint overlays (the quint field paint pass).
pub mod coupling_paint;
/// Cartography geometry: the settled-layout sidecar the host persists.
pub mod geometry;
/// Render a cartography `Projection` into a `paint_list_api` paint list.
pub mod scene_paint;

/// Sprite collider-hull tracing (RGBA → face-normalized convex hull), promoted
/// from meerkat at the 2026-07-18 harvest so every host shares one tracer.
pub mod sprite_hull;
/// The canvas scene-paint underlay (edges + demoted node rects + overlays).
pub mod underlay;

pub use cartography_scene::{
    CANVAS_LAYOUT_STRATEGIES, CanvasStrategyProjection, CartographySceneOptions,
    build_projection_request, project_canvas_lens, project_canvas_strategy,
    project_canvas_strategy_with_score, project_canvas_subgraph, project_with, signal_overlays,
};
pub use geometry::CartographyGeometry;

/// The node accent palette every representation of a node tints from.
pub mod palette;

mod edge_cells;
mod fields;
mod frame;
mod input;
mod resolved_image_cache;
pub use resolved_image_cache::DEFAULT_RESOLVED_IMAGE_CACHE_BYTES;

/// Ambient-sim backdrops (non-rapier liveliness painted behind the graph): Conway's
/// [`GameOfLife`] is the first. (Physics scenes P5.)
mod ambient;
mod community_lane;
pub use ambient::{AmbientSim, GameOfLife, NBody, ParticleLife, SandFall, Tincture};

mod physics;
use physics::Physics;

/// Force-directed settle length (frames) after a (re)seed, ~6s at 60fps.
const SETTLE_TICKS: u32 = 360;
/// A gentle re-separation burst (~1.5s at 60fps) after node colliders resize, so
/// neighbors ease away from a grown node without a full re-layout. (P0/P5 collider.)
const SIZE_RESETTLE_TICKS: u32 = 90;
/// The five face-size presets (px) the on-graph size editor steps between — the notch
/// points of the object-card resize control. The default (36) is tier 1 (0-indexed), so
/// an un-sized node reads as the second notch; the ends span dense-small to big-hub, inside
/// the `set_node_size` 16..160 clamp. (Node-rep — size tiers.)
pub const SIZE_TIERS: [f32; 5] = [24.0, 36.0, 56.0, 84.0, 120.0];
/// Per-tick timestep handed to the seiche simulation.
const TICK_DT: f32 = 1.0 / 60.0;
/// Minimum structural-Jaccard similarity for a pair to enter the affinity force. Prunes the
/// long tail of weakly-similar pairs so the force list stays lean and only meaningful clusters
/// pull. (Graph signals — P4.)
const AFFINITY_MIN_SIMILARITY: f32 = 0.1;
/// Gloss ring radii as multiples of the gloss swatch's node size, and the cluster-halo alpha. The
/// bridge ring is larger so it reads outside a cluster halo when a broker sits in a community.
/// (Graph signals — P6b, gloss overlays.)
const GLOSS_CLUSTER_RING_FACTOR: f32 = 1.7;
const GLOSS_BRIDGE_RING_FACTOR: f32 = 2.5;
const GLOSS_CLUSTER_RING_ALPHA: f32 = 0.9;
/// The bold near-white the gloss bridge ring wears (matches the main-view bridge ring's intent).
const GLOSS_BRIDGE_RING_RGBA: [f32; 4] = [0.95, 0.96, 1.0, 0.92];
/// Pixels per wheel line-notch (the host scales `LineDelta` by this before
/// calling [`Canvas::wheel`]; `wheel` divides back out to recover notches for zoom).
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
/// The favicon face inset: the icon quad occupies this fraction of the face so
/// the node's accent (activation state, selection-wins amber) reads as a frame
/// around the icon instead of surviving only at the icon's corner cutouts. A
/// cover-fit face remains available as `Face::Sprite`; this knob is a design
/// setting candidate (status-communication design space, 2026-07-09: inset
/// frame now; caption chip / hover ring / status card tracked as alternatives).
const FAVICON_INSET: f32 = 0.72;

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

/// The canvas content-root: the graph, its physics, the camera, and the abs-pos
/// node-children pool. Window-agnostic — see the module docs.
pub struct Canvas {
    graph: Graph,
    /// The force-directed layout backend — in-thread (tests / wasm) or an
    /// off-thread armillary actor (native always-offload). The canvas never reads
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
    /// The pre-materialized node-children pool: a persistent genet DOM with one
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
    /// Currently-selected relation cells (edge-pick, or covered by a marquee).
    /// Layout stays pair-based, but selection is relation-selector-aware.
    selected_edges: HashSet<EdgeCell>,
    /// Relation cells the user has hidden from the canvas. The relation and its physics
    /// spring persist; this is display-only. Persistence rides view-intent's
    /// `hidden_relations`.
    hidden_edges: HashSet<EdgeCell>,
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
    /// Scene-prop sprite textures, keyed by the opaque handle a [`seiche::SceneBodySpec`] carries:
    /// raw RGBA8 (straight alpha) plus dimensions, the [`PaintCmd::DrawImage`] shape (as favicons
    /// use). A scene prop whose `sprite` handle resolves here paints as a textured billboard instead
    /// of the abstract orb / polygon; unset handles fall back. A registry (persists across scenes,
    /// not cleared on `clear_scene`), populated via [`register_scene_sprite`](Self::register_scene_sprite).
    /// (Physics scenes — scene-prop sprites.)
    scene_sprite_textures: HashMap<String, (Vec<u8>, u32, u32)>,
    /// Decoded pixels for the content-addressed images nodes now reference,
    /// keyed by [`kernel::types::ImageRef`] digest.
    ///
    /// After the node-image externalization a node carries a ~40-byte handle,
    /// not bytes, so the paint path cannot read pixels off the node any more.
    /// The host resolves a handle through `session_runtime::image_store` (that
    /// read is async and store-backed, neither of which belongs in the frame
    /// loop) and registers the decoded RGBA here via
    /// [`register_resolved_image`](Self::register_resolved_image). An
    /// unresolved handle simply does not paint, which is the correct behavior
    /// for a blob that is missing, not yet synced, or swept.
    ///
    /// Byte-bounded LRU: decoded pixels are experience, so only the visible
    /// working set stays resident. Evicted visible digests are requested from
    /// the host again through `pending_image_requests`.
    resolved_images: resolved_image_cache::ResolvedImageCache,
    /// Misses observed by the paint pass, drained by the host after a frame.
    pending_image_requests: BTreeMap<[u8; 32], kernel::types::ImageRef>,
    /// Digests already handed to the host. Keeps an absent or undecodable blob
    /// from producing an I/O request every frame. LRU eviction removes a digest
    /// from this set so it can be resolved again when visible.
    requested_images: HashSet<[u8; 32]>,
    /// An optional ambient-sim backdrop (a non-rapier sim painted behind the graph for atmosphere):
    /// any [`AmbientSim`] (Game of Life, n-body drift, particle-life), advanced + painted as the
    /// bottom layer each [`frame`](Self::frame). `None` until one is loaded. (Physics scenes P5.)
    ambient: Option<Box<dyn AmbientSim>>,
    /// The base colour (tincture) the active ambient backdrop is painted in. Set from the sim's
    /// default on load; overridable via [`set_ambient_tincture`](Self::set_ambient_tincture). (P5.)
    ambient_tincture: Tincture,
    /// Per-node physical **material** overrides (restitution / friction / density) on the Body
    /// axis. A node absent here takes the default [`seiche::NodeMaterial`] (the spawn values), so
    /// this holds only deliberate overrides. Pushed to physics and persisted in the cartography
    /// sidecar. (Node body & face — material.)
    node_materials: HashMap<NodeKey, seiche::NodeMaterial>,
    /// Scene toggle: when on, a node's face grows with its undirected degree (capped),
    /// so the spatial map reads connection weight at a glance. Default off (uniform).
    /// (P0 resize — size-by-degree.)
    size_by_degree: bool,
    /// Scene toggle: when on, a node's face grows with its **importance signal** (the
    /// graph-signals layer — degree-based now, betweenness later), normalized so the most
    /// important node hits the cap. The signal-driven size channel; wins over size-by-degree,
    /// loses to a manual override. Default off. (Graph signals — importance encoding.)
    size_by_importance: bool,
    /// Scene toggle: when on, a node's face grows with how **recently** it was visited, so the
    /// newest content reads largest and what you left behind shrinks (the brand's "age shrinks
    /// what you leave behind"; the meristem reading of the spiral). Normalized across the graph so
    /// the freshest node hits the cap. Loses to a manual override and to size-by-importance; wins
    /// over size-by-degree. Default off. (Projection proofs — P3, recency.)
    size_by_recency: bool,
    /// The cached per-node recency (normalized `0..=1`, newest = `1.0`), recomputed from
    /// `last_visited` whenever geometry is pushed while [`size_by_recency`](Self::size_by_recency)
    /// is on. Unlike importance this needs no dirty flag: it is cheap (one O(N) min/max + map) and
    /// recomputed each push so a visit's fresh timestamp always reads current. Empty when off.
    node_recency: HashMap<NodeKey, f32>,
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
    /// The cached **community partition** (Louvain) — the genuinely expensive structural signal.
    /// `None` until first computed; refreshed by [`refresh_community_cache`](Self::refresh_community_cache)
    /// only when the active strategy needs it and the kernel's [`Graph::revision`](kernel::graph::Graph::revision)
    /// has advanced past [`community_cache_revision`](Self::community_cache_revision). Gating on the
    /// kernel revision (bumped at the mutation source) rather than a host hook means a non-structural
    /// event (a selection change) cannot invalidate it. The off-thread armillary lane is a drop-in
    /// behind this same accessor (native-only, like physics offload). (Graph signals — P3.)
    community_cache: Option<signals::ClusterSet>,
    /// The [`Graph::revision`](kernel::graph::Graph::revision) [`community_cache`](Self::community_cache)
    /// was computed at, so a stale partition is recomputed and a fresh one reused. (Graph signals.)
    community_cache_revision: u64,
    /// The inputs the active analytic layout was last computed for: `(strategy_id, graph revision,
    /// width, height, focus)`. The host gates its per-frame `project_canvas_strategy` call on these
    /// via [`needs_strategy_recompute`](Self::needs_strategy_recompute), so an unchanged analytic
    /// layout (grid, kanban, penrose, radial, ...) is computed once per real change, not every frame.
    /// `focus` is only recorded for focus-driven strategies (radial), so a selection change does not
    /// invalidate the others. Reset when the strategy changes. (Arrangements — the layout cache.)
    last_strategy_inputs: Option<(String, u64, u32, u32, Option<NodeKey>)>,
    /// Scene toggle: when on, each node wears a halo in the colour of its Louvain community, so the
    /// partition reads as spatial clusters under any layout. Drives the same generation-gated
    /// [`community_cache`](Self::community_cache) the cluster-kanban strategy uses. Default off.
    /// (Graph signals — community to a ring.)
    show_community_rings: bool,
    /// The host's off-thread wake (it pokes the on-demand render loop when a worker result lands),
    /// captured from [`offload_physics`](Self::offload_physics). `Some` means we are native +
    /// offloaded, which is exactly when the community lane runs off-thread; `None` (wasm / tests /
    /// physics not offloaded) means community computes inline. (Graph signals — the background lane.)
    offthread_wake: Option<armillary::Wake>,
    /// The off-thread community worker, spawned lazily the first time community is needed while
    /// [`offthread_wake`](Self::offthread_wake) is set. `None` = computing inline. (Graph signals.)
    community_actor: Option<community_lane::CommunityActor>,
    /// Scene toggle: when on, the structural **bridge** nodes (high-betweenness brokers) wear a bold
    /// ring, so the graph's key connectors stand out. Default off. (Graph signals — bridges.)
    show_bridge_rings: bool,
    /// The cached bridge set + the [`Graph::revision`](kernel::graph::Graph::revision) it was computed
    /// at. Betweenness is cheap (O(V·E), like degree at current scale), so this is computed inline,
    /// gated on the revision so it is not redone per frame. (Graph signals — bridges.)
    bridge_cache: Option<signals::BridgeNodes>,
    bridge_cache_revision: u64,
    /// Which notion of "critical connector" the bridge ring highlights: betweenness brokers (default)
    /// or articulation points (cut vertices). Changing it invalidates [`bridge_cache`](Self::bridge_cache)
    /// so the next [`ensure_bridges_fresh`](Self::ensure_bridges_fresh) recomputes under the new
    /// metric. (Graph signals — bridges / articulation points.)
    bridge_metric: signals::BridgeMetric,
    /// Scene toggle: when on, a pairwise **affinity force** (a weighted, attract-only seiche spring
    /// over structural-Jaccard similarity) runs on top of the force-directed layout, drawing
    /// structurally-similar nodes into tight clusters ("cluster by affinity"). Default off; only
    /// visible under force-directed (an analytic strategy overrides the physics snapshot).
    /// (Graph signals — P4, the affinity force.)
    cluster_by_affinity: bool,
    /// The cached affinity signal + the [`Graph::revision`](kernel::graph::Graph::revision) it was
    /// computed at. Jaccard is cheap (like betweenness at current scale), so it is computed inline,
    /// revision-gated so it is not redone per frame. (Graph signals — P4.)
    affinity_cache: Option<signals::AffinityScores>,
    affinity_cache_revision: u64,
    /// The graph revision the affinity force currently installed in the sim was built from, or
    /// `None` when no force is installed. Lets [`sync_affinity_force`](Self::sync_affinity_force)
    /// rebuild the force only when the signal actually changed (or the toggle flips), not per frame.
    /// (Graph signals — P4.)
    installed_affinity_revision: Option<u64>,
    /// A host-injected **content-affinity** signal (semantic similarity from node embeddings),
    /// superseding the internal structural-Jaccard one while `Some`. `None` = use structural. The
    /// host owns the embedding provider (burn stays out of the canvas) and re-injects on node-content
    /// change; the canvas installs it under the `cluster_by_affinity` toggle. `Some(empty)` is
    /// authoritative-but-inert (clusters found nothing). (burn brief Lane 5 — P4, content source.)
    content_affinity: Option<Vec<(NodeKey, NodeKey, f32)>>,
    /// Set when the host injects a fresh content signal; drives a single (re)install on the next
    /// frame. The content signal is host-fresh (it tracks node *content*), so it is dirty-gated
    /// rather than graph-revision-gated like the structural one. (burn brief Lane 5 — P4.)
    content_affinity_dirty: bool,
    /// Whether *any* affinity force is currently installed in the sim (structural or content), so
    /// the toggle-off branch clears exactly once regardless of which source was live. (Graph
    /// signals — P4.)
    affinity_force_installed: bool,
    /// How the structural and content affinity signals combine ([`AffinityBlend`]). Default
    /// [`Blend`](AffinityBlend::Blend) — a noisy-OR of the two, degrading to structural alone when
    /// no content is injected. (burn brief Lane 5 — P6, blended affinity.)
    affinity_blend: AffinityBlend,
    /// The gloss swatch's own layout strategy id, or `None` to mirror the main view (a minimap).
    /// `Some` makes the gloss an independent lens (e.g. spectral while the main view is force-
    /// directed). (Graph signals — P6, the independent gloss projection.)
    gloss_strategy: Option<String>,
    /// The gloss lens's cached positions + the `(strategy, graph revision, width, height, community
    /// rings, bridge rings)` they were computed for, so the host recomputes the gloss arrangement
    /// only on a real change, not per frame (the gloss may use an expensive layout like spectral).
    /// The two ring-toggle booleans are part of the key because the gloss's overlays
    /// ([`gloss_overlays`](Self::gloss_overlays)) ride the same cached projection (P6b): flipping a
    /// ring toggle changes which overlays the lens carries, so it must re-fetch (rare, so paying a
    /// spectral recompute on a ring toggle is fine). (Graph signals — P6.)
    gloss_positions: Option<HashMap<NodeKey, PortablePoint>>,
    gloss_cache_inputs: Option<(
        String,
        u64,
        u32,
        u32,
        bool,
        bool,
        Option<Vec<NodeKey>>,
        Option<NodeKey>,
    )>,
    /// The gloss lens's signal overlays (community halos + bridge emphasis), captured from the same
    /// `project_canvas_lens` projection as [`gloss_positions`](Self::gloss_positions) — the overlay
    /// pipe's first consumer. [`gloss_geometry`](Self::gloss_geometry) resolves them into rings at
    /// the lens's own positions, so a second lens shows the clusters/brokers under its own layout.
    /// (Graph signals — P6b, the overlay pipe.)
    gloss_overlays: Vec<signals::Overlay>,
    /// Gloss scope: when on, the gloss lens shows only the current **selection** (+ its induced
    /// edges + the halos over selected members), cropped + auto-refit so the swatch zooms to it. A
    /// render-time filter on the whole-graph lens — it costs nothing extra and needs no recompute, so
    /// changing the selection re-crops live. Empty selection falls back to the whole graph. (Graph
    /// signals — P6c, the independent gloss scope.)
    gloss_scope_selection: bool,
    /// Gloss encoding: when on, the gloss lens sizes each node by the **importance** signal (the same
    /// `node_importance` the main view's size-by-importance reads), independent of the main view's own
    /// sizing. A render-time factor; the importance cache is ensured fresh in `frame`. (Graph signals
    /// — P6c, the independent gloss encoding.)
    gloss_size_by_importance: bool,
    /// Revision-gated memo of the collapsed, multiplicity-weighted edge topology (the kernel-query
    /// memo, cache generalization **C**): `dedup_edges_weighted` walks every `relations()` row, and
    /// the gloss redraws every frame, so without this it re-dedups the whole graph each frame. `frame`
    /// refreshes it once per structural change; [`gloss_geometry`](Self::gloss_geometry) reads it,
    /// falling back to a fresh compute when stale (a direct call before the first frame). (Graph
    /// signals — query memos, C.)
    weighted_edges_cache: Option<(u64, Vec<(NodeKey, NodeKey, u32)>)>,
    /// How many times [`refresh_weighted_edges`](Self::refresh_weighted_edges) actually recomputed
    /// (test introspection for the memo: a static frame must not bump it).
    weighted_edges_rebuilds: u64,
    /// Scene toggle: when on, a node floats above the ground by its undirected degree
    /// (hubs highest), with a stem to its ground anchor — the isometric "fake height".
    /// Purely visual (the seiche body does not move). Default off. (Isometric camera P3.)
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
    /// The current viewport (px), updated by [`frame`](Canvas::frame) /
    /// [`resize`](Canvas::resize); used by `world_viewport` (cull) + `recenter`.
    view_w: u32,
    view_h: u32,
    /// The pane's active layout strategy (a cartography adapter `projection_id`, e.g.
    /// `"phyllotaxis.default"`), or `None` for the default force-directed (seiche) layout.
    /// Persisted per pane via view-intent; the host pushes positions for it via
    /// [`apply_strategy_positions`](Canvas::apply_strategy_positions). (Layout picker.)
    active_strategy: Option<String>,
    /// Buffered positions for the active non-seiche strategy, applied into `view` each
    /// frame **after** the physics snapshot (so they win over seiche regardless of the
    /// off-thread actor's timing). `None` under force-directed. (Layout picker.)
    strategy_positions: Option<Vec<(NodeKey, PortablePoint)>>,
    /// The persisted product-free score that drove the current analytic view.
    /// This is view state, not graph truth. (Projection proofs — P3.)
    projection_score: Option<sceno::Score>,
    /// How strongly a *playing* graph is pulled toward the active arrangement's
    /// slots (`seiche::AnchorSpring` stiffness). `0.0` makes an arrangement a
    /// pure initial condition; higher holds its shape against the graph's own
    /// forces. The dial between "layout as authority" and "layout as
    /// participant". (Arrangement as attractor.)
    arrangement_pull: f32,
    /// A restored score's `(strategy id, graph revision)` claim on the layout.
    /// [`restore_projection_score`](Self::restore_projection_score) buffers the
    /// score's own positions; without this the host's very next
    /// [`needs_strategy_recompute`](Self::needs_strategy_recompute) would report
    /// stale (the cache is empty after a restore) and recompute the arrangement
    /// from scratch, discarding the restored score before it ever painted.
    /// Cleared once the graph actually changes, the user picks a strategy, or a
    /// recompute is recorded. (Projection proofs — P3 score restore.)
    restored_score_hold: Option<(String, u64)>,
    /// The pane's "scope" lens: when `Some`, the canvas renders only these nodes (a
    /// curated subset), projecting through a curated forme arrangement instead of the
    /// full Identity one. `None` shows the whole graph. (Curated canvas.)
    scope: Option<Vec<NodeKey>>,
    /// When set, the scene omits the on-screen gnode + favicon layers: the host renders
    /// those gnodes as DOM elements in the shell document instead (the focused canvas only;
    /// secondary panes keep their in-scene gnodes). Edges + demoted dots stay as the underlay.
    /// A gnode is the node's rendered body either way — a Scene layer here, a chrome DOM
    /// element there — never the node's referenced document. (Canvas-as-element — Phase 2.)
    render_gnodes_as_dom: bool,
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

mod cartography;
mod gloss;
mod lifecycle;
mod nodes;
mod selection;
mod strategy;
mod view;

#[cfg(test)]
mod tests;
