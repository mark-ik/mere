// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Stress: every pair a spring whose rest length is its graph distance.
//!
//! Kamada–Kawai's energy, as a force: for nodes `i, j` at graph distance
//! `d_ij` hops, a spring of rest length `d_ij · L` and stiffness `k / d_ij²`,
//! so near pairs are held precisely and far pairs only loosely. What it
//! reveals is *global* distance fidelity — a path of `n` edges unrolls to
//! `n · L`, and two nodes far apart in the graph end far apart on the canvas
//! — where an edge spring only knows its own edge.
//!
//! The distance table is the host's to supply (it owns the graph); the
//! kernel-free [`graph_distances`] computes it from node keys and edges for
//! any host that has nothing better. Disconnected pairs have no spring.

use std::collections::{HashMap, VecDeque};

use rapier2d::prelude::*;

use crate::{Force, ForceContext, NodeKey};

use super::node_positions;

/// Pairwise springs at graph-distance rest lengths (Kamada–Kawai).
#[derive(Clone, Debug)]
pub struct StressSpring {
    /// `(a, b, hops)` for every connected pair, each once.
    pairs: Vec<(NodeKey, NodeKey, f32)>,
    /// World units per hop: the length a single edge settles toward.
    pub unit_length: f32,
    /// Stiffness at one hop; a pair at `d` hops gets `stiffness / d²`.
    pub stiffness: f32,
}

impl StressSpring {
    /// Springs from a distance table of `(a, b, hops)`; `hops == 0` and
    /// self-pairs are ignored.
    pub fn from_distances(
        distances: impl IntoIterator<Item = (NodeKey, NodeKey, u32)>,
        unit_length: f32,
    ) -> Self {
        Self::from_weighted_distances(
            distances
                .into_iter()
                .map(|(a, b, hops)| (a, b, hops as f32)),
            unit_length,
        )
    }

    /// Springs from a weighted distance table of `(a, b, distance)` in units
    /// of hops — a host that weights its edges (a pair joined by three
    /// relations at a third of a hop, say) hands the shortest-path lengths in
    /// here. Non-positive distances and self-pairs are ignored.
    pub fn from_weighted_distances(
        distances: impl IntoIterator<Item = (NodeKey, NodeKey, f32)>,
        unit_length: f32,
    ) -> Self {
        Self {
            pairs: distances
                .into_iter()
                .filter(|&(a, b, d)| a != b && d > 0.0 && d.is_finite())
                .collect(),
            unit_length,
            stiffness: 40.0,
        }
    }

    pub fn with_stiffness(mut self, stiffness: f32) -> Self {
        self.stiffness = stiffness.max(0.0);
        self
    }

    /// How many pairs this law holds.
    pub fn pair_count(&self) -> usize {
        self.pairs.len()
    }
}

impl Force for StressSpring {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        let positions: HashMap<NodeKey, (RigidBodyHandle, Vector)> = node_positions(ctx)
            .into_iter()
            .map(|(key, handle, position)| (key, (handle, position)))
            .collect();
        for &(a, b, hops) in &self.pairs {
            let (Some(&(ha, pa)), Some(&(hb, pb))) = (positions.get(&a), positions.get(&b)) else {
                continue;
            };
            let delta = pb - pa;
            let dist = delta.length();
            if dist < 1e-3 {
                continue;
            }
            let rest = hops * self.unit_length;
            let k = self.stiffness / (hops * hops);
            let pull = delta / dist * (k * (dist - rest));
            if let Some(body) = ctx.bodies.get_mut(ha) {
                body.add_force(pull, true);
            }
            if let Some(body) = ctx.bodies.get_mut(hb) {
                body.add_force(-pull, true);
            }
        }
    }
}

/// All-pairs graph distances in hops over an undirected edge list, one BFS
/// per node: `(a, b, hops)` for every connected ordered-once pair. Kernel-free
/// so any host can feed it its own keys.
pub fn graph_distances(
    nodes: impl IntoIterator<Item = NodeKey>,
    edges: &[(NodeKey, NodeKey)],
) -> Vec<(NodeKey, NodeKey, u32)> {
    let nodes: Vec<NodeKey> = nodes.into_iter().collect();
    let mut adjacency: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
    for &(a, b) in edges {
        if a == b {
            continue;
        }
        adjacency.entry(a).or_default().push(b);
        adjacency.entry(b).or_default().push(a);
    }
    let mut out = Vec::new();
    for (index, &start) in nodes.iter().enumerate() {
        let mut seen: HashMap<NodeKey, u32> = HashMap::new();
        seen.insert(start, 0);
        let mut queue = VecDeque::from([start]);
        while let Some(node) = queue.pop_front() {
            let hops = seen[&node];
            for &next in adjacency.get(&node).map(Vec::as_slice).unwrap_or(&[]) {
                if !seen.contains_key(&next) {
                    seen.insert(next, hops + 1);
                    queue.push_back(next);
                }
            }
        }
        // Each pair once: only partners later in `nodes` order.
        for &other in &nodes[index + 1..] {
            if let Some(&hops) = seen.get(&other) {
                out.push((start, other, hops));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Simulation;
    use euclid::default::Point2D;

    fn path(n: usize) -> (Vec<NodeKey>, Vec<(NodeKey, NodeKey)>) {
        let keys: Vec<NodeKey> = (0..n).map(NodeKey::new).collect();
        let edges = keys.windows(2).map(|w| (w[0], w[1])).collect();
        (keys, edges)
    }

    #[test]
    fn distances_count_hops_along_a_path() {
        let (keys, edges) = path(4);
        let table = graph_distances(keys.iter().copied(), &edges);
        assert_eq!(table.len(), 6, "every connected pair once");
        let hops = |a: usize, b: usize| {
            table
                .iter()
                .find(|(x, y, _)| {
                    (*x == keys[a] && *y == keys[b]) || (*x == keys[b] && *y == keys[a])
                })
                .map(|(_, _, h)| *h)
        };
        assert_eq!(hops(0, 3), Some(3));
        assert_eq!(hops(1, 2), Some(1));
    }

    /// The law's claim: a 4-node path settles with its ends `3 · L` apart —
    /// global distance fidelity, which no edge spring alone produces.
    #[test]
    fn a_path_unrolls_to_its_graph_length() {
        let (keys, edges) = path(4);
        let unit = 60.0;
        let mut sim = Simulation::new();
        // Start folded: every node near the origin, so the law must do the work.
        sim.sync_nodes(
            keys.iter()
                .enumerate()
                .map(|(i, &k)| (k, Point2D::new(i as f32 * 3.0, (i % 2) as f32 * 2.0))),
        );
        sim.sync_edges(edges.clone());
        sim.set_forces(vec![Box::new(StressSpring::from_distances(
            graph_distances(keys.iter().copied(), &edges),
            unit,
        ))]);
        for _ in 0..900 {
            sim.tick(1.0 / 60.0);
        }
        let ends = (sim.position_of(keys[0]).unwrap() - sim.position_of(keys[3]).unwrap()).length();
        let adjacent =
            (sim.position_of(keys[1]).unwrap() - sim.position_of(keys[2]).unwrap()).length();
        assert!(
            (ends - 3.0 * unit).abs() < 0.15 * 3.0 * unit,
            "end-to-end {ends:.1} should be near {}",
            3.0 * unit
        );
        assert!(
            (adjacent - unit).abs() < 0.2 * unit,
            "an edge {adjacent:.1} should be near {unit}"
        );
    }
}
