// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Degree repulsion: hubs push their surroundings apart.
//!
//! Extra separation from every node scaled by its weight — `ln(degree + 1)`
//! from the edges unless the host supplies its own — within a radius, so a
//! well-connected node clears room for the spokes it will draw in. The
//! donor's `DegreeRepulsion` extra, over any law. The falloff is `1/d`, not
//! `1/d²`: it has to still be felt at spring length (the springs hold spokes
//! at ~170 under [`EdgeSpring`](crate::EdgeSpring)'s default), where an
//! inverse-square push sized safely for close range is nothing.

use std::collections::HashMap;

use rapier2d::prelude::*;

use crate::laws::{degrees, node_positions};
use crate::{Force, ForceContext, NodeKey};

#[derive(Clone, Debug)]
pub struct DegreeRepulsion {
    /// Push at unit distance per unit of weight.
    pub strength: f32,
    /// Beyond this a hub pushes nothing.
    pub radius: f32,
    pub min_distance: f32,
    /// The host's per-node weights; `None` reads `ln(degree + 1)` from the
    /// edges each tick.
    weights: Option<HashMap<NodeKey, f32>>,
}

impl Default for DegreeRepulsion {
    fn default() -> Self {
        Self {
            strength: 40_000.0,
            radius: 400.0,
            min_distance: 8.0,
            weights: None,
        }
    }
}

impl DegreeRepulsion {
    /// Use these weights instead of log degree — a node absent here pushes
    /// nothing.
    pub fn with_weights(mut self, weights: impl IntoIterator<Item = (NodeKey, f32)>) -> Self {
        self.weights = Some(weights.into_iter().collect());
        self
    }
}

impl Force for DegreeRepulsion {
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
        let radius2 = self.radius * self.radius;
        let mut forces = vec![Vector::ZERO; nodes.len()];
        for i in 0..nodes.len() {
            let weight = weights[i];
            if weight <= 0.0 {
                continue;
            }
            for j in 0..nodes.len() {
                if i == j {
                    continue;
                }
                let delta = nodes[j].2 - nodes[i].2;
                if delta.length_squared() > radius2 {
                    continue;
                }
                let dist = delta.length().max(self.min_distance);
                forces[j] += delta / dist * (self.strength * weight / dist);
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

    /// A hub's spokes end further from it with the overlay than without.
    #[test]
    fn a_hub_clears_room() {
        let run = |overlay: bool| {
            let keys: Vec<NodeKey> = (0..6).map(NodeKey::new).collect();
            let mut sim = Simulation::new();
            sim.sync_nodes(keys.iter().enumerate().map(|(i, &k)| {
                let a = i as f32 * 1.2;
                let r = if i > 0 { 60.0 } else { 0.0 };
                (k, Point2D::new(r * a.cos(), r * a.sin()))
            }));
            sim.sync_edges(
                keys[1..]
                    .iter()
                    .map(|&leaf| (keys[0], leaf))
                    .collect::<Vec<_>>(),
            );
            let mut forces: Vec<Box<dyn Force>> = vec![
                Box::new(NodeExclusion::default()),
                Box::new(EdgeSpring::default()),
                Box::new(Boundary::default()),
            ];
            if overlay {
                forces.push(Box::new(DegreeRepulsion::default()));
            }
            sim.set_forces(forces);
            for _ in 0..600 {
                sim.tick(1.0 / 60.0);
            }
            let hub = sim.position_of(keys[0]).unwrap();
            keys[1..]
                .iter()
                .map(|&k| (sim.position_of(k).unwrap() - hub).length())
                .sum::<f32>()
                / 5.0
        };
        let with = run(true);
        let without = run(false);
        assert!(
            with > without * 1.15,
            "spokes with {with:.0} should exceed without {without:.0}"
        );
    }
}
