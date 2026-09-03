// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Anneal: Davidson–Harel simulated annealing.
//!
//! Not a force law at all, and in this catalog on purpose: positions descend
//! an energy — edge lengths near an ideal, nodes apart, everything near the
//! centre — by proposing random moves and accepting the worse ones with a
//! probability that falls as a temperature cools. Early, the layout jumps out
//! of bad configurations; late, it polishes. The result is a still, balanced
//! picture arrived at stochastically, and a seeded run is the same picture
//! twice. It writes positions rather than forces, which the force context
//! permits, and it stops of its own accord when the temperature is spent.

use std::sync::Mutex;

use rapier2d::prelude::*;

use crate::{Force, ForceContext, NodeKey};

use super::{Rng, node_positions};

/// Simulated annealing on a layout energy.
#[derive(Debug)]
pub struct Anneal {
    state: Mutex<(f32, Rng)>,
    /// Starting temperature, in energy units.
    pub initial_temperature: f32,
    /// Multiplier per tick.
    pub cooling: f32,
    /// Below this the law is finished and does nothing.
    pub floor: f32,
    /// The largest proposed move at the initial temperature.
    pub step: f32,
    pub ideal_edge: f32,
    pub repulsion: f32,
    pub gravity: f32,
}

impl Anneal {
    pub fn seeded(seed: u64) -> Self {
        let initial = 4_000.0;
        Self {
            state: Mutex::new((initial, Rng::new(seed))),
            initial_temperature: initial,
            cooling: 0.995,
            floor: 1.0,
            step: 80.0,
            ideal_edge: 120.0,
            repulsion: 500_000.0,
            gravity: 0.01,
        }
    }

    /// The current temperature; `<= floor` once the schedule is spent.
    pub fn temperature(&self) -> f32 {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .0
    }

    /// The energy of a layout: edge-length error squared, plus pairwise
    /// repulsion, plus a gravity term. Public so a test or receipt measures
    /// what the law minimizes.
    pub fn energy(&self, positions: &[(NodeKey, Vector)], edges: &[(NodeKey, NodeKey)]) -> f32 {
        let index: std::collections::HashMap<NodeKey, usize> = positions
            .iter()
            .enumerate()
            .map(|(i, (k, _))| (*k, i))
            .collect();
        let mut energy = 0.0;
        for i in 0..positions.len() {
            for j in (i + 1)..positions.len() {
                let d = (positions[i].1 - positions[j].1).length().max(1.0);
                energy += self.repulsion / d;
            }
            energy += self.gravity * positions[i].1.length_squared();
        }
        for &(a, b) in edges {
            if let (Some(&i), Some(&j)) = (index.get(&a), index.get(&b)) {
                let d = (positions[i].1 - positions[j].1).length();
                let e = d - self.ideal_edge;
                energy += e * e;
            }
        }
        energy
    }

    /// The part of the energy involving node `i` at `candidate`.
    fn local_energy(
        &self,
        i: usize,
        candidate: Vector,
        positions: &[(NodeKey, Vector)],
        neighbours: &[usize],
    ) -> f32 {
        let mut energy = self.gravity * candidate.length_squared();
        for (j, (_, p)) in positions.iter().enumerate() {
            if j == i {
                continue;
            }
            energy += self.repulsion / (candidate - *p).length().max(1.0);
        }
        for &j in neighbours {
            let e = (candidate - positions[j].1).length() - self.ideal_edge;
            energy += e * e;
        }
        energy
    }
}

impl Force for Anneal {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (temperature, rng) = &mut *state;
        if *temperature <= self.floor {
            return;
        }
        let nodes = node_positions(ctx);
        let mut positions: Vec<(NodeKey, Vector)> =
            nodes.iter().map(|(k, _, p)| (*k, *p)).collect();
        let index: std::collections::HashMap<NodeKey, usize> = positions
            .iter()
            .enumerate()
            .map(|(i, (k, _))| (*k, i))
            .collect();
        let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); positions.len()];
        for &(a, b) in ctx.edges {
            if let (Some(&i), Some(&j)) = (index.get(&a), index.get(&b)) {
                if i != j {
                    neighbours[i].push(j);
                    neighbours[j].push(i);
                }
            }
        }
        let scale = self.step * (*temperature / self.initial_temperature).sqrt().max(0.05);
        for i in 0..positions.len() {
            let angle = rng.unit() * std::f32::consts::TAU;
            let candidate =
                positions[i].1 + Vector::new(angle.cos(), angle.sin()) * (scale * rng.unit());
            let before = self.local_energy(i, positions[i].1, &positions, &neighbours[i]);
            let after = self.local_energy(i, candidate, &positions, &neighbours[i]);
            let delta = after - before;
            let accept = delta < 0.0 || rng.unit() < (-delta / *temperature).exp();
            if accept {
                positions[i].1 = candidate;
                if let Some(body) = ctx.bodies.get_mut(nodes[i].1) {
                    body.set_translation(candidate, true);
                    body.set_linvel(Vector::ZERO, true);
                }
            }
        }
        *temperature *= self.cooling;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Simulation;
    use euclid::default::Point2D;

    /// The law's claim: the energy after the schedule is below the energy
    /// before, and the run is reproducible from its seed.
    #[test]
    fn energy_falls_and_the_seed_reproduces() {
        let keys: Vec<NodeKey> = (0..8).map(NodeKey::new).collect();
        let edges: Vec<(NodeKey, NodeKey)> = keys.windows(2).map(|w| (w[0], w[1])).collect();
        let run = |seed: u64| {
            let mut sim = Simulation::new();
            sim.sync_nodes(
                keys.iter()
                    .enumerate()
                    .map(|(i, &k)| (k, Point2D::new(i as f32 * 5.0, 0.0))),
            );
            sim.sync_edges(edges.clone());
            let law = Anneal::seeded(seed);
            let before = law.energy(
                &keys
                    .iter()
                    .map(|&k| {
                        let p = sim.position_of(k).unwrap();
                        (k, Vector::new(p.x, p.y))
                    })
                    .collect::<Vec<_>>(),
                &edges,
            );
            let probe = Anneal::seeded(seed);
            sim.set_forces(vec![Box::new(law)]);
            for _ in 0..1200 {
                sim.tick(1.0 / 60.0);
            }
            let after_positions: Vec<(NodeKey, Vector)> = keys
                .iter()
                .map(|&k| {
                    let p = sim.position_of(k).unwrap();
                    (k, Vector::new(p.x, p.y))
                })
                .collect();
            let after = probe.energy(&after_positions, &edges);
            (before, after, after_positions)
        };
        let (before, after, positions_a) = run(11);
        assert!(
            after < before * 0.5,
            "energy should fall: {before:.0} -> {after:.0}"
        );
        let (_, _, positions_b) = run(11);
        for ((_, a), (_, b)) in positions_a.iter().zip(&positions_b) {
            assert!((a - b).length() < 1e-3, "a seeded run must reproduce");
        }
    }
}
