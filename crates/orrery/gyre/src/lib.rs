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
//!
//! ## Decomposition
//!
//! The `Simulation` impl is split across files by concern so no one stays over
//! the per-file size ceiling: the struct + `new` + force registration + read
//! model + the [`tick`](Simulation::tick) heartbeat live here; the node-body
//! shape/material axis in [`node_body`], body lifecycle in [`sync`], the rigid
//! scene tier in [`scene_sim`], the fluid seam in [`fluid_coupling`], the
//! force-field tier in [`field`], and emitters in [`emitter`]. (Rust lets an
//! `impl` span modules; private fields stay reachable from these child modules.)

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

/// The pairwise **affinity** force: a weighted, attract-only spring over a
/// `(a, b, weight)` signal that clusters structurally-similar nodes ("cluster by
/// affinity" on force-directed). (Graph signals — P4.)
pub mod affinity_force;
pub use affinity_force::{
    AffinitySpring, DEFAULT_AFFINITY_REST_LENGTH, DEFAULT_AFFINITY_STIFFNESS,
};

/// Position-Based Fluids (PBF): our own small SPH liquid for the orrery (salva lags rapier badly,
/// so we roll our own). The solver lives here; its seam onto the rigid world (loading + the two-way
/// coupling) is [`Simulation`]'s fluid tier in [`fluid_coupling`]. (Physics scenes P4c.)
pub mod fluid;
pub use fluid::{Basin, ContactShape, Fluid, FluidContact, FluidParams};

/// The declarative scene library: ready-made [`SceneSpec`]s the orrery loads behind the
/// graph (drop-bowl, pyramid, dominoes, Galton board, funnel, drift, chain, whirlpool, fountain).
/// Data, not engine code; the growing catalog lives here to keep `lib.rs` under the size ceiling.
pub mod scenes;
pub use scenes::{
    ball_and_chain_scene, bridge_scene, chain_scene, cradle_scene, domino_scene, drift_scene,
    drop_bowl_scene, fountain_scene, funnel_scene, galton_scene, mixer_scene, pyramid_scene,
    whirlpool_scene,
};

/// The declarative scene **format**: the data types a [`SceneSpec`] is built from (bodies,
/// joints, world settings). Split from `lib.rs` because the format grows independently of the
/// simulation core. (Physics scenes P4b.)
mod scene_spec;
pub use scene_spec::{
    JointMotorSpec, SceneBodyId, SceneBodySpec, SceneBodyType, SceneEmitter, SceneField,
    SceneJoint, SceneJointSpec, SceneSpec,
};

/// The node-body axis: a node's collider **shape** ([`NodeCollider`]) and physical **material**
/// ([`NodeMaterial`]), and the [`Simulation`] methods that re-apply each to live bodies. (Node-rep.)
mod node_body;
pub use node_body::{NodeCollider, NodeMaterial};

/// Node-body lifecycle on [`Simulation`]: reconciling the body set with the graph (`sync_*`),
/// the spring-edge topology, and the body accessors the read model is built from.
mod sync;

/// The rigid **scene** tier on [`Simulation`]: scene-decoration bodies, declarative [`SceneSpec`]
/// loading + joints, world gravity, and the per-node tangibility lever.
mod scene_sim;

/// The **fluid** tier on [`Simulation`]: loading the PBF pool and the two-way coupling between
/// pool and rapier bodies (the solver itself is [`fluid`]).
mod fluid_coupling;

/// The force-**field** tier on [`Simulation`] (a whirlpool / well) plus the actor's keep-ticking
/// predicate [`Simulation::wants_continuous_tick`].
mod field;

/// Continuous body **emitters** (fountains / streams) on [`Simulation`].
mod emitter;
use emitter::LiveEmitter;

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

