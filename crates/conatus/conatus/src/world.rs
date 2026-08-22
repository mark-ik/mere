use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use rapier3d::control::{
    CharacterAutostep as RapierCharacterAutostep, CharacterLength, KinematicCharacterController,
};
use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, Group, IVector, InteractionGroups, InteractionTestMode,
    PhysicsWorld, QueryFilter as RapierQueryFilter, QueryFilterFlags, RigidBodyBuilder,
    RigidBodyHandle, RigidBodyType, Rotation, SharedShape, Vector,
};

use crate::{
    BodyDesc, BodyId, BodyKind, BodyState, CharacterCollision, CharacterConfig, CharacterMove,
    ColliderDesc, ColliderId, ColliderShape, CollisionLayers, SpatialFilter, Transform, Velocity,
    VoxelChange, VoxelEdit,
};

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
    body: Option<RigidBodyHandle>,
    colliders: Vec<ColliderHandle>,
    collider_revisions: Vec<u64>,
}

impl Slot {
    fn vacant(generation: u32) -> Self {
        Self {
            generation,
            body: None,
            colliders: Vec::new(),
            collider_revisions: Vec::new(),
        }
    }
}

/// A host-neutral 3D tactile world.
///
/// The backend is deliberately private. Callers retain Conatus ids, arrays,
/// descriptors, and frame changes instead of backend handles.
pub struct BodyWorld {
    physics: PhysicsWorld,
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
        let mut physics = PhysicsWorld::new();
        physics.gravity = vector(gravity);
        Ok(Self {
            physics,
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
        self.slots.iter().filter(|slot| slot.body.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, id: BodyId) -> bool {
        self.body_handle(id).is_some()
    }

    pub fn gravity(&self) -> [f32; 3] {
        array(self.physics.gravity)
    }

    pub fn set_gravity(&mut self, gravity: [f32; 3]) -> Result<(), BodyError> {
        if !finite3(gravity) {
            return Err(BodyError::InvalidBody("gravity must be finite"));
        }
        self.physics.gravity = vector(gravity);
        self.bump_revision();
        Ok(())
    }

    pub fn spawn(&mut self, desc: BodyDesc) -> Result<BodyId, BodyError> {
        validate_body(&desc)?;
        let id = self.reserve_id();

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

        let body_handle = self.physics.insert_body(builder);
        let mut collider_handles = Vec::with_capacity(desc.colliders.len());
        for (part, collider) in desc.colliders.into_iter().enumerate() {
            let collider_id = ColliderId::new(id, part as u32);
            let builder = collider_builder(collider, collider_id);
            collider_handles.push(self.physics.insert_collider(builder, Some(body_handle)));
        }

        let revision = self.bump_revision();
        let slot = &mut self.slots[id.slot() as usize];
        slot.body = Some(body_handle);
        slot.collider_revisions = vec![revision; collider_handles.len()];
        slot.colliders = collider_handles;
        self.dirty_bodies.insert(id);
        Ok(id)
    }

    pub fn despawn(&mut self, id: BodyId) -> Result<BodyState, BodyError> {
        let state = self.state(id).ok_or(BodyError::UnknownBody(id))?;
        let handle = self.body_handle(id).ok_or(BodyError::UnknownBody(id))?;

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

        self.physics.remove_body(handle);
        let slot = &mut self.slots[id.slot() as usize];
        slot.body = None;
        slot.colliders.clear();
        slot.collider_revisions.clear();
        slot.generation = slot.generation.wrapping_add(1).max(1);
        self.free.push(id.slot());
        self.dirty_bodies.remove(&id);
        self.pending_removed.push(id);
        self.bump_revision();
        Ok(state)
    }

    pub fn state(&self, id: BodyId) -> Option<BodyState> {
        let handle = self.body_handle(id)?;
        let body = self.physics.bodies.get(handle)?;
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

    /// A stable-slot-ordered state snapshot for save preparation, renderer
    /// bootstrap, or a newly attached consumer.
    pub fn states(&self) -> Vec<BodyState> {
        self.ids().filter_map(|id| self.state(id)).collect()
    }

    pub fn ids(&self) -> impl Iterator<Item = BodyId> + '_ {
        self.slots.iter().enumerate().filter_map(|(slot, entry)| {
            entry
                .body
                .map(|_| BodyId::from_parts(slot as u32, entry.generation))
        })
    }

    pub fn collider_ids(&self, body: BodyId) -> Result<Vec<ColliderId>, BodyError> {
        let slot = self.slot(body).ok_or(BodyError::UnknownBody(body))?;
        Ok((0..slot.colliders.len())
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
        let handle = self.body_handle(id).ok_or(BodyError::UnknownBody(id))?;
        self.physics.bodies[handle].set_position(pose(transform), wake);
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
        let handle = self.body_handle(id).ok_or(BodyError::UnknownBody(id))?;
        let body = &mut self.physics.bodies[handle];
        if !body.is_kinematic() {
            return Err(BodyError::InvalidOperation {
                body: id,
                operation: "a kinematic target",
            });
        }
        body.set_next_kinematic_position(pose(transform));
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
        let handle = self.body_handle(id).ok_or(BodyError::UnknownBody(id))?;
        let body = &mut self.physics.bodies[handle];
        body.set_linvel(vector(velocity.linear), wake);
        body.set_angvel(vector(velocity.angular), wake);
        self.dirty_bodies.insert(id);
        self.bump_revision();
        Ok(())
    }

    pub fn set_kind(&mut self, id: BodyId, kind: BodyKind) -> Result<(), BodyError> {
        let handle = self.body_handle(id).ok_or(BodyError::UnknownBody(id))?;
        self.physics.bodies[handle].set_body_type(rapier_body_kind(kind), true);
        self.dirty_bodies.insert(id);
        self.bump_revision();
        Ok(())
    }

    pub fn apply_force(&mut self, id: BodyId, force: [f32; 3]) -> Result<(), BodyError> {
        if !finite3(force) {
            return Err(BodyError::InvalidBody("force must be finite"));
        }
        let handle = self.body_handle(id).ok_or(BodyError::UnknownBody(id))?;
        let body = &mut self.physics.bodies[handle];
        if !body.is_dynamic() {
            return Err(BodyError::InvalidOperation {
                body: id,
                operation: "force application",
            });
        }
        body.add_force(vector(force), true);
        self.bump_revision();
        Ok(())
    }

    pub fn apply_torque(&mut self, id: BodyId, torque: [f32; 3]) -> Result<(), BodyError> {
        if !finite3(torque) {
            return Err(BodyError::InvalidBody("torque must be finite"));
        }
        let handle = self.body_handle(id).ok_or(BodyError::UnknownBody(id))?;
        let body = &mut self.physics.bodies[handle];
        if !body.is_dynamic() {
            return Err(BodyError::InvalidOperation {
                body: id,
                operation: "torque application",
            });
        }
        body.add_torque(vector(torque), true);
        self.bump_revision();
        Ok(())
    }

    pub fn apply_impulse(&mut self, id: BodyId, impulse: [f32; 3]) -> Result<(), BodyError> {
        if !finite3(impulse) {
            return Err(BodyError::InvalidBody("impulse must be finite"));
        }
        let handle = self.body_handle(id).ok_or(BodyError::UnknownBody(id))?;
        let body = &mut self.physics.bodies[handle];
        if !body.is_dynamic() {
            return Err(BodyError::InvalidOperation {
                body: id,
                operation: "impulse application",
            });
        }
        body.apply_impulse(vector(impulse), true);
        self.bump_revision();
        Ok(())
    }

    pub fn apply_torque_impulse(&mut self, id: BodyId, impulse: [f32; 3]) -> Result<(), BodyError> {
        if !finite3(impulse) {
            return Err(BodyError::InvalidBody("torque impulse must be finite"));
        }
        let handle = self.body_handle(id).ok_or(BodyError::UnknownBody(id))?;
        let body = &mut self.physics.bodies[handle];
        if !body.is_dynamic() {
            return Err(BodyError::InvalidOperation {
                body: id,
                operation: "torque impulse application",
            });
        }
        body.apply_torque_impulse(vector(impulse), true);
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
        let body_handle = self
            .body_handle(collider.body())
            .ok_or(BodyError::UnknownBody(collider.body()))?;
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
        self.bump_revision();

        Ok(CharacterMove {
            requested,
            applied: array(movement.translation),
            grounded: movement.grounded,
            sliding_down_slope: movement.is_sliding_down_slope,
            collisions,
        })
    }

    /// Apply sparse edits directly to a voxel collider's acceleration
    /// structure. The collider retains its identity and material.
    pub fn edit_voxels(
        &mut self,
        collider: ColliderId,
        edits: impl IntoIterator<Item = VoxelEdit>,
    ) -> Result<VoxelEditSummary, BodyError> {
        let handle = self.collider_handle(collider)?;
        let edits: Vec<_> = edits.into_iter().collect();
        let previous_revision = self.collider_revision(collider)?;
        let shape = self.physics.colliders[handle].shape_mut();
        let voxels = shape
            .as_voxels_mut()
            .ok_or(BodyError::NotVoxelCollider(collider))?;

        let mut effective = Vec::new();
        for edit in &edits {
            let previous = voxels.set_voxel(ivector(edit.cell), edit.filled);
            if previous.is_empty() == edit.filled {
                effective.push(*edit);
            }
        }

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
        let direction = direction.map(|v| v / direction_length);
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
        self.physics.integration_parameters.dt = dt;
        self.physics.step();
        self.tick = self.tick.saturating_add(1);
        let revision = self.bump_revision();

        let current_interactions = self.collect_interactions();
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
        (slot.generation == id.generation() && slot.body.is_some()).then_some(slot)
    }

    fn body_handle(&self, id: BodyId) -> Option<RigidBodyHandle> {
        self.slot(id)?.body
    }

    fn collider_slot(&self, id: ColliderId) -> Result<(&Slot, usize), BodyError> {
        let slot = self
            .slot(id.body())
            .ok_or(BodyError::UnknownBody(id.body()))?;
        let index = id.part() as usize;
        if index >= slot.colliders.len() {
            return Err(BodyError::UnknownCollider(id));
        }
        Ok((slot, index))
    }

    fn collider_slot_mut(&mut self, id: ColliderId) -> Result<(&mut Slot, usize), BodyError> {
        let index = id.part() as usize;
        let slot = self
            .slots
            .get_mut(id.body().slot() as usize)
            .filter(|slot| slot.generation == id.body().generation() && slot.body.is_some())
            .ok_or(BodyError::UnknownBody(id.body()))?;
        if index >= slot.colliders.len() {
            return Err(BodyError::UnknownCollider(id));
        }
        Ok((slot, index))
    }

    fn collider_handle(&self, id: ColliderId) -> Result<ColliderHandle, BodyError> {
        let (slot, index) = self.collider_slot(id)?;
        Ok(slot.colliders[index])
    }

    fn collider_id(&self, handle: ColliderHandle) -> Option<ColliderId> {
        let raw = self.physics.colliders.get(handle)?.user_data;
        let body = BodyId::from_raw(raw as u64);
        let part = (raw >> 64) as u32;
        self.collider_slot(ColliderId::new(body, part))
            .ok()
            .map(|_| ColliderId::new(body, part))
    }

    fn collect_interactions(&self) -> BTreeSet<InteractionKey> {
        let mut current = BTreeSet::new();
        for pair in self.physics.contact_pairs() {
            if !pair.has_any_active_contact() {
                continue;
            }
            let Some(a) = self.collider_id(pair.collider1) else {
                continue;
            };
            let Some(b) = self.collider_id(pair.collider2) else {
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
            let Some(a) = self.collider_id(a_handle) else {
                continue;
            };
            let Some(b) = self.collider_id(b_handle) else {
                continue;
            };
            if let Some(key) = InteractionKey::new(Interaction::Sensor, a, b) {
                current.insert(key);
            }
        }
        current
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
            .map(|id| self.body_handle(id).ok_or(BodyError::UnknownBody(id)))
            .transpose()?;
        Ok(RapierQueryFilter {
            flags,
            groups: Some(interaction_groups(filter.layers)),
            exclude_rigid_body,
            ..RapierQueryFilter::default()
        })
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
