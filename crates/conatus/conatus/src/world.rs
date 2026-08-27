// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use crate::{
    BodyDesc, BodyId, BodyKind, BodyState, CharacterConfig, CharacterMove, ColliderDesc,
    ColliderId, ColliderShape, SpatialFilter, Transform, Velocity, VoxelChange, VoxelEdit,
};

mod rapier;

use rapier::RapierBodyBackend;

#[derive(Clone, Debug, PartialEq)]
pub enum BodyError {
    UnknownBody(BodyId),
    UnknownCollider(ColliderId),
    InvalidBody(&'static str),
    InvalidCollider {
        part: u32,
        reason: &'static str,
    },
    InvalidQuery(&'static str),
    InvalidOperation {
        body: BodyId,
        operation: &'static str,
    },
    NotVoxelCollider(ColliderId),
}

impl fmt::Display for BodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownBody(id) => write!(f, "unknown or stale body id {}", id.raw()),
            Self::UnknownCollider(id) => write!(
                f,
                "unknown collider part {} on body {}",
                id.part(),
                id.body().raw()
            ),
            Self::InvalidBody(reason) => write!(f, "invalid body: {reason}"),
            Self::InvalidCollider { part, reason } => {
                write!(f, "invalid collider part {part}: {reason}")
            }
            Self::InvalidQuery(reason) => write!(f, "invalid spatial query: {reason}"),
            Self::InvalidOperation { body, operation } => {
                write!(f, "body {} does not support {operation}", body.raw())
            }
            Self::NotVoxelCollider(id) => write!(
                f,
                "collider part {} on body {} is not a voxel grid",
                id.part(),
                id.body().raw()
            ),
        }
    }
}

