/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Scene composition types: colliders, physics materials, body kinds, and
//! avatar bindings.
//!
//! These are pure data types with no physics-engine dependency. They describe
//! the physical properties of scene objects so that a host-side simulation
//! adapter can construct a rigid-body world.
//!
//! The types follow the scene composition vocabulary from the architecture
//! plan (`2026-04-10_vello_scene_canvas_rapier_scene_mode_architecture_plan.md`).

use euclid::default::{Point2D, Vector2D};
use serde::{Deserialize, Serialize};

/// Collider shape specification for a scene body.
///
/// These are authored shapes — the runtime physics engine converts them
/// to its native collider representation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColliderSpec {
    /// Circle collider centered at the body origin.
    Circle { radius: f32 },
    /// Axis-aligned rectangle centered at the body origin.
    Rect { half_extents: Vector2D<f32> },
    /// Capsule: a rectangle with semicircle caps, oriented vertically.
    Capsule { half_height: f32, radius: f32 },
    /// Convex hull from a set of points.
    ConvexHull { points: Vec<Point2D<f32>> },
    /// Compound: union of multiple collider specs.
    Compound(Vec<ColliderSpec>),
}

impl ColliderSpec {
    /// Convenience: circle collider from radius.
    pub fn circle(radius: f32) -> Self {
        Self::Circle { radius }
    }

    /// Convenience: rect collider from full width and height.
    pub fn rect(width: f32, height: f32) -> Self {
        Self::Rect {
            half_extents: Vector2D::new(width * 0.5, height * 0.5),
        }
    }
}

/// Physical material properties for a scene body.
///
/// These control how the body behaves in a rigid-body simulation. When no
/// simulation is active, they are inert data.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PhysicsMaterial {
    /// Mass density (affects derived mass from collider area).
    pub density: f32,
    /// Coulomb friction coefficient.
    pub friction: f32,
    /// Coefficient of restitution (bounciness). 0 = no bounce, 1 = perfect.
    pub restitution: f32,
    /// Linear velocity damping (drag).
    pub linear_damping: f32,
    /// Angular velocity damping (rotational drag).
    pub angular_damping: f32,
    /// Gravity scale. 0 = no gravity, 1 = normal, negative = anti-gravity.
    pub gravity_scale: f32,
}

impl Default for PhysicsMaterial {
    fn default() -> Self {
        Self {
            density: 1.0,
            friction: 0.5,
            restitution: 0.3,
            linear_damping: 0.5,
            angular_damping: 1.0,
            gravity_scale: 0.0, // Default no gravity for 2D graph canvas
        }
    }
}

/// Body kind for a scene object in the physics simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SceneBodyKind {
    /// Fully simulated: forces, collisions, gravity all apply.
    #[default]
    Dynamic,
    /// Immovable obstacle. Participates in collision but never moves.
    Static,
    /// Moves only through explicit position updates (not forces).
    /// Useful for user-dragged objects that should still push other bodies.
    KinematicPositionBased,
    /// Moves only through explicit velocity updates.
    KinematicVelocityBased,
}

/// Binding between a graph node and its physical representation in the scene.
///
/// The host creates these from graph metadata. Simulation adapters use them to
/// construct rigid bodies and colliders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeAvatarBinding<N> {
    /// The graph node this binding applies to.
    pub node_id: N,
    /// Collider shape.
    pub collider: ColliderSpec,
    /// Physics material.
    pub material: PhysicsMaterial,
    /// Body kind.
    pub body_kind: SceneBodyKind,
}

impl<N> NodeAvatarBinding<N> {
    /// Create a simple circle binding with default material.
    pub fn circle(node_id: N, radius: f32) -> Self {
        Self {
            node_id,
            collider: ColliderSpec::circle(radius),
            material: PhysicsMaterial::default(),
            body_kind: SceneBodyKind::Dynamic,
        }
    }

