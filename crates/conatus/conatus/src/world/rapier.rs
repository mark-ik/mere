// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::collections::{BTreeMap, BTreeSet};

use rapier3d::control::{
    CharacterAutostep as RapierCharacterAutostep, CharacterLength, KinematicCharacterController,
};
use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, Group, IVector, InteractionGroups, InteractionTestMode,
    PhysicsWorld, QueryFilter as RapierQueryFilter, QueryFilterFlags, RigidBodyBuilder,
    RigidBodyHandle, RigidBodyType, Rotation, SharedShape, Vector,
};

use super::{BodyError, Interaction, InteractionKey, RayHit};
use crate::{
    BodyDesc, BodyId, BodyKind, BodyState, CharacterCollision, CharacterConfig, CharacterMove,
    ColliderDesc, ColliderId, ColliderShape, CollisionLayers, SpatialFilter, Transform, Velocity,
    VoxelEdit,
};

struct RapierBody {
    body: RigidBodyHandle,
    colliders: Vec<ColliderHandle>,
}

/// The incumbent tactile implementation. It contains every Rapier handle,
/// conversion, query, step, and interaction normalization path; `BodyWorld`
/// owns the stable Conatus identity and publication bookkeeping above it.
pub(super) struct RapierBodyBackend {
    physics: PhysicsWorld,
    bodies: BTreeMap<BodyId, RapierBody>,
}

impl RapierBodyBackend {
    pub(super) fn new(gravity: [f32; 3]) -> Self {
        let mut physics = PhysicsWorld::new();
        physics.gravity = vector(gravity);
        Self {
            physics,
            bodies: BTreeMap::new(),
        }
    }

    pub(super) fn gravity(&self) -> [f32; 3] {
        array(self.physics.gravity)
    }

    pub(super) fn set_gravity(&mut self, gravity: [f32; 3]) {
        self.physics.gravity = vector(gravity);
    }

    pub(super) fn insert(&mut self, id: BodyId, desc: BodyDesc) {
        let builder = match desc.kind {
            BodyKind::Fixed => RigidBodyBuilder::fixed(),
            BodyKind::Dynamic => RigidBodyBuilder::dynamic(),
            BodyKind::KinematicPosition => RigidBodyBuilder::kinematic_position_based(),
            BodyKind::KinematicVelocity => RigidBodyBuilder::kinematic_velocity_based(),
        }
        .pose(pose(desc.transform))
        .linvel(vector(desc.velocity.linear))
        .angvel(vector(desc.velocity.angular))
        .linear_damping(desc.linear_damping)
        .angular_damping(desc.angular_damping)
        .gravity_scale(desc.gravity_scale)
        .ccd_enabled(desc.continuous_collision_detection)
        .user_data(id.raw() as u128);

        let body = self.physics.insert_body(builder);
        let mut colliders = Vec::with_capacity(desc.colliders.len());
        for (part, collider) in desc.colliders.into_iter().enumerate() {
            let collider_id = ColliderId::new(id, part as u32);
            colliders.push(
                self.physics
                    .insert_collider(collider_builder(collider, collider_id), Some(body)),
            );
        }
        let previous = self.bodies.insert(id, RapierBody { body, colliders });
        debug_assert!(previous.is_none(), "BodyWorld reserved an occupied body id");
    }

    pub(super) fn remove(&mut self, id: BodyId) -> Result<(), BodyError> {
        let body = self.body_handle(id)?;
        self.physics.remove_body(body);
        self.bodies.remove(&id);
        Ok(())
    }

    pub(super) fn collider_count(&self, id: BodyId) -> usize {
        self.bodies[&id].colliders.len()
    }

    pub(super) fn state(&self, id: BodyId) -> Option<BodyState> {
        let body = self.physics.bodies.get(self.bodies.get(&id)?.body)?;
        Some(BodyState {
            id,
            kind: body_kind(body.body_type()),
            transform: transform(body.position()),
            velocity: Velocity {
                linear: array(body.linvel()),
                angular: array(body.angvel()),
            },
            sleeping: body.is_sleeping(),
        })
    }

    pub(super) fn set_transform(
        &mut self,
        id: BodyId,
        transform: Transform,
        wake: bool,
    ) -> Result<(), BodyError> {
        let body = self.body_handle(id)?;
        self.physics.bodies[body].set_position(pose(transform), wake);
        Ok(())
    }

