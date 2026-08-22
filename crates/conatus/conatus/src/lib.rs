//! The shared spatial game-engine core.
//!
//! Conatus owns the game-facing vocabulary for bodies, collision, spatial
//! queries, fixed stepping, and frame changes. The current tactile backend is
//! Rapier 3D, kept private so games do not acquire Rapier handles or math
//! types. A later Nexus or resident-GPU backend can replace that machinery
//! behind the same body and frame vocabulary.
//!
//! This crate owns simulation state. Renderers consume [`BodyState`] changes;
//! they do not own transforms. Game records decide which simulation outcomes
//! become durable facts.

mod body;
mod clock;
mod command;
mod engine;
mod schedule;
mod world;

pub use body::{
    BodyDesc, BodyId, BodyKind, BodyState, CharacterAutostep, CharacterCollision, CharacterConfig,
    CharacterMove, ColliderDesc, ColliderId, ColliderShape, CollisionLayers, Material,
    SpatialFilter, Transform, Velocity, VoxelChange, VoxelEdit,
};
pub use clock::{ClockAdvance, ClockError, FixedClock};
pub use command::{BodyCommand, CommandEffect, CommandId, CommandResult};
pub use engine::{Engine, EngineConfig, EngineConfigError, EngineError, FrameUpdate};
pub use schedule::{Phase, Resources, SystemContext, SystemError};
pub use world::{
    BodyError, BodyWorld, Interaction, InteractionEvent, InteractionState, RayHit, StepUpdate,
    VoxelEditSummary,
};
