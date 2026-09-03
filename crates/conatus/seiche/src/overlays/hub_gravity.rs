// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Hub gravity: everything is drawn toward the hubs.
//!
//! Each node is pulled toward every other node in proportion to that node's
//! `ln(degree + 1)` and inversely to distance, so high-degree nodes become
//! attractors and the picture reads hub-and-spoke. The donor's `HubGravity`
//! extra (its constellation preset), over any law.

use rapier2d::prelude::*;

use crate::laws::{degrees, node_positions};
use crate::{Force, ForceContext};

#[derive(Clone, Copy, Debug)]
pub struct HubGravity {
    /// Pull at unit distance per unit of a hub's log degree.
    pub strength: f32,
    pub min_distance: f32,
}

impl Default for HubGravity {
    fn default() -> Self {
        Self {
            strength: 2_000.0,
            min_distance: 30.0,
        }
    }
}

impl Force for HubGravity {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        let nodes = node_positions(ctx);
        let degree = degrees(ctx.edges);
        let weights: Vec<f32> = nodes
            .iter()
            .map(|(key, _, _)| ((degree.get(key).copied().unwrap_or(0) + 1) as f32).ln())
            .collect();
        let mut forces = vec![Vector::ZERO; nodes.len()];
        for i in 0..nodes.len() {
            for j in 0..nodes.len() {
                if i == j || weights[j] <= 0.0 {
                    continue;
                }
                let delta = nodes[j].2 - nodes[i].2;
                let dist = delta.length().max(self.min_distance);
                forces[i] += delta / dist * (self.strength * weights[j] / dist);
            }
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

    /// Unconnected nodes end nearer a hub with the overlay than without.
    #[test]
    fn strays_gather_around_the_hub() {
        let run = |overlay: bool| {
            let keys: Vec<NodeKey> = (0..7).map(NodeKey::new).collect();
            let mut sim = Simulation::new();
            sim.sync_nodes(keys.iter().enumerate().map(|(i, &k)| {
                (
                    k,
                    Point2D::new(
                        i as f32 * 90.0 - 270.0,
                        if i % 2 == 0 { 60.0 } else { -60.0 },
                    ),
                )
            }));
            // Node 3 is the hub of 1, 2, 4, 5; nodes 0 and 6 are strays.
            sim.sync_edges(vec![
                (keys[3], keys[1]),
                (keys[3], keys[2]),
                (keys[3], keys[4]),
                (keys[3], keys[5]),
            ]);
            let mut forces: Vec<Box<dyn Force>> = vec![
                Box::new(NodeExclusion::default()),
                Box::new(EdgeSpring::default()),
                Box::new(Boundary::default()),
            ];
            if overlay {
                forces.push(Box::new(HubGravity::default()));
            }
            sim.set_forces(forces);
            for _ in 0..600 {
                sim.tick(1.0 / 60.0);
            }
            let hub = sim.position_of(keys[3]).unwrap();
            ((sim.position_of(keys[0]).unwrap() - hub).length()
                + (sim.position_of(keys[6]).unwrap() - hub).length())
                / 2.0
        };
        let with = run(true);
        let without = run(false);
        assert!(
            with < without * 0.85,
            "strays with {with:.0} should be nearer than without {without:.0}"
        );
    }
}