impl Error for BodyError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Interaction {
    Contact,
    Sensor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionState {
    Started,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InteractionEvent {
    pub tick: u64,
    pub state: InteractionState,
    pub interaction: Interaction,
    pub a: ColliderId,
    pub b: ColliderId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub collider: ColliderId,
    pub distance: f32,
    pub point: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StepUpdate {
    pub tick: u64,
    pub revision: u64,
    pub changed: Vec<BodyState>,
    pub removed: Vec<BodyId>,
    pub voxel_changes: Vec<VoxelChange>,
    pub interactions: Vec<InteractionEvent>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoxelEditSummary {
    pub previous_revision: u64,
    pub revision: u64,
    pub requested: usize,
    pub changed: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct InteractionKey {
    interaction: Interaction,
    a: ColliderId,
    b: ColliderId,
}

impl InteractionKey {
    fn new(interaction: Interaction, a: ColliderId, b: ColliderId) -> Option<Self> {
        if a.body() == b.body() {
            return None;
        }
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        Some(Self { interaction, a, b })
    }

    fn event(self, tick: u64, state: InteractionState) -> InteractionEvent {
        InteractionEvent {
            tick,
            state,
            interaction: self.interaction,
            a: self.a,
            b: self.b,
        }
    }

    fn contains(self, body: BodyId) -> bool {
        self.a.body() == body || self.b.body() == body
    }
}

struct Slot {
    generation: u32,
    occupied: bool,
    collider_revisions: Vec<u64>,
}

impl Slot {
    fn vacant(generation: u32) -> Self {
        Self {
            generation,
            occupied: false,
            collider_revisions: Vec::new(),
        }
    }
}

/// A host-neutral 3D tactile world.
///
/// The backend is deliberately private. Callers retain Conatus ids, arrays,
/// descriptors, and frame changes instead of backend handles.
pub struct BodyWorld {
    backend: RapierBodyBackend,
    slots: Vec<Slot>,
    free: Vec<u32>,
    tick: u64,
    revision: u64,
    dirty_bodies: BTreeSet<BodyId>,
    pending_removed: Vec<BodyId>,
    pending_voxel_changes: Vec<VoxelChange>,
    active_interactions: BTreeSet<InteractionKey>,
    pending_events: Vec<InteractionEvent>,
}

impl Default for BodyWorld {
    fn default() -> Self {
        Self::new([0.0, -9.81, 0.0])
    }
}

impl BodyWorld {
    pub fn new(gravity: [f32; 3]) -> Self {
        Self::try_new(gravity).expect("BodyWorld gravity must be finite")
    }

    pub fn try_new(gravity: [f32; 3]) -> Result<Self, BodyError> {
        if !finite3(gravity) {
            return Err(BodyError::InvalidBody("gravity must be finite"));
        }
        Ok(Self {
            backend: RapierBodyBackend::new(gravity),
            slots: Vec::new(),
            free: Vec::new(),
            tick: 0,
            revision: 0,
            dirty_bodies: BTreeSet::new(),
            pending_removed: Vec::new(),
            pending_voxel_changes: Vec::new(),
            active_interactions: BTreeSet::new(),
            pending_events: Vec::new(),
        })
    }

    pub const fn tick(&self) -> u64 {
        self.tick
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|slot| slot.occupied).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, id: BodyId) -> bool {
        self.slot(id).is_some()
    }

    pub fn gravity(&self) -> [f32; 3] {
        self.backend.gravity()
    }

    pub fn set_gravity(&mut self, gravity: [f32; 3]) -> Result<(), BodyError> {
        if !finite3(gravity) {
            return Err(BodyError::InvalidBody("gravity must be finite"));
        }
        self.backend.set_gravity(gravity);
        self.bump_revision();
        Ok(())
    }

    pub fn spawn(&mut self, desc: BodyDesc) -> Result<BodyId, BodyError> {
        validate_body(&desc)?;
        let id = self.reserve_id();

        self.backend.insert(id, desc);

        let revision = self.bump_revision();
        let slot = &mut self.slots[id.slot() as usize];
        slot.occupied = true;
        slot.collider_revisions = vec![revision; self.backend.collider_count(id)];
        self.dirty_bodies.insert(id);
        Ok(id)
    }

    pub fn despawn(&mut self, id: BodyId) -> Result<BodyState, BodyError> {
        let state = self.state(id).ok_or(BodyError::UnknownBody(id))?;
        let ended: Vec<_> = self
            .active_interactions
            .iter()
            .copied()
            .filter(|interaction| interaction.contains(id))
            .collect();
        for interaction in ended {
            self.active_interactions.remove(&interaction);
            self.pending_events
                .push(interaction.event(self.tick, InteractionState::Stopped));
        }

        self.backend.remove(id)?;
        let slot = &mut self.slots[id.slot() as usize];
        slot.occupied = false;
        slot.collider_revisions.clear();
        slot.generation = slot.generation.wrapping_add(1).max(1);
        self.free.push(id.slot());
        self.dirty_bodies.remove(&id);
        self.pending_removed.push(id);
        self.bump_revision();
        Ok(state)
    }

    pub fn state(&self, id: BodyId) -> Option<BodyState> {
        self.slot(id)?;
        self.backend.state(id)
    }

    /// A stable-slot-ordered state snapshot for save preparation, renderer
    /// bootstrap, or a newly attached consumer.
    pub fn states(&self) -> Vec<BodyState> {
        self.ids().filter_map(|id| self.state(id)).collect()
    }

    pub fn ids(&self) -> impl Iterator<Item = BodyId> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.occupied)
            .map(|(slot, entry)| BodyId::from_parts(slot as u32, entry.generation))
    }

    pub fn collider_ids(&self, body: BodyId) -> Result<Vec<ColliderId>, BodyError> {
        let slot = self.slot(body).ok_or(BodyError::UnknownBody(body))?;
        Ok((0..slot.collider_revisions.len())
            .map(|part| ColliderId::new(body, part as u32))
            .collect())
    }

    pub fn collider_revision(&self, collider: ColliderId) -> Result<u64, BodyError> {
        let (slot, index) = self.collider_slot(collider)?;
        Ok(slot.collider_revisions[index])
    }

    pub fn set_transform(
        &mut self,
        id: BodyId,
        transform: Transform,
        wake: bool,
    ) -> Result<(), BodyError> {
        validate_transform(transform).map_err(BodyError::InvalidBody)?;
        self.ensure_body(id)?;
        self.backend.set_transform(id, transform, wake)?;
        self.dirty_bodies.insert(id);
        self.bump_revision();
        Ok(())
    }

    pub fn set_next_kinematic_transform(
        &mut self,
        id: BodyId,
        transform: Transform,
    ) -> Result<(), BodyError> {
        validate_transform(transform).map_err(BodyError::InvalidBody)?;
        self.ensure_body(id)?;
        self.backend.set_next_kinematic_transform(id, transform)?;
        self.bump_revision();
        Ok(())
    }

    pub fn set_velocity(
        &mut self,
        id: BodyId,
        velocity: Velocity,
        wake: bool,
    ) -> Result<(), BodyError> {
        if !finite3(velocity.linear) || !finite3(velocity.angular) {
            return Err(BodyError::InvalidBody("velocity must be finite"));
        }
        self.ensure_body(id)?;
        self.backend.set_velocity(id, velocity, wake)?;
        self.dirty_bodies.insert(id);
        self.bump_revision();
        Ok(())
    }

    pub fn set_kind(&mut self, id: BodyId, kind: BodyKind) -> Result<(), BodyError> {
        self.ensure_body(id)?;
        self.backend.set_kind(id, kind)?;
        self.dirty_bodies.insert(id);
        self.bump_revision();
        Ok(())
    }

    pub fn apply_force(&mut self, id: BodyId, force: [f32; 3]) -> Result<(), BodyError> {
        if !finite3(force) {
            return Err(BodyError::InvalidBody("force must be finite"));
        }
        self.ensure_body(id)?;
        self.backend.apply_force(id, force)?;
        self.bump_revision();
        Ok(())
    }

    pub fn apply_torque(&mut self, id: BodyId, torque: [f32; 3]) -> Result<(), BodyError> {
        if !finite3(torque) {
            return Err(BodyError::InvalidBody("torque must be finite"));
        }
        self.ensure_body(id)?;
        self.backend.apply_torque(id, torque)?;
        self.bump_revision();
        Ok(())
    }

    pub fn apply_impulse(&mut self, id: BodyId, impulse: [f32; 3]) -> Result<(), BodyError> {
        if !finite3(impulse) {
            return Err(BodyError::InvalidBody("impulse must be finite"));
        }
        self.ensure_body(id)?;
        self.backend.apply_impulse(id, impulse)?;
        self.bump_revision();
        Ok(())
    }

    pub fn apply_torque_impulse(&mut self, id: BodyId, impulse: [f32; 3]) -> Result<(), BodyError> {
        if !finite3(impulse) {
            return Err(BodyError::InvalidBody("torque impulse must be finite"));
        }
        self.ensure_body(id)?;
        self.backend.apply_torque_impulse(id, impulse)?;
        self.bump_revision();
        Ok(())
    }

    /// Constrain a requested translation against the world and queue the
    /// resulting target on a position-kinematic body. The selected collider is
    /// the character shape; its sibling colliders move with the same body.
    pub fn move_character(
        &mut self,
        collider: ColliderId,
        requested: [f32; 3],
        dt: f32,
        config: CharacterConfig,
    ) -> Result<CharacterMove, BodyError> {
        validate_character(config)?;
        if !finite3(requested) || !dt.is_finite() || dt <= 0.0 {
            return Err(BodyError::InvalidOperation {
                body: collider.body(),
                operation: "finite character movement with a positive step",
            });
        }
        self.collider_slot(collider)?;
        let movement = self
            .backend
            .move_character(collider, requested, dt, config)?;
        self.bump_revision();
        Ok(movement)
    }

    /// Apply sparse edits directly to a voxel collider's acceleration
    /// structure. The collider retains its identity and material.
    pub fn edit_voxels(
        &mut self,
        collider: ColliderId,
        edits: impl IntoIterator<Item = VoxelEdit>,
    ) -> Result<VoxelEditSummary, BodyError> {
        let edits: Vec<_> = edits.into_iter().collect();
        let previous_revision = self.collider_revision(collider)?;
        self.collider_slot(collider)?;
        let effective = self.backend.edit_voxels(collider, &edits)?;

        if effective.is_empty() {
            return Ok(VoxelEditSummary {
                previous_revision,
                revision: previous_revision,
                requested: edits.len(),
                changed: 0,
            });
        }

        let revision = self.bump_revision();
        let (slot, index) = self.collider_slot_mut(collider)?;
        slot.collider_revisions[index] = revision;
        self.dirty_bodies.insert(collider.body());
        let changed = effective.len();
        self.pending_voxel_changes.push(VoxelChange {
            collider,
            previous_revision,
            revision,
            edits: effective,
        });
        Ok(VoxelEditSummary {
            previous_revision,
            revision,
            requested: edits.len(),
            changed,
        })
    }

    pub fn raycast(
        &self,
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        solid: bool,
        filter: SpatialFilter,
    ) -> Result<Option<RayHit>, BodyError> {
        if !finite3(origin) || !finite3(direction) || !max_distance.is_finite() {
            return Err(BodyError::InvalidQuery("ray values must be finite"));
        }
        if max_distance < 0.0 {
            return Err(BodyError::InvalidQuery(
                "ray maximum distance must not be negative",
            ));
        }
        if !filter.include_sensors && !filter.include_solids {
            return Ok(None);
        }
        let direction_length = squared_length(direction).sqrt();
        if direction_length <= f32::EPSILON {
            return Err(BodyError::InvalidQuery("ray direction must be non-zero"));
        }
        self.backend.raycast(
            origin,
            direction,
            direction_length,
            max_distance,
            solid,
            filter,
        )
    }

    pub fn overlaps(
        &self,
        transform: Transform,
        shape: &ColliderShape,
        filter: SpatialFilter,
    ) -> Result<Vec<ColliderId>, BodyError> {
        validate_transform(transform).map_err(BodyError::InvalidQuery)?;
        validate_shape(shape).map_err(BodyError::InvalidQuery)?;
        if !filter.include_sensors && !filter.include_solids {
            return Ok(Vec::new());
        }
        self.backend.overlaps(transform, shape, filter)
    }

    pub fn step(&mut self, dt: f32) -> Result<StepUpdate, BodyError> {
        if !dt.is_finite() || dt <= 0.0 {
            return Err(BodyError::InvalidBody(
                "simulation step must be finite and greater than zero",
            ));
        }

        let before: BTreeMap<_, _> = self
            .ids()
            .filter_map(|id| self.state(id).map(|state| (id, state.transform)))
            .collect();
        self.backend.step(dt);
        self.tick = self.tick.saturating_add(1);
        let revision = self.bump_revision();

        let current_interactions = self.backend.collect_interactions();
        let mut interactions = self.drain_events();
        let removed = self.drain_removed();
        let voxel_changes = self.drain_voxel_changes();
        interactions.extend(
            current_interactions
                .difference(&self.active_interactions)
                .copied()
                .map(|key| key.event(self.tick, InteractionState::Started)),
        );
        interactions.extend(
            self.active_interactions
                .difference(&current_interactions)
                .copied()
                .map(|key| key.event(self.tick, InteractionState::Stopped)),
        );
        self.active_interactions = current_interactions;

        let dirty = std::mem::take(&mut self.dirty_bodies);
        let changed = self
            .ids()
            .filter(|id| {
                dirty.contains(id)
                    || self
                        .state(*id)
                        .is_some_and(|state| before.get(id) != Some(&state.transform))
            })
            .filter_map(|id| self.state(id))
            .collect();

        Ok(StepUpdate {
            tick: self.tick,
            revision,
            changed,
            removed,
            voxel_changes,
            interactions,
        })
    }

    /// Interaction stops caused by a despawn are available before another
    /// physics step. Engine frames drain these even when elapsed time produces
    /// zero fixed steps.
    pub fn drain_events(&mut self) -> Vec<InteractionEvent> {
        std::mem::take(&mut self.pending_events)
    }

    /// Drain bodies changed by commands since the last step. A renderer can
    /// use this on a zero-step frame so spawns, despawns of its own handles,
    /// teleports, and voxel edits are not held until simulation advances.
    pub fn drain_changes(&mut self) -> Vec<BodyState> {
        let dirty = std::mem::take(&mut self.dirty_bodies);
        dirty.into_iter().filter_map(|id| self.state(id)).collect()
    }

    /// Drain stable ids removed since the previous step or drain. Renderers
    /// and other projections use this to retire their own instances.
    pub fn drain_removed(&mut self) -> Vec<BodyId> {
        std::mem::take(&mut self.pending_removed)
    }

    pub fn drain_voxel_changes(&mut self) -> Vec<VoxelChange> {
        std::mem::take(&mut self.pending_voxel_changes)
    }

    fn reserve_id(&mut self) -> BodyId {
        if let Some(slot) = self.free.pop() {
            let generation = self.slots[slot as usize].generation;
            BodyId::from_parts(slot, generation)
        } else {
            let slot = self.slots.len() as u32;
            self.slots.push(Slot::vacant(1));
            BodyId::from_parts(slot, 1)
        }
    }

    fn slot(&self, id: BodyId) -> Option<&Slot> {
        let slot = self.slots.get(id.slot() as usize)?;
        (slot.generation == id.generation() && slot.occupied).then_some(slot)
    }

    fn ensure_body(&self, id: BodyId) -> Result<(), BodyError> {
        self.slot(id).map(|_| ()).ok_or(BodyError::UnknownBody(id))
    }

    fn collider_slot(&self, id: ColliderId) -> Result<(&Slot, usize), BodyError> {
        let slot = self
            .slot(id.body())
            .ok_or(BodyError::UnknownBody(id.body()))?;
        let index = id.part() as usize;
        if index >= slot.collider_revisions.len() {
            return Err(BodyError::UnknownCollider(id));
        }
        Ok((slot, index))
    }

    fn collider_slot_mut(&mut self, id: ColliderId) -> Result<(&mut Slot, usize), BodyError> {
        let index = id.part() as usize;
        let slot = self
            .slots
            .get_mut(id.body().slot() as usize)
            .filter(|slot| slot.generation == id.body().generation() && slot.occupied)
            .ok_or(BodyError::UnknownBody(id.body()))?;
        if index >= slot.collider_revisions.len() {
            return Err(BodyError::UnknownCollider(id));
        }
        Ok((slot, index))
    }

    fn bump_revision(&mut self) -> u64 {
        self.revision = self.revision.saturating_add(1);
        self.revision
    }
}

fn validate_body(desc: &BodyDesc) -> Result<(), BodyError> {
    validate_transform(desc.transform).map_err(BodyError::InvalidBody)?;
    if !finite3(desc.velocity.linear) || !finite3(desc.velocity.angular) {
        return Err(BodyError::InvalidBody("velocity must be finite"));
    }
    for (value, name) in [
        (desc.linear_damping, "linear damping"),
        (desc.angular_damping, "angular damping"),
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err(BodyError::InvalidBody(match name {
                "linear damping" => "linear damping must be finite and non-negative",
                _ => "angular damping must be finite and non-negative",
            }));
        }
    }
    if !desc.gravity_scale.is_finite() {
        return Err(BodyError::InvalidBody("gravity scale must be finite"));
    }
    for (part, collider) in desc.colliders.iter().enumerate() {
        validate_collider(collider).map_err(|reason| BodyError::InvalidCollider {
            part: part as u32,
            reason,
        })?;
    }
    Ok(())
}

fn validate_character(config: CharacterConfig) -> Result<(), BodyError> {
    if !finite3(config.up) || squared_length(config.up) <= f32::EPSILON {
        return Err(BodyError::InvalidBody(
            "character up direction must be finite and non-zero",
        ));
    }
    if !config.offset.is_finite() || config.offset <= 0.0 {
        return Err(BodyError::InvalidBody(
            "character offset must be finite and positive",
        ));
    }
    for angle in [config.max_slope_climb_angle, config.min_slope_slide_angle] {
        if !angle.is_finite() || !(0.0..=std::f32::consts::PI).contains(&angle) {
            return Err(BodyError::InvalidBody(
                "character slope angles must be finite and between zero and pi",
            ));
        }
    }
    if config
        .snap_to_ground
        .is_some_and(|distance| !distance.is_finite() || distance < 0.0)
    {
        return Err(BodyError::InvalidBody(
            "character ground snap must be finite and non-negative",
        ));
    }
    if config.autostep.is_some_and(|step| {
        !step.max_height.is_finite()
            || step.max_height <= 0.0
            || !step.min_width.is_finite()
            || step.min_width <= 0.0
    }) {
        return Err(BodyError::InvalidBody(
            "character autostep dimensions must be finite and positive",
        ));
    }
    Ok(())
}

fn validate_collider(desc: &ColliderDesc) -> Result<(), &'static str> {
    validate_transform(desc.local_transform)?;
    validate_shape(&desc.shape)?;
    if !desc.material.density.is_finite() || desc.material.density < 0.0 {
        return Err("density must be finite and non-negative");
    }
    if !desc.material.friction.is_finite() || desc.material.friction < 0.0 {
        return Err("friction must be finite and non-negative");
    }
    if !desc.material.restitution.is_finite() || !(0.0..=1.0).contains(&desc.material.restitution) {
        return Err("restitution must be finite and between zero and one");
    }
    Ok(())
}

