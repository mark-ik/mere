/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Edge taxonomy types — families, kinds, sub-kinds, and per-family
//! data structs.
//!
//! Extracted from `graph/mod.rs` per the 2026-05-11 kernel-mod
//! decomposition pass (memory: feedback_mere_file_size_ceiling). The
//! [`EdgePayload`](super::EdgePayload) impl in `edge_payload.rs` is
//! the primary consumer; `Graph` methods in `mod.rs` reach for these
//! types via the parent module re-exports.

use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use rkyv::{Archive, Deserialize, Serialize};

/// Type of edge connection
#[derive(Debug, Clone, Copy, PartialEq, Archive, Serialize, Deserialize)]
pub enum EdgeType {
    /// Hyperlink from one page to another
    Hyperlink,

    /// Browser history traversal
    History,

    /// Explicit user grouping association
    UserGrouped,

    /// Workbench/layout arrangement relation.
    ArrangementRelation(ArrangementSubKind),

    /// URL-derived containment hierarchy relation.
    ContainmentRelation(ContainmentSubKind),

    /// Relation imported from an external system (bookmarks folder, RSS feed, etc.).
    /// Derived-readonly at import time; promoted to durable only by explicit user action.
    ImportedRelation,

    /// Agent-inferred relation; provisional until accepted or evicted by decay.
    /// `decay_progress` is in [0.0, 1.0] — 0.0 = freshly asserted, 1.0 = at eviction threshold.
    AgentDerived { decay_progress: f32 },
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(compare(PartialEq, PartialOrd), derive(PartialEq, Eq, PartialOrd, Ord))]
pub enum EdgeFamily {
    Semantic,
    Traversal,
    Containment,
    Arrangement,
    Imported,
    Provenance,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(compare(PartialEq, PartialOrd), derive(PartialEq, Eq, PartialOrd, Ord))]
pub enum SemanticSubKind {
    Hyperlink,
    UserGrouped,
    AgentDerived,
    Cites,
    Quotes,
    Summarizes,
    Elaborates,
    ExampleOf,
    Supports,
    Contradicts,
    Questions,
    SameEntityAs,
    DuplicateOf,
    CanonicalMirrorOf,
    DependsOn,
    Blocks,
    NextStep,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(compare(PartialEq, PartialOrd), derive(PartialEq, Eq, PartialOrd, Ord))]
pub enum ArrangementSubKind {
    FrameMember,
    TileGroup,
    SplitPair,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(compare(PartialEq, PartialOrd), derive(PartialEq, Eq, PartialOrd, Ord))]
pub enum ContainmentSubKind {
    UrlPath,
    Domain,
    FileSystem,
    UserFolder,
    ClipSource,
    NotebookSection,
    CollectionMember,
}

impl ContainmentSubKind {
    pub fn as_tag(self) -> &'static str {
        match self {
            Self::UrlPath => "url-path",
            Self::Domain => "domain",
            Self::FileSystem => "filesystem",
            Self::UserFolder => "user-folder",
            Self::ClipSource => "clip-source",
            Self::NotebookSection => "notebook-section",
            Self::CollectionMember => "collection-member",
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(compare(PartialEq, PartialOrd), derive(PartialEq, Eq, PartialOrd, Ord))]
pub enum ImportedSubKind {
    BookmarkFolder,
    HistoryImport,
    SessionImport,
    RssMembership,
    FileSystemImport,
    ArchiveMembership,
    SharedCollection,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(compare(PartialEq, PartialOrd), derive(PartialEq, Eq, PartialOrd, Ord))]
pub enum ProvenanceSubKind {
    ClippedFrom,
    ExcerptedFrom,
    SummarizedFrom,
    TranslatedFrom,
    RewrittenFrom,
    GeneratedFrom,
    ExtractedFrom,
    ImportedFromSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub enum RelationDurability {
    Durable,
    Session,
}

impl RelationDurability {
    pub fn as_tag(self) -> &'static str {
        match self {
            Self::Durable => "durable",
            Self::Session => "session",
        }
    }
}

impl ArrangementSubKind {
    pub fn as_tag(self) -> &'static str {
        match self {
            Self::FrameMember => "frame-member",
            Self::TileGroup => "tile-group",
            Self::SplitPair => "split-pair",
        }
    }

    pub fn durability(self) -> RelationDurability {
        match self {
            Self::FrameMember => RelationDurability::Durable,
            Self::TileGroup | Self::SplitPair => RelationDurability::Session,
        }
    }

    pub fn provenance(self) -> &'static str {
        match self {
            Self::FrameMember => "workbench.frame_snapshot",
            Self::TileGroup => "workbench.tile_grouping",
            Self::SplitPair => "workbench.split_pairing",
        }
    }
}

