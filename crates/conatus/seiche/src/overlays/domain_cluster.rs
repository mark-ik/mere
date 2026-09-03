// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Domain cluster: nodes drift toward the centroid of their group.
//!
//! The host groups nodes however it likes — by site, facet, relation family
//! — and each node is pulled, weakly, toward its group's centre of mass,
//! recomputed every tick. The donor's `DomainCluster` extra (its groups were
//! eTLD+1), over any law; the pull is a bias the law's own forces still act
//! against.

use std::collections::HashMap;

use rapier2d::prelude::*;

use crate::laws::node_positions;
use crate::{Force, ForceContext, NodeKey};

#[derive(Clone, Debug)]
pub struct DomainCluster {
    group_of: HashMap<NodeKey, u32>,
    /// Pull per unit distance from the group centroid.
    pub strength: f32,
}

impl DomainCluster {
    /// Groups per node; a node absent here belongs to no group and feels nothing.
    pub fn new(groups: impl IntoIterator<Item = (NodeKey, u32)>) -> Self {
        Self {
            group_of: groups.into_iter().collect(),
            strength: 0.4,
        }
    }

    pub fn with_strength(mut self, strength: f32) -> Self {
        self.strength = strength.max(0.0);
        self
    }
}

impl Force for DomainCluster {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        let nodes = node_positions(ctx);
        let mut sums: HashMap<u32, (Vector, f32)> = HashMap::new();
        for (key, _, position) in &nodes {
            if let Some(&group) = self.group_of.get(key) {
                let entry = sums.entry(group).or_insert((Vector::ZERO, 0.0));
                entry.0 += *position;
                entry.1 += 1.0;
            }
        }
        for (key, handle, position) in &nodes {
            let Some(&group) = self.group_of.get(key) else {
                continue;
            };
            let (sum, count) = sums[&group];
            if count < 2.0 {
                continue;
            }
            let centroid = sum / count;
            if let Some(body) = ctx.bodies.get_mut(*handle) {
                body.add_force((centroid - *position) * self.strength, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Boundary, EdgeSpring, NodeExclusion, Simulation};
    use euclid::default::Point2D;

    /// Two interleaved groups end with a smaller spread within each group
    /// than without the overlay.
    #[test]
    fn groups_gather() {
        let run = |overlay: bool| {
            let keys: Vec<NodeKey> = (0..8).map(NodeKey::new).collect();
            let mut sim = Simulation::new();
            sim.sync_nodes(keys.iter().enumerate().map(|(i, &k)| {
                (
                    k,
                    Point2D::new((i % 4) as f32 * 80.0, (i / 4) as f32 * 80.0),
                )
            }));
            sim.sync_edges(Vec::<(NodeKey, NodeKey)>::new());
            let mut forces: Vec<Box<dyn Force>> = vec![
                Box::new(NodeExclusion::default()),
                Box::new(EdgeSpring::default()),
                Box::new(Boundary::default()),
            ];
            if overlay {
                forces.push(Box::new(
                    DomainCluster::new(keys.iter().enumerate().map(|(i, &k)| (k, (i % 2) as u32)))
                        .with_strength(2.0),
                ));
            }
            sim.set_forces(forces);
            for _ in 0..600 {
                sim.tick(1.0 / 60.0);
            }
            let spread = |group: usize| {
                let members: Vec<Vector> = keys
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| i % 2 == group)
                    .map(|(_, &k)| {
                        let p = sim.position_of(k).unwrap();
                        Vector::new(p.x, p.y)
                    })
                    .collect();
                let c = members.iter().fold(Vector::ZERO, |a, p| a + *p) / members.len() as f32;
                members.iter().map(|p| (*p - c).length()).sum::<f32>() / members.len() as f32
            };
            (spread(0) + spread(1)) / 2.0
        };
        let with = run(true);
        let without = run(false);
        assert!(
            with < without * 0.8,
            "group spread with {with:.0} should be below without {without:.0}"
        );
    }
}
