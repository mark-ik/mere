/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # aether
//!
//! Rapier-backed body/field simulation for Mere. The medium through
//! which forces propagate.
//!
//! ## Place in the architecture
//!
//! [`kernel::graph::Graph`] is the source of truth for node
//! identity, topology, and the *committed* position used for
//! persistence. Aether holds the rapier world that drives the
//! *projected* position — bodies bound to nodes, forces from
//! pluggable [`Field`] implementors, and a tick that mirrors body
//! translations back to the graph each step.
//!
//! Vello renders whatever the graph currently says. Petgraph (inside
//! kernel) owns topology. Aether owns physical state. The three
//! sit at the same architectural tier — substrate layers consumed by
//! everything above.
//!
//! ## Scope
//!
//! This scaffold lands the substrate without enabling any forces:
//!
//! - [`Simulation`] — owns the rapier world + a [`NodeKey`] ↔
//!   [`RigidBodyHandle`] bimap.
//! - [`Simulation::sync_with_graph`] — add bodies for new nodes,
//!   remove bodies whose nodes have disappeared. Idempotent.
//! - [`Simulation::tick`] — step the world, run each registered
//!   field, then mirror body positions into a caller-provided
//!   buffer (or directly into a [`Graph`]).
//! - [`Field`] trait — fields read the body store + apply forces.
//!   Built-in fields (NodeExclusion, EdgeSpring, Boundary, …) ship
//!   in follow-up slices.
//!
//! Until a `Field` is registered, the world ticks empty — bodies
//! settle to rest under damping alone, which keeps placement
//! deterministic exactly as today.

#![doc(html_root_url = "https://docs.rs/aether/0.0.1")]

use std::collections::HashMap;

use euclid::default::{Box2D, Point2D};
use kernel::graph::{Graph, NodeKey};
use rapier2d::prelude::*;

/// Built-in force fields for the force-directed orrery layout.
pub mod fields;
pub use fields::{Boundary, EdgeSpring, NodeExclusion};

/// Physical radius used for every node body's collider.
///
/// Mirrors `platen::CanvasSceneOptions::default()`'s
/// `default_node_radius` so simulation-driven separation matches
/// the renderer's drawn radius. When per-node radii become a real
/// thing, this constant will get replaced by a per-node lookup.
pub const NODE_BODY_RADIUS: f32 = 18.0;

/// Density for node-body colliders, chosen so each node's mass is ~= 1.0
/// (mass = density * pi * r^2 ~= 0.001 * pi * 18^2 ~= 1.0). The layout forces in
/// [`fields`] are then intuitive accelerations rather than being swamped by the
/// large mass a default density would give an 18px ball.
pub const NODE_BODY_DENSITY: f32 = 0.001;

/// Linear damping applied to every node body. High enough to make
/// the simulation settle without continuous input — without forces
/// the bodies decay to rest in a few hundred ms.
const DEFAULT_LINEAR_DAMPING: f32 = 4.0;

/// Angular damping. Bodies don't visibly rotate in the orrery
/// renderer (circles are isotropic), but rapier still tracks angular
/// velocity; high damping prevents the integrator from accumulating
/// spurious spin.
const DEFAULT_ANGULAR_DAMPING: f32 = 4.0;

/// A pluggable force-applier. Fields read the body store and apply
/// forces / impulses; aether's tick walks every registered field
/// before stepping the world.
///
/// Implementors should be cheap — the tick budget is ~16ms total —
/// and write through `bodies.get_mut(...).add_force(...)` or
/// equivalent. Fields are queried in registration order each tick.
pub trait Field: Send {
    fn apply(&self, ctx: &mut FieldContext<'_>, dt: f32);
}

/// Per-tick view a [`Field`] sees: mutable access to the rapier body
/// store plus the NodeKey ↔ handle bimap so fields can reason about
/// pairs / topology when they need to.
pub struct FieldContext<'a> {
    pub bodies: &'a mut RigidBodySet,
    pub colliders: &'a ColliderSet,
    pub joints: &'a mut ImpulseJointSet,
    pub bodies_by_node: &'a HashMap<NodeKey, RigidBodyHandle>,
    pub nodes_by_body: &'a HashMap<RigidBodyHandle, NodeKey>,
    /// Topology the layout fields pull along (e.g. [`EdgeSpring`]). Node-key
    /// pairs, set via [`Simulation::sync_edges`]; aether stays relation-taxonomy
    /// agnostic, so the caller decides which edge families feed the layout.
    pub edges: &'a [(NodeKey, NodeKey)],
    /// The spatial index, for fields that find neighbors by region instead of
    /// scanning all pairs (e.g. [`NodeExclusion`] cull-based repulsion). Current
    /// as of the last [`Simulation::tick`] step or [`Simulation::sync_with_graph`]
    /// (a tick's worth of staleness is harmless for soft forces).
    pub query_index: &'a QueryPipeline,
}

