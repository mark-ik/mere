/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # gyre
//!
//! Rapier-backed force integration for Mere: the crate that realizes the
//! graph's bodies as motion. Sibling to the forthcoming `aether` field-algebra
//! crate — `aether` defines the fields and couplings and resolves them into
//! forces; `gyre` integrates those forces (rapier bodies, collision, stepping)
//! into positions. (`gyre` is the wheeling motion of the bodies; kin to the
//! `orrery` view they compose.)
//!
//! ## Place in the architecture
//!
//! [`kernel::graph::Graph`] is the source of truth for node identity, topology,
//! and the *committed* position used for persistence. Gyre holds the rapier
//! world that drives the *projected* position — bodies bound to nodes, forces
//! from pluggable [`Force`] implementors, and a tick that mirrors body
//! translations back to the graph each step.
//!
//! Petgraph (inside kernel) owns topology; gyre owns physical state. Both sit at
//! the substrate tier, consumed by everything above.
//!
//! ## Forces
//!
//! - [`Simulation`] — owns the rapier world, a [`NodeKey`] ↔
//!   [`RigidBodyHandle`] bimap, the query index (hit-test / cull), and the
//!   registered forces.
//! - [`Simulation::sync_with_graph`] / [`Simulation::sync_edges`] — keep bodies
//!   and edge topology in step with the graph. Idempotent.
//! - [`Simulation::tick`] — apply each registered [`Force`], then step the world.
//! - Built-in forces ([`NodeExclusion`], [`EdgeSpring`], [`Boundary`]) are the
//!   fast Rust default-path; `aether`'s couplings are the general, scriptable
//!   path that compiles to the same [`Force`] contract.
//!
//! Until a [`Force`] is registered, the world ticks empty — bodies settle to
//! rest under damping alone.

#![doc(html_root_url = "https://docs.rs/gyre/0.0.1")]

use std::collections::HashMap;

use euclid::default::{Box2D, Point2D};
use kernel::graph::{Graph, NodeKey};
use rapier2d::prelude::*;

/// Built-in force forces for the force-directed orrery layout.
pub mod forces;
pub use forces::{Boundary, EdgeSpring, NodeExclusion};

/// Barnes–Hut quadtree for O(n log n) approximate n-body repulsion — harvested
/// from the retired `graph-layout`, for big-graph charge repulsion. The
/// [`barnes_hut::repulsion_forces`] primitive is ready; wiring it as a live
/// [`Force`] is a tuning step (it would supplement [`NodeExclusion`]'s local
/// separation with global spreading).
pub mod barnes_hut;
pub use barnes_hut::{BarnesHutConfig, BarnesHutRepulsion, repulsion_forces};

/// The aether→gyre seam: a kernel [`kernel::graph::Coupling`] compiled to a
/// [`Force`] (the general, scriptable path the built-in forces specialize).
pub mod coupling_force;
pub use coupling_force::CouplingForce;

/// Scene-geometry queries for the orrery canvas: edge geometry, edge picking,
/// and marquee rect-select (node point-pick + cull live on `Simulation`
/// directly). Split out to keep `lib.rs` under the per-file size ceiling.
mod query;

/// The rapier-free read model — a position-only [`LayoutView`] the host renders
/// and hit-tests from, plus the [`LayoutSnapshot`] a physics actor emits to
/// refresh it. The seam that lets the simulation run off the UI thread.
mod view;
pub use view::{LayoutSnapshot, LayoutView, RectSelection};

/// Physical radius used for every node body's collider.
///
/// Mirrors `platen::CanvasSceneOptions::default()`'s
/// `default_node_radius` so simulation-driven separation matches
/// the renderer's drawn radius. When per-node radii become a real
/// thing, this constant will get replaced by a per-node lookup.
pub const NODE_BODY_RADIUS: f32 = 18.0;

/// The collider shape the host wants for a node's physics body, so the hard-collision /
/// hit geometry matches the node's *visible* face rather than a uniform ball. The host maps
/// its own form vocabulary (orrery `NodeShape`, a custom sprite hull) onto this; gyre lowers
/// each to a parry shape in [`Simulation::set_node_colliders`]. Sizes/points are in world
/// units (the same space as positions). (Node-rep — collider matches shape.)
#[derive(Clone, Debug, PartialEq)]
pub enum NodeCollider {
    /// A circle of `radius` — the default, and the circle content silhouette.
    Ball { radius: f32 },
    /// An axis-aligned square with half-extent `half` (a document face).
    Square { half: f32 },
    /// A square with rounded corners: total half-extent `half`, corners rounded by `border`
    /// (a menu / rounded face).
    RoundedSquare { half: f32, border: f32 },
    /// A custom convex hull in body-local world units — the sprite's traced outline or a
    /// hand-edited polygon. Falls back to a ball of `fallback` if the hull is degenerate
    /// (fewer than 3 points / collinear). (Node-rep — sprite hull / shape editor.)
    Hull { points: Vec<(f32, f32)>, fallback: f32 },
}

impl NodeCollider {
    /// Lower to the parry shape rapier collides + queries with. Extents are clamped to a
    /// positive minimum so a zero-size node still has a pickable body.
    fn to_shared_shape(&self) -> SharedShape {
        match self {
            NodeCollider::Ball { radius } => SharedShape::ball(radius.max(1.0)),
            NodeCollider::Square { half } => {
                let h = half.max(1.0);
                SharedShape::cuboid(h, h)
            }
            NodeCollider::RoundedSquare { half, border } => {
                let b = border.clamp(0.1, half - 0.1).max(0.1);
                let h = (half - b).max(1.0);
                SharedShape::round_cuboid(h, h, b)
            }
            NodeCollider::Hull { points, fallback } => {
                // parry's `convex_hull` *asserts* `len >= 2` (it panics, not returns `None`) and
                // still yields nothing useful for a collinear set, so guard the degenerate cases
                // to the ball fallback; a real polygon needs at least 3 points.
                if points.len() < 3 {
                    SharedShape::ball(fallback.max(1.0))
                } else {
                    let pts: Vec<Point<f32>> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
                    SharedShape::convex_hull(&pts)
                        .unwrap_or_else(|| SharedShape::ball(fallback.max(1.0)))
                }
            }
        }
    }
}

