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
//!   [`RigidBodyHandle`] bimap, position-based hit-test / cull, and the
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

use std::collections::{HashMap, VecDeque};

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

/// Position-Based Fluids (PBF): our own small SPH liquid for the orrery (salva lags rapier badly,
/// so we roll our own). Standalone solver for now (a settling pool); wiring it into the live
/// [`Simulation`] + render is the next phase. (Physics scenes P4c.)
pub mod fluid;
pub use fluid::{Basin, Fluid, FluidContact, FluidParams};

/// The declarative scene library: ready-made [`SceneSpec`]s the orrery loads behind the
/// graph (drop-bowl, pyramid, dominoes, Galton board, funnel, drift). Data, not engine
/// code; the growing catalog lives here to keep `lib.rs` under the size ceiling.
pub mod scenes;
pub use scenes::{
    chain_scene, domino_scene, drift_scene, drop_bowl_scene, fountain_scene, funnel_scene,
    galton_scene, pyramid_scene, whirlpool_scene,
};

/// The declarative scene **format**: the data types a [`SceneSpec`] is built from (bodies,
/// joints, world settings). Split from `lib.rs` because the format grows independently of the
/// simulation core. (Physics scenes P4b.)
mod scene_spec;
pub use scene_spec::{
    JointMotorSpec, SceneBodyId, SceneBodySpec, SceneBodyType, SceneEmitter, SceneField, SceneJoint,
    SceneJointSpec, SceneSpec,
};

/// Scene-geometry queries for the orrery canvas: edge geometry, edge picking,
/// and marquee rect-select (node point-pick + cull live on `Simulation`
/// directly). Split out to keep `lib.rs` under the per-file size ceiling.
mod query;

/// The rapier-free read model — a position-only [`LayoutView`] the host renders
/// and hit-tests from, plus the [`LayoutSnapshot`] a physics actor emits to
/// refresh it. The seam that lets the simulation run off the UI thread.
mod view;
pub use view::{LayoutSnapshot, LayoutView, RectSelection, SceneBodyView};

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
                    let pts: Vec<Vector> = points.iter().map(|&(x, y)| Vector::new(x, y)).collect();
                    SharedShape::convex_hull(&pts)
                        .unwrap_or_else(|| SharedShape::ball(fallback.max(1.0)))
                }
            }
        }
    }
}

/// A node's physical **material** on the Body axis: how its rapier body responds to contact
/// and weight. The defaults match the spawn constants, so an unconfigured node is unchanged;
/// the orrery pushes per-node overrides via [`Simulation::set_node_materials`] (the same
/// re-apply-to-live-bodies shape as [`Simulation::set_linear_damping`]). Independent of the
/// node's shape (the collider geometry) and its face (the texture). (Node body & face — material.)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeMaterial {
    /// Bounciness on contact: 0 (dead) to 1 (perfectly elastic).
    pub restitution: f32,
    /// Surface grip: 0 (frictionless) upward.
    pub friction: f32,
    /// Mass density (mass = density * area); higher is heavier and harder to push.
    pub density: f32,
}