/// One rapier world + bookkeeping. The host owns one of these per
/// graph; aether stays pure (no global state, no static handles).
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
    fields: Vec<Box<dyn Field>>,
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
            // No global gravity — fields supply directional forces
            // when they want them. A "gravity" field is just another
            // Field impl.
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
            fields: Vec::new(),
        }
    }

    /// Register a field. Fields apply in registration order on every
    /// tick.
    pub fn add_field<F: Field + 'static>(&mut self, field: F) {
        self.fields.push(Box::new(field));
    }

    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Replace the topology the layout fields pull along (see
    /// [`FieldContext::edges`]). Caller-chosen node-key pairs: aether does not
    /// read graph edges itself, so the caller filters to the relation families
    /// that should shape the layout (e.g. semantic edges, not arrangement).
    /// Idempotent; replaces the whole edge list.
    pub fn sync_edges(&mut self, edges: impl IntoIterator<Item = (NodeKey, NodeKey)>) {
        self.edges.clear();
        self.edges.extend(edges);
    }

    /// Number of edges currently feeding the layout fields.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Number of currently-tracked bodies. Useful for traces / tests.
    pub fn body_count(&self) -> usize {
        self.bodies_by_node.len()
    }

    /// Look up the body handle for a node, if any. Field implementors
    /// already get the bimap through [`FieldContext`]; this is for
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
    /// on purpose: aether is a kernel-tier substrate and must not depend on the
    /// projection layer above it. The caller maps `Projection.nodes` to these
    /// pairs (see the cartography-aether layout seam doc).
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
    /// are already in sync.
    pub fn sync_with_graph(&mut self, graph: &Graph) {
        let mut changed = false;

        // 1. Add bodies for new nodes.
        let mut seen = std::collections::HashSet::with_capacity(self.bodies_by_node.len());
        for (key, node) in graph.nodes() {
            seen.insert(key);
            if self.bodies_by_node.contains_key(&key) {
                continue;
            }
            let position = node.projected_position();
            let body = RigidBodyBuilder::dynamic()
                .translation(vector![position.x, position.y])
                .linear_damping(DEFAULT_LINEAR_DAMPING)
                .angular_damping(DEFAULT_ANGULAR_DAMPING)
                .build();
            let handle = self.bodies.insert(body);
            let collider = ColliderBuilder::ball(NODE_BODY_RADIUS)
                .density(NODE_BODY_DENSITY)
                .restitution(0.0)
                .friction(0.0)
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
        // cull-based fields (e.g. NodeExclusion) see the bodies on the very
        // next tick rather than after the first step rebuilds the index.
        if changed {
            self.query_pipeline.update(&self.colliders);
        }
    }

    /// Advance the simulation by `dt` seconds. Walks every registered
    /// [`Field`] first (so forces accumulate), then steps the world.
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
        // steps until reset), so clear last tick's field forces before this
        // tick's fields set fresh ones. Without this, per-tick forces compound
        // and the layout goes unstable.
        if !self.fields.is_empty() {
            for (_, body) in self.bodies.iter_mut() {
                body.reset_forces(false);
            }
            let mut ctx = FieldContext {
                bodies: &mut self.bodies,
                colliders: &self.colliders,
                joints: &mut self.impulse_joints,
                bodies_by_node: &self.bodies_by_node,
                nodes_by_body: &self.nodes_by_body,
                edges: &self.edges,
                query_index: &self.query_pipeline,
            };
            for field in &self.fields {
                field.apply(&mut ctx, dt);
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
    /// holds the mouse, then `unpin` on release so other fields can
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
/// fields they need without capturing the whole `Simulation`. `pub(crate)` so
/// [`fields`] can reuse it for cull-based neighbor queries.
pub(crate) fn collider_to_node(
    colliders: &ColliderSet,
    nodes_by_body: &HashMap<RigidBodyHandle, NodeKey>,
    handle: ColliderHandle,
) -> Option<NodeKey> {
    let body = colliders.get(handle)?.parent()?;
    nodes_by_body.get(&body).copied()
}

#[cfg(test)]
mod tests;
