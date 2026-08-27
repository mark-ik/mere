// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The force-**field** tier on [`Simulation`]: a continuous field (a whirlpool / well) over the
//! scene's dynamic bodies, plus [`Simulation::wants_continuous_tick`] — the predicate the physics
//! actor reads to keep ticking when the world has self-driving motion even at rest. Split from
//! `lib.rs` to keep the simulation core under the per-file size ceiling. (Physics scenes P4.)

use rapier2d::prelude::*;

use crate::{SceneField, Simulation};

impl Simulation {
    /// Set (or clear) the scene-wide force-field — a whirlpool / well over the dynamic scene bodies.
    /// (Physics scenes P4 — force-field tier.)
    pub fn set_scene_field(&mut self, field: Option<SceneField>) {
        self.scene_field = field;
    }

    /// Whether the world has live, self-driving motion the physics actor must keep ticking for even
    /// at rest: a perpetual backdrop, a fluid pool, a force-field, or a live emitter. (Physics
    /// scenes P4.)
    pub fn wants_continuous_tick(&self) -> bool {
        self.scene_perpetual
            || self.fluid.is_some()
            || self.scene_field.is_some()
            || !self.emitters.is_empty()
    }

    /// Apply the scene force-field (if any) to every dynamic scene body — a whirlpool's tangential
    /// swirl + inward pull. A no-op without a field. (Physics scenes P4 — force-field tier.)
    pub(crate) fn apply_scene_field(&mut self) {
        let Some(SceneField::Vortex {
            center,
            strength,
            inward,
        }) = self.scene_field
        else {
            return;
        };
        let c = Vector::new(center.0, center.1);
        let handles: Vec<RigidBodyHandle> = self.scene_bodies.values().map(|(h, _)| *h).collect();
        for handle in handles {
            if let Some(body) = self.bodies.get_mut(handle) {
                if !body.is_dynamic() {
                    continue;
                }
                let r = body.translation() - c;
                let d = r.length().max(8.0);
                let rhat = r / d;
                // Counter-clockwise tangential swirl plus an inward pull toward the centre.
                let tangential = Vector::new(-rhat.y, rhat.x);
                let force = tangential * strength - rhat * inward;
                body.add_force(force, true);
            }
        }
    }
}