    pub(super) fn set_next_kinematic_transform(
        &mut self,
        id: BodyId,
        transform: Transform,
    ) -> Result<(), BodyError> {
        let body = self.body_handle(id)?;
        let body = &mut self.physics.bodies[body];
        if !body.is_kinematic() {
            return Err(BodyError::InvalidOperation {
                body: id,
                operation: "a kinematic target",
            });
        }
        body.set_next_kinematic_position(pose(transform));
        Ok(())
    }

    pub(super) fn set_velocity(
        &mut self,
        id: BodyId,
        velocity: Velocity,
        wake: bool,
    ) -> Result<(), BodyError> {
        let body = self.body_handle(id)?;
        let body = &mut self.physics.bodies[body];
        body.set_linvel(vector(velocity.linear), wake);
        body.set_angvel(vector(velocity.angular), wake);
        Ok(())
    }

    pub(super) fn set_kind(&mut self, id: BodyId, kind: BodyKind) -> Result<(), BodyError> {
        let body = self.body_handle(id)?;
        self.physics.bodies[body].set_body_type(rapier_body_kind(kind), true);
        Ok(())
    }

    pub(super) fn apply_force(&mut self, id: BodyId, force: [f32; 3]) -> Result<(), BodyError> {
        self.dynamic_body(id, "force application")?
            .add_force(vector(force), true);
        Ok(())
    }

    pub(super) fn apply_torque(&mut self, id: BodyId, torque: [f32; 3]) -> Result<(), BodyError> {
        self.dynamic_body(id, "torque application")?
            .add_torque(vector(torque), true);
        Ok(())
    }

    pub(super) fn apply_impulse(&mut self, id: BodyId, impulse: [f32; 3]) -> Result<(), BodyError> {
        self.dynamic_body(id, "impulse application")?
            .apply_impulse(vector(impulse), true);
        Ok(())
    }

    pub(super) fn apply_torque_impulse(
        &mut self,
        id: BodyId,
        impulse: [f32; 3],
    ) -> Result<(), BodyError> {
        self.dynamic_body(id, "torque impulse application")?
            .apply_torque_impulse(vector(impulse), true);
        Ok(())
    }

    pub(super) fn move_character(
        &mut self,
        collider: ColliderId,
        requested: [f32; 3],
        dt: f32,
        config: CharacterConfig,
    ) -> Result<CharacterMove, BodyError> {
        let body_handle = self.body_handle(collider.body())?;
        if body_kind(self.physics.bodies[body_handle].body_type()) != BodyKind::KinematicPosition {
            return Err(BodyError::InvalidOperation {
                body: collider.body(),
                operation: "position-kinematic character movement",
            });
        }
        let collider_handle = self.collider_handle(collider)?;
        let controller = KinematicCharacterController {
            up: vector(config.up).normalize(),
            offset: CharacterLength::Absolute(config.offset),
            slide: config.slide,
            autostep: config.autostep.map(|step| RapierCharacterAutostep {
                max_height: CharacterLength::Absolute(step.max_height),
                min_width: CharacterLength::Absolute(step.min_width),
                include_dynamic_bodies: step.include_dynamic_bodies,
            }),
            max_slope_climb_angle: config.max_slope_climb_angle,
            min_slope_slide_angle: config.min_slope_slide_angle,
            snap_to_ground: config.snap_to_ground.map(CharacterLength::Absolute),
            ..KinematicCharacterController::default()
        };

        let mut collisions = Vec::new();
        let movement = {
            let character = &self.physics.colliders[collider_handle];
            let filter = RapierQueryFilter {
                groups: Some(character.collision_groups()),
                flags: QueryFilterFlags::EXCLUDE_SENSORS,
                exclude_rigid_body: Some(body_handle),
                ..RapierQueryFilter::default()
            };
            let queries = self.physics.query_pipeline_with_filter(filter);
            controller.move_shape(
                dt,
                &queries,
                character.shape(),
                character.position(),
                vector(requested),
                |collision| {
                    if let Some(collider) = self.collider_id(collision.handle) {
                        collisions.push(CharacterCollision {
                            collider,
                            time_of_impact: collision.hit.time_of_impact,
                            normal: array(collision.hit.normal1),
                        });
                    }
                },
            )
        };

        let mut target = *self.physics.bodies[body_handle].position();
        target.translation += movement.translation;
        self.physics.bodies[body_handle].set_next_kinematic_position(target);
        Ok(CharacterMove {
            requested,
            applied: array(movement.translation),
            grounded: movement.grounded,
            sliding_down_slope: movement.is_sliding_down_slope,
            collisions,
        })
    }

