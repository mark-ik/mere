use serde::{Deserialize, Serialize};

/// A stable-index lifetime. Compaction or incompatible rebuilding starts a
/// fresh epoch and sends a complete snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SceneEpoch(pub u64);

/// A monotonically increasing state revision within one scene epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Revision(pub u64);

/// Stable backdrop-table index within one epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BackdropId(pub u32);

/// Stable relation-table index within one epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RelationId(pub u32);

/// Stable region-table index within one epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RegionId(pub u32);
