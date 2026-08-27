// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use serde::{Deserialize, Serialize};

use conatus_voxel::VoxelEdit;

/// Stable, generational identity for one simulated body.
///
/// Removing a body invalidates its id. Reusing the slot produces a new
/// generation, so a delayed command cannot accidentally address a replacement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BodyId(u64);

impl BodyId {
    pub(crate) const fn from_parts(slot: u32, generation: u32) -> Self {
        Self(((generation as u64) << 32) | slot as u64)
    }

    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    pub const fn slot(self) -> u32 {
        self.0 as u32
    }

    pub const fn generation(self) -> u32 {
        (self.0 >> 32) as u32
    }
}

/// Stable address of one collider belonging to a body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ColliderId {
    body: BodyId,
    part: u32,
}

impl ColliderId {
    pub const fn new(body: BodyId, part: u32) -> Self {
        Self { body, part }
    }

    pub const fn body(self) -> BodyId {
        self.body
    }

    pub const fn part(self) -> u32 {
        self.part
    }
}

/// How a body advances.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyKind {
    Fixed,
    #[default]
    Dynamic,
    KinematicPosition,
    KinematicVelocity,
}

/// A right-handed, Y-up transform. Quaternion order is `[x, y, z, w]`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };

    pub const fn from_translation(translation: [f32; 3]) -> Self {
        Self {
            translation,
            ..Self::IDENTITY
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Velocity {
    pub linear: [f32; 3],
    pub angular: [f32; 3],
}

/// Collision shape vocabulary owned by Conatus rather than its backend.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum ColliderShape {
    Sphere {
        radius: f32,
    },
    Box {
        half_extents: [f32; 3],
    },
    CapsuleY {
        half_height: f32,
        radius: f32,
    },
    CylinderY {
        half_height: f32,
        radius: f32,
    },
    /// Sparse occupied cells. Coordinates name cells; `cell_size` is their
    /// world-space extent. This shape can be edited in place through
    /// [`crate::BodyWorld::edit_voxels`].
    VoxelGrid {
        cell_size: [f32; 3],
        occupied: Vec<[i32; 3]>,
    },
}

impl ColliderShape {
    pub const fn sphere(radius: f32) -> Self {
        Self::Sphere { radius }
    }

    pub const fn cuboid(half_extents: [f32; 3]) -> Self {
        Self::Box { half_extents }
    }

    pub const fn capsule_y(half_height: f32, radius: f32) -> Self {
        Self::CapsuleY {
            half_height,
            radius,
        }
    }

    pub const fn cylinder_y(half_height: f32, radius: f32) -> Self {
        Self::CylinderY {
            half_height,
            radius,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub density: f32,
    pub friction: f32,
    pub restitution: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            density: 1.0,
            friction: 0.5,
            restitution: 0.0,
        }
    }
}

/// Symmetric layer filtering. Two colliders interact when each collider's
/// membership is present in the other's filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollisionLayers {
    pub memberships: u32,
    pub filter: u32,
}

impl CollisionLayers {
    pub const ALL: Self = Self {
        memberships: u32::MAX,
        filter: u32::MAX,
    };

    pub const fn new(memberships: u32, filter: u32) -> Self {
        Self {
            memberships,
            filter,
        }
    }
}

impl Default for CollisionLayers {
    fn default() -> Self {
        Self::ALL
    }
}

/// Filtering shared by ray, shape, and overlap queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpatialFilter {
    pub layers: CollisionLayers,
    pub exclude_body: Option<BodyId>,
    pub include_sensors: bool,
    pub include_solids: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharacterAutostep {
    pub max_height: f32,
    pub min_width: f32,
    pub include_dynamic_bodies: bool,
}

/// Configuration for collision-constrained kinematic movement.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharacterConfig {
    pub up: [f32; 3],
    pub offset: f32,
    pub slide: bool,
    pub autostep: Option<CharacterAutostep>,
    pub max_slope_climb_angle: f32,
    pub min_slope_slide_angle: f32,
    pub snap_to_ground: Option<f32>,
}

