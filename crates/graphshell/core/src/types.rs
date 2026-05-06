/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable leaf types used by the graph model and persistence schema.
//!
//! These types are shared between `graphshell-core` modules and must be
//! WASM-clean: no platform I/O, no UI framework dependencies.

use std::collections::HashMap;

use rkyv::{Archive, Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Frame layout types
// ---------------------------------------------------------------------------

/// Durable member reference used by frame layout hints.
///
/// `NodeKey` is process-local and not stable across restart, so persistent frame
/// layout metadata uses the member node's stable UUID string instead.
pub type FrameLayoutNodeId = String;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq, Eq))]
pub enum SplitOrientation {
    Vertical,
    Horizontal,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq, Eq))]
pub enum DominantEdge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq, Eq))]
pub enum FrameLayoutHint {
    SplitHalf {
        first: FrameLayoutNodeId,
        second: FrameLayoutNodeId,
        orientation: SplitOrientation,
    },
    SplitPamphlet {
        members: [FrameLayoutNodeId; 3],
        orientation: SplitOrientation,
    },
    SplitTriptych {
        dominant: FrameLayoutNodeId,
        dominant_edge: DominantEdge,
        wings: [FrameLayoutNodeId; 2],
    },
    SplitQuartered {
        top_left: FrameLayoutNodeId,
        top_right: FrameLayoutNodeId,
        bottom_left: FrameLayoutNodeId,
        bottom_right: FrameLayoutNodeId,
    },
}

// ---------------------------------------------------------------------------
// Import provenance types
// ---------------------------------------------------------------------------

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct NodeImportProvenance {
    pub source_id: String,
    pub source_label: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct ImportRecordMembership {
    pub node_id: String,
    pub suppressed: bool,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct ImportRecord {
    pub record_id: String,
    pub source_id: String,
    pub source_label: String,
    pub imported_at_secs: u64,
    pub memberships: Vec<ImportRecordMembership>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeImportRecordSummary {
    pub record_id: String,
    pub source_id: String,
    pub source_label: String,
    pub imported_at_secs: u64,
}

pub fn format_imported_at_secs(imported_at_secs: u64) -> String {
    time::OffsetDateTime::from_unix_timestamp(imported_at_secs as i64)
        .ok()
        .and_then(|timestamp| {
            timestamp
                .format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_else(|| format!("{}s", imported_at_secs))
}

// ---------------------------------------------------------------------------
// Node classification — Stage A durable enrichment schema
// ---------------------------------------------------------------------------

/// Classification scheme identifier.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum ClassificationScheme {
    /// Universal Decimal Classification (primary semantic taxonomy).
    #[default]
    Udc,
    /// Content-kind classification (page, article, repo, …).
    ContentKind,
    /// Custom namespaced scheme (e.g. `"myns:custom"`).
    Custom(String),
}

/// Origin of a classification or tag.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum ClassificationProvenance {
    /// Explicitly authored by the user.
    #[default]
    UserAuthored,
    /// Imported from an external data source (bookmarks, history, file, …).
    Imported,
    /// Inherited from a source/parent node relationship.
    InheritedFromSource,
    /// Derived by the knowledge registry (UDC lookup, content analysis, …).
    RegistryDerived,
    /// Proposed by an agent/model; not yet accepted by the user.
    AgentSuggested,
    /// Synced from the community/Verse network.
    CommunitySynced,
}

/// Lifecycle status of a classification record.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum ClassificationStatus {
    /// User has explicitly accepted this classification.
    Accepted,
    /// Proposed but not yet reviewed (e.g. agent-suggested).
    #[default]
    Suggested,
    /// User has explicitly rejected this classification.
    Rejected,
    /// Verified by an authoritative external source.
    Verified,
    /// Imported from an external record without explicit user review.
    Imported,
}

/// A single provenance-bearing classification record on a node.
///
/// Multiple records can coexist; at most one should have `primary: true` per scheme.
#[derive(
    Debug, Clone, PartialEq, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize,
)]
pub struct NodeClassification {
    pub scheme: ClassificationScheme,
    /// Scheme-specific classification value (e.g. `"udc:519.6"`, `"article"`).
    pub value: String,
    /// Human-readable label resolved from the scheme (e.g. `"Computational mathematics"`).
    pub label: Option<String>,
    /// Confidence score in `[0.0, 1.0]`; `1.0` for user-authored.
    pub confidence: f32,
    pub provenance: ClassificationProvenance,
    pub status: ClassificationStatus,
    /// Whether this is the primary presentation classification for its scheme.
    pub primary: bool,
}

// ---------------------------------------------------------------------------
// Badge / tag presentation types (from badge.rs carve-out)
// ---------------------------------------------------------------------------

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq, Eq))]
pub enum BadgeIcon {
    Emoji(String),
    Lucide(String),
    None,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq, Eq))]
pub struct NodeTagPresentationState {
    pub ordered_tags: Vec<String>,
    pub icon_overrides: HashMap<String, BadgeIcon>,
}
