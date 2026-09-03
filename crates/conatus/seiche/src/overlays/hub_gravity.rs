// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Hub gravity: everything is drawn toward the hubs.
//!
//! Each node is pulled toward every other node in proportion to that node's
//! weight and inversely to distance, so heavy nodes become attractors and the
//! picture reads hub-and-spoke. The weight is `ln(degree + 1)` from the
//! edges unless the host supplies its own (PageRank, say). The donor's
//! `HubGravity` extra (its constellation preset), over any law.

use std::collections::HashMap;

use rapier2d::prelude::*;

use crate::laws::{degrees, node_positions};
use crate::{Force, ForceContext, NodeKey};

#[derive(Clone, Debug)]
pub struct HubGravity {
    /// Pull at unit distance per unit of a hub's weight.
    pub strength: f32,
    pub min_distance: f32,
    /// The host's per-node weights; `None` reads `ln(degree + 1)` from the
    /// edges each tick.
    weights: Option<HashMap<NodeKey, f32>>,
}

impl Default for HubGravity {
    fn default() -> Self {
        Self {
            strength: 2_000.0,
            min_distance: 30.0,
            weights: None,
        }
    }
}

impl HubGravity {
    /// Use these weights instead of log degree — a node absent here weighs
    /// nothing. Sized so the mean is about one, like log degree on a sparse
    /// graph, they pull as the default does.
    pub fn with_weights(mut self, weights: impl IntoIterator<Item = (NodeKey, f32)>) -> Self {
        self.weights = Some(weights.into_iter().collect());
        self
    }
}

impl Force for HubGravity {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        let nodes = node_positions(ctx);
        let weights: Vec<f32> = match &self.weights {
            Some(map) => nodes
                .iter()
                .map(|(key, _, _)| map.get(key).copied().unwrap_or(0.0).max(0.0))
                .collect(),
            None => {
                let degree = degrees(ctx.edges);
                nodes
                    .iter()
                    .map(|(key, _, _)| ((degree.get(key).copied().unwrap_or(0) + 1) as f32).ln())
                    .collect()
            }
        };
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
    use crate::{Boundary, EdgeSpring, NodeExclusion, Simulation};
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

    /// With host weights, the weighted node is the attractor even when it has
    /// no edges at all.
    #[test]
    fn host_weights_name_the_hub() {
        let keys: Vec<NodeKey> = (0..3).map(NodeKey::new).collect();
        let mut sim = Simulation::new();
        sim.set_linear_damping(2.0);
        sim.sync_nodes([
            (keys[0], Point2D::new(-200.0, 0.0)),
            (keys[1], Point2D::new(0.0, 0.0)),
            (keys[2], Point2D::new(200.0, 0.0)),
        ]);
        sim.sync_edges(Vec::<(NodeKey, NodeKey)>::new());
        sim.set_forces(vec![Box::new(
            HubGravity::default().with_weights([(keys[2], 3.0)]),
        )]);
        for _ in 0..300 {
            sim.tick(1.0 / 60.0);
        }
        let far = sim.position_of(keys[0]).unwrap().x;
        assert!(
            far > -190.0,
            "the stray moved toward the weighted node: {far:.0}"
        );
        let hub = sim.position_of(keys[2]).unwrap().x;
        assert!(
            (hub - 200.0).abs() < 1.0,
            "the weighted node itself feels nothing: {hub:.0}"
        );
    }
}
