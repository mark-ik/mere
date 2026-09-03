// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Hold: bodies stay where they are.
//!
//! The Still law's one force. It zeroes every node body's velocity each
//! tick, so the picture is the arrangement's (or a hand's) and nothing
//! drifts. An empty force set is not the same thing: rapier's contact
//! solver still runs, and on a tight, overlapping seed it accumulates
//! separating velocity tick after tick until the graph explodes — with
//! nothing to spread the bodies gently first, as every other law's
//! repulsion does. Held, a contact can only nudge, one step at a time.

use rapier2d::prelude::*;

use crate::{Force, ForceContext};

#[derive(Clone, Copy, Debug, Default)]
pub struct Hold;

impl Force for Hold {
    fn apply(&self, ctx: &mut ForceContext<'_>, _dt: f32) {
        for &handle in ctx.bodies_by_node.values() {
            if let Some(body) = ctx.bodies.get_mut(handle) {
                body.set_linvel(Vector::ZERO, false);
                body.set_angvel(0.0, false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeKey, Simulation};
    use euclid::default::Point2D;

    /// A moving body stops on the next tick and stays put.
    #[test]
    fn a_held_body_stops_where_it_is() {
        let key = NodeKey::new(0);
        let mut sim = Simulation::new();
        sim.sync_nodes([(key, Point2D::new(40.0, 0.0))]);
        sim.sync_edges(Vec::<(NodeKey, NodeKey)>::new());
        // Give it a shove first, under no forces.
        sim.set_forces(Vec::new());
        sim.pin(key, Point2D::new(40.0, 0.0));
        sim.unpin(key);
        sim.set_forces(vec![Box::new(Hold)]);
        for _ in 0..30 {
            sim.tick(1.0 / 60.0);
        }
        let p = sim.position_of(key).unwrap();
        assert!((p.x - 40.0).abs() < 1.0 && p.y.abs() < 1.0, "held at {p:?}");
        assert!(sim.kinetic_energy() < 1e-6);
    }

    /// Two bodies seeded on top of each other come apart gently, not
    /// explosively: after a second they are still within a body's diameter
    /// or two, not flung off.
    #[test]
    fn overlapping_held_bodies_do_not_explode() {
        let a = NodeKey::new(0);
        let b = NodeKey::new(1);
        let mut sim = Simulation::new();
        sim.sync_nodes([(a, Point2D::new(0.0, 0.0)), (b, Point2D::new(2.0, 1.0))]);
        sim.sync_edges(Vec::<(NodeKey, NodeKey)>::new());
        sim.set_forces(vec![Box::new(Hold)]);
        for _ in 0..60 {
            sim.tick(1.0 / 60.0);
        }
        let gap = (sim.position_of(a).unwrap() - sim.position_of(b).unwrap()).length();
        assert!(gap < 120.0, "held bodies stay near: {gap:.0}");
        assert!(sim.kinetic_energy() < 1e-6);
    }
}