/// Upper bound on bodies a single [`SceneSpec`] loads (and a live emitter spawns), so a runaway
/// scene can't swamp the physics actor's per-tick budget. (Physics scenes P3.)
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
    InteractionGroups::new(
        SCENE_GROUP,
        SCENE_GROUP | NODE_GROUP,
        InteractionTestMode::And,
    )
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
    /// The pairwise **affinity** force, if installed: a weighted attract-only spring that clusters
    /// structurally-similar nodes ("cluster by affinity"). Like the couplings it is a snapshot —
    /// the host rebuilds it wholesale via [`set_affinity_force`](Self::set_affinity_force) when the
    /// affinity signal recomputes — and it applies in the same reset window as the built-ins.
    /// `None` = off (the default). (Graph signals — P4.)
    affinity_force: Option<AffinitySpring>,
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
    /// Sparse sprite handles for scene bodies that wear one (a textured crate / barrel), keyed by
    /// [`SceneBodyId`]. An opaque host key gyre never interprets — it rides through to the body's
    /// [`SceneBodyView`] for the paint, like the collider shape. Most props have none. (Scene-prop
    /// sprites.)
    scene_sprites: HashMap<SceneBodyId, String>,
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
            affinity_force: None,
            linear_damping: DEFAULT_LINEAR_DAMPING,
            scene_bodies: HashMap::new(),
            scene_sprites: HashMap::new(),
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

    /// Install (or clear, with `None`) the pairwise **affinity** force wholesale — the rebuild a
    /// fresh affinity signal triggers. Position-preserving (forces never touch body state), so
    /// swapping it leaves the layout exactly where it is; the host follows with a `settle` to let
    /// the new equilibrium take. (Graph signals — P4.)
    pub fn set_affinity_force(&mut self, force: Option<AffinitySpring>) {
        self.affinity_force = force;
    }

    /// Number of affinity pairs the installed affinity force pulls along (`0` when none is set).
    pub fn affinity_pair_count(&self) -> usize {
        self.affinity_force
            .as_ref()
            .map_or(0, AffinitySpring::pair_count)
    }

    /// A rapier-free [`LayoutView`] over the current layout: the live positions,
    /// the spring edge topology, and the node radius the picks/cull use. The host
    /// reads node picks, edge picks, and cull from this rather than the rapier
    /// query index, so those reads can run on the UI thread while the simulation
    /// itself ticks elsewhere (the always-offload physics actor, P6).
    pub fn view(&self) -> LayoutView {
        LayoutView::from_parts(
            self.positions(),
            self.edges.iter().copied(),
            NODE_BODY_RADIUS,
        )
    }

    /// A `Send` [`LayoutSnapshot`] of the current positions, stamped with
    /// `generation` — the payload a physics actor emits each tick for the host to
    /// fold into its [`LayoutView`]. Positions only; edges stay with the host's
    /// graph, so they need not cross the actor boundary every frame.
    pub fn snapshot(&self, generation: u64) -> LayoutSnapshot {
        LayoutSnapshot {
            positions: self.positions().collect(),
            scene: self.scene_bodies().collect(),
            fluid: self
                .fluid
                .as_ref()
                .map(|f| f.positions().collect())
                .unwrap_or_default(),
            fluid_radius: self
                .fluid
                .as_ref()
                .map(|f| f.params().particle_radius)
                .unwrap_or(0.0),
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
                Box2D::new(
                    Point2D::new(p.x - r, p.y - r),
                    Point2D::new(p.x + r, p.y + r),
                )
                .intersects(&region)
            })
            .map(|(node, _)| node)
            .collect()
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
        if !self.forces.is_empty()
            || !self.coupling_forces.is_empty()
            || self.affinity_force.is_some()
            || self.scene_field.is_some()
        {
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
            // The affinity force (if installed) applies last, over the same context — a weighted
            // clustering pull on top of the topology springs. (Graph signals — P4.)
            if let Some(force) = &self.affinity_force {
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

/// Integration tests for the scene / fluid / field / emitter tiers (split alongside them).
#[cfg(test)]
mod scene_tests;
