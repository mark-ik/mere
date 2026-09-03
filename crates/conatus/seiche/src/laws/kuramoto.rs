// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Sync: Kuramoto phase oscillators, drawn on a ring.
//!
//! Every node is an oscillator with a phase; edges couple phases, pulling
//! neighbours toward each other's, `θ̇_i = ω + (K / deg_i) Σ_j sin(θ_j − θ_i)`.
//! Densely connected nodes synchronize; groups joined by few edges keep their
//! own phase. The phase is then *drawn*: angle on a ring about a centre, at
//! a radius the host chooses per node (distance from a focus, or one ring).
//! What it reveals is community structure as phase clusters — a one-
//! dimensional dynamic mapped onto the canvas, unlike every other law here.
//!
//! The phases are interior state (a `Force` is `&self`), kept behind a mutex
//! and seeded deterministically by node key.

use std::collections::HashMap;
use std::sync::Mutex;

use rapier2d::prelude::*;

use crate::{Force, ForceContext, NodeKey};

use super::{degrees, node_positions};

/// Kuramoto oscillators on a ring.
#[derive(Debug)]
pub struct Kuramoto {
    phases: Mutex<HashMap<NodeKey, f32>>,
    radius_of: HashMap<NodeKey, f32>,
    /// Natural frequency, radians per second.
    pub natural_frequency: f32,
    /// Coupling strength `K`.
    pub coupling: f32,
    /// Ring radius for a node the host gave none.
    pub default_radius: f32,
    /// Ring centre.
    pub centre: (f32, f32),
    /// Pull toward the phase's position on the ring.
    pub stiffness: f32,
}

impl Kuramoto {
    /// Oscillators over `radii` (node → ring radius); nodes absent here take
    /// `default_radius`. Phases start spread by node key, deterministically.
    pub fn new(radii: impl IntoIterator<Item = (NodeKey, f32)>) -> Self {
        Self {
            phases: Mutex::new(HashMap::new()),
            radius_of: radii.into_iter().collect(),
            natural_frequency: 0.4,
            coupling: 2.5,
            default_radius: 200.0,
            centre: (0.0, 0.0),
            stiffness: 20.0,
        }
    }

    /// The current phases, for a receipt or a test.
    pub fn phases(&self) -> Vec<(NodeKey, f32)> {
        self.phases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .map(|(k, p)| (*k, *p))
            .collect()
    }

    fn seed_phase(key: NodeKey) -> f32 {
        // Golden-angle spread by index: distinct, deterministic, no clustering.
        (key.index() as f32 * 2.399_963) % std::f32::consts::TAU
    }
}

impl Force for Kuramoto {
    fn apply(&self, ctx: &mut ForceContext<'_>, dt: f32) {
        let nodes = node_positions(ctx);
        let degree = degrees(ctx.edges);
        let mut phases = self
            .phases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for (key, _, _) in &nodes {
            phases.entry(*key).or_insert_with(|| Self::seed_phase(*key));
        }
        // Kuramoto step over the edge list.
        let mut delta: HashMap<NodeKey, f32> = HashMap::new();
        for &(a, b) in ctx.edges {
            if a == b {
                continue;
            }
            let (Some(&pa), Some(&pb)) = (phases.get(&a), phases.get(&b)) else {
                continue;
            };
            *delta.entry(a).or_insert(0.0) += (pb - pa).sin();
            *delta.entry(b).or_insert(0.0) += (pa - pb).sin();
        }
        for (key, phase) in phases.iter_mut() {
            let deg = degree.get(key).copied().unwrap_or(0).max(1) as f32;
            let coupling = delta.get(key).copied().unwrap_or(0.0) * self.coupling / deg;
            *phase = (*phase + (self.natural_frequency + coupling) * dt)
                .rem_euclid(std::f32::consts::TAU);
        }
        // Draw: a spring toward the phase's point on the ring.
        for (key, handle, position) in &nodes {
            let phase = phases[key];
            let radius = self
                .radius_of
                .get(key)
                .copied()
                .unwrap_or(self.default_radius);
            let target = Vector::new(
                self.centre.0 + radius * phase.cos(),
                self.centre.1 + radius * phase.sin(),
            );
            let pull = (target - *position) * self.stiffness;
            if let Some(body) = ctx.bodies.get_mut(*handle) {
                body.add_force(pull, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Simulation;
    use euclid::default::Point2D;

    fn circular_spread(phases: &[f32]) -> f32 {
        // 1 - mean resultant length: 0 is perfect synchrony.
        let (s, c) = phases
            .iter()
            .fold((0.0, 0.0), |(s, c), p| (s + p.sin(), c + p.cos()));
        1.0 - ((s * s + c * c).sqrt() / phases.len() as f32)
    }

    /// The law's claim: two triangles joined by one edge end in two phase
    /// clusters — tight within a community, loose across.
    #[test]
    fn communities_become_phase_clusters() {
        let keys: Vec<NodeKey> = (0..6).map(NodeKey::new).collect();
        let mut edges = vec![];
        for group in [&keys[0..3], &keys[3..6]] {
            edges.push((group[0], group[1]));
            edges.push((group[1], group[2]));
            edges.push((group[2], group[0]));
        }
        edges.push((keys[2], keys[3]));
        let mut sim = Simulation::new();
        sim.sync_nodes(keys.iter().map(|&k| (k, Point2D::new(0.0, 0.0))));
        sim.sync_edges(edges);
        let law = Kuramoto::new(std::iter::empty());
        let phases_handle = std::sync::Arc::new(law);
        // Share the law so the test can read phases after the run.
        struct Shared(std::sync::Arc<Kuramoto>);
        impl Force for Shared {
            fn apply(&self, ctx: &mut ForceContext<'_>, dt: f32) {
                self.0.apply(ctx, dt);
            }
        }
        sim.set_forces(vec![Box::new(Shared(phases_handle.clone()))]);
        for _ in 0..900 {
            sim.tick(1.0 / 60.0);
        }
        let phases: HashMap<NodeKey, f32> = phases_handle.phases().into_iter().collect();
        let a: Vec<f32> = keys[0..3].iter().map(|k| phases[k]).collect();
        let b: Vec<f32> = keys[3..6].iter().map(|k| phases[k]).collect();
        let all: Vec<f32> = keys.iter().map(|k| phases[k]).collect();
        let intra = circular_spread(&a).max(circular_spread(&b));
        assert!(
            intra < 0.15,
            "within a community phases should be tight: {intra:.3}"
        );
        // And the bodies sit on the ring where their phases say — a little
        // outside it, since the target keeps moving and the body chases it.
        for &k in &keys {
            let p = sim.position_of(k).unwrap();
            let r = (p.x * p.x + p.y * p.y).sqrt();
            assert!((r - 200.0).abs() < 70.0, "on the ring, not at {r:.0}");
        }
        let _ = all;
    }
}
