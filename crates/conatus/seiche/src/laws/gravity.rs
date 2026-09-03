// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Orbit: n-body gravitation, the graph as a solar system.
//!
//! Every body attracts every other with `G · m_i · m_j / (d² + ε²)`, mass by
//! degree (a hub is a sun), and on the first tick each body receives a
//! tangential kick about the mass centre so it orbits rather than falls in.
//! Nothing here settles: the law's whole point is motion, and a host that
//! wants it to rest sets the damping high or switches law. What it reveals is
//! hierarchy as gravity — leaves circle their hubs, hubs circle each other.

use std::collections::HashMap;
use std::sync::Mutex;

use rapier2d::prelude::*;

use crate::{Force, ForceContext, NodeKey};

use super::node_positions;

/// N-body attraction with an orbital kick.
#[derive(Debug)]
pub struct Gravity {
    masses: HashMap<NodeKey, f32>,
    /// The gravitational constant, in force per unit mass² at unit distance.
    pub strength: f32,
    /// Softening length: no singularity at close approach.
    pub softening: f32,
    /// Tangential speed given on the first tick, per unit distance from the
    /// mass centre; `0.0` lets the graph simply fall together.
    pub orbital_kick: f32,
    kicked: Mutex<bool>,
}

impl Gravity {
    /// Masses per node; a node absent here weighs `1.0`. A host typically
    /// passes `degree + 1`.
    pub fn new(masses: impl IntoIterator<Item = (NodeKey, f32)>) -> Self {
        Self {
            masses: masses.into_iter().collect(),
            strength: 9_000.0,
            softening: 24.0,
            orbital_kick: 1.0,
            kicked: Mutex::new(false),
        }
    }

    fn mass(&self, key: &NodeKey) -> f32 {
        self.masses.get(key).copied().unwrap_or(1.0).max(0.1)
    }
}

impl Force for Gravity {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        let nodes = node_positions(ctx);
        if nodes.is_empty() {
            return;
        }
        let masses: Vec<f32> = nodes.iter().map(|(key, _, _)| self.mass(key)).collect();
        let total: f32 = masses.iter().sum();
        let centre = nodes
            .iter()
            .zip(&masses)
            .fold(Vector::ZERO, |acc, ((_, _, p), m)| acc + *p * *m)
            / total;

        // The kick, once: perpendicular to the radius vector at the circular
        // orbital speed for the mass inside that radius, `√(G M / r)`, scaled
        // by `orbital_kick` (1.0 is a circle, less an ellipse that falls in).
        let mut kicked = self
            .kicked
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !*kicked && self.orbital_kick > 0.0 {
            // The mass that pulls a body inward is the mass inside its radius
            // (the shell theorem, near enough for a graph): order by radius
            // and accumulate, so an outer leaf is not kicked for a mass that
            // is beside it rather than beneath it.
            let mut by_radius: Vec<(usize, f32)> = nodes
                .iter()
                .enumerate()
                .map(|(i, (_, _, p))| (i, (*p - centre).length()))
                .collect();
            by_radius.sort_by(|a, b| a.1.total_cmp(&b.1));
            let mut enclosed = 0.0;
            for (i, r) in by_radius {
                let (_, handle, position) = &nodes[i];
                if r >= 1e-3 && enclosed > 0.0 {
                    let speed = (self.strength * enclosed / r).sqrt() * self.orbital_kick;
                    let radius = *position - centre;
                    let tangent = Vector::new(-radius.y, radius.x) / r * speed;
                    if let Some(body) = ctx.bodies.get_mut(*handle) {
                        body.set_linvel(tangent, true);
                    }
                }
                enclosed += masses[i];
            }
            *kicked = true;
        }
        drop(kicked);

        // Gravitational mass is the host's map; inertial mass is the body's
        // own, so the acceleration a body feels is `G · m_other / d²` whatever
        // rapier weighed it at — the law, not the collider density, decides
        // who circles whom.
        let soft2 = self.softening * self.softening;
        let mut accelerations = vec![Vector::ZERO; nodes.len()];
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let delta = nodes[j].2 - nodes[i].2;
                let dist2 = delta.length_squared() + soft2;
                let direction = delta / dist2.sqrt();
                accelerations[i] += direction * (self.strength * masses[j] / dist2);
                accelerations[j] -= direction * (self.strength * masses[i] / dist2);
            }
        }
        for (i, (_, handle, _)) in nodes.iter().enumerate() {
            if let Some(body) = ctx.bodies.get_mut(*handle) {
                let inertial = body.mass().max(1e-3);
                body.add_force(accelerations[i] * inertial, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Simulation;
    use euclid::default::Point2D;

    /// The law's claim: a hub with leaves keeps moving — kinetic energy stays
    /// above a floor after 600 ticks — where Springs would have come to rest.
    #[test]
    fn a_solar_system_never_rests() {
        let keys: Vec<NodeKey> = (0..7).map(NodeKey::new).collect();
        let mut sim = Simulation::new();
        sim.set_linear_damping(0.0);
        sim.sync_nodes(keys.iter().enumerate().map(|(i, &k)| {
            let angle = i as f32 * 0.9;
            let r = if i == 0 { 0.0 } else { 120.0 + 20.0 * i as f32 };
            (k, Point2D::new(r * angle.cos(), r * angle.sin()))
        }));
        sim.sync_edges(
            keys[1..]
                .iter()
                .map(|&leaf| (keys[0], leaf))
                .collect::<Vec<_>>(),
        );
        let masses = keys
            .iter()
            .enumerate()
            .map(|(i, &k)| (k, if i == 0 { 12.0 } else { 1.0 }));
        sim.set_forces(vec![Box::new(Gravity::new(masses))]);
        let mut energies = Vec::new();
        for tick in 0..600 {
            sim.tick(1.0 / 60.0);
            if tick % 100 == 99 {
                energies.push(sim.kinetic_energy());
            }
        }
        assert!(
            energies.iter().all(|&e| e > 1.0),
            "kinetic energy should stay above the floor throughout: {energies:?}"
        );
        // And nothing flew off: every leaf is still bound — within a few
        // starting radii of the hub after ten seconds of orbit.
        let hub = sim.position_of(keys[0]).unwrap();
        for &leaf in &keys[1..] {
            let r = (sim.position_of(leaf).unwrap() - hub).length();
            assert!(r < 800.0, "a leaf escaped to {r:.0}");
        }
    }
}
