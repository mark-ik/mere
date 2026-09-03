// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Kinds: particle life over the graph.
//!
//! Each node has a *kind* (the host's choice: relation family, site,
//! facet). A `k × k` matrix says how strongly kind `a` is drawn to or pushed
//! from kind `b` — and it is asymmetric, which is the whole trick: `a` may
//! chase `b` while `b` flees `a`, and out of that come the chasing, sorting
//! and cell-like clustering the particle-life sims show. Within a short core
//! radius every pair repels regardless, so bodies never collapse; beyond the
//! interaction radius nothing acts. Edges play no part — this law reads the
//! graph's kinds, not its topology, which is exactly what makes it a
//! different picture.
//!
//! The matrix is the host's to author; [`ParticleLife::seeded`] draws a
//! deterministic one for a receipt.

use std::collections::HashMap;

use rapier2d::prelude::*;

use crate::{Force, ForceContext, NodeKey};

use super::{Rng, node_positions};

/// Particle life: kinds attract or repel by an asymmetric matrix.
#[derive(Clone, Debug)]
pub struct ParticleLife {
    kind_of: HashMap<NodeKey, u8>,
    kind_count: usize,
    /// Row-major `kind_count × kind_count`: `matrix[a * k + b]` is how kind
    /// `a` responds to kind `b`, in `[-1, 1]`.
    matrix: Vec<f32>,
    /// Interaction radius: beyond it a pair does nothing.
    pub radius: f32,
    /// Fraction of `radius` inside which every pair repels (the core).
    pub core: f32,
    /// Force scale.
    pub strength: f32,
    /// Weak centering, so the kinds sort on the canvas rather than fleeing
    /// off it — the bounded box of the particle-life sims.
    pub gravity: f32,
}

impl ParticleLife {
    /// A law over `kinds` (node → kind index) with an explicit matrix of
    /// `kind_count²` entries in `[-1, 1]`.
    pub fn new(
        kinds: impl IntoIterator<Item = (NodeKey, u8)>,
        kind_count: usize,
        matrix: Vec<f32>,
    ) -> Self {
        let kind_count = kind_count.max(1);
        let mut matrix = matrix;
        matrix.resize(kind_count * kind_count, 0.0);
        Self {
            kind_of: kinds.into_iter().collect(),
            kind_count,
            matrix,
            radius: 220.0,
            core: 0.3,
            strength: 2_500.0,
            gravity: 0.02,
        }
    }

    /// The same, with a deterministic matrix drawn from `seed` — the same
    /// picture every run.
    pub fn seeded(
        kinds: impl IntoIterator<Item = (NodeKey, u8)>,
        kind_count: usize,
        seed: u64,
    ) -> Self {
        let kind_count = kind_count.max(1);
        let mut rng = Rng::new(seed);
        let matrix = (0..kind_count * kind_count).map(|_| rng.signed()).collect();
        Self::new(kinds, kind_count, matrix)
    }

    /// The matrix entry for `a` responding to `b`.
    pub fn rule(&self, a: u8, b: u8) -> f32 {
        let (a, b) = (a as usize % self.kind_count, b as usize % self.kind_count);
        self.matrix[a * self.kind_count + b]
    }

    /// The particle-life response curve over `d / radius`: a repulsive core,
    /// then a tent of the matrix's sign and size peaking mid-range.
    fn response(&self, rule: f32, ratio: f32) -> f32 {
        if ratio < self.core {
            ratio / self.core - 1.0
        } else if ratio < 1.0 {
            rule * (1.0 - (2.0 * ratio - 1.0 - self.core).abs() / (1.0 - self.core))
        } else {
            0.0
        }
    }
}

impl Force for ParticleLife {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        let nodes = node_positions(ctx);
        let kinds: Vec<u8> = nodes
            .iter()
            .map(|(key, _, _)| self.kind_of.get(key).copied().unwrap_or(0))
            .collect();
        let mut forces = vec![Vector::ZERO; nodes.len()];
        for i in 0..nodes.len() {
            for j in 0..nodes.len() {
                if i == j {
                    continue;
                }
                let delta = nodes[j].2 - nodes[i].2;
                let dist = delta.length();
                if dist < 1e-3 || dist >= self.radius {
                    continue;
                }
                // Asymmetric on purpose: i's response to j is i's rule.
                let f = self.response(self.rule(kinds[i], kinds[j]), dist / self.radius);
                forces[i] += delta / dist * (f * self.strength);
            }
            forces[i] -= nodes[i].2 * self.gravity;
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

    /// The law's claim: two kinds that like their own and shun the other end
    /// with intra-kind distances below inter-kind, from an interleaved start.
    #[test]
    fn like_kinds_gather_and_unlike_kinds_part() {
        let keys: Vec<NodeKey> = (0..8).map(NodeKey::new).collect();
        let mut sim = Simulation::new();
        sim.set_linear_damping(2.0);
        sim.sync_nodes(keys.iter().enumerate().map(|(i, &k)| {
            (
                k,
                Point2D::new((i % 4) as f32 * 50.0, (i / 4) as f32 * 50.0),
            )
        }));
        // Kinds alternate along the row, so each starts beside the other kind.
        let kinds = keys.iter().enumerate().map(|(i, &k)| (k, (i % 2) as u8));
        // Same attracts (+0.6), other repels (-0.6), symmetric here for clarity.
        let law = ParticleLife::new(kinds, 2, vec![0.6, -0.6, -0.6, 0.6]);
        sim.set_forces(vec![Box::new(law)]);
        for _ in 0..600 {
            sim.tick(1.0 / 60.0);
        }
        let dist = |a: usize, b: usize| {
            (sim.position_of(keys[a]).unwrap() - sim.position_of(keys[b]).unwrap()).length()
        };
        let mut intra = vec![];
        let mut inter = vec![];
        for a in 0..8 {
            for b in (a + 1)..8 {
                if a % 2 == b % 2 {
                    intra.push(dist(a, b));
                } else {
                    inter.push(dist(a, b));
                }
            }
        }
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        assert!(
            mean(&intra) < mean(&inter),
            "intra-kind {:.1} should be below inter-kind {:.1}",
            mean(&intra),
            mean(&inter)
        );
    }

    #[test]
    fn a_seeded_matrix_is_reproducible() {
        let keys: Vec<NodeKey> = (0..3).map(NodeKey::new).collect();
        let a = ParticleLife::seeded(keys.iter().map(|&k| (k, 0)), 3, 7);
        let b = ParticleLife::seeded(keys.iter().map(|&k| (k, 0)), 3, 7);
        assert_eq!(a.matrix, b.matrix);
        assert!(a.matrix.iter().all(|v| (-1.0..1.0).contains(v)));
    }
}