/// Density for node-body colliders, chosen so each node's mass is ~= 1.0
/// (mass = density * pi * r^2 ~= 0.001 * pi * 18^2 ~= 1.0). The layout forces in
/// [`forces`] are then intuitive accelerations rather than being swamped by the
/// large mass a default density would give an 18px ball.
pub const NODE_BODY_DENSITY: f32 = 0.001;

/// Linear damping applied to every node body. Tuned for a slippery,
/// inertial feel: a released node coasts a moment before settling, rather
/// than stopping dead. Low enough to glide, high enough that the layout
/// still comes to rest without continuous input.
const DEFAULT_LINEAR_DAMPING: f32 = 2.5;

/// Angular damping. Bodies don't visibly rotate in the orrery
/// renderer (circles are isotropic), but rapier still tracks angular
/// velocity; high damping prevents the integrator from accumulating
/// spurious spin.
const DEFAULT_ANGULAR_DAMPING: f32 = 4.0;

/// Linear damping for scene-decoration bodies — low, so a drifting backdrop body
/// coasts a long while rather than settling quickly. (Physics scenes P1.)
const SCENE_DAMPING: f32 = 0.3;

/// Upper bound on bodies a single [`SceneSpec`] loads, so a runaway scene can't swamp
/// the physics actor's per-tick budget. (Physics scenes P3.)
const SCENE_BODY_CAP: usize = 200;

/// Collision groups that keep the graph and the scene from hard-colliding by
/// default. Node colliders are in [`NODE_GROUP`] and collide only with other nodes
/// (intangible to the scene); scene colliders are in [`SCENE_GROUP`] and collide
/// with each other while *admitting* nodes — so a node opts into touching the scene
/// by adding [`SCENE_GROUP`] to its own filter (the tangibility lever, P2). rapier's
/// pairwise test needs both sides to admit the other, so the node's filter alone
/// gates node-scene contact. (Physics scenes P1.)
const NODE_GROUP: Group = Group::GROUP_1;
const SCENE_GROUP: Group = Group::GROUP_2;

/// Interaction groups for a node collider: member of [`NODE_GROUP`], colliding only
/// with [`NODE_GROUP`] (intangible to the scene by default).
fn node_groups() -> InteractionGroups {
    InteractionGroups::new(NODE_GROUP, NODE_GROUP)
}

/// Interaction groups for a scene-decoration collider: member of [`SCENE_GROUP`],
/// colliding with the scene and admitting nodes (a node still passes through unless
/// its own filter opts in).
fn scene_groups() -> InteractionGroups {
    InteractionGroups::new(SCENE_GROUP, SCENE_GROUP | NODE_GROUP)
}

/// A stable, opaque handle to a scene-decoration body — a non-graph rapier body
/// sharing the orrery's world, distinct from a [`NodeKey`]. Minted by
/// [`Simulation::add_scene_body`]. (Physics scenes P1.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SceneBodyId(u64);

/// Whether a scene body moves under forces / collision (`Dynamic`) or is immovable
/// terrain (`Fixed`) — a floor, a wall, a peg. (Physics scenes P3.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneBodyType {
    Dynamic,
    Fixed,
}

/// One body in a declarative scene: its shape, world spawn, initial velocity, kind, and
/// bounciness. Data, not code — a [`SceneSpec`] is a list of these. (Physics scenes P3.)
#[derive(Clone, Debug)]
pub struct SceneBodySpec {
    pub collider: NodeCollider,
    pub position: (f32, f32),
    pub velocity: (f32, f32),
    pub body_type: SceneBodyType,
    pub restitution: f32,
}

/// A declarative physics scene the orrery's world can load: a set of bodies, the world
/// gravity to apply, and whether the graph collides with it by default. Loading one
/// ([`Simulation::load_scene`]) clears any prior scene. (Physics scenes P3.)
#[derive(Clone, Debug)]
pub struct SceneSpec {
    pub bodies: Vec<SceneBodySpec>,
    /// World gravity (px/s^2; `+y` is down at Mere's screen scale). `(0, 0)` for a
    /// gravity-free scene (a drifting backdrop, a bouncing box).
    pub gravity: (f32, f32),
    /// Whether the graph collides with the scene on load (the tangibility default).
    pub default_tangible: bool,
}

/// A transplanted demo scene: a bumpy fixed floor (a row of big fixed balls) with a
/// handful of dynamic balls falling onto it under gravity and piling up. The
/// "interactive scene" proof — data-defined, and the graph can knock the balls around
/// once made tangible. (Physics scenes P3.)
pub fn drop_bowl_scene() -> SceneSpec {
    let mut bodies = Vec::new();
    // The bumpy fixed floor.
    for i in 0..5 {
        bodies.push(SceneBodySpec {
            collider: NodeCollider::Ball { radius: 60.0 },
            position: (-280.0 + i as f32 * 140.0, 340.0),
            velocity: (0.0, 0.0),
            body_type: SceneBodyType::Fixed,
            restitution: 0.4,
        });
    }
    // Dynamic balls dropped from above the graph.
    for i in 0..8 {
        bodies.push(SceneBodySpec {
            collider: NodeCollider::Ball { radius: 22.0 },
            position: (-210.0 + i as f32 * 60.0, -260.0 - (i % 3) as f32 * 40.0),
            velocity: (0.0, 0.0),
            body_type: SceneBodyType::Dynamic,
            restitution: 0.5,
        });
    }
    SceneSpec { bodies, gravity: (0.0, 520.0), default_tangible: false }
}