fn validate_shape(shape: &ColliderShape) -> Result<(), &'static str> {
    match shape {
        ColliderShape::Sphere { radius } => positive(*radius, "sphere radius"),
        ColliderShape::Box { half_extents } => positive3(*half_extents, "box half-extents"),
        ColliderShape::CapsuleY {
            half_height,
            radius,
        } => {
            positive(*half_height, "capsule half-height")?;
            positive(*radius, "capsule radius")
        }
        ColliderShape::CylinderY {
            half_height,
            radius,
        } => {
            positive(*half_height, "cylinder half-height")?;
            positive(*radius, "cylinder radius")
        }
        ColliderShape::VoxelGrid { cell_size, .. } => positive3(*cell_size, "voxel cell size"),
    }
}

fn validate_transform(transform: Transform) -> Result<(), &'static str> {
    if !finite3(transform.translation) || !transform.rotation.iter().all(|v| v.is_finite()) {
        return Err("transform values must be finite");
    }
    let norm = transform.rotation.iter().map(|v| v * v).sum::<f32>();
    if norm <= f32::EPSILON {
        return Err("rotation quaternion must be non-zero");
    }
    Ok(())
}

fn positive(value: f32, name: &'static str) -> Result<(), &'static str> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(match name {
            "sphere radius" => "sphere radius must be finite and positive",
            "capsule half-height" => "capsule half-height must be finite and positive",
            "capsule radius" => "capsule radius must be finite and positive",
            "cylinder half-height" => "cylinder half-height must be finite and positive",
            "cylinder radius" => "cylinder radius must be finite and positive",
            _ => "shape dimension must be finite and positive",
        })
    }
}

