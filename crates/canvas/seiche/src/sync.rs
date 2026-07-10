/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Node-body lifecycle on [`Simulation`]: reconciling the rapier body set with the graph
//! (`sync_with_graph` / `sync_nodes`), the spring-edge topology (`sync_edges`), and the body
//! accessors (`positions` / `position_of` / `body_for`) the read model is built from. Split from
//! `lib.rs` to keep the simulation core under the per-file size ceiling.

use std::collections::HashSet;

use euclid::default::Point2D;
use crate::NodeKey;
#[cfg(feature = "kernel-bridge")]
use kernel::graph::Graph;
use rapier2d::prelude::*;

use crate::{
    DEFAULT_ANGULAR_DAMPING, NODE_BODY_DENSITY, NODE_BODY_RADIUS, Simulation, node_groups,
};

impl Simulation {
    /// Replace the topology the layout forces pull along (see
    /// [`ForceContext::edges`](crate::ForceContext::edges)). Caller-chosen node-key pairs: seiche
    /// does not read graph edges itself, so the caller filters to the relation families that
    /// should shape the layout (e.g. semantic edges, not arrangement). Idempotent; replaces the
    /// whole edge list.
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
    /// already get the bimap through [`ForceContext`](crate::ForceContext); this is for
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
    /// on purpose: seiche is a kernel-tier substrate and must not depend on the
    /// projection layer above it. The caller maps `Projection.nodes` to these
    /// pairs (see the cartography-seiche layout seam doc).
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

    /// Make the simulation match the graph: spawn a body for every
    /// node that doesn't already have one, remove bodies whose nodes
    /// have vanished. New bodies start at the node's current
    /// projected position with zero velocity.
    ///
    /// Idempotent: safe to call every frame, every graph mutation,
    /// or once at startup — does nothing when the graph and bimap
    /// are already in sync. A thin convenience over [`Self::sync_nodes`]
    /// that reads each node's projected position from the graph.
    #[cfg(feature = "kernel-bridge")]
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
        let mut seen = HashSet::with_capacity(self.bodies_by_node.len());
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeExclusion;

    /// Physics runs on abstract node keys with no graph at all: seiche's graph-free
    /// surface (`sync_nodes` / `sync_edges` / `position_of` / `positions`) is the
    /// primary interface, so any app (a raw chartulary graph, a bespoke store) drives
    /// the simulation by feeding `(key, position)` pairs and reading positions back,
    /// with no `Graph` in sight. Two nodes nearly on top of each other are pushed apart
    /// by exclusion. (The graph-convenience suite is in `tests.rs`, behind
    /// `kernel-bridge`.)
    #[test]
    fn physics_runs_on_abstract_nodes_without_a_graph() {
        // Keys are minted directly (petgraph indices), not read from any graph.
        let a = NodeKey::new(0);
        let b = NodeKey::new(1);

        let mut sim = Simulation::new();
        sim.add_force(NodeExclusion::default());
        sim.sync_nodes([(a, Point2D::new(0.0, 0.0)), (b, Point2D::new(1.0, 0.0))]);

        let before = (sim.position_of(a).unwrap() - sim.position_of(b).unwrap()).length();
        for _ in 0..120 {
            sim.tick(1.0 / 60.0);
        }
        let after = (sim.position_of(a).unwrap() - sim.position_of(b).unwrap()).length();

        assert!(
            after > before,
            "exclusion pushed the two nodes apart with no graph ({after} > {before})"
        );
        assert_eq!(sim.positions().count(), 2, "positions read back, still no graph");
    }
}
