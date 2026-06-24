/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The **fluid** tier on [`Simulation`]: loading / clearing the PBF pool and the two-way coupling
//! between the pool and the rapier bodies (the bowl bounds the pool, props float, a tangible graph
//! stirs it). The solver itself lives in [`crate::fluid`]; this is its seam onto the rigid world.
//! Split from `lib.rs` to keep the simulation core under the per-file size ceiling. (P4c.)

use euclid::default::Point2D;
use rapier2d::prelude::*;

use crate::{Basin, Fluid, FluidContact, FluidParams, NODE_BODY_RADIUS, NodeCollider, Simulation};

/// Scale from a fluid contact's net push-out (world units) to the impulse applied back on a dynamic
/// body — the two-way fluid coupling strength. Gentle, so a tangible node is nudged / bobs on the
/// pool rather than launched. (Physics scenes P4c.)
const FLUID_REACTION: f32 = 0.25;

impl Simulation {
    /// Load a liquid pool into the world: a `cols × rows` block of PBF particles dropped into an
    /// analytic [`Basin`], replacing any prior fluid. The pool settles under its own gravity and
    /// rides the snapshot for rendering. (Physics scenes P4c.)
    pub fn load_fluid(
        &mut self,
        params: FluidParams,
        basin: Basin,
        spawn_origin: Point2D<f32>,
        cols: usize,
        rows: usize,
        spacing: f32,
    ) {
        let mut fluid = Fluid::new(params, basin);
        fluid.spawn_block(spawn_origin, cols, rows, spacing);
        self.fluid = Some(fluid);
    }

    /// Remove the liquid pool. (Physics scenes P4c.)
    pub fn clear_fluid(&mut self) {
        self.fluid = None;
    }

    /// Whether a liquid pool is loaded. The physics actor keeps ticking while one is, so the pool
    /// keeps flowing / settling. (Physics scenes P4c.)
    pub fn has_fluid(&self) -> bool {
        self.fluid.is_some()
    }

    /// Iterate the live fluid particle positions (empty when no pool is loaded) — the render read.
    /// (Physics scenes P4c.)
    pub fn fluid_particles(&self) -> impl Iterator<Item = Point2D<f32>> + '_ {
        self.fluid.iter().flat_map(|f| f.positions())
    }

    /// Couple the fluid to the rapier bodies: push particles out of every scene body (the bowl
    /// bounds the pool, dynamic props float) and out of node bodies when the graph is tangible,
    /// then shove the dynamic bodies back with a gentle impulse. A no-op without a fluid. Circle-
    /// approximated (true collider-shape coupling is a refinement). (Physics scenes P4c.)
    pub(crate) fn couple_fluid_to_bodies(&mut self) {
        if self.fluid.is_none() {
            return;
        }
        let mut handles: Vec<RigidBodyHandle> = Vec::new();
        let mut contacts: Vec<FluidContact> = Vec::new();
        for (handle, collider) in self.scene_bodies.values() {
            if let Some(body) = self.bodies.get(*handle) {
                let t = body.translation();
                let radius = match collider {
                    NodeCollider::Ball { radius } => *radius,
                    NodeCollider::Square { half } => *half,
                    NodeCollider::RoundedSquare { half, .. } => *half,
                    NodeCollider::Hull { fallback, .. } => *fallback,
                };
                handles.push(*handle);
                contacts.push(FluidContact { center: Point2D::new(t.x, t.y), radius });
            }
        }
        if self.nodes_tangible {
            for handle in self.bodies_by_node.values() {
                if let Some(body) = self.bodies.get(*handle) {
                    let t = body.translation();
                    handles.push(*handle);
                    contacts
                        .push(FluidContact { center: Point2D::new(t.x, t.y), radius: NODE_BODY_RADIUS });
                }
            }
        }
        let reactions = self.fluid.as_mut().unwrap().resolve_contacts(&contacts);
        for (handle, reaction) in handles.iter().zip(reactions) {
            if let Some(body) = self.bodies.get_mut(*handle) {
                if body.is_dynamic() {
                    body.apply_impulse(Vector::new(reaction.x, reaction.y) * FLUID_REACTION, true);
                }
            }
        }
    }
}
