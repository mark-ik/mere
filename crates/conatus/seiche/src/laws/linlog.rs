// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Energy: the LinLog / ForceAtlas2 law.
//!
//! Noack's LinLog energy model attracts along edges *linearly* in distance
//! and repels between all pairs *logarithmically* — as forces, attraction
//! `a · d` and repulsion `r / d`. The balance makes the settled distance
//! between two groups proportional to how few edges join them, which is why
//! communities come out as separated islands where a spring-electrical
//! layout smears them. ForceAtlas2 adds degree weighting to the repulsion,
//! `(deg_i + 1)(deg_j + 1)`, so hubs push everything away and sit central.
//! Both are here: `degree_weighted` on is ForceAtlas2's reading, off is
//! LinLog's.

use rapier2d::prelude::*;

use crate::{Force, ForceContext};

use super::{degrees, node_positions};

/// LinLog attraction and repulsion, optionally degree-weighted (ForceAtlas2).
#[derive(Clone, Copy, Debug)]
pub struct LinLogForce {
    /// Attraction per unit distance along an edge.
    pub attraction: f32,
    /// Repulsion at unit distance between any two bodies.
    pub repulsion: f32,
    /// Weight repulsion by `(deg_i + 1)(deg_j + 1)` — ForceAtlas2's hub term.
    pub degree_weighted: bool,
    /// Distance floor for the repulsion.
    pub min_distance: f32,
    /// Weak centering so disconnected pieces stay bounded.
    pub gravity: f32,
}

impl Default for LinLogForce {
    fn default() -> Self {
        Self {
            attraction: 4.0,
            repulsion: 60_000.0,
            degree_weighted: true,
            min_distance: 10.0,
            gravity: 0.02,
        }
    }
}

impl Force for LinLogForce {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        let nodes = node_positions(ctx);
        let degree = degrees(ctx.edges);
        let mut forces = vec![Vector::ZERO; nodes.len()];
        let index: std::collections::HashMap<_, _> = nodes
            .iter()
            .enumerate()
            .map(|(i, (key, _, _))| (*key, i))
            .collect();
        // Repulsion between all pairs: r · w_i · w_j / d, along the separation.
        for i in 0..nodes.len() {
            let w_i = if self.degree_weighted {
                (degree.get(&nodes[i].0).copied().unwrap_or(0) + 1) as f32
            } else {
                1.0
            };
            for j in (i + 1)..nodes.len() {
                let w_j = if self.degree_weighted {
                    (degree.get(&nodes[j].0).copied().unwrap_or(0) + 1) as f32
                } else {
                    1.0
                };
                let delta = nodes[i].2 - nodes[j].2;
                let dist = delta.length().max(self.min_distance);
                let push = delta / dist * (self.repulsion * w_i * w_j / dist);
                forces[i] += push;
                forces[j] -= push;
            }
            // Gravity toward the origin, proportional to distance and degree.
            forces[i] -= nodes[i].2 * (self.gravity * w_i);
        }
        // Attraction along edges, linear in distance.
        for &(a, b) in ctx.edges {
            let (Some(&i), Some(&j)) = (index.get(&a), index.get(&b)) else {
                continue;
            };
            if i == j {
                continue;
            }
            let delta = nodes[j].2 - nodes[i].2;
            let pull = delta * self.attraction;
            forces[i] += pull;
            forces[j] -= pull;
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
    use crate::{Boundary, EdgeSpring, NodeExclusion, NodeKey, Simulation};
    use euclid::default::Point2D;

    /// Two triangles joined by a single edge, seeded interleaved so the law
    /// has to separate them.
    fn two_cliques() -> (Vec<NodeKey>, Vec<(NodeKey, NodeKey)>) {
        let keys: Vec<NodeKey> = (0..6).map(NodeKey::new).collect();
        let mut edges = vec![];
        for group in [&keys[0..3], &keys[3..6]] {
            edges.push((group[0], group[1]));
            edges.push((group[1], group[2]));
            edges.push((group[2], group[0]));
        }
        edges.push((keys[2], keys[3]));
        (keys, edges)
    }

    fn centroid(sim: &Simulation, keys: &[NodeKey]) -> Vector {
        let sum = keys
            .iter()
            .filter_map(|k| sim.position_of(*k))
            .fold(Vector::ZERO, |acc, p| acc + Vector::new(p.x, p.y));
        sum / keys.len() as f32
    }

    fn settle(forces: Vec<Box<dyn Force>>) -> f32 {
        let (keys, edges) = two_cliques();
        let mut sim = Simulation::new();
        sim.sync_nodes(keys.iter().enumerate().map(|(i, &k)| {
            (
                k,
                Point2D::new((i % 2) as f32 * 40.0, (i / 2) as f32 * 40.0),
            )
        }));
        sim.sync_edges(edges);
        sim.set_forces(forces);
        for _ in 0..900 {
            sim.tick(1.0 / 60.0);
        }
        let gap = centroid(&sim, &keys[0..3]) - centroid(&sim, &keys[3..6]);
        let spread = keys[0..3]
            .iter()
            .filter_map(|k| sim.position_of(*k))
            .map(|p| (Vector::new(p.x, p.y) - centroid(&sim, &keys[0..3])).length())
            .fold(0.0, f32::max);
        gap.length() / spread.max(1.0)
    }

    /// The law's claim: two cliques joined by one edge separate further,
    /// relative to their own size, under Energy than under Springs.
    #[test]
    fn communities_separate_further_than_under_springs() {
        let energy = settle(vec![Box::new(LinLogForce::default())]);
        let springs = settle(vec![
            Box::new(NodeExclusion::default()),
            Box::new(EdgeSpring::default()),
            Box::new(Boundary::default()),
        ]);
        assert!(
            energy > springs * 1.3,
            "island separation ratio under Energy {energy:.2} should exceed Springs {springs:.2}"
        );
    }
}
