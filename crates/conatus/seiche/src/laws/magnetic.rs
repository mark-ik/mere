// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Flow: magnetic springs, direction by physics.
//!
//! Sugiyama and Misue's magnetic-spring model: every edge is a spring *and*
//! a compass needle, torqued toward a field direction, so a directed edge
//! `a → b` comes to point the field's way. A chain lines up along the field,
//! a DAG reads top-down (or left-right), and cycles show themselves as edges
//! that cannot all comply. The hierarchy Sugiyama's layered algorithm
//! computes falls out of the dynamics instead, without layers.
//!
//! Direction is the synced edge tuple's order: `(a, b)` is `a → b`. A host
//! whose edges are undirected gets a flow of whichever order it synced.

use rapier2d::prelude::*;

use crate::{Force, ForceContext, NodeKey};

use super::node_positions;

/// Springs torqued toward a field direction.
#[derive(Clone, Copy, Debug)]
pub struct MagneticSpring {
    /// The field direction edges align to (normalized on use).
    pub field: (f32, f32),
    /// Spring stiffness toward `rest_length`.
    pub stiffness: f32,
    pub rest_length: f32,
    /// Torque per unit of misalignment, scaled by edge length.
    pub torque: f32,
    /// Pairwise repulsion so unrelated nodes do not stack along the field.
    pub repulsion: f32,
    pub min_distance: f32,
}

impl Default for MagneticSpring {
    fn default() -> Self {
        Self {
            field: (0.0, 1.0),
            stiffness: 8.0,
            rest_length: 120.0,
            torque: 30.0,
            repulsion: 60_000.0,
            min_distance: 10.0,
        }
    }
}

impl Force for MagneticSpring {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        let nodes = node_positions(ctx);
        let index: std::collections::HashMap<NodeKey, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, (key, _, _))| (*key, i))
            .collect();
        let field = {
            let f = Vector::new(self.field.0, self.field.1);
            let n = f.length();
            if n < 1e-6 {
                Vector::new(0.0, 1.0)
            } else {
                f / n
            }
        };
        let mut forces = vec![Vector::ZERO; nodes.len()];
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let delta = nodes[i].2 - nodes[j].2;
                let dist = delta.length().max(self.min_distance);
                let push = delta / dist * (self.repulsion / (dist * dist));
                forces[i] += push;
                forces[j] -= push;
            }
        }
        for &(a, b) in ctx.edges {
            let (Some(&i), Some(&j)) = (index.get(&a), index.get(&b)) else {
                continue;
            };
            if i == j {
                continue;
            }
            let delta = nodes[j].2 - nodes[i].2;
            let dist = delta.length();
            if dist < 1e-3 {
                // Coincident: nudge b along the field so the needle has a length.
                forces[j] += field * self.stiffness;
                continue;
            }
            let direction = delta / dist;
            // The spring.
            let pull = direction * (self.stiffness * (dist - self.rest_length));
            forces[i] += pull;
            forces[j] -= pull;
            // The needle: rotate the edge toward the field by pushing its ends
            // apart along the misalignment, more the longer the edge.
            let misalignment = field - direction;
            let turn = misalignment * (self.torque * dist.min(self.rest_length * 2.0) * 0.5);
            forces[j] += turn;
            forces[i] -= turn;
        }
        for (i, (_, handle, _)) in nodes.iter().enumerate() {
            if let Some(body) = ctx.bodies.get_mut(*handle) {
                body.add_force(forces[i], true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Simulation;
    use euclid::default::Point2D;

    /// The law's claim: a directed chain laid out sideways ends monotone
    /// along the field.
    #[test]
    fn a_directed_chain_lines_up_with_the_field() {
        let keys: Vec<NodeKey> = (0..4).map(NodeKey::new).collect();
        let mut sim = Simulation::new();
        // Horizontal start, in reverse order, so the field must turn every edge.
        sim.sync_nodes(
            keys.iter()
                .enumerate()
                .map(|(i, &k)| (k, Point2D::new(300.0 - i as f32 * 100.0, 0.0))),
        );
        sim.sync_edges(keys.windows(2).map(|w| (w[0], w[1])).collect::<Vec<_>>());
        sim.set_forces(vec![Box::new(MagneticSpring::default())]);
        for _ in 0..900 {
            sim.tick(1.0 / 60.0);
        }
        let ys: Vec<f32> = keys
            .iter()
            .map(|&k| sim.position_of(k).unwrap().y)
            .collect();
        assert!(
            ys.windows(2).all(|w| w[1] > w[0] + 20.0),
            "the chain should ascend along +y: {ys:?}"
        );
    }
}