impl Default for CharacterConfig {
    fn default() -> Self {
        Self {
            up: [0.0, 1.0, 0.0],
            offset: 0.01,
            slide: true,
            autostep: None,
            max_slope_climb_angle: std::f32::consts::FRAC_PI_4,
            min_slope_slide_angle: std::f32::consts::FRAC_PI_4,
            snap_to_ground: Some(0.2),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharacterCollision {
    pub collider: ColliderId,
    pub time_of_impact: f32,
    pub normal: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharacterMove {
    pub requested: [f32; 3],
    pub applied: [f32; 3],
    pub grounded: bool,
    pub sliding_down_slope: bool,
    pub collisions: Vec<CharacterCollision>,
}

impl Default for SpatialFilter {
    fn default() -> Self {
        Self {
            layers: CollisionLayers::ALL,
            exclude_body: None,
            include_sensors: true,
            include_solids: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColliderDesc {
    pub shape: ColliderShape,
    pub local_transform: Transform,
    pub material: Material,
    pub layers: CollisionLayers,
    pub sensor: bool,
}

impl ColliderDesc {
    pub fn new(shape: ColliderShape) -> Self {
        Self {
            shape,
            local_transform: Transform::IDENTITY,
            material: Material::default(),
            layers: CollisionLayers::ALL,
            sensor: false,
        }
    }

    pub fn at(mut self, local_transform: Transform) -> Self {
        self.local_transform = local_transform;
        self
    }

    pub fn with_material(mut self, material: Material) -> Self {
        self.material = material;
        self
    }

    pub fn with_layers(mut self, layers: CollisionLayers) -> Self {
        self.layers = layers;
        self
    }

    pub fn sensor(mut self, sensor: bool) -> Self {
        self.sensor = sensor;
        self
    }
}

/// Complete construction data for one body. Multiple colliders form a
/// compound body while retaining distinct [`ColliderId`] parts for queries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BodyDesc {
    pub kind: BodyKind,
    pub transform: Transform,
    pub velocity: Velocity,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub gravity_scale: f32,
    pub continuous_collision_detection: bool,
    pub colliders: Vec<ColliderDesc>,
}

impl BodyDesc {
    pub fn new(kind: BodyKind) -> Self {
        Self {
            kind,
            transform: Transform::IDENTITY,
            velocity: Velocity::default(),
            linear_damping: 0.0,
            angular_damping: 0.0,
            gravity_scale: 1.0,
            continuous_collision_detection: false,
            colliders: Vec::new(),
        }
    }

    pub fn dynamic() -> Self {
        Self::new(BodyKind::Dynamic)
    }

    pub fn fixed() -> Self {
        Self::new(BodyKind::Fixed)
    }

    pub fn kinematic_position() -> Self {
        Self::new(BodyKind::KinematicPosition)
    }

    pub fn kinematic_velocity() -> Self {
        Self::new(BodyKind::KinematicVelocity)
    }

    pub fn at(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }

    pub fn with_velocity(mut self, velocity: Velocity) -> Self {
        self.velocity = velocity;
        self
    }

    pub fn with_damping(mut self, linear: f32, angular: f32) -> Self {
        self.linear_damping = linear;
        self.angular_damping = angular;
        self
    }

    pub fn with_gravity_scale(mut self, gravity_scale: f32) -> Self {
        self.gravity_scale = gravity_scale;
        self
    }

    pub fn with_ccd(mut self, enabled: bool) -> Self {
        self.continuous_collision_detection = enabled;
        self
    }

    pub fn with_collider(mut self, collider: ColliderDesc) -> Self {
        self.colliders.push(collider);
        self
    }
}

/// Backend-neutral state suitable for profile and spatial-frame preparation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BodyState {
    pub id: BodyId,
    pub kind: BodyKind,
    pub transform: Transform,
    pub velocity: Velocity,
    pub sleeping: bool,
}

/// Effective voxel-collider edits published to materialization systems and
/// frame consumers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoxelChange {
    pub collider: ColliderId,
    pub previous_revision: u64,
    pub revision: u64,
    pub edits: Vec<VoxelEdit>,
}