    /// Create a circle binding for a pinned/static node.
    pub fn pinned_circle(node_id: N, radius: f32) -> Self {
        Self {
            node_id,
            collider: ColliderSpec::circle(radius),
            material: PhysicsMaterial::default(),
            body_kind: SceneBodyKind::Static,
        }
    }
}

/// Edge joint specification for the physics simulation.
///
/// Describes how a graph edge maps to a physics constraint between two bodies.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EdgeJointSpec {
    /// Rest length of the spring/rope joint.
    pub rest_length: f32,
    /// Spring stiffness. Higher = stiffer connection.
    pub stiffness: f32,
    /// Damping coefficient for the joint.
    pub damping: f32,
}

impl Default for EdgeJointSpec {
    fn default() -> Self {
        Self {
            rest_length: 80.0,
            stiffness: 0.5,
            damping: 0.3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collider_spec_circle() {
        let c = ColliderSpec::circle(16.0);
        match c {
            ColliderSpec::Circle { radius } => assert_eq!(radius, 16.0),
            _ => panic!("expected circle"),
        }
    }

    #[test]
    fn collider_spec_rect() {
        let c = ColliderSpec::rect(40.0, 20.0);
        match c {
            ColliderSpec::Rect { half_extents } => {
                assert_eq!(half_extents.x, 20.0);
                assert_eq!(half_extents.y, 10.0);
            }
            _ => panic!("expected rect"),
        }
    }

    #[test]
    fn physics_material_default() {
        let m = PhysicsMaterial::default();
        assert_eq!(m.density, 1.0);
        assert_eq!(m.gravity_scale, 0.0);
    }

    #[test]
    fn body_kind_default_is_dynamic() {
        assert_eq!(SceneBodyKind::default(), SceneBodyKind::Dynamic);
    }

    #[test]
    fn node_avatar_binding_circle() {
        let binding = NodeAvatarBinding::circle(42u32, 16.0);
        assert_eq!(binding.node_id, 42);
        assert_eq!(binding.body_kind, SceneBodyKind::Dynamic);
    }

    #[test]
    fn node_avatar_binding_pinned() {
        let binding = NodeAvatarBinding::pinned_circle(0u32, 10.0);
        assert_eq!(binding.body_kind, SceneBodyKind::Static);
    }

    #[test]
    fn edge_joint_spec_default() {
        let j = EdgeJointSpec::default();
        assert!(j.rest_length > 0.0);
        assert!(j.stiffness > 0.0);
    }

    #[test]
    fn serde_roundtrip_collider_spec() {
        let specs = vec![
            ColliderSpec::circle(10.0),
            ColliderSpec::rect(40.0, 20.0),
            ColliderSpec::Capsule {
                half_height: 15.0,
                radius: 5.0,
            },
            ColliderSpec::ConvexHull {
                points: vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(10.0, 0.0),
                    Point2D::new(5.0, 10.0),
                ],
            },
            ColliderSpec::Compound(vec![
                ColliderSpec::circle(5.0),
                ColliderSpec::rect(10.0, 10.0),
            ]),
        ];
        let json = serde_json::to_string(&specs).unwrap();
        let back: Vec<ColliderSpec> = serde_json::from_str(&json).unwrap();
        assert_eq!(specs, back);
    }

    #[test]
    fn serde_roundtrip_avatar_binding() {
        let binding = NodeAvatarBinding {
            node_id: 7u32,
            collider: ColliderSpec::circle(12.0),
            material: PhysicsMaterial {
                density: 2.0,
                friction: 0.8,
                restitution: 0.1,
                linear_damping: 1.0,
                angular_damping: 2.0,
                gravity_scale: -0.5,
            },
            body_kind: SceneBodyKind::KinematicPositionBased,
        };
        let json = serde_json::to_string(&binding).unwrap();
        let back: NodeAvatarBinding<u32> = serde_json::from_str(&json).unwrap();
        assert_eq!(binding, back);
    }
}
