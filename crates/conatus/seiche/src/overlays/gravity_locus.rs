// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Gravity locus: a pull toward a point, still or moving.
//!
//! Every node is drawn toward a locus in proportion to its distance. Still,
//! it is the donor's `GravityLocus` (its liquid preset's gentle centre);
//! given an oscillation it becomes the donor's *tide* — the locus rides a
//! slow sine, and the graph is never fully settled, a living display. The
//! clock is interior state (a `Force` is `&self`) behind a mutex, advanced by
//! the tick's own `dt`, so it is deterministic per tick count and needs no
//! wall clock.

use std::sync::Mutex;

use rapier2d::prelude::*;

use crate::laws::node_positions;
use crate::{Force, ForceContext};

#[derive(Debug)]
pub struct GravityLocus {
    /// The locus at rest.
    pub target: (f32, f32),
    /// Pull per unit of distance from the locus.
    pub strength: f32,
    /// `Some((amplitude, period_secs))`: the locus moves along x on a sine.
    pub oscillation: Option<(f32, f32)>,
    clock: Mutex<f32>,
}

impl GravityLocus {
    pub fn at(target: (f32, f32)) -> Self {
        Self {
            target,
            strength: 0.3,
            oscillation: None,
            clock: Mutex::new(0.0),
        }
    }

    /// The tide: the locus rides a sine of `amplitude` over `period_secs`.
    pub fn tidal(target: (f32, f32), amplitude: f32, period_secs: f32) -> Self {
        Self {
            oscillation: Some((amplitude, period_secs.max(1e-3))),
            ..Self::at(target)
        }
    }

    /// Where the locus is now.
    pub fn locus(&self) -> (f32, f32) {
        let t = *self
            .clock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match self.oscillation {
            Some((amplitude, period)) => (
                self.target.0 + amplitude * (std::f32::consts::TAU * t / period).sin(),
                self.target.1,
            ),
            None => self.target,
        }
    }
}

impl Force for GravityLocus {
    fn apply(&self, ctx: &mut ForceContext<'_>, dt: f32) {
        {
            let mut clock = self
                .clock
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *clock += dt;
        }
        let (lx, ly) = self.locus();
        let locus = Vector::new(lx, ly);
        for (_, handle, position) in node_positions(ctx) {
            if let Some(body) = ctx.bodies.get_mut(handle) {
                body.add_force((locus - position) * self.strength, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeKey, Simulation};
    use euclid::default::Point2D;

    #[test]
    fn a_still_locus_draws_the_graph_in() {
        let key = NodeKey::new(0);
        let mut sim = Simulation::new();
        sim.set_linear_damping(2.0);
        sim.sync_nodes([(key, Point2D::new(400.0, 300.0))]);
        sim.sync_edges(Vec::<(NodeKey, NodeKey)>::new());
        let mut law = GravityLocus::at((0.0, 0.0));
        law.strength = 1.0;
        sim.set_forces(vec![Box::new(law)]);
        for _ in 0..900 {
            sim.tick(1.0 / 60.0);
        }
        let p = sim.position_of(key).unwrap();
        assert!(p.to_vector().length() < 60.0, "drawn in to {p:?}");
    }

    /// The tide keeps the graph moving: the locus at one quarter period and
    /// at three quarters are on opposite sides, and the body follows (a
    /// little behind, the lag of a damped follower).
    #[test]
    fn a_tidal_locus_never_settles() {
        let key = NodeKey::new(0);
        let mut sim = Simulation::new();
        sim.set_linear_damping(2.0);
        sim.sync_nodes([(key, Point2D::new(0.0, 0.0))]);
        sim.sync_edges(Vec::<(NodeKey, NodeKey)>::new());
        let period = 12.0;
        let mut law = GravityLocus::tidal((0.0, 0.0), 200.0, period);
        law.strength = 2.0;
        sim.set_forces(vec![Box::new(law)]);
        let ticks = (period * 60.0) as usize;
        let mut xs = vec![];
        for tick in 0..ticks * 2 {
            sim.tick(1.0 / 60.0);
            if tick == ticks / 4 + ticks || tick == ticks * 3 / 4 + ticks {
                xs.push(sim.position_of(key).unwrap().x);
            }
        }
        assert!(
            xs[0] > 40.0 && xs[1] < -40.0,
            "the body should ride the tide: {xs:?}"
        );
    }
}