/// A pluggable force-applier. Forces read the body store and apply
/// forces / impulses; gyre's tick walks every registered force
/// before stepping the world.
///
/// Implementors should be cheap — the tick budget is ~16ms total —
/// and write through `bodies.get_mut(...).add_force(...)` or
/// equivalent. Forces are queried in registration order each tick.
pub trait Force: Send {
    fn apply(&self, ctx: &mut ForceContext<'_>, dt: f32);
}

/// Per-tick view a [`Force`] sees: mutable access to the rapier body
/// store plus the NodeKey ↔ handle bimap so forces can reason about
/// pairs / topology when they need to.
pub struct ForceContext<'a> {
    pub bodies: &'a mut RigidBodySet,
    pub colliders: &'a ColliderSet,
    pub joints: &'a mut ImpulseJointSet,
    pub bodies_by_node: &'a HashMap<NodeKey, RigidBodyHandle>,
    pub nodes_by_body: &'a HashMap<RigidBodyHandle, NodeKey>,
    /// Topology the layout forces pull along (e.g. [`EdgeSpring`]). Node-key
    /// pairs, set via [`Simulation::sync_edges`]; gyre stays relation-taxonomy
    /// agnostic, so the caller decides which edge families feed the layout.
    pub edges: &'a [(NodeKey, NodeKey)],
    /// The spatial index, for forces that find neighbors by region instead of
    /// scanning all pairs (e.g. [`NodeExclusion`] cull-based repulsion). Current
    /// as of the last [`Simulation::tick`] step or [`Simulation::sync_with_graph`]
    /// (a tick's worth of staleness is harmless for soft forces).
    pub query_index: &'a QueryPipeline,
}

/// One rapier world + bookkeeping. The host owns one of these per
/// graph; gyre stays pure (no global state, no static handles).
pub struct Simulation {
    pipeline: PhysicsPipeline,
    parameters: IntegrationParameters,
    gravity: Vector<Real>,
    islands: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd_solver: CCDSolver,
    query_pipeline: QueryPipeline,
    bodies_by_node: HashMap<NodeKey, RigidBodyHandle>,
    nodes_by_body: HashMap<RigidBodyHandle, NodeKey>,
    edges: Vec<(NodeKey, NodeKey)>,
    forces: Vec<Box<dyn Force>>,
    /// Field couplings as a separately-replaceable force list (built-in layout
    /// forces stay in `forces`). A `CouplingForce` snapshots its target node set at
    /// build time, so a field move / resize / new node needs the whole set
    /// re-resolved — the host rebuilds these wholesale via
    /// [`set_coupling_forces`](Self::set_coupling_forces). (Field regions —
    /// rebuild-on-mutation.)
    coupling_forces: Vec<CouplingForce>,
    /// Linear damping applied to every node body — runtime-tunable (the "inertia"
    /// the physics settings expose): lower keeps more drift after a settle, higher
    /// brings nodes to rest sooner. New bodies take this; [`set_linear_damping`]
    /// also re-applies it to the live ones. (Physics settings.)
    linear_damping: f32,
    /// Non-graph scene-decoration bodies sharing this world (the "living backdrop" /
    /// interactive scene), each stored as `(handle, paint radius)` keyed by a
    /// [`SceneBodyId`]. They tick and collide like any body but carry no [`NodeKey`],
    /// so the layout forces (which key off `bodies_by_node`) ignore them. (Physics
    /// scenes P1.)
    scene_bodies: HashMap<SceneBodyId, (RigidBodyHandle, f32)>,
    /// Monotonic id source for [`scene_bodies`](Self::scene_bodies).
    next_scene_id: u64,
}

impl Default for Simulation {
    fn default() -> Self {
        Self::new()
    }
}

impl Simulation {
    pub fn new() -> Self {
        let mut parameters = IntegrationParameters::default();
        // Mere lives at world-coordinate scale where 1 unit ≈ 1
        // pixel — the default 1/60s timestep is fine, but we also
        // expose `tick(dt)` so callers can vary it.
        parameters.dt = 1.0 / 60.0;
        Self {
            pipeline: PhysicsPipeline::new(),
            parameters,
            // No global gravity — forces supply directional forces
            // when they want them. A "gravity" force is just another
            // Force impl.
            gravity: Vector::zeros(),
            islands: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            query_pipeline: QueryPipeline::new(),
            bodies_by_node: HashMap::new(),
            nodes_by_body: HashMap::new(),
            edges: Vec::new(),
            forces: Vec::new(),
            coupling_forces: Vec::new(),
            linear_damping: DEFAULT_LINEAR_DAMPING,
            scene_bodies: HashMap::new(),
            next_scene_id: 0,
        }
    }

    /// The linear damping every node body carries (the "inertia" tunable).
    pub fn linear_damping(&self) -> f32 {
        self.linear_damping
    }

    /// Set the linear damping for new node bodies and re-apply it to every live
    /// one, so a settings change takes effect immediately rather than only for
    /// nodes added afterward. (Physics settings.)
    pub fn set_linear_damping(&mut self, damping: f32) {
        self.linear_damping = damping;
        for (_, body) in self.bodies.iter_mut() {
            body.set_linear_damping(damping);
        }
    }