/// Canonical edge kind set entry — internal index tag inside [`EdgePayload`].
/// Callers outside this module should use [`EdgeType`] with [`EdgePayload::has_edge_type`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Archive, Serialize, Deserialize)]
#[rkyv(compare(PartialEq, PartialOrd), derive(PartialEq, Eq, PartialOrd, Ord))]
pub enum EdgeKind {
    SemanticRelation,
    Hyperlink,
    TraversalDerived,
    UserGrouped,
    AgentDerived,
    ArrangementRelation,
    ContainmentRelation,
    ImportedRelation,
}

#[derive(Debug, Clone, PartialEq, Archive, Serialize, Deserialize)]
pub enum EdgeAssertion {
    Semantic {
        sub_kind: SemanticSubKind,
        label: Option<String>,
        decay_progress: Option<f32>,
    },
    Containment {
        sub_kind: ContainmentSubKind,
    },
    Arrangement {
        sub_kind: ArrangementSubKind,
    },
    Imported {
        sub_kind: ImportedSubKind,
    },
    Provenance {
        sub_kind: ProvenanceSubKind,
    },
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum RelationSelector {
    Family(EdgeFamily),
    Semantic(SemanticSubKind),
    Containment(ContainmentSubKind),
    Arrangement(ArrangementSubKind),
    Imported(ImportedSubKind),
    Provenance(ProvenanceSubKind),
}

/// Trigger classification for a traversal event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub enum NavigationTrigger {
    Unknown,
    LinkClick,
    Back,
    Forward,
    AddressBarEntry,
    PanePromotion,
    Programmatic,
}

impl NavigationTrigger {
    fn contributes_to_forward_count(self) -> bool {
        !matches!(self, Self::Back)
    }
}

/// A temporal traversal event recorded on an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub struct Traversal {
    pub timestamp_ms: u64,
    pub trigger: NavigationTrigger,
}

impl Traversal {
    pub fn now(trigger: NavigationTrigger) -> Self {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            timestamp_ms,
            trigger,
        }
    }
}

/// Durable traversal aggregates retained even when rolling-window records are evicted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub struct EdgeMetrics {
    pub total_navigations: u64,
    pub forward_navigations: u64,
    pub backward_navigations: u64,
    pub last_navigated_at: Option<u64>,
}

impl EdgeMetrics {
    fn new() -> Self {
        Self {
            total_navigations: 0,
            forward_navigations: 0,
            backward_navigations: 0,
            last_navigated_at: None,
        }
    }

    fn record(&mut self, traversal: Traversal) {
        self.total_navigations = self.total_navigations.saturating_add(1);
        if traversal.trigger.contributes_to_forward_count() {
            self.forward_navigations = self.forward_navigations.saturating_add(1);
        } else {
            self.backward_navigations = self.backward_navigations.saturating_add(1);
        }
        self.last_navigated_at = Some(traversal.timestamp_ms);
    }
}

impl Default for EdgeMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct UserGroupedData {
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct SemanticData {
    pub sub_kinds: BTreeSet<SemanticSubKind>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct TraversalData {
    pub traversals: Vec<Traversal>,
    pub metrics: EdgeMetrics,
}

impl TraversalData {
    pub(crate) fn push(&mut self, traversal: Traversal) {
        self.metrics.record(traversal);
        self.traversals.push(traversal);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct ArrangementData {
    pub sub_kinds: BTreeSet<ArrangementSubKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct ContainmentData {
    pub sub_kinds: BTreeSet<ContainmentSubKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct ImportedData {
    pub sub_kinds: BTreeSet<ImportedSubKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct ProvenanceData {
    pub sub_kinds: BTreeSet<ProvenanceSubKind>,
}

impl ContainmentData {
    pub(crate) fn insert(&mut self, sub_kind: ContainmentSubKind) -> bool {
        self.sub_kinds.insert(sub_kind)
    }

    pub(crate) fn remove(&mut self, sub_kind: ContainmentSubKind) -> bool {
        self.sub_kinds.remove(&sub_kind)
    }

    pub(crate) fn contains(&self, sub_kind: ContainmentSubKind) -> bool {
        self.sub_kinds.contains(&sub_kind)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sub_kinds.is_empty()
    }
}

impl ArrangementData {
    pub(crate) fn insert(&mut self, sub_kind: ArrangementSubKind) -> bool {
        self.sub_kinds.insert(sub_kind)
    }

    pub(crate) fn remove(&mut self, sub_kind: ArrangementSubKind) -> bool {
        self.sub_kinds.remove(&sub_kind)
    }

    pub(crate) fn contains(&self, sub_kind: ArrangementSubKind) -> bool {
        self.sub_kinds.contains(&sub_kind)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sub_kinds.is_empty()
    }

    pub(crate) fn has_durable_relation(&self) -> bool {
        self.sub_kinds
            .iter()
            .copied()
            .any(|sub_kind| sub_kind.durability() == RelationDurability::Durable)
    }

    pub(crate) fn has_session_relation(&self) -> bool {
        self.sub_kinds
            .iter()
            .copied()
            .any(|sub_kind| sub_kind.durability() == RelationDurability::Session)
    }
}
