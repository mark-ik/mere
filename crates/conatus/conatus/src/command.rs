use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::{
    BodyDesc, BodyError, BodyId, BodyKind, BodyState, BodyWorld, CharacterConfig, CharacterMove,
    ColliderId, Transform, Velocity, VoxelEdit, VoxelEditSummary,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommandId(u64);

impl CommandId {
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Structural and tactile operations systems can defer to a phase boundary.
/// This is also the command vocabulary exposed to scripts and remote inputs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum BodyCommand {
    Spawn {
        body: BodyDesc,
    },
    Despawn {
        body: BodyId,
    },
    SetTransform {
        body: BodyId,
        transform: Transform,
        wake: bool,
    },
    SetNextKinematicTransform {
        body: BodyId,
        transform: Transform,
    },
    SetVelocity {
        body: BodyId,
        velocity: Velocity,
        wake: bool,
    },
    SetKind {
        body: BodyId,
        kind: BodyKind,
    },
    SetGravity {
        gravity: [f32; 3],
    },
    ApplyForce {
        body: BodyId,
        force: [f32; 3],
    },
    ApplyTorque {
        body: BodyId,
        torque: [f32; 3],
    },
    ApplyImpulse {
        body: BodyId,
        impulse: [f32; 3],
    },
    ApplyTorqueImpulse {
        body: BodyId,
        impulse: [f32; 3],
    },
    MoveCharacter {
        collider: ColliderId,
        requested: [f32; 3],
        config: CharacterConfig,
    },
    EditVoxels {
        collider: ColliderId,
        edits: Vec<VoxelEdit>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandEffect {
    Applied,
    Spawned(BodyId),
    Despawned(BodyState),
    CharacterMoved(CharacterMove),
    VoxelsEdited(VoxelEditSummary),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandResult {
    pub id: CommandId,
    pub result: Result<CommandEffect, BodyError>,
}

#[derive(Default)]
pub(crate) struct CommandQueue {
    next: u64,
    queued: VecDeque<(CommandId, BodyCommand)>,
}

impl CommandQueue {
    pub(crate) fn push(&mut self, command: BodyCommand) -> CommandId {
        let id = CommandId(self.next);
        self.next = self.next.saturating_add(1);
        self.queued.push_back((id, command));
        id
    }

    pub(crate) fn len(&self) -> usize {
        self.queued.len()
    }

    pub(crate) fn drain_apply(&mut self, bodies: &mut BodyWorld, dt: f32) -> Vec<CommandResult> {
        self.queued
            .drain(..)
            .map(|(id, command)| CommandResult {
                id,
                result: apply(command, bodies, dt),
            })
            .collect()
    }
}

fn apply(
    command: BodyCommand,
    bodies: &mut BodyWorld,
    dt: f32,
) -> Result<CommandEffect, BodyError> {
    match command {
        BodyCommand::Spawn { body } => bodies.spawn(body).map(CommandEffect::Spawned),
        BodyCommand::Despawn { body } => bodies.despawn(body).map(CommandEffect::Despawned),
        BodyCommand::SetTransform {
            body,
            transform,
            wake,
        } => bodies
            .set_transform(body, transform, wake)
            .map(|()| CommandEffect::Applied),
        BodyCommand::SetNextKinematicTransform { body, transform } => bodies
            .set_next_kinematic_transform(body, transform)
            .map(|()| CommandEffect::Applied),
        BodyCommand::SetVelocity {
            body,
            velocity,
            wake,
        } => bodies
            .set_velocity(body, velocity, wake)
            .map(|()| CommandEffect::Applied),
        BodyCommand::SetKind { body, kind } => {
            bodies.set_kind(body, kind).map(|()| CommandEffect::Applied)
        }
        BodyCommand::SetGravity { gravity } => {
            bodies.set_gravity(gravity).map(|()| CommandEffect::Applied)
        }
        BodyCommand::ApplyForce { body, force } => bodies
            .apply_force(body, force)
            .map(|()| CommandEffect::Applied),
        BodyCommand::ApplyTorque { body, torque } => bodies
            .apply_torque(body, torque)
            .map(|()| CommandEffect::Applied),
        BodyCommand::ApplyImpulse { body, impulse } => bodies
            .apply_impulse(body, impulse)
            .map(|()| CommandEffect::Applied),
        BodyCommand::ApplyTorqueImpulse { body, impulse } => bodies
            .apply_torque_impulse(body, impulse)
            .map(|()| CommandEffect::Applied),
        BodyCommand::MoveCharacter {
            collider,
            requested,
            config,
        } => bodies
            .move_character(collider, requested, dt, config)
            .map(CommandEffect::CharacterMoved),
        BodyCommand::EditVoxels { collider, edits } => bodies
            .edit_voxels(collider, edits)
            .map(CommandEffect::VoxelsEdited),
    }
}