    /// Resize **and reshape** each listed node's collider (hit-target + hard-collision
    /// geometry) to match its visible face — a ball, square, rounded square, or custom hull —
    /// so a node collides and picks at its true face shape and size, not the uniform ball
    /// (Decision 5: the face IS the collider, so physics and picture stay in sync). Bodies
    /// keep position and velocity; only the shape changes, and the spatial index is refreshed
    /// so the next pick / cull / force query sees it. Mass is left at the spawn value — it is
    /// the face geometry, not the inertia, that tracks size. Nodes without a body are skipped.
    /// (P0/P5 collider; node-rep — collider matches shape.)
    pub fn set_node_colliders(&mut self, colliders: impl IntoIterator<Item = (NodeKey, NodeCollider)>) {
        let mut changed = false;
        for (node, collider) in colliders {
            let Some(&body_handle) = self.bodies_by_node.get(&node) else {
                continue;
            };
            // Copy the handles out so the immutable body borrow ends before the
            // mutable collider borrow (distinct fields, but the borrow checker needs
            // the split made explicit).
            let collider_handles: Vec<ColliderHandle> = self
                .bodies
                .get(body_handle)
                .map(|body| body.colliders().to_vec())
                .unwrap_or_default();
            let shape = collider.to_shared_shape();
            for handle in collider_handles {
                if let Some(c) = self.colliders.get_mut(handle) {
                    c.set_shape(shape.clone());
                    changed = true;
                }
            }
        }
        if changed {
            self.query_pipeline.update(&self.colliders);
        }
    }

