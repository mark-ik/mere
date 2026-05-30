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

use kernel::graph::NodeKey;
use rapier2d::prelude::*;

use crate::{Field, FieldContext};

/// Pairwise repulsion that spreads nodes apart (the force-directed charge).
///
/// Inverse-square falloff with a `min_distance` floor (no singularity at
/// near-zero separation) and a `cutoff` beyond which a pair does not interact
/// (keeps the per-tick cost near-linear in practice for sparse layouts; the
/// raw walk is still O(n^2), a spatial-index cutoff is a later refinement).
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
        // Snapshot (handle, position) for every node body, immutably, before
        // touching forces. Two steps so no borrow of `bodies` overlaps.
        let handles: Vec<RigidBodyHandle> = ctx.bodies_by_node.values().copied().collect();
        let nodes: Vec<(RigidBodyHandle, Vector<Real>)> = handles
            .iter()
            .filter_map(|&h| ctx.bodies.get(h).map(|b| (h, *b.translation())))
            .collect();

        let mut forces = vec![vector![0.0, 0.0]; nodes.len()];
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let delta = nodes[i].1 - nodes[j].1;
                let dist = delta.norm().max(self.min_distance);
                if dist > self.cutoff {
                    continue;
                }
                let push = delta / dist * (self.strength / (dist * dist));
                forces[i] += push;
                forces[j] -= push;
            }
        }

        for (idx, (handle, _)) in nodes.iter().enumerate() {
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
