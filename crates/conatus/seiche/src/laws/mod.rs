// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The physics catalog's laws.
//!
//! A law is a dynamical system over the node bodies — what force each feels,
//! from whom, as a function of what — and different laws produce different
//! layouts because they are different physics, not because one law was
//! tuned. Each law here is one [`Force`](crate::Force); a host installs a law
//! (and any overlays) with [`Simulation::set_forces`](crate::Simulation::set_forces)
//! and switches laws on a live simulation without reseeding.
//!
//! The laws, by what they reveal:
//!
//! - [`StressSpring`] — every pair a spring whose rest length is its graph
//!   distance: paths unroll to their true length, far things are far
//!   (Kamada–Kawai).
//! - [`LinLogForce`] — attraction linear in distance along edges, repulsion
//!   logarithmic between all, degree-weighted: communities as islands, hubs
//!   central (LinLog / ForceAtlas2).
//! - [`Gravity`] — n-body attraction with mass by degree and an orbital kick:
//!   the graph as a solar system, never at rest.
//! - [`ParticleLife`] — each node has a kind; kinds attract or repel by an
//!   asymmetric matrix: sorting, chasing and fleeing, never at rest.
//! - [`Boids`] — separation, alignment, cohesion with edge-neighbours as
//!   flockmates: constellations that move as groups.
//! - [`Kuramoto`] — phase oscillators coupled along edges, drawn on a ring:
//!   communities as phase clusters.
//! - [`MagneticSpring`] — directed edges align to a field: hierarchy and
//!   direction by physics.
//! - [`Anneal`] — stochastic descent on an energy under a cooling schedule.
//!
//! Every law reads positions and topology from the [`ForceContext`](crate::ForceContext);
//! what a law needs beyond that (degree, kind, graph distance, mass) it takes
//! at construction from the host, the way `CouplingForce` snapshots its
//! targets, and the host rebuilds it when topology changes.

mod anneal;
mod boids;
mod gravity;
mod hold;
mod kuramoto;
mod linlog;
mod magnetic;
mod particle_life;
mod stress;

pub use anneal::Anneal;
pub use boids::Boids;
pub use gravity::Gravity;
pub use hold::Hold;
pub use kuramoto::Kuramoto;
pub use linlog::LinLogForce;
pub use magnetic::MagneticSpring;
pub use particle_life::ParticleLife;
pub use stress::{StressSpring, graph_distances};

use rapier2d::prelude::*;

use crate::{ForceContext, NodeKey};

/// Every node body's `(key, handle, position)`, in node-key order — the
/// snapshot every law starts from, taken immutably before any force is
/// added. Sorted, not in `bodies_by_node`'s hash order: a law that draws
/// random numbers per node (Anneal) or seeds by index (Boids, Sync) must
/// visit nodes the same way in every process, or a seeded run is not
/// reproducible.
pub(crate) fn node_positions(ctx: &ForceContext<'_>) -> Vec<(NodeKey, RigidBodyHandle, Vector)> {
    let mut nodes: Vec<(NodeKey, RigidBodyHandle, Vector)> = ctx
        .bodies_by_node
        .iter()
        .filter_map(|(&key, &handle)| {
            ctx.bodies
                .get(handle)
                .map(|b| (key, handle, b.translation()))
        })
        .collect();
    nodes.sort_by_key(|(key, _, _)| key.index());
    nodes
}

/// The degree of every node in the synced edge list (self-edges ignored).
pub(crate) fn degrees(edges: &[(NodeKey, NodeKey)]) -> std::collections::HashMap<NodeKey, u32> {
    let mut degree = std::collections::HashMap::new();
    for &(a, b) in edges {
        if a == b {
            continue;
        }
        *degree.entry(a).or_insert(0) += 1;
        *degree.entry(b).or_insert(0) += 1;
    }
    degree
}

/// A small deterministic generator (SplitMix64) for the laws that need
/// randomness — a seeded law gives the same picture on every run, which is
/// what a receipt needs.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub(crate) fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform in `[-1, 1)`.
    pub(crate) fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}