    /// Register a force. Forces apply in registration order on every
    /// tick.
    pub fn add_force<F: Force + 'static>(&mut self, force: F) {
        self.forces.push(Box::new(force));
    }

    pub fn force_count(&self) -> usize {
        self.forces.len()
    }

    /// Replace the field-coupling forces wholesale (the built-in layout forces in
    /// `forces` are untouched). The host re-resolves every coupling against the
    /// current graph + node set and hands the fresh set here — the move / resize /
    /// new-node rebuild. Position-preserving: forces never touch body state, so
    /// swapping them leaves the layout exactly where it is. (Field regions.)
    pub fn set_coupling_forces(&mut self, forces: Vec<CouplingForce>) {
        self.coupling_forces = forces;
    }

    /// Number of field-coupling forces currently applied.
    pub fn coupling_force_count(&self) -> usize {
        self.coupling_forces.len()
    }

    /// Replace the topology the layout forces pull along (see
    /// [`ForceContext::edges`]). Caller-chosen node-key pairs: gyre does not
    /// read graph edges itself, so the caller filters to the relation families
    /// that should shape the layout (e.g. semantic edges, not arrangement).
    /// Idempotent; replaces the whole edge list.
    pub fn sync_edges(&mut self, edges: impl IntoIterator<Item = (NodeKey, NodeKey)>) {
        self.edges.clear();
        self.edges.extend(edges);
    }

    /// Number of edges currently feeding the layout forces.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Number of currently-tracked bodies. Useful for traces / tests.
    pub fn body_count(&self) -> usize {
        self.bodies_by_node.len()
    }

    /// Look up the body handle for a node, if any. Force implementors
    /// already get the bimap through [`ForceContext`]; this is for
    /// external callers (host drag handlers, etc.).
    pub fn body_for(&self, node: NodeKey) -> Option<RigidBodyHandle> {
        self.bodies_by_node.get(&node).copied()
    }

    /// Current projected position of a node's body, if it has one. Reads the
    /// live rapier translation, so it reflects the most recent [`Self::tick`].
    pub fn position_of(&self, node: NodeKey) -> Option<Point2D<f32>> {
        let handle = *self.bodies_by_node.get(&node)?;
        let t = self.bodies.get(handle)?.translation();
        Some(Point2D::new(t.x, t.y))
    }

    /// Seed (override) the positions of existing node bodies, e.g. from a
    /// cartography projection's positioned nodes, so a real layout strategy
    /// (radial, astroid, a converged force-directed pass) becomes the physics
    /// starting point instead of the graph's stored positions. Resets each
    /// seeded body's velocity so the settle starts clean, and refreshes the
    /// query index since this moves bodies outside a tick. Nodes without a body
    /// are skipped (seed after [`Self::sync_with_graph`]).
    ///
    /// Takes plain `(NodeKey, Point2D)` rather than a `cartography::Projection`
    /// on purpose: gyre is a kernel-tier substrate and must not depend on the
    /// projection layer above it. The caller maps `Projection.nodes` to these
    /// pairs (see the cartography-gyre layout seam doc).
    pub fn seed_positions(&mut self, positions: impl IntoIterator<Item = (NodeKey, Point2D<f32>)>) {
        let mut touched = false;
        for (node, pos) in positions {
            let Some(&handle) = self.bodies_by_node.get(&node) else {
                continue;
            };
            if let Some(body) = self.bodies.get_mut(handle) {
                body.set_translation(vector![pos.x, pos.y], true);
                body.set_linvel(vector![0.0, 0.0], true);
                touched = true;
            }
        }
        if touched {
            self.query_pipeline.update(&self.colliders);
        }
    }

    /// Iterate the current `(node, position)` of every node body — the live
    /// layout, e.g. for the caller to build a cartography projection for
    /// downstream consumers. Reflects the most recent [`Self::tick`]; order is
    /// unspecified.
    pub fn positions(&self) -> impl Iterator<Item = (NodeKey, Point2D<f32>)> + '_ {
        self.bodies_by_node.iter().filter_map(|(&node, &handle)| {
            self.bodies
                .get(handle)
                .map(|b| (node, Point2D::new(b.translation().x, b.translation().y)))
        })
    }

    /// A rapier-free [`LayoutView`] over the current layout: the live positions,
    /// the spring edge topology, and the node radius the picks/cull use. The host
    /// reads node picks, edge picks, and cull from this rather than the rapier
    /// query index, so those reads can run on the UI thread while the simulation
    /// itself ticks elsewhere (the always-offload physics actor, P6).
    pub fn view(&self) -> LayoutView {
        LayoutView::from_parts(self.positions(), self.edges.iter().copied(), NODE_BODY_RADIUS)
    }

    /// A `Send` [`LayoutSnapshot`] of the current positions, stamped with
    /// `generation` — the payload a physics actor emits each tick for the host to
    /// fold into its [`LayoutView`]. Positions only; edges stay with the host's
    /// graph, so they need not cross the actor boundary every frame.
    pub fn snapshot(&self, generation: u64) -> LayoutSnapshot {
        LayoutSnapshot {
            positions: self.positions().collect(),
            scene: self.scene_bodies().collect(),
            generation,
        }
    }

    /// Refresh the spatial query index so [`Self::hit_test`] and
    /// [`Self::cull_aabb`] reflect the colliders' current positions.
    /// [`Self::tick`] already updates the index each step; call this when
    /// you've moved bodies outside a tick (a drag via [`Self::pin`], or a
    /// fresh [`Self::sync_with_graph`] on a layout you don't intend to step).
    pub fn refresh_spatial_index(&mut self) {
        self.query_pipeline.update(&self.colliders);
    }

    /// Hit-test a world-space point: the node whose body collider contains it,
    /// or `None`. Reads the index as of the last [`Self::tick`] /
    /// [`Self::refresh_spatial_index`]. Node bodies are kept separated, so on
    /// the rare overlap this returns one of the hits, not a defined "topmost".
    ///
    /// Because every node is already a rapier collider, this is the
    /// `QueryPipeline` doing node picking for free — the orrery's canvas needs
    /// no separate index for *node* hit-testing (adoption roadmap R1b spike).
    pub fn hit_test(&self, point: Point2D<f32>) -> Option<NodeKey> {
        let colliders = &self.colliders;
        let nodes_by_body = &self.nodes_by_body;
        let p = Point::new(point.x, point.y);
        let mut hit = None;
        self.query_pipeline.intersections_with_point(
            &self.bodies,
            colliders,
            &p,
            QueryFilter::default(),
            |handle| {
                if let Some(node) = collider_to_node(colliders, nodes_by_body, handle) {
                    hit = Some(node);
                    return false; // stop at the first hit
                }
                true
            },
        );
        hit
    }

    /// Frustum cull: every node whose body collider's Aabb intersects `region`
    /// (world space). Reads the index as of the last tick / refresh. Order is
    /// unspecified.
    pub fn cull_aabb(&self, region: Box2D<f32>) -> Vec<NodeKey> {
        let colliders = &self.colliders;
        let nodes_by_body = &self.nodes_by_body;
        let aabb = Aabb::new(
            Point::new(region.min.x, region.min.y),
            Point::new(region.max.x, region.max.y),
        );
        let mut out = Vec::new();
        self.query_pipeline
            .colliders_with_aabb_intersecting_aabb(&aabb, |handle| {
                if let Some(node) = collider_to_node(colliders, nodes_by_body, *handle) {
                    out.push(node);
                }
                true // visit all
            });
        out
    }

    /// Make the simulation match the graph: spawn a body for every
    /// node that doesn't already have one, remove bodies whose nodes
    /// have vanished. New bodies start at the node's current
    /// projected position with zero velocity.
    ///
    /// Idempotent: safe to call every frame, every graph mutation,
    /// or once at startup — does nothing when the graph and bimap
    /// are already in sync. A thin convenience over [`Self::sync_nodes`]
    /// that reads each node's projected position from the graph.
    pub fn sync_with_graph(&mut self, graph: &Graph) {
        let nodes: Vec<(NodeKey, Point2D<f32>)> = graph
            .nodes()
            .map(|(key, node)| {
                let p = node.projected_position();
                (key, Point2D::new(p.x, p.y))
            })
            .collect();
        self.sync_nodes(nodes);
    }

    /// Reconcile the body set to exactly `nodes` (keyed by [`NodeKey`]): spawn a
    /// body at the given position for every key without one, remove bodies whose
    /// key is absent. The position is only used for *new* bodies (an existing
    /// body keeps its simulated position; use [`Self::seed_positions`] to move
    /// one).
    ///
    /// Decoupled from [`Graph`] on purpose: a physics actor drives the sim from a
    /// `Send` node list the host computed, so the graph never crosses the actor
    /// boundary. [`Self::sync_with_graph`] is the in-thread convenience wrapper.
    /// Idempotent.
    pub fn sync_nodes(&mut self, nodes: impl IntoIterator<Item = (NodeKey, Point2D<f32>)>) {
        let mut changed = false;

        // 1. Add bodies for new nodes (at the supplied position).
        let mut seen = std::collections::HashSet::with_capacity(self.bodies_by_node.len());
        for (key, position) in nodes {
            seen.insert(key);
            if self.bodies_by_node.contains_key(&key) {
                continue;
            }
            let body = RigidBodyBuilder::dynamic()
                .translation(vector![position.x, position.y])
                .linear_damping(self.linear_damping)
                .angular_damping(DEFAULT_ANGULAR_DAMPING)
                // Nodes never fall: scene gravity acts only on scene bodies, so the graph
                // layout is unaffected by a gravity scene. (Physics scenes P3.)
                .gravity_scale(0.0)
                .build();
            let handle = self.bodies.insert(body);
            let collider = ColliderBuilder::ball(NODE_BODY_RADIUS)
                .density(NODE_BODY_DENSITY)
                .restitution(0.0)
                .friction(0.0)
                .collision_groups(node_groups())
                .build();
            self.colliders
                .insert_with_parent(collider, handle, &mut self.bodies);
            self.bodies_by_node.insert(key, handle);
            self.nodes_by_body.insert(handle, key);
            changed = true;
        }

        // 2. Remove bodies whose nodes are gone.
        let stale: Vec<NodeKey> = self
            .bodies_by_node
            .keys()
            .copied()
            .filter(|key| !seen.contains(key))
            .collect();
        for key in stale {
            if let Some(handle) = self.bodies_by_node.remove(&key) {
                self.nodes_by_body.remove(&handle);
                self.bodies.remove(
                    handle,
                    &mut self.islands,
                    &mut self.colliders,
                    &mut self.impulse_joints,
                    &mut self.multibody_joints,
                    /* remove_attached_colliders */ true,
                );
                changed = true;
            }
        }

        // Keep the spatial index current with the new collider set, so
        // cull-based forces (e.g. NodeExclusion) see the bodies on the very
        // next tick rather than after the first step rebuilds the index.
        if changed {
            self.query_pipeline.update(&self.colliders);
        }
    }

    /// Add a non-graph **scene body** to this world — a decoration / interactive-scene
    /// element that ticks and collides like any body but carries no [`NodeKey`], so the
    /// graph's layout forces ignore it and (by the default collision groups) nodes pass
    /// through it. `collider` is its shape (the node shape vocabulary, reused),
    /// `position` its world spawn, `velocity` an initial drift in px/s. Returns its
    /// [`SceneBodyId`]. (Physics scenes P1.)
    pub fn add_scene_body(
        &mut self,
        collider: NodeCollider,
        position: Point2D<f32>,
        velocity: (f32, f32),
    ) -> SceneBodyId {
        let body = RigidBodyBuilder::dynamic()
            .translation(vector![position.x, position.y])
            .linvel(vector![velocity.0, velocity.1])
            .linear_damping(SCENE_DAMPING)
            .angular_damping(DEFAULT_ANGULAR_DAMPING)
            .build();
        let handle = self.bodies.insert(body);
        let shape = collider.to_shared_shape();
        let c = ColliderBuilder::new(shape)
            .density(NODE_BODY_DENSITY)
            .restitution(0.6)
            .friction(0.0)
            .collision_groups(scene_groups())
            .build();
        self.colliders.insert_with_parent(c, handle, &mut self.bodies);
        // A representative radius for the host's backdrop paint (the shape vocabulary's
        // nominal half-extent); the collider itself keeps the true shape.
        let radius = match &collider {
            NodeCollider::Ball { radius } => *radius,
            NodeCollider::Square { half } => *half,
            NodeCollider::RoundedSquare { half, .. } => *half,
            NodeCollider::Hull { fallback, .. } => *fallback,
        };
        let id = SceneBodyId(self.next_scene_id);
        self.next_scene_id += 1;
        self.scene_bodies.insert(id, (handle, radius));
        self.query_pipeline.update(&self.colliders);
        id
    }

    /// Remove a scene body. A no-op for an unknown id. (Physics scenes P1.)
    pub fn remove_scene_body(&mut self, id: SceneBodyId) {
        if let Some((handle, _)) = self.scene_bodies.remove(&id) {
            self.bodies.remove(
                handle,
                &mut self.islands,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                /* remove_attached_colliders */ true,
            );
            self.query_pipeline.update(&self.colliders);
        }
    }

    /// Remove every scene body, leaving the graph untouched. (Physics scenes P1.)
    pub fn clear_scene(&mut self) {
        let handles: Vec<RigidBodyHandle> = self.scene_bodies.values().map(|(h, _)| *h).collect();
        for handle in handles {
            self.bodies.remove(
                handle,
                &mut self.islands,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                true,
            );
        }
        self.scene_bodies.clear();
        self.query_pipeline.update(&self.colliders);
    }

    /// Iterate the live `(id, position, paint radius)` of every scene body — the host's
    /// read for painting the backdrop. Reflects the last [`Self::tick`]; order is
    /// unspecified. (Physics scenes P1.)
    pub fn scene_bodies(&self) -> impl Iterator<Item = (SceneBodyId, Point2D<f32>, f32)> + '_ {
        self.scene_bodies.iter().filter_map(|(&id, &(handle, radius))| {
            self.bodies
                .get(handle)
                .map(|b| (id, Point2D::new(b.translation().x, b.translation().y), radius))
        })
    }

    /// Number of scene bodies in the world. (Physics scenes P1.)
    pub fn scene_body_count(&self) -> usize {
        self.scene_bodies.len()
    }

    /// Make a single node tangible (it collides with scene bodies) or intangible (it
    /// passes through), by re-masking its collider's filter. Node-node collision is
    /// unaffected. A no-op for an unknown node. (Physics scenes P2 — tangibility lever.)
    pub fn set_node_tangibility(&mut self, node: NodeKey, tangible: bool) {
        let Some(&body_handle) = self.bodies_by_node.get(&node) else {
            return;
        };
        self.remask_node(body_handle, tangible);
        self.query_pipeline.update(&self.colliders);
    }

    /// Set every node's tangibility at once (the scene-wide lever): `true` lets the graph
    /// collide with scene bodies, `false` (the default) passes through. Node-node collision
    /// is unaffected either way. (Physics scenes P2.)
    pub fn set_nodes_tangible(&mut self, tangible: bool) {
        let handles: Vec<RigidBodyHandle> = self.bodies_by_node.values().copied().collect();
        for handle in handles {
            self.remask_node(handle, tangible);
        }
        self.query_pipeline.update(&self.colliders);
    }

    /// Re-mask one node body's collider(s) to the intangible (`NODE`) or tangible
    /// (`NODE | SCENE`) filter. (Physics scenes P2.)
    fn remask_node(&mut self, body_handle: RigidBodyHandle, tangible: bool) {
        let groups = if tangible {
            InteractionGroups::new(NODE_GROUP, NODE_GROUP | SCENE_GROUP)
        } else {
            node_groups()
        };
        let collider_handles: Vec<ColliderHandle> = self
            .bodies
            .get(body_handle)
            .map(|b| b.colliders().to_vec())
            .unwrap_or_default();
        for ch in collider_handles {
            if let Some(c) = self.colliders.get_mut(ch) {
                c.set_collision_groups(groups);
            }
        }
    }

    /// Set the world gravity (px/s^2). Node bodies carry `gravity_scale(0)`, so only
    /// scene bodies fall; the graph layout is unaffected. (Physics scenes P3.)
    pub fn set_gravity(&mut self, gravity: (f32, f32)) {
        self.gravity = vector![gravity.0, gravity.1];
    }

    /// Load a declarative [`SceneSpec`] into the world: clear any prior scene, set its
    /// gravity, spawn its bodies (capped at [`SCENE_BODY_CAP`]), and apply its default
    /// tangibility. (Physics scenes P3.)
    pub fn load_scene(&mut self, spec: &SceneSpec) {
        self.clear_scene();
        self.set_gravity(spec.gravity);
        for b in spec.bodies.iter().take(SCENE_BODY_CAP) {
            let builder = match b.body_type {
                SceneBodyType::Fixed => RigidBodyBuilder::fixed(),
                SceneBodyType::Dynamic => RigidBodyBuilder::dynamic(),
            };
            let body = builder
                .translation(vector![b.position.0, b.position.1])
                .linvel(vector![b.velocity.0, b.velocity.1])
                .linear_damping(SCENE_DAMPING)
                .angular_damping(DEFAULT_ANGULAR_DAMPING)
                .build();
            let handle = self.bodies.insert(body);
            let shape = b.collider.to_shared_shape();
            let c = ColliderBuilder::new(shape)
                .density(NODE_BODY_DENSITY)
                .restitution(b.restitution)
                .friction(0.3)
                .collision_groups(scene_groups())
                .build();
            self.colliders.insert_with_parent(c, handle, &mut self.bodies);
            let radius = match &b.collider {
                NodeCollider::Ball { radius } => *radius,
                NodeCollider::Square { half } => *half,
                NodeCollider::RoundedSquare { half, .. } => *half,
                NodeCollider::Hull { fallback, .. } => *fallback,
            };
            let id = SceneBodyId(self.next_scene_id);
            self.next_scene_id += 1;
            self.scene_bodies.insert(id, (handle, radius));
        }
        self.set_nodes_tangible(spec.default_tangible);
        self.query_pipeline.update(&self.colliders);
    }

    /// Advance the simulation by `dt` seconds. Walks every registered
    /// [`Force`] first (so forces accumulate), then steps the world.
    /// Position writeback to the graph is a separate call —
    /// [`Simulation::write_positions_to`] — so callers can read
    /// positions for purposes other than committing to the graph
    /// (debug overlays, e.g.).
    pub fn tick(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        self.parameters.dt = dt;

        // rapier's `add_force` is a *persistent* force (it survives across
        // steps until reset), so clear last tick's force forces before this
        // tick's forces set fresh ones. Without this, per-tick forces compound
        // and the layout goes unstable.
        if !self.forces.is_empty() || !self.coupling_forces.is_empty() {
            for (_, body) in self.bodies.iter_mut() {
                body.reset_forces(false);
            }
            let mut ctx = ForceContext {
                bodies: &mut self.bodies,
                colliders: &self.colliders,
                joints: &mut self.impulse_joints,
                bodies_by_node: &self.bodies_by_node,
                nodes_by_body: &self.nodes_by_body,
                edges: &self.edges,
                query_index: &self.query_pipeline,
            };
            for force in &self.forces {
                force.apply(&mut ctx, dt);
            }
            // Field couplings apply after the built-ins, in the same reset window.
            for force in &self.coupling_forces {
                force.apply(&mut ctx, dt);
            }
        }

        let physics_hooks = ();
        let event_handler = ();
        self.pipeline.step(
            &self.gravity,
            &self.parameters,
            &mut self.islands,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            Some(&mut self.query_pipeline),
            &physics_hooks,
            &event_handler,
        );
    }

    /// Copy each body's current translation onto its corresponding
    /// graph node. Returns the number of node positions that were
    /// actually changed (helpful for "only notify when something
    /// moved" tick loops).
    pub fn write_positions_to(&self, graph: &mut Graph) -> usize {
        let mut changed = 0;
        for (key, handle) in &self.bodies_by_node {
            let Some(body) = self.bodies.get(*handle) else {
                continue;
            };
            let t = body.translation();
            let next = Point2D::new(t.x, t.y);
            let prev = graph.get_node(*key).map(|n| n.projected_position());
            if prev != Some(next) {
                if graph.set_node_position(*key, next) {
                    changed += 1;
                }
            }
        }
        changed
    }

    /// True when every body's linear velocity is below
    /// `velocity_epsilon`. Useful for "suspend the tick when the
    /// graph is at rest" idle-detection.
    pub fn is_at_rest(&self, velocity_epsilon: f32) -> bool {
        let eps_sq = velocity_epsilon * velocity_epsilon;
        for (_, body) in self.bodies.iter() {
            let v = body.linvel();
            if v.x * v.x + v.y * v.y > eps_sq {
                return false;
            }
        }
        true
    }

    /// Pin a node's body to a position via a kinematic-position
    /// override. Use this during a drag: call `pin` while the user
    /// holds the mouse, then `unpin` on release so other forces can
    /// react to the final position again.
    pub fn pin(&mut self, node: NodeKey, position: Point2D<f32>) {
        let Some(handle) = self.bodies_by_node.get(&node).copied() else {
            return;
        };
        let Some(body) = self.bodies.get_mut(handle) else {
            return;
        };
        body.set_body_type(RigidBodyType::KinematicPositionBased, true);
        body.set_next_kinematic_translation(vector![position.x, position.y]);
    }

    pub fn unpin(&mut self, node: NodeKey) {
        let Some(handle) = self.bodies_by_node.get(&node).copied() else {
            return;
        };
        let Some(body) = self.bodies.get_mut(handle) else {
            return;
        };
        body.set_body_type(RigidBodyType::Dynamic, true);
    }
}