impl Default for NodeMaterial {
    fn default() -> Self {
        Self { restitution: 0.0, friction: 0.0, density: NODE_BODY_DENSITY }
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

/// Linear damping for the dynamic bodies of a *perpetual* scene (a drifting backdrop):
/// near-zero so the motion coasts for minutes rather than bleeding to rest under the
/// heavier [`SCENE_DAMPING`] that lets a settling scene come to rest. (Physics scenes P4a.)
const PERPETUAL_SCENE_DAMPING: f32 = 0.01;

/// Scale from a fluid contact's net push-out (world units) to the impulse applied back on a dynamic
/// body — the two-way fluid coupling strength. Gentle, so a tangible node is nudged / bobs on the
/// pool rather than launched. (Physics scenes P4c.)
const FLUID_REACTION: f32 = 0.25;

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
    InteractionGroups::new(NODE_GROUP, NODE_GROUP, InteractionTestMode::And)
}

/// Interaction groups for a scene-decoration collider: member of [`SCENE_GROUP`],
/// colliding with the scene and admitting nodes (a node still passes through unless
/// its own filter opts in).
fn scene_groups() -> InteractionGroups {
    InteractionGroups::new(SCENE_GROUP, SCENE_GROUP | NODE_GROUP, InteractionTestMode::And)
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
    /// Topology the layout forces pull along (e.g. [`EdgeSpring`]). Node-key
    /// pairs, set via [`Simulation::sync_edges`]; gyre stays relation-taxonomy
    /// agnostic, so the caller decides which edge families feed the layout.
    pub edges: &'a [(NodeKey, NodeKey)],
}

/// A live emitter: its spec plus the per-tick spawn accumulator, a deterministic jitter RNG, and the
/// ids+ages of the bodies it has spawned (a FIFO so the oldest reap first). (Physics scenes —
/// emitters.)
struct LiveEmitter {
    spec: SceneEmitter,
    accumulator: f32,
    rng: u32,
    spawned: VecDeque<(SceneBodyId, f32)>,
}

/// xorshift32 step — a tiny deterministic PRNG for emitter jitter (no `rand` dep, reproducible
/// headless). (Physics scenes — emitters.)
fn xorshift(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

/// A deterministic jitter in `[-amount, amount]`.
fn jitter(state: &mut u32, amount: f32) -> f32 {
    if amount == 0.0 {
        return 0.0;
    }
    let unit = xorshift(state) as f32 / u32::MAX as f32; // [0, 1]
    (unit * 2.0 - 1.0) * amount
}

/// One rapier world + bookkeeping. The host owns one of these per
/// graph; gyre stays pure (no global state, no static handles).
pub struct Simulation {
    pipeline: PhysicsPipeline,
    parameters: IntegrationParameters,
    gravity: Vector,
    islands: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd_solver: CCDSolver,
    bodies_by_node: HashMap<NodeKey, RigidBodyHandle>,
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
    /// interactive scene), each stored as `(handle, collider shape)` keyed by a
    /// [`SceneBodyId`]. The shape rides along so the host can paint the true face (P4b).
    /// They tick and collide like any body but carry no [`NodeKey`], so the layout forces
    /// (which key off `bodies_by_node`) ignore them. (Physics scenes P1.)
    scene_bodies: HashMap<SceneBodyId, (RigidBodyHandle, NodeCollider)>,
    /// Monotonic id source for [`scene_bodies`](Self::scene_bodies).
    next_scene_id: u64,
    /// Whether the loaded scene wants to keep moving forever (a perpetual backdrop) rather
    /// than settle — the physics actor reads this via [`Self::scene_perpetual`] to keep
    /// ticking instead of parking at rest. Reset by [`Self::clear_scene`]. (Physics scenes P4a.)
    scene_perpetual: bool,
    /// The liquid pool sharing this world, if any ([`fluid`] PBF). `None` until one is loaded;
    /// stepped each [`Self::tick`] and emitted in the snapshot for rendering. (Physics scenes P4c.)
    fluid: Option<Fluid>,
    /// Scene-wide tangibility lever state (mirrors [`Self::set_nodes_tangible`]): when `true`, node
    /// bodies couple to the fluid (the graph stirs the pool); when `false` they pass through. (P4c.)
    nodes_tangible: bool,
    /// A continuous force-field over the scene's dynamic bodies (a whirlpool / well), if any. Keeps
    /// the actor ticking while set; cleared by [`Self::clear_scene`]. (Physics scenes P4 fields.)
    scene_field: Option<SceneField>,
    /// Continuous body emitters (fountains / streams). Stepped each tick: spawn by rate, reap by
    /// age. Keep the actor ticking while present. (Physics scenes — emitters.)
    emitters: Vec<LiveEmitter>,
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
            gravity: Vector::ZERO,
            islands: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            bodies_by_node: HashMap::new(),
            edges: Vec::new(),
            forces: Vec::new(),
            coupling_forces: Vec::new(),
            linear_damping: DEFAULT_LINEAR_DAMPING,
            scene_bodies: HashMap::new(),
            next_scene_id: 0,
            scene_perpetual: false,
            fluid: None,
            nodes_tangible: false,
            scene_field: None,
            emitters: Vec::new(),
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
    /// so a node collides at its true face shape and size, not the uniform ball
    /// (Decision 5: the face IS the collider, so physics and picture stay in sync). Bodies
    /// keep position and velocity; only the shape changes. Mass is left at the spawn value — it is
    /// the face geometry, not the inertia, that tracks size. Nodes without a body are skipped.
    /// (P0/P5 collider; node-rep — collider matches shape.)
    pub fn set_node_colliders(&mut self, colliders: impl IntoIterator<Item = (NodeKey, NodeCollider)>) {
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
                }
            }
        }
    }

    /// Apply each listed node's physical **material** (restitution / friction / density) to its
    /// live body's colliders, re-applying immediately like [`set_linear_damping`](Self::set_linear_damping).
    /// A density change re-derives the body's mass from its colliders, so a heavier node resists
    /// pushing. The defaults match the spawn values, so an unconfigured node is a no-op. Nodes
    /// without a body are skipped. (Node body & face — material.)
    pub fn set_node_materials(&mut self, materials: impl IntoIterator<Item = (NodeKey, NodeMaterial)>) {
        for (node, material) in materials {
            let Some(&body_handle) = self.bodies_by_node.get(&node) else {
                continue;
            };
            // Split the borrows (immutable body read for the handles, then mutable collider
            // writes) the way `set_node_colliders` does.
            let collider_handles: Vec<ColliderHandle> = self
                .bodies
                .get(body_handle)
                .map(|body| body.colliders().to_vec())
                .unwrap_or_default();
            for handle in collider_handles {
                if let Some(c) = self.colliders.get_mut(handle) {
                    c.set_restitution(material.restitution);
                    c.set_friction(material.friction);
                    c.set_density(material.density);
                }
            }
            // Density feeds mass, which rapier caches on the body — recompute it so the new
            // weight takes effect (distinct fields: `bodies` mut, `colliders` read).
            if let Some(body) = self.bodies.get_mut(body_handle) {
                body.recompute_mass_properties_from_colliders(&self.colliders);
            }
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
    /// seeded body's velocity so the settle starts clean. Nodes without a body
    /// are skipped (seed after [`Self::sync_with_graph`]).
    ///
    /// Takes plain `(NodeKey, Point2D)` rather than a `cartography::Projection`
    /// on purpose: gyre is a kernel-tier substrate and must not depend on the
    /// projection layer above it. The caller maps `Projection.nodes` to these
    /// pairs (see the cartography-gyre layout seam doc).
    pub fn seed_positions(&mut self, positions: impl IntoIterator<Item = (NodeKey, Point2D<f32>)>) {
        for (node, pos) in positions {
            let Some(&handle) = self.bodies_by_node.get(&node) else {
                continue;
            };
            if let Some(body) = self.bodies.get_mut(handle) {
                body.set_translation(Vector::new(pos.x, pos.y), true);
                body.set_linvel(Vector::ZERO, true);
            }
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
            fluid: self.fluid.as_ref().map(|f| f.positions().collect()).unwrap_or_default(),
            fluid_radius: self.fluid.as_ref().map(|f| f.params().particle_radius).unwrap_or(0.0),
            generation,
        }
    }

    /// Retained for call-site compatibility; a no-op now. [`Self::hit_test`] and
    /// [`Self::cull_aabb`] read live body translations directly (the rapier
    /// `QueryPipeline` went ephemeral in rapier 0.33), so there is no separate
    /// index to refresh after moving bodies outside a tick.
    pub fn refresh_spatial_index(&mut self) {}

    /// Hit-test a world-space point: the node whose body lies within
    /// [`NODE_BODY_RADIUS`] of it, or `None`. Reads live positions, so it always
    /// reflects the most recent [`Self::tick`]. Node bodies are kept separated,
    /// so on the rare overlap this returns one of the hits, not a defined
    /// "topmost".
    ///
    /// Position-and-radius, matching the rapier-free [`LayoutView`] the orrery
    /// actually picks from on the UI thread — the two stay consistent. (Restore
    /// shape-accurate, collider-true picking in `LayoutView` if a non-ball node
    /// face ever needs it; that is the live path, not this in-thread helper.)
    pub fn hit_test(&self, point: Point2D<f32>) -> Option<NodeKey> {
        let r2 = NODE_BODY_RADIUS * NODE_BODY_RADIUS;
        self.positions()
            .find(|(_, p)| (*p - point).square_length() <= r2)
            .map(|(node, _)| node)
    }

    /// Frustum cull: every node whose body (center ± [`NODE_BODY_RADIUS`])
    /// intersects `region` (world space). Reads live positions. Order is
    /// unspecified.
    pub fn cull_aabb(&self, region: Box2D<f32>) -> Vec<NodeKey> {
        let r = NODE_BODY_RADIUS;
        self.positions()
            .filter(|(_, p)| {
                Box2D::new(Point2D::new(p.x - r, p.y - r), Point2D::new(p.x + r, p.y + r))
                    .intersects(&region)
            })
            .map(|(node, _)| node)
            .collect()
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
        // 1. Add bodies for new nodes (at the supplied position).
        let mut seen = std::collections::HashSet::with_capacity(self.bodies_by_node.len());
        for (key, position) in nodes {
            seen.insert(key);
            if self.bodies_by_node.contains_key(&key) {
                continue;
            }
            let body = RigidBodyBuilder::dynamic()
                .translation(Vector::new(position.x, position.y))
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
                self.bodies.remove(
                    handle,
                    &mut self.islands,
                    &mut self.colliders,
                    &mut self.impulse_joints,
                    &mut self.multibody_joints,
                    /* remove_attached_colliders */ true,
                );
            }
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
            .translation(Vector::new(position.x, position.y))
            .linvel(Vector::new(velocity.0, velocity.1))
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
        // Keep the collider shape alongside the handle for the host's shape-aware paint. (P4b.)
        let id = SceneBodyId(self.next_scene_id);
        self.next_scene_id += 1;
        self.scene_bodies.insert(id, (handle, collider));
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
        self.scene_perpetual = false;
        self.scene_field = None;
        self.emitters.clear();
    }

    /// Iterate every scene body as a paintable [`SceneBodyView`] (id, position, rotation, shape)
    /// — the host's read for the shape-aware backdrop paint. Reflects the last [`Self::tick`];
    /// order is unspecified. (Physics scenes P1 / P4b.)
    pub fn scene_bodies(&self) -> impl Iterator<Item = SceneBodyView> + '_ {
        self.scene_bodies.iter().filter_map(|(&id, (handle, collider))| {
            self.bodies.get(*handle).map(|b| {
                let t = b.translation();
                SceneBodyView {
                    id,
                    position: Point2D::new(t.x, t.y),
                    rotation: b.rotation().angle(),
                    collider: collider.clone(),
                }
            })
        })
    }

    /// Number of scene bodies in the world. (Physics scenes P1.)
    pub fn scene_body_count(&self) -> usize {
        self.scene_bodies.len()
    }

    /// Whether the loaded scene wants to keep moving forever (a perpetual backdrop) rather
    /// than settle. The physics actor reads this to keep ticking instead of parking at rest;
    /// `false` once the scene is cleared. (Physics scenes P4a.)
    pub fn scene_perpetual(&self) -> bool {
        self.scene_perpetual
    }

    /// Load a liquid pool into the world: a `cols × rows` block of PBF particles dropped into an
    /// analytic [`Basin`], replacing any prior fluid. The pool settles under its own gravity and
    /// rides the snapshot for rendering. (Physics scenes P4c.)
    pub fn load_fluid(
        &mut self,
        params: FluidParams,
        basin: Basin,
        spawn_origin: Point2D<f32>,
        cols: usize,
        rows: usize,
        spacing: f32,
    ) {
        let mut fluid = Fluid::new(params, basin);
        fluid.spawn_block(spawn_origin, cols, rows, spacing);
        self.fluid = Some(fluid);
    }

    /// Remove the liquid pool. (Physics scenes P4c.)
    pub fn clear_fluid(&mut self) {
        self.fluid = None;
    }

    /// Whether a liquid pool is loaded. The physics actor keeps ticking while one is, so the pool
    /// keeps flowing / settling. (Physics scenes P4c.)
    pub fn has_fluid(&self) -> bool {
        self.fluid.is_some()
    }

    /// Set (or clear) the scene-wide force-field — a whirlpool / well over the dynamic scene bodies.
    /// (Physics scenes P4 — force-field tier.)
    pub fn set_scene_field(&mut self, field: Option<SceneField>) {
        self.scene_field = field;
    }

    /// Whether the world has live, self-driving motion the physics actor must keep ticking for even
    /// at rest: a perpetual backdrop, a fluid pool, or a force-field. (Physics scenes P4.)
    pub fn wants_continuous_tick(&self) -> bool {
        self.scene_perpetual
            || self.fluid.is_some()
            || self.scene_field.is_some()
            || !self.emitters.is_empty()
    }

    /// Add a continuous body emitter (a fountain / stream). Seeded deterministically. (Emitters.)
    pub fn add_emitter(&mut self, spec: SceneEmitter) {
        let seed = 0x9E3779B9u32 ^ (self.emitters.len() as u32 + 1).wrapping_mul(0x85EBCA6B);
        self.emitters.push(LiveEmitter {
            spec,
            accumulator: 0.0,
            rng: seed | 1,
            spawned: VecDeque::new(),
        });
    }

    /// Remove every emitter and the bodies it spawned. (Emitters.)
    pub fn clear_emitters(&mut self) {
        let emitters = std::mem::take(&mut self.emitters);
        for em in emitters {
            for (id, _) in em.spawned {
                self.remove_scene_body(id);
            }
        }
    }

    /// Number of active emitters. (Emitters.)
    pub fn emitter_count(&self) -> usize {
        self.emitters.len()
    }

    /// Step every emitter: age + reap its bodies past `lifetime_secs` (oldest first), then spawn by
    /// `rate_per_sec`, capped by `max_alive` and the global [`SCENE_BODY_CAP`]. (Emitters.)
    fn step_emitters(&mut self, dt: f32) {
        if self.emitters.is_empty() {
            return;
        }
        let mut emitters = std::mem::take(&mut self.emitters);
        for em in &mut emitters {
            for entry in em.spawned.iter_mut() {
                entry.1 += dt;
            }
            while let Some(&(id, age)) = em.spawned.front() {
                if age < em.spec.lifetime_secs {
                    break;
                }
                self.remove_scene_body(id);
                em.spawned.pop_front();
            }
            em.accumulator += em.spec.rate_per_sec * dt;
            while em.accumulator >= 1.0
                && em.spawned.len() < em.spec.max_alive
                && self.scene_body_count() < SCENE_BODY_CAP
            {
                em.accumulator -= 1.0;
                let px = em.spec.position.0 + jitter(&mut em.rng, em.spec.position_jitter.0);
                let py = em.spec.position.1 + jitter(&mut em.rng, em.spec.position_jitter.1);
                let vx = em.spec.velocity.0 + jitter(&mut em.rng, em.spec.velocity_jitter.0);
                let vy = em.spec.velocity.1 + jitter(&mut em.rng, em.spec.velocity_jitter.1);
                let id =
                    self.add_scene_body(em.spec.collider.clone(), Point2D::new(px, py), (vx, vy));
                em.spawned.push_back((id, 0.0));
            }
        }
        self.emitters = emitters;
    }

    /// Iterate the live fluid particle positions (empty when no pool is loaded) — the render read.
    /// (Physics scenes P4c.)
    pub fn fluid_particles(&self) -> impl Iterator<Item = Point2D<f32>> + '_ {
        self.fluid.iter().flat_map(|f| f.positions())
    }

    /// Couple the fluid to the rapier bodies: push particles out of every scene body (the bowl
    /// bounds the pool, dynamic props float) and out of node bodies when the graph is tangible,
    /// then shove the dynamic bodies back with a gentle impulse. A no-op without a fluid. Circle-
    /// approximated (true collider-shape coupling is a refinement). (Physics scenes P4c.)
    fn couple_fluid_to_bodies(&mut self) {
        if self.fluid.is_none() {
            return;
        }
        let mut handles: Vec<RigidBodyHandle> = Vec::new();
        let mut contacts: Vec<FluidContact> = Vec::new();
        for (handle, collider) in self.scene_bodies.values() {
            if let Some(body) = self.bodies.get(*handle) {
                let t = body.translation();
                let radius = match collider {
                    NodeCollider::Ball { radius } => *radius,
                    NodeCollider::Square { half } => *half,
                    NodeCollider::RoundedSquare { half, .. } => *half,
                    NodeCollider::Hull { fallback, .. } => *fallback,
                };
                handles.push(*handle);
                contacts.push(FluidContact { center: Point2D::new(t.x, t.y), radius });
            }
        }
        if self.nodes_tangible {
            for handle in self.bodies_by_node.values() {
                if let Some(body) = self.bodies.get(*handle) {
                    let t = body.translation();
                    handles.push(*handle);
                    contacts
                        .push(FluidContact { center: Point2D::new(t.x, t.y), radius: NODE_BODY_RADIUS });
                }
            }
        }
        let reactions = self.fluid.as_mut().unwrap().resolve_contacts(&contacts);
        for (handle, reaction) in handles.iter().zip(reactions) {
            if let Some(body) = self.bodies.get_mut(*handle) {
                if body.is_dynamic() {
                    body.apply_impulse(Vector::new(reaction.x, reaction.y) * FLUID_REACTION, true);
                }
            }
        }
    }

    /// Apply the scene force-field (if any) to every dynamic scene body — a whirlpool's tangential
    /// swirl + inward pull. A no-op without a field. (Physics scenes P4 — force-field tier.)
    fn apply_scene_field(&mut self) {
        let Some(SceneField::Vortex { center, strength, inward }) = self.scene_field else {
            return;
        };
        let c = Vector::new(center.0, center.1);
        let handles: Vec<RigidBodyHandle> = self.scene_bodies.values().map(|(h, _)| *h).collect();
        for handle in handles {
            if let Some(body) = self.bodies.get_mut(handle) {
                if !body.is_dynamic() {
                    continue;
                }
                let r = body.translation() - c;
                let d = r.length().max(8.0);
                let rhat = r / d;
                // Counter-clockwise tangential swirl plus an inward pull toward the centre.
                let tangential = Vector::new(-rhat.y, rhat.x);
                let force = tangential * strength - rhat * inward;
                body.add_force(force, true);
            }
        }
    }

    /// Make a single node tangible (it collides with scene bodies) or intangible (it
    /// passes through), by re-masking its collider's filter. Node-node collision is
    /// unaffected. A no-op for an unknown node. (Physics scenes P2 — tangibility lever.)
    pub fn set_node_tangibility(&mut self, node: NodeKey, tangible: bool) {
        let Some(&body_handle) = self.bodies_by_node.get(&node) else {
            return;
        };
        self.remask_node(body_handle, tangible);
    }

    /// Set every node's tangibility at once (the scene-wide lever): `true` lets the graph
    /// collide with scene bodies, `false` (the default) passes through. Node-node collision
    /// is unaffected either way. (Physics scenes P2.)
    pub fn set_nodes_tangible(&mut self, tangible: bool) {
        self.nodes_tangible = tangible;
        let handles: Vec<RigidBodyHandle> = self.bodies_by_node.values().copied().collect();
        for handle in handles {
            self.remask_node(handle, tangible);
        }
    }

    /// Re-mask one node body's collider(s) to the intangible (`NODE`) or tangible
    /// (`NODE | SCENE`) filter. (Physics scenes P2.)
    fn remask_node(&mut self, body_handle: RigidBodyHandle, tangible: bool) {
        let groups = if tangible {
            InteractionGroups::new(NODE_GROUP, NODE_GROUP | SCENE_GROUP, InteractionTestMode::And)
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
        self.gravity = Vector::new(gravity.0, gravity.1);
    }

    /// Load a declarative [`SceneSpec`] into the world: clear any prior scene, set its
    /// gravity, spawn its bodies (capped at [`SCENE_BODY_CAP`]) with their per-body gravity
    /// scale + initial rotation, bind its joints, and apply its default tangibility. (Physics
    /// scenes P3 / P4b.)
    pub fn load_scene(&mut self, spec: &SceneSpec) {
        self.clear_scene();
        self.set_gravity(spec.gravity);
        // Perpetual backdrops coast (near-zero damping); settling scenes use the heavier
        // SCENE_DAMPING so they come to rest. (Physics scenes P4a.)
        let damping = if spec.perpetual { PERPETUAL_SCENE_DAMPING } else { SCENE_DAMPING };
        // Spawn bodies, keeping their handles in spec order so joints reference them by index.
        let mut handles: Vec<RigidBodyHandle> =
            Vec::with_capacity(spec.bodies.len().min(SCENE_BODY_CAP));
        for b in spec.bodies.iter().take(SCENE_BODY_CAP) {
            let builder = match b.body_type {
                SceneBodyType::Fixed => RigidBodyBuilder::fixed(),
                SceneBodyType::Dynamic => RigidBodyBuilder::dynamic(),
            };
            let body = builder
                .translation(Vector::new(b.position.0, b.position.1))
                .rotation(b.rotation)
                .linvel(Vector::new(b.velocity.0, b.velocity.1))
                .linear_damping(damping)
                .angular_damping(DEFAULT_ANGULAR_DAMPING)
                .gravity_scale(b.gravity_scale)
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
            let id = SceneBodyId(self.next_scene_id);
            self.next_scene_id += 1;
            self.scene_bodies.insert(id, (handle, b.collider.clone()));
            handles.push(handle);
        }
        // Bind joints between the spawned bodies (by spec index). Out-of-range or capped-out
        // indices are skipped. rapier reaps these when the bodies are removed (clear_scene),
        // so no separate joint bookkeeping is needed.
        for j in &spec.joints {
            let (Some(&a), Some(&b)) = (handles.get(j.body_a), handles.get(j.body_b)) else {
                continue;
            };
            self.insert_scene_joint(a, b, &j.joint);
        }
        self.set_nodes_tangible(spec.default_tangible);
        self.scene_perpetual = spec.perpetual;
    }

    /// Build one [`SceneJoint`] and insert it into the impulse-joint set between two bodies.
    /// Anchors are body-local points (world units). (Physics scenes P4b.)
    fn insert_scene_joint(&mut self, a: RigidBodyHandle, b: RigidBodyHandle, joint: &SceneJoint) {
        let v = |p: (f32, f32)| Vector::new(p.0, p.1);
        match *joint {
            SceneJoint::Fixed { anchor_a, anchor_b } => {
                let jt = FixedJointBuilder::new()
                    .local_anchor1(v(anchor_a))
                    .local_anchor2(v(anchor_b))
                    .build();
                self.impulse_joints.insert(a, b, jt, true);
            }
            SceneJoint::Revolute { anchor_a, anchor_b, motor } => {
                let mut builder =
                    RevoluteJointBuilder::new().local_anchor1(v(anchor_a)).local_anchor2(v(anchor_b));
                if let Some(m) = motor {
                    builder = builder.motor_velocity(m.target_vel, m.factor);
                }
                self.impulse_joints.insert(a, b, builder.build(), true);
            }
            SceneJoint::Rope { anchor_a, anchor_b, length } => {
                let jt = RopeJointBuilder::new(length)
                    .local_anchor1(v(anchor_a))
                    .local_anchor2(v(anchor_b))
                    .build();
                self.impulse_joints.insert(a, b, jt, true);
            }
            SceneJoint::Spring { anchor_a, anchor_b, rest_length, stiffness, damping } => {
                let jt = SpringJointBuilder::new(rest_length, stiffness, damping)
                    .local_anchor1(v(anchor_a))
                    .local_anchor2(v(anchor_b))
                    .build();
                self.impulse_joints.insert(a, b, jt, true);
            }
        }
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

        // Emitters spawn / reap scene bodies before the step, so fresh droplets are simulated this
        // frame. (Physics scenes — emitters.)
        self.step_emitters(dt);

        // rapier's `add_force` is a *persistent* force (it survives across
        // steps until reset), so clear last tick's force forces before this
        // tick's forces set fresh ones. Without this, per-tick forces compound
        // and the layout goes unstable.
        if !self.forces.is_empty() || !self.coupling_forces.is_empty() || self.scene_field.is_some() {
            for (_, body) in self.bodies.iter_mut() {
                body.reset_forces(false);
            }
            let mut ctx = ForceContext {
                bodies: &mut self.bodies,
                colliders: &self.colliders,
                joints: &mut self.impulse_joints,
                bodies_by_node: &self.bodies_by_node,
                edges: &self.edges,
            };
            for force in &self.forces {
                force.apply(&mut ctx, dt);
            }
            // Field couplings apply after the built-ins, in the same reset window.
            for force in &self.coupling_forces {
                force.apply(&mut ctx, dt);
            }
        }

        // The scene force-field (a whirlpool / well) drives the dynamic scene bodies — applied after
        // the force loops (so the body borrow is free), still before the step. (Physics scenes P4.)
        self.apply_scene_field();

        let physics_hooks = ();
        let event_handler = ();
        self.pipeline.step(
            self.gravity,
            &self.parameters,
            &mut self.islands,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            &physics_hooks,
            &event_handler,
        );

        // Step the liquid pool (if any) after the rigid world; it reads the same dt and folds into
        // the same snapshot. (Physics scenes P4c.)
        if let Some(fluid) = &mut self.fluid {
            fluid.step(dt);
        }
        // Two-way coupling: the pool bounces off the scene bodies + (when the graph is tangible) the
        // node bodies, and shoves the dynamic ones back. (Physics scenes P4c.)
        self.couple_fluid_to_bodies();
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
        body.set_next_kinematic_translation(Vector::new(position.x, position.y));
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

        let min_before = sim.scene_bodies().map(|b| b.position.y).fold(f32::INFINITY, f32::min);
        for _ in 0..60 {
            sim.tick(1.0 / 60.0);
        }
        let min_after = sim.scene_bodies().map(|b| b.position.y).fold(f32::INFINITY, f32::min);
        assert!(min_after > min_before + 5.0, "gravity pulled the falling balls down");

        // The node carries gravity_scale(0), so scene gravity never drags the graph down.
        let np = sim.position_of(node).expect("node has a position");
        assert!(np.y.abs() < 5.0, "nodes float under scene gravity (was {np:?})");
    }

    #[test]
    fn tangible_node_displaces_the_fluid_pool() {
        use crate::{Basin, FluidParams};
        use euclid::default::Vector2D;
        // A node at the origin with a fluid block sitting on it (gravity off, so it stays put).
        let mut sim = Simulation::new();
        let node = NodeKey::new(0);
        sim.sync_nodes([(node, Point2D::new(0.0, 0.0))]);
        let params = FluidParams { gravity: Vector2D::zero(), ..FluidParams::default() };
        let basin = Basin { min_x: -200.0, max_x: 200.0, floor_y: 300.0 };
        sim.load_fluid(params, basin, Point2D::new(-30.0, -30.0), 5, 5, 14.0);

        // Intangible (default): the node does not push the fluid — a particle can sit near the origin.
        for _ in 0..8 {
            sim.tick(1.0 / 60.0);
        }
        let nearest_intangible =
            sim.fluid_particles().map(|p| p.to_vector().length()).fold(f32::INFINITY, f32::min);

        // Tangible: the node now displaces the pool — the nearest particle is pushed out to ~its radius.
        sim.set_nodes_tangible(true);
        for _ in 0..8 {
            sim.tick(1.0 / 60.0);
        }
        let nearest_tangible =
            sim.fluid_particles().map(|p| p.to_vector().length()).fold(f32::INFINITY, f32::min);

        assert!(
            nearest_tangible > nearest_intangible,
            "tangible node pushed the fluid out ({nearest_intangible} -> {nearest_tangible})"
        );
        assert!(
            nearest_tangible >= NODE_BODY_RADIUS - 2.0,
            "nearest particle pushed out to ~the node radius (was {nearest_tangible})"
        );
    }

    #[test]
    fn vortex_field_swirls_scene_bodies() {
        use crate::SceneField;
        // A loose scene ball on the +x axis; a counter-clockwise vortex at the origin.
        let mut sim = Simulation::new();
        sim.add_scene_body(NodeCollider::Ball { radius: 14.0 }, Point2D::new(100.0, 0.0), (0.0, 0.0));
        sim.set_scene_field(Some(SceneField::Vortex {
            center: (0.0, 0.0),
            strength: 90.0,
            inward: 30.0,
        }));
        assert!(sim.wants_continuous_tick(), "a scene field keeps the actor ticking");

        for _ in 0..60 {
            sim.tick(1.0 / 60.0);
        }
        // A CCW vortex pushes a body on the +x axis toward +y.
        let p = sim.scene_bodies().next().map(|b| b.position).expect("the scene body");
        assert!(p.y > 5.0, "CCW vortex swirled the body toward +y (was at +x): {p:?}");

        // Clearing the scene drops the field too.
        sim.clear_scene();
        assert!(!sim.wants_continuous_tick(), "clearing the scene clears the field");
    }

    #[test]
    fn emitter_spawns_then_reaps_to_a_bounded_count() {
        use crate::SceneEmitter;
        let mut sim = Simulation::new();
        sim.add_emitter(SceneEmitter {
            collider: NodeCollider::Ball { radius: 6.0 },
            position: (0.0, 0.0),
            position_jitter: (4.0, 4.0),
            velocity: (0.0, -100.0),
            velocity_jitter: (20.0, 20.0),
            rate_per_sec: 30.0,
            lifetime_secs: 0.5,
            max_alive: 40,
        });
        assert_eq!(sim.emitter_count(), 1);
        assert!(sim.wants_continuous_tick(), "an emitter keeps the actor ticking");

        // After more than one lifetime, spawning (~30/s) and reaping (age > 0.5s) balance near
        // rate*lifetime (~15) — a bounded steady count, not unbounded growth.
        for _ in 0..90 {
            sim.tick(1.0 / 60.0);
        }
        let steady = sim.scene_body_count();
        assert!((6..=40).contains(&steady), "emitter holds a bounded steady count (was {steady})");

        // Clearing the emitters removes the bodies it spawned.
        sim.clear_emitters();
        assert_eq!(sim.scene_body_count(), 0, "clearing emitters removes its bodies");
        assert_eq!(sim.emitter_count(), 0);
    }
}
