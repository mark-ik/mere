/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The node-body axis: a node's collider **shape** ([`NodeCollider`] — the geometry the body
//! collides + hit-tests at) and its physical **material** ([`NodeMaterial`] — restitution /
//! friction / density), plus the [`Simulation`] methods that re-apply each to live bodies. Split
//! from `lib.rs` to keep the simulation core under the per-file size ceiling. (Node-rep.)

use crate::NodeKey;
use rapier2d::prelude::*;

use crate::{NODE_BODY_DENSITY, Simulation};

/// The collider shape the host wants for a node's physics body, so the hard-collision /
/// hit geometry matches the node's *visible* face rather than a uniform ball. The host maps
/// its own form vocabulary (orrery `NodeShape`, a custom sprite hull) onto this; seiche lowers
/// each to a parry shape in [`Simulation::set_node_colliders`]. Sizes/points are in world
/// units (the same space as positions). (Node-rep — collider matches shape.)
#[derive(Clone, Debug, PartialEq)]
pub enum NodeCollider {
    /// A circle of `radius` — the default, and the circle content silhouette.
    Ball { radius: f32 },
    /// An axis-aligned square with half-extent `half` (a document face).
    Square { half: f32 },
    /// A square with rounded corners: total half-extent `half`, corners rounded by `border`
    /// (a menu / rounded face).
    RoundedSquare { half: f32, border: f32 },
    /// A custom convex hull in body-local world units — the sprite's traced outline or a
    /// hand-edited polygon. Falls back to a ball of `fallback` if the hull is degenerate
    /// (fewer than 3 points / collinear). (Node-rep — sprite hull / shape editor.)
    Hull {
        points: Vec<(f32, f32)>,
        fallback: f32,
    },
}

impl NodeCollider {
    /// Lower to the parry shape rapier collides + queries with. Extents are clamped to a
    /// positive minimum so a zero-size node still has a pickable body.
    pub(crate) fn to_shared_shape(&self) -> SharedShape {
        match self {
            NodeCollider::Ball { radius } => SharedShape::ball(radius.max(1.0)),
            NodeCollider::Square { half } => {
                let h = half.max(1.0);
                SharedShape::cuboid(h, h)
            }
            NodeCollider::RoundedSquare { half, border } => {
                let b = border.clamp(0.1, half - 0.1).max(0.1);
                let h = (half - b).max(1.0);
                SharedShape::round_cuboid(h, h, b)
            }
            NodeCollider::Hull { points, fallback } => {
                // parry's `convex_hull` *asserts* `len >= 2` (it panics, not returns `None`) and
                // still yields nothing useful for a collinear set, so guard the degenerate cases
                // to the ball fallback; a real polygon needs at least 3 points.
                if points.len() < 3 {
                    SharedShape::ball(fallback.max(1.0))
                } else {
                    let pts: Vec<Vector> = points.iter().map(|&(x, y)| Vector::new(x, y)).collect();
                    SharedShape::convex_hull(&pts)
                        .unwrap_or_else(|| SharedShape::ball(fallback.max(1.0)))
                }
            }
        }
    }
}

/// A node's physical **material** on the Body axis: how its rapier body responds to contact
/// and weight. The defaults match the spawn constants, so an unconfigured node is unchanged;
/// the orrery pushes per-node overrides via [`Simulation::set_node_materials`] (the same
/// re-apply-to-live-bodies shape as [`Simulation::set_linear_damping`]). Independent of the
/// node's shape (the collider geometry) and its face (the texture). (Node body & face — material.)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeMaterial {
    /// Bounciness on contact: 0 (dead) to 1 (perfectly elastic).
    pub restitution: f32,
    /// Surface grip: 0 (frictionless) upward.
    pub friction: f32,
    /// Mass density (mass = density * area); higher is heavier and harder to push.
    pub density: f32,
}

impl Default for NodeMaterial {
    fn default() -> Self {
        Self {
            restitution: 0.0,
            friction: 0.0,
            density: NODE_BODY_DENSITY,
        }
    }
}

impl Simulation {
    /// Resize **and reshape** each listed node's collider (hit-target + hard-collision
    /// geometry) to match its visible face — a ball, square, rounded square, or custom hull —
    /// so a node collides at its true face shape and size, not the uniform ball
    /// (Decision 5: the face IS the collider, so physics and picture stay in sync). Bodies
    /// keep position and velocity; only the shape changes. Mass is left at the spawn value — it is
    /// the face geometry, not the inertia, that tracks size. Nodes without a body are skipped.
    /// (P0/P5 collider; node-rep — collider matches shape.)
    pub fn set_node_colliders(
        &mut self,
        colliders: impl IntoIterator<Item = (NodeKey, NodeCollider)>,
    ) {
        for (node, collider) in colliders {
            let Some(&body_handle) = self.bodies_by_node.get(&node) else {
                continue;
            };
            // Copy the handles out so the immutable body borrow ends before the
            // mutable collider borrow (distinct fields, but the borrow checker needs
            // the split made explicit).
            let collider_handles: Vec<ColliderHandle> = self
                .bodies
                .get(body_handle)
                .map(|body| body.colliders().to_vec())
                .unwrap_or_default();
            let shape = collider.to_shared_shape();
            for handle in collider_handles {
                if let Some(c) = self.colliders.get_mut(handle) {
                    c.set_shape(shape.clone());
                }
            }
        }
    }

    /// Apply each listed node's physical **material** (restitution / friction / density) to its
    /// live body's colliders, re-applying immediately like [`set_linear_damping`](Self::set_linear_damping).
    /// A density change re-derives the body's mass from its colliders, so a heavier node resists
    /// pushing. The defaults match the spawn values, so an unconfigured node is a no-op. Nodes
    /// without a body are skipped. (Node body & face — material.)
    pub fn set_node_materials(
        &mut self,
        materials: impl IntoIterator<Item = (NodeKey, NodeMaterial)>,
    ) {
        for (node, material) in materials {
            let Some(&body_handle) = self.bodies_by_node.get(&node) else {
                continue;
            };
            // Split the borrows (immutable body read for the handles, then mutable collider
            // writes) the way `set_node_colliders` does.
            let collider_handles: Vec<ColliderHandle> = self
                .bodies
                .get(body_handle)
                .map(|body| body.colliders().to_vec())
                .unwrap_or_default();
            for handle in collider_handles {
                if let Some(c) = self.colliders.get_mut(handle) {
                    c.set_restitution(material.restitution);
                    c.set_friction(material.friction);
                    c.set_density(material.density);
                }
            }
            // Density feeds mass, which rapier caches on the body — recompute it so the new
            // weight takes effect (distinct fields: `bodies` mut, `colliders` read).
            if let Some(body) = self.bodies.get_mut(body_handle) {
                body.recompute_mass_properties_from_colliders(&self.colliders);
            }
        }
    }
}
