//! A shared spatial runtime.
//!
//! Conatus owns the product-facing vocabulary for bodies, collision, spatial
//! queries, fixed stepping, and spatial changes. Product runtime profiles
//! decide when to advance it and which other organs participate. The current
//! tactile backend is Rapier 3D, kept private so products do not acquire
//! Rapier handles or math types. A later Nexus or resident-GPU backend can
//! replace that machinery behind the same body vocabulary.
//!
//! This crate owns the spatial state it advances. Renderers consume
//! [`BodyState`] changes; they do not own transforms. Product records decide
//! which spatial outcomes become durable facts.

mod body;
mod clock;
mod command;
mod engine;
mod schedule;
mod voxel;
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
pub use voxel::{
    VoxelAddress, VoxelCellChange, VoxelCellEdit, VoxelChunk, VoxelChunkError, VoxelPatch,
    VoxelRegion, split_voxel_address,
};
pub use world::{
    BodyError, BodyWorld, Interaction, InteractionEvent, InteractionState, RayHit, StepUpdate,
    VoxelEditSummary,
};
