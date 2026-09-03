// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Depth gravity: depth from a root drives one axis.
//!
//! Each node is pulled along a direction to the coordinate its depth earns
//! — roots at the top, leaves `depth · spacing` below — while the law keeps
//! doing everything else across the other axis. The donor's `DepthGravity`
//! extra (its sediment preset), over any law. Depths are the host's (a BFS
//! from the roots it chooses); a node without one feels nothing.

use std::collections::HashMap;

use rapier2d::prelude::*;

use crate::laws::node_positions;
use crate::{Force, ForceContext, NodeKey};

#[derive(Clone, Debug)]
pub struct DepthGravity {
    depth_of: HashMap<NodeKey, u32>,
    /// The axis depth runs along (normalized on use); `(0, 1)` is downward.
    pub direction: (f32, f32),
    /// World units per level of depth.
    pub spacing: f32,
    /// Pull per unit of offset from the depth's coordinate.
    pub strength: f32,
}

impl DepthGravity {
    pub fn new(depths: impl IntoIterator<Item = (NodeKey, u32)>) -> Self {
        Self {
            depth_of: depths.into_iter().collect(),
            direction: (0.0, 1.0),
            spacing: 110.0,
            strength: 6.0,
        }
    }
}

impl Force for DepthGravity {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        let axis = {
            let d = Vector::new(self.direction.0, self.direction.1);
            let n = d.length();
            if n < 1e-6 {
                Vector::new(0.0, 1.0)
            } else {
                d / n
            }
        };
        for (key, handle, position) in node_positions(ctx) {
            let Some(&depth) = self.depth_of.get(&key) else {
                continue;
            };
            let along = position.dot(axis);
            let wanted = depth as f32 * self.spacing;
            if let Some(body) = ctx.bodies.get_mut(handle) {
                body.add_force(axis * ((wanted - along) * self.strength), true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Boundary, EdgeSpring, NodeExclusion, Simulation};
    use euclid::default::Point2D;

    /// A small tree laid out flat ends ordered by depth along the axis.
    #[test]
    fn a_tree_reads_top_down() {
        let keys: Vec<NodeKey> = (0..7).map(NodeKey::new).collect();
        let mut sim = Simulation::new();
        sim.sync_nodes(
            keys.iter()
                .enumerate()
                .map(|(i, &k)| (k, Point2D::new(i as f32 * 70.0, 0.0))),
        );
        // 0 -> 1, 2 ; 1 -> 3, 4 ; 2 -> 5, 6
        sim.sync_edges(vec![
            (keys[0], keys[1]),
            (keys[0], keys[2]),
            (keys[1], keys[3]),
            (keys[1], keys[4]),
            (keys[2], keys[5]),
            (keys[2], keys[6]),
        ]);
        let depths = [
            (keys[0], 0),
            (keys[1], 1),
            (keys[2], 1),
            (keys[3], 2),
            (keys[4], 2),
            (keys[5], 2),
            (keys[6], 2),
        ];
        sim.set_forces(vec![
            Box::new(NodeExclusion::default()),
            Box::new(EdgeSpring::default()),
            Box::new(Boundary::default()),
            Box::new(DepthGravity::new(depths)),
        ]);
        for _ in 0..900 {
            sim.tick(1.0 / 60.0);
        }
        let y = |i: usize| sim.position_of(keys[i]).unwrap().y;
        assert!(
            y(0) < y(1) - 40.0 && y(0) < y(2) - 40.0,
            "the root sits above its children"
        );
        for leaf in 3..7 {
            assert!(
                y(leaf) > y(1) + 40.0 || y(leaf) > y(2) + 40.0,
                "leaf {leaf} sits below the middle level"
            );
        }
    }
}