fn positive3(values: [f32; 3], name: &'static str) -> Result<(), &'static str> {
    if values.iter().all(|value| value.is_finite() && *value > 0.0) {
        Ok(())
    } else {
        Err(match name {
            "box half-extents" => "box half-extents must be finite and positive",
            "voxel cell size" => "voxel cell size must be finite and positive",
            _ => "shape dimensions must be finite and positive",
        })
    }
}

fn finite3(value: [f32; 3]) -> bool {
    value.iter().all(|value| value.is_finite())
}

fn squared_length(value: [f32; 3]) -> f32 {
    value.iter().map(|v| v * v).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ColliderDesc, ColliderShape};

    fn ball(position: [f32; 3]) -> BodyDesc {
        BodyDesc::dynamic()
            .at(Transform::from_translation(position))
            .with_collider(ColliderDesc::new(ColliderShape::sphere(0.5)))
    }

    #[test]
    fn ids_are_stable_and_stale_ids_do_not_alias_replacements() {
        let mut world = BodyWorld::default();
        let first = world.spawn(ball([0.0, 2.0, 0.0])).unwrap();
        world.despawn(first).unwrap();
        let replacement = world.spawn(ball([0.0, 2.0, 0.0])).unwrap();

        assert_eq!(first.slot(), replacement.slot());
        assert_ne!(first.generation(), replacement.generation());
        assert!(!world.contains(first));
        assert!(world.contains(replacement));
    }

    #[test]
    fn a_dynamic_body_falls_and_reports_changed_state() {
        let mut world = BodyWorld::default();
        let body = world.spawn(ball([0.0, 2.0, 0.0])).unwrap();
        let update = world.step(1.0 / 60.0).unwrap();

        assert_eq!(update.changed.len(), 1);
        assert_eq!(update.changed[0].id, body);
        assert!(update.changed[0].transform.translation[1] < 2.0);
    }

    #[test]
    fn raycast_returns_conatus_collider_identity() {
        let mut world = BodyWorld::new([0.0; 3]);
        let body = world
            .spawn(
                BodyDesc::fixed()
                    .with_collider(ColliderDesc::new(ColliderShape::cuboid([1.0, 1.0, 1.0]))),
            )
            .unwrap();
        world.step(1.0 / 60.0).unwrap();

        let hit = world
            .raycast(
                [0.0, 4.0, 0.0],
                [0.0, -1.0, 0.0],
                10.0,
                true,
                SpatialFilter::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(hit.collider, ColliderId::new(body, 0));
        assert!((hit.distance - 3.0).abs() < 0.001);
        assert_eq!(hit.normal, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn sensor_entries_and_exits_are_frame_events() {
        let mut world = BodyWorld::new([0.0; 3]);
        let sensor = world
            .spawn(BodyDesc::fixed().with_collider(
                ColliderDesc::new(ColliderShape::cuboid([1.0, 1.0, 1.0])).sensor(true),
            ))
            .unwrap();
        let mover = world.spawn(ball([0.0, 0.0, 0.0])).unwrap();
        let entered = world.step(1.0 / 60.0).unwrap();
        assert!(entered.interactions.iter().any(|event| {
            event.state == InteractionState::Started
                && event.interaction == Interaction::Sensor
                && [event.a.body(), event.b.body()].contains(&sensor)
                && [event.a.body(), event.b.body()].contains(&mover)
        }));

        world
            .set_transform(mover, Transform::from_translation([10.0, 0.0, 0.0]), true)
            .unwrap();
        let exited = world.step(1.0 / 60.0).unwrap();
        assert!(exited.interactions.iter().any(|event| {
            event.state == InteractionState::Stopped && event.interaction == Interaction::Sensor
        }));
    }

    #[test]
    fn voxel_colliders_edit_in_place_and_revise() {
        let mut world = BodyWorld::new([0.0; 3]);
        let body = world
            .spawn(
                BodyDesc::fixed().with_collider(ColliderDesc::new(ColliderShape::VoxelGrid {
                    cell_size: [1.0; 3],
                    occupied: vec![[0, 0, 0]],
                })),
            )
            .unwrap();
        let collider = ColliderId::new(body, 0);
        let before = world.collider_revision(collider).unwrap();
        let edited = world
            .edit_voxels(
                collider,
                [
                    VoxelEdit {
                        cell: [0, 0, 0],
                        filled: false,
                    },
                    VoxelEdit {
                        cell: [1, 0, 0],
                        filled: true,
                    },
                ],
            )
            .unwrap();

        assert_eq!(edited.previous_revision, before);
        assert_eq!(edited.changed, 2);
        assert!(edited.revision > before);
        assert_eq!(world.collider_revision(collider).unwrap(), edited.revision);
    }

    #[test]
    fn character_movement_stops_at_world_collision() {
        let mut world = BodyWorld::new([0.0; 3]);
        let wall = world
            .spawn(
                BodyDesc::fixed()
                    .at(Transform::from_translation([2.0, 0.0, 0.0]))
                    .with_collider(ColliderDesc::new(ColliderShape::cuboid([0.5, 2.0, 2.0]))),
            )
            .unwrap();
        let character = world
            .spawn(
                BodyDesc::kinematic_position()
                    .with_collider(ColliderDesc::new(ColliderShape::sphere(0.5))),
            )
            .unwrap();
        world.step(1.0 / 60.0).unwrap();

        let movement = world
            .move_character(
                ColliderId::new(character, 0),
                [5.0, 0.0, 0.0],
                1.0 / 60.0,
                CharacterConfig {
                    snap_to_ground: None,
                    ..CharacterConfig::default()
                },
            )
            .unwrap();
        assert!(movement.applied[0] < 1.1, "applied {:?}", movement.applied);
        assert!(
            movement
                .collisions
                .iter()
                .any(|collision| collision.collider.body() == wall)
        );

        world.step(1.0 / 60.0).unwrap();
        let x = world.state(character).unwrap().transform.translation[0];
        assert!((x - movement.applied[0]).abs() < 0.001);
    }
}
