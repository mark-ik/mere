// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Continuous body **emitters** (fountains / streams) on [`Simulation`]: spawn scene bodies over
//! time and reap them by age, with a tiny deterministic PRNG jittering each spawn (no `rand` dep,
//! reproducible headless). Split from `lib.rs` to keep the simulation core under the per-file size
//! ceiling. (Physics scenes — emitters.)

use std::collections::VecDeque;

use euclid::default::Point2D;

use crate::{SCENE_BODY_CAP, SceneBodyId, SceneEmitter, Simulation};

/// A live emitter: its spec plus the per-tick spawn accumulator, a deterministic jitter RNG, and the
/// ids+ages of the bodies it has spawned (a FIFO so the oldest reap first). (Physics scenes —
/// emitters.)
pub(crate) struct LiveEmitter {
    spec: SceneEmitter,
    accumulator: f32,
    rng: u32,
    spawned: VecDeque<(SceneBodyId, f32)>,
}

/// xorshift32 step — a tiny deterministic PRNG for emitter jitter (no `rand` dep, reproducible
/// headless). (Physics scenes — emitters.)
fn xorshift(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

/// A deterministic jitter in `[-amount, amount]`.
fn jitter(state: &mut u32, amount: f32) -> f32 {
    if amount == 0.0 {
        return 0.0;
    }
    let unit = xorshift(state) as f32 / u32::MAX as f32; // [0, 1]
    (unit * 2.0 - 1.0) * amount
}

impl Simulation {
    /// Add a continuous body emitter (a fountain / stream). Seeded deterministically. (Emitters.)
    pub fn add_emitter(&mut self, spec: SceneEmitter) {
        let seed = 0x9E3779B9u32 ^ (self.emitters.len() as u32 + 1).wrapping_mul(0x85EBCA6B);
        self.emitters.push(LiveEmitter {
            spec,
            accumulator: 0.0,
            rng: seed | 1,
            spawned: VecDeque::new(),
        });
    }

    /// Remove every emitter and the bodies it spawned. (Emitters.)
    pub fn clear_emitters(&mut self) {
        let emitters = std::mem::take(&mut self.emitters);
        for em in emitters {
            for (id, _) in em.spawned {
                self.remove_scene_body(id);
            }
        }
    }

    /// Number of active emitters. (Emitters.)
    pub fn emitter_count(&self) -> usize {
        self.emitters.len()
    }

    /// Step every emitter: age + reap its bodies past `lifetime_secs` (oldest first), then spawn by
    /// `rate_per_sec`, capped by `max_alive` and the global [`SCENE_BODY_CAP`]. (Emitters.)
    pub(crate) fn step_emitters(&mut self, dt: f32) {
        if self.emitters.is_empty() {
            return;
        }
        let mut emitters = std::mem::take(&mut self.emitters);
        for em in &mut emitters {
            for entry in em.spawned.iter_mut() {
                entry.1 += dt;
            }
            while let Some(&(id, age)) = em.spawned.front() {
                if age < em.spec.lifetime_secs {
                    break;
                }
                self.remove_scene_body(id);
                em.spawned.pop_front();
            }
            em.accumulator += em.spec.rate_per_sec * dt;
            while em.accumulator >= 1.0
                && em.spawned.len() < em.spec.max_alive
                && self.scene_body_count() < SCENE_BODY_CAP
            {
                em.accumulator -= 1.0;
                let px = em.spec.position.0 + jitter(&mut em.rng, em.spec.position_jitter.0);
                let py = em.spec.position.1 + jitter(&mut em.rng, em.spec.position_jitter.1);
                let vx = em.spec.velocity.0 + jitter(&mut em.rng, em.spec.velocity_jitter.0);
                let vy = em.spec.velocity.1 + jitter(&mut em.rng, em.spec.velocity_jitter.1);
                let id =
                    self.add_scene_body(em.spec.collider.clone(), Point2D::new(px, py), (vx, vy));
                em.spawned.push_back((id, 0.0));
            }
        }
        self.emitters = emitters;
    }
}