    pub(super) fn edit_voxels(
        &mut self,
        collider: ColliderId,
        edits: &[VoxelEdit],
    ) -> Result<Vec<VoxelEdit>, BodyError> {
        let collider_handle = self.collider_handle(collider)?;
        let voxels = self.physics.colliders[collider_handle]
            .shape_mut()
            .as_voxels_mut()
            .ok_or(BodyError::NotVoxelCollider(collider))?;
        let mut effective = Vec::new();
        for edit in edits {
            let previous = voxels.set_voxel(ivector(edit.cell), edit.filled);
            if previous.is_empty() == edit.filled {
                effective.push(*edit);
            }
        }
        Ok(effective)
    }

    pub(super) fn raycast(
        &self,
        origin: [f32; 3],
        direction: [f32; 3],
        direction_length: f32,
        max_distance: f32,
        solid: bool,
        filter: SpatialFilter,
    ) -> Result<Option<RayHit>, BodyError> {
        let direction = direction.map(|value| value / direction_length);
        let ray = rapier3d::prelude::Ray::new(vector(origin), vector(direction));
        let query_filter = self.query_filter(filter)?;
        let Some((handle, hit)) =
            self.physics
                .cast_ray_and_get_normal(&ray, max_distance, solid, query_filter)
        else {
            return Ok(None);
        };
        let collider = self.collider_id(handle).ok_or(BodyError::InvalidQuery(
            "backend returned an unowned collider",
        ))?;
        let point = ray.origin + ray.dir * hit.time_of_impact;
        Ok(Some(RayHit {
            collider,
            distance: hit.time_of_impact,
            point: array(point),
            normal: array(hit.normal),
        }))
    }

    pub(super) fn overlaps(
        &self,
        transform: Transform,
        shape: &ColliderShape,
        filter: SpatialFilter,
    ) -> Result<Vec<ColliderId>, BodyError> {
        let shape = shared_shape(shape);
        let query_filter = self.query_filter(filter)?;
        let mut hits: Vec<_> = self
            .physics
            .query_pipeline_with_filter(query_filter)
            .intersect_shape(pose(transform), shape.as_ref())
            .filter_map(|(handle, _)| self.collider_id(handle))
            .collect();
        hits.sort_unstable();
        hits.dedup();
        Ok(hits)
    }

    pub(super) fn step(&mut self, dt: f32) {
        self.physics.integration_parameters.dt = dt;
        self.physics.step();
    }

    pub(super) fn collect_interactions(&self) -> BTreeSet<InteractionKey> {
        let mut current = BTreeSet::new();
        for pair in self.physics.contact_pairs() {
            if !pair.has_any_active_contact() {
                continue;
            }
            let (Some(a), Some(b)) = (
                self.collider_id(pair.collider1),
                self.collider_id(pair.collider2),
            ) else {
                continue;
            };
            if let Some(key) = InteractionKey::new(Interaction::Contact, a, b) {
                current.insert(key);
            }
        }
        for (a_handle, _, b_handle, _, intersecting) in self.physics.intersection_pairs() {
            if !intersecting {
                continue;
            }
            let (Some(a), Some(b)) = (self.collider_id(a_handle), self.collider_id(b_handle))
            else {
                continue;
            };
            if let Some(key) = InteractionKey::new(Interaction::Sensor, a, b) {
                current.insert(key);
            }
        }
        current
    }

    fn body_handle(&self, id: BodyId) -> Result<RigidBodyHandle, BodyError> {
        self.bodies
            .get(&id)
            .map(|body| body.body)
            .ok_or(BodyError::UnknownBody(id))
    }

    fn dynamic_body(
        &mut self,
        id: BodyId,
        operation: &'static str,
    ) -> Result<&mut rapier3d::dynamics::RigidBody, BodyError> {
        let handle = self.body_handle(id)?;
        let body = &mut self.physics.bodies[handle];
        if !body.is_dynamic() {
            return Err(BodyError::InvalidOperation {
                body: id,
                operation,
            });
        }
        Ok(body)
    }

    fn collider_handle(&self, id: ColliderId) -> Result<ColliderHandle, BodyError> {
        let body = self
            .bodies
            .get(&id.body())
            .ok_or(BodyError::UnknownBody(id.body()))?;
        body.colliders
            .get(id.part() as usize)
            .copied()
            .ok_or(BodyError::UnknownCollider(id))
    }

    fn collider_id(&self, handle: ColliderHandle) -> Option<ColliderId> {
        let raw = self.physics.colliders.get(handle)?.user_data;
        let body = BodyId::from_raw(raw as u64);
        let collider = ColliderId::new(body, (raw >> 64) as u32);
        (self.collider_handle(collider).ok() == Some(handle)).then_some(collider)
    }