/// Map a collider back to the node it represents, via its parent rigid body.
/// A free function (not a method) so the query callbacks can borrow the two
/// forces they need without capturing the whole `Simulation`. `pub(crate)` so
/// [`forces`] can reuse it for cull-based neighbor queries.
pub(crate) fn collider_to_node(
    colliders: &ColliderSet,
    nodes_by_body: &HashMap<RigidBodyHandle, NodeKey>,
    handle: ColliderHandle,
) -> Option<NodeKey> {
    let body = colliders.get(handle)?.parent()?;
    nodes_by_body.get(&body).copied()
}

// The simulation must be `Send` so a host can build it on, and run its tick on,
// a dedicated physics actor thread (P6 always-offload). It is intentionally not
// required to be `Sync` — only one thread (the actor) ever touches it.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<Simulation>();
    assert_send::<LayoutSnapshot>();
};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod scene_tests {
    use euclid::default::Point2D;
    use kernel::graph::NodeKey;

    use crate::{NODE_BODY_RADIUS, NodeCollider, Simulation};

    #[test]
    fn scene_body_is_intangible_to_nodes_and_ignored_by_forces() {
        let mut sim = Simulation::new();
        let node = NodeKey::new(0);
        sim.sync_nodes([(node, Point2D::new(0.0, 0.0))]);
        // A scene body overlapping the node (centers 5px apart, radii sum 36) — a hard
        // collision would shove the node off the origin.
        let id = sim.add_scene_body(
            NodeCollider::Ball { radius: NODE_BODY_RADIUS },
            Point2D::new(5.0, 0.0),
            (0.0, 0.0),
        );
        assert_eq!(sim.scene_body_count(), 1);
        assert_eq!(sim.body_count(), 1, "the scene body is not counted as a node");
        assert_eq!(sim.scene_bodies().count(), 1);

        // No forces registered, so the node only moves if the scene body collides with
        // it. Under the default groups it is intangible: tick and the node stays put.
        for _ in 0..60 {
            sim.tick(1.0 / 60.0);
        }
        let p = sim.position_of(node).expect("node has a position");
        assert!(
            p.x.abs() < 1.0 && p.y.abs() < 1.0,
            "an intangible scene body must not push the node off the origin (was {p:?})",
        );

        // Clearing the scene leaves the node untouched; a stale remove is a no-op.
        sim.clear_scene();
        assert_eq!(sim.scene_body_count(), 0);
        assert_eq!(sim.body_count(), 1);
        sim.remove_scene_body(id);
    }

    #[test]
    fn node_tangibility_toggles_scene_collision() {
        let mut sim = Simulation::new();
        let node = NodeKey::new(0);
        sim.sync_nodes([(node, Point2D::new(0.0, 0.0))]);
        sim.add_scene_body(
            NodeCollider::Ball { radius: NODE_BODY_RADIUS },
            Point2D::new(5.0, 0.0),
            (0.0, 0.0),
        );

        // Intangible (default): the overlapping scene body never pushes the node.
        for _ in 0..60 {
            sim.tick(1.0 / 60.0);
        }
        let intangible = sim.position_of(node).expect("node has a position");
        assert!(intangible.x.abs() < 1.0, "intangible: node not pushed (was {intangible:?})");

        // Flip the node tangible: now the overlapping scene body's hard collision shoves it
        // off the origin (away from the body at +x, so toward -x).
        sim.set_nodes_tangible(true);
        for _ in 0..60 {
            sim.tick(1.0 / 60.0);
        }
        let tangible = sim.position_of(node).expect("node has a position");
        assert!(
            tangible.x < -1.0,
            "tangible: the scene body pushed the node off the origin (was {tangible:?})",
        );
    }

    #[test]
    fn load_scene_falls_under_gravity_while_nodes_float() {
        let mut sim = Simulation::new();
        let node = NodeKey::new(0);
        sim.sync_nodes([(node, Point2D::new(0.0, 0.0))]);
        sim.load_scene(&crate::drop_bowl_scene());
        assert_eq!(sim.scene_body_count(), 13, "5 floor + 8 falling balls");

        let min_before = sim.scene_bodies().map(|(_, p, _)| p.y).fold(f32::INFINITY, f32::min);
        for _ in 0..60 {
            sim.tick(1.0 / 60.0);
        }
        let min_after = sim.scene_bodies().map(|(_, p, _)| p.y).fold(f32::INFINITY, f32::min);
        assert!(min_after > min_before + 5.0, "gravity pulled the falling balls down");

        // The node carries gravity_scale(0), so scene gravity never drags the graph down.
        let np = sim.position_of(node).expect("node has a position");
        assert!(np.y.abs() < 5.0, "nodes float under scene gravity (was {np:?})");
    }
}
