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
//! mere-kernel) owns topology. Aether owns physical state. The three
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

use euclid::default::Point2D;
use kernel::graph::{Graph, NodeKey};
use rapier2d::prelude::*;

/// Physical radius used for every node body's collider.
///
/// Mirrors `platen::CanvasSceneOptions::default()`'s
/// `default_node_radius` so simulation-driven separation matches
/// the renderer's drawn radius. When per-node radii become a real
/// thing, this constant will get replaced by a per-node lookup.
pub const NODE_BODY_RADIUS: f32 = 18.0;

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

    /// Make the simulation match the graph: spawn a body for every
    /// node that doesn't already have one, remove bodies whose nodes
    /// have vanished. New bodies start at the node's current
    /// projected position with zero velocity.
    ///
    /// Idempotent: safe to call every frame, every graph mutation,
    /// or once at startup — does nothing when the graph and bimap
    /// are already in sync.
    pub fn sync_with_graph(&mut self, graph: &Graph) {
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
                .restitution(0.0)
                .friction(0.0)
                .build();
            self.colliders
                .insert_with_parent(collider, handle, &mut self.bodies);
            self.bodies_by_node.insert(key, handle);
            self.nodes_by_body.insert(handle, key);
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
            }
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

        // Field forces fold into rapier's `add_force` accumulator;
        // step() applies them, integrates, and clears the
        // accumulator for the next frame.
        if !self.fields.is_empty() {
            let mut ctx = FieldContext {
                bodies: &mut self.bodies,
                colliders: &self.colliders,
                joints: &mut self.impulse_joints,
                bodies_by_node: &self.bodies_by_node,
                nodes_by_body: &self.nodes_by_body,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn graph_with_two_nodes() -> Graph {
        let mut g = Graph::new();
        g.add_node_with_id(
            uuid::Uuid::from_u128(1),
            "mere://a".to_string(),
            Point2D::new(0.0, 0.0),
        );
        g.add_node_with_id(
            uuid::Uuid::from_u128(2),
            "mere://b".to_string(),
            Point2D::new(100.0, 0.0),
        );
        g
    }

    #[test]
    fn sync_with_graph_creates_bodies_for_new_nodes() {
        let mut sim = Simulation::new();
        let graph = graph_with_two_nodes();
        sim.sync_with_graph(&graph);
        assert_eq!(sim.body_count(), 2);
        for (key, _) in graph.nodes() {
            assert!(sim.body_for(key).is_some());
        }
    }

    #[test]
    fn sync_is_idempotent() {
        let mut sim = Simulation::new();
        let graph = graph_with_two_nodes();
        sim.sync_with_graph(&graph);
        sim.sync_with_graph(&graph);
        sim.sync_with_graph(&graph);
        assert_eq!(sim.body_count(), 2);
    }

    #[test]
    fn empty_simulation_settles_to_rest() {
        let mut sim = Simulation::new();
        let graph = graph_with_two_nodes();
        sim.sync_with_graph(&graph);
        for _ in 0..120 {
            sim.tick(1.0 / 60.0);
        }
        assert!(sim.is_at_rest(0.01));
    }

    #[test]
    fn write_positions_to_returns_zero_when_nothing_moves() {
        let mut sim = Simulation::new();
        let mut graph = graph_with_two_nodes();
        sim.sync_with_graph(&graph);
        // No forces, no time elapsed — bodies are at the same
        // position as the graph reports.
        let changed = sim.write_positions_to(&mut graph);
        assert_eq!(changed, 0);
    }
}