    fn query_filter(&self, filter: SpatialFilter) -> Result<RapierQueryFilter<'_>, BodyError> {
        let flags = match (filter.include_sensors, filter.include_solids) {
            (true, true) => QueryFilterFlags::empty(),
            (true, false) => QueryFilterFlags::EXCLUDE_SOLIDS,
            (false, true) => QueryFilterFlags::EXCLUDE_SENSORS,
            (false, false) => QueryFilterFlags::EXCLUDE_SOLIDS | QueryFilterFlags::EXCLUDE_SENSORS,
        };
        let exclude_rigid_body = filter
            .exclude_body
            .map(|id| self.body_handle(id))
            .transpose()?;
        Ok(RapierQueryFilter {
            flags,
            groups: Some(interaction_groups(filter.layers)),
            exclude_rigid_body,
            ..RapierQueryFilter::default()
        })
    }
}

fn collider_builder(desc: ColliderDesc, id: ColliderId) -> ColliderBuilder {
    ColliderBuilder::new(shared_shape(&desc.shape))
        .position(pose(desc.local_transform))
        .density(desc.material.density)
        .friction(desc.material.friction)
        .restitution(desc.material.restitution)
        .sensor(desc.sensor)
        .collision_groups(interaction_groups(desc.layers))
        .user_data(collider_user_data(id))
}

fn shared_shape(shape: &ColliderShape) -> SharedShape {
    match shape {
        ColliderShape::Sphere { radius } => SharedShape::ball(*radius),
        ColliderShape::Box { half_extents } => {
            SharedShape::cuboid(half_extents[0], half_extents[1], half_extents[2])
        }
        ColliderShape::CapsuleY {
            half_height,
            radius,
        } => SharedShape::capsule_y(*half_height, *radius),
        ColliderShape::CylinderY {
            half_height,
            radius,
        } => SharedShape::cylinder(*half_height, *radius),
        ColliderShape::VoxelGrid {
            cell_size,
            occupied,
        } => {
            let occupied: Vec<_> = occupied.iter().copied().map(ivector).collect();
            SharedShape::voxels(vector(*cell_size), &occupied)
        }
    }
}

fn collider_user_data(id: ColliderId) -> u128 {
    id.body().raw() as u128 | ((id.part() as u128) << 64)
}

fn interaction_groups(layers: CollisionLayers) -> InteractionGroups {
    InteractionGroups::new(
        Group::from_bits_truncate(layers.memberships),
        Group::from_bits_truncate(layers.filter),
        InteractionTestMode::And,
    )
}

fn rapier_body_kind(kind: BodyKind) -> RigidBodyType {
    match kind {
        BodyKind::Fixed => RigidBodyType::Fixed,
        BodyKind::Dynamic => RigidBodyType::Dynamic,
        BodyKind::KinematicPosition => RigidBodyType::KinematicPositionBased,
        BodyKind::KinematicVelocity => RigidBodyType::KinematicVelocityBased,
    }
}

fn body_kind(kind: RigidBodyType) -> BodyKind {
    match kind {
        RigidBodyType::Fixed => BodyKind::Fixed,
        RigidBodyType::Dynamic => BodyKind::Dynamic,
        RigidBodyType::KinematicPositionBased => BodyKind::KinematicPosition,
        RigidBodyType::KinematicVelocityBased => BodyKind::KinematicVelocity,
    }
}

fn pose(transform: Transform) -> rapier3d::prelude::Pose {
    let [x, y, z, w] = transform.rotation;
    rapier3d::prelude::Pose::from_parts(
        Vector::new(
            transform.translation[0],
            transform.translation[1],
            transform.translation[2],
        ),
        Rotation::from_xyzw(x, y, z, w).normalize(),
    )
}

fn transform(pose: &rapier3d::prelude::Pose) -> Transform {
    let translation = pose.translation;
    let rotation = pose.rotation;
    Transform {
        translation: [translation.x, translation.y, translation.z],
        rotation: [rotation.x, rotation.y, rotation.z, rotation.w],
    }
}

fn vector(value: [f32; 3]) -> Vector {
    Vector::new(value[0], value[1], value[2])
}

fn ivector(value: [i32; 3]) -> IVector {
    IVector::new(value[0], value[1], value[2])
}

fn array(value: Vector) -> [f32; 3] {
    [value.x, value.y, value.z]
}
