/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Built-in force fields for the orrery's force-directed layout.
//!
//! Three fields compose into a Fruchterman-Reingold-shaped layout:
//!
//! - [`NodeExclusion`] — every node repels every other, spreading them apart
//!   (the "charge"). This is the long-range push *beyond* the hard ball-collider
//!   contact that already prevents overlap.
//! - [`EdgeSpring`] — connected nodes attract along their edge (Hooke's law
//!   toward a rest length), pulling neighbors together.
//! - [`Boundary`] — a weak centering pull toward the origin so disconnected
//!   pieces stay on screen and the whole layout stays bounded.
//!
//! Each field reads body positions and accumulates forces through
//! `add_force`; [`crate::Simulation::tick`] walks the registered fields before
//! stepping, and rapier's per-body damping settles the result to rest.
//!
//! All three constants are public so the host can tune feel (per the
//! configurability-over-defaults stance); the defaults give a readable layout
//! at Mere's world scale (1 unit ~= 1 px, node radius
//! [`crate::NODE_BODY_RADIUS`]).

use std::collections::HashMap;

use kernel::graph::NodeKey;
use rapier2d::prelude::*;

use crate::{Field, FieldContext, collider_to_node};

/// Pairwise repulsion that spreads nodes apart (the force-directed charge).
///
/// Inverse-square falloff with a `min_distance` floor (no singularity at
/// near-zero separation) and a `cutoff` beyond which a pair does not interact.
/// Neighbors within `cutoff` are found through the spatial index (the
/// `QueryPipeline` rapier maintains for collision), making the per-tick cost
/// O(n * local) rather than an O(n^2) all-pairs scan.
#[derive(Clone, Copy, Debug)]
pub struct NodeExclusion {
    /// Repulsion strength (force at unit distance, before the inverse-square).
    pub strength: f32,
    /// Pairs farther apart than this exert no force.
    pub cutoff: f32,
    /// Distance floor, so coincident nodes do not produce infinite force.
    pub min_distance: f32,
}

impl Default for NodeExclusion {
    fn default() -> Self {
        Self {
            strength: 24_000.0,
            cutoff: 600.0,
            min_distance: 8.0,
        }
    }
}

impl Field for NodeExclusion {
    fn apply(&self, ctx: &mut FieldContext<'_>, _dt: f32) {
        // Snapshot every node's (key, handle, position) and an index by key,
        // immutably, before touching forces.
        let mut nodes: Vec<(NodeKey, RigidBodyHandle, Vector<Real>)> =
            Vec::with_capacity(ctx.bodies_by_node.len());
        for (&key, &handle) in ctx.bodies_by_node.iter() {
            if let Some(body) = ctx.bodies.get(handle) {
                nodes.push((key, handle, *body.translation()));
            }
        }
        let index_of: HashMap<NodeKey, usize> =
            nodes.iter().enumerate().map(|(i, n)| (n.0, i)).collect();

        // For each node, ask the spatial index for the nodes within `cutoff` and
        // accumulate repulsion from just those — O(n * local) instead of the
        // O(n^2) all-pairs scan. Each node's force comes from its own query, so a
        // pair is handled once per side: symmetric, no double application.
        let query = ctx.query_index;
        let colliders = ctx.colliders;
        let nodes_by_body = ctx.nodes_by_body;
        let mut forces = vec![vector![0.0, 0.0]; nodes.len()];
        for i in 0..nodes.len() {
            let pos_i = nodes[i].2;
            let aabb = Aabb::new(
                Point::new(pos_i.x - self.cutoff, pos_i.y - self.cutoff),
                Point::new(pos_i.x + self.cutoff, pos_i.y + self.cutoff),
            );
            let mut force_i = vector![0.0, 0.0];
            query.colliders_with_aabb_intersecting_aabb(&aabb, |handle| {
                if let Some(node_j) = collider_to_node(colliders, nodes_by_body, *handle) {
                    if let Some(&j) = index_of.get(&node_j) {
                        if j != i {
                            let delta = pos_i - nodes[j].2;
                            let dist = delta.norm().max(self.min_distance);
                            if dist <= self.cutoff {
                                force_i += delta / dist * (self.strength / (dist * dist));
                            }
                        }
                    }
                }
                true
            });
            forces[i] = force_i;
        }

        for (idx, (_, handle, _)) in nodes.iter().enumerate() {
            if let Some(body) = ctx.bodies.get_mut(*handle) {
                body.add_force(forces[idx], true);
            }
        }
    }
}

/// Hooke's-law attraction along edges: connected nodes pull toward a rest
/// length. Reads [`FieldContext::edges`], so it pulls along whatever topology
/// the caller synced via [`crate::Simulation::sync_edges`].
#[derive(Clone, Copy, Debug)]
pub struct EdgeSpring {
    /// Spring stiffness (force per unit of stretch beyond the rest length).
    pub stiffness: f32,
    /// The separation the spring settles toward when no other force acts.
    pub rest_length: f32,
}

impl Default for EdgeSpring {
    fn default() -> Self {
        Self {
            stiffness: 12.0,
            rest_length: 140.0,
        }
    }
}

impl Field for EdgeSpring {
    fn apply(&self, ctx: &mut FieldContext<'_>, _dt: f32) {
        for &(a, b) in ctx.edges {
            if a == b {
                continue; // no self-springs
            }
            let (Some(&ha), Some(&hb)) =
                (ctx.bodies_by_node.get(&a), ctx.bodies_by_node.get(&b))
            else {
                continue; // endpoint without a body (stale edge / not yet synced)
            };
            let (Some(pa), Some(pb)) = (
                ctx.bodies.get(ha).map(|x| *x.translation()),
                ctx.bodies.get(hb).map(|x| *x.translation()),
            ) else {
                continue;
            };
            let delta = pb - pa;
            let dist = delta.norm();
            if dist < 1e-3 {
                continue;
            }
            // Positive when stretched past the rest length: pulls a toward b and
            // b toward a; negative (compressed) pushes them apart to rest.
            let pull = delta / dist * (self.stiffness * (dist - self.rest_length));
            if let Some(body) = ctx.bodies.get_mut(ha) {
                body.add_force(pull, true);
            }
            if let Some(body) = ctx.bodies.get_mut(hb) {
                body.add_force(-pull, true);
            }
        }
    }
}

/// Weak centering pull toward the origin, proportional to distance. Keeps
/// disconnected components from drifting off and bounds the whole layout.
#[derive(Clone, Copy, Debug)]
pub struct Boundary {
    /// Centering strength (force per unit of distance from the origin).
    pub strength: f32,
}

impl Default for Boundary {
    fn default() -> Self {
        Self { strength: 1.5 }
    }
}

impl Field for Boundary {
    fn apply(&self, ctx: &mut FieldContext<'_>, _dt: f32) {
        let handles: Vec<RigidBodyHandle> = ctx.bodies_by_node.values().copied().collect();
        for handle in handles {
            let Some(pos) = ctx.bodies.get(handle).map(|b| *b.translation()) else {
                continue;
            };
            if let Some(body) = ctx.bodies.get_mut(handle) {
                body.add_force(-pos * self.strength, true);
            }
        }
    }
}
