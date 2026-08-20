//! Scenotime: the runtime vocabulary for a scene through time.
//!
//! A one-shot [`sceno::Scene`] is dense. Scenotime turns it into an epoch-scoped
//! slot table whose indexes are never reused, applies revision-based diffs
//! transactionally, and preserves tombstones in resynchronization snapshots.
//! Networking, presentation resources, and product authority remain outside.

mod diff;
mod ids;
mod pick;
mod snapshot;
mod transition;

pub use diff::{ApplyOutcome, DiffError, SceneDiff, SceneOp};
pub use ids::{RegionId, RelationId, Revision, SceneEpoch};
pub use snapshot::{SceneSnapshot, SceneTables, SnapshotError};
pub use transition::{
    ScheduledItem, TransitionClass, TransitionEasing, TransitionError, TransitionFrame,
    TransitionSample, TransitionSchedule, TransitionSpec, TransitionStage, TransitionValue,
};
