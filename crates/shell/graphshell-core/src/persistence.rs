/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Serializable types for graph persistence.
//!
//! These types define the portable snapshot and edge schema shared by all hosts.
//! Storage adapters (fjall, IndexedDB, etc.) live in the host, not here.

use rkyv::{Archive, Deserialize, Serialize};

use crate::graph::NodeNavigationMemory;
use crate::types::{
    FrameLayoutHint, ImportRecord, NodeClassification, NodeImportProvenance,
    NodeTagPresentationState,
};

// ---------------------------------------------------------------------------
// Address persistence types
// ---------------------------------------------------------------------------

/// Address type hint for persistence (mirrors `AddressKind` in the graph model).
///
/// Deprecated: superseded by [`PersistedAddress`]. Kept for rkyv backward compatibility
/// with old snapshots. No new values are written.
#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum PersistedAddressKind {
    #[default]
    Http,
    File,
    Data,
    GraphshellClip,
    Directory,
    Unknown,
}

/// Typed address for persistence — carries both the URL scheme classification
/// and the raw URL string.
///
/// All variants store the full URL string so that [`PersistedAddress::as_url_str`]
/// is always a round-trip identity.
#[derive(
    Archive, Serialize, Deserialize, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum PersistedAddress {
    Http(String),
    File(String),
    Data(String),
    /// Clip route (`verso://clip/<id>` or `graphshell://clip/<id>`). Stores the full URL.
    Clip(String),
    Directory(String),
    Custom(String),
}

impl Default for PersistedAddress {
    /// Fallback used when deserializing old snapshots that lack the `address` field.
    /// The load path detects the empty URL and uses the legacy `url` field instead.
    fn default() -> Self {
        PersistedAddress::Custom(String::new())
    }
}

impl PersistedAddress {
    /// Return the raw URL string for this address.
    pub fn as_url_str(&self) -> &str {
        match self {
            PersistedAddress::Http(s)
            | PersistedAddress::File(s)
            | PersistedAddress::Data(s)
            | PersistedAddress::Clip(s)
            | PersistedAddress::Directory(s)
            | PersistedAddress::Custom(s) => s.as_str(),
        }
    }
}

impl ArchivedPersistedAddress {
    /// Return the raw URL string from the archived address.
    pub fn as_url_str(&self) -> &str {
        match self {
            ArchivedPersistedAddress::Http(s)
            | ArchivedPersistedAddress::File(s)
            | ArchivedPersistedAddress::Data(s)
            | ArchivedPersistedAddress::Clip(s)
            | ArchivedPersistedAddress::Directory(s)
            | ArchivedPersistedAddress::Custom(s) => s.as_str(),
        }
    }
}

// ---------------------------------------------------------------------------
// Node persistence types
// ---------------------------------------------------------------------------

/// Persisted per-node session fidelity state.
#[derive(Archive, Serialize, Deserialize, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PersistedNodeSessionState {
    pub scroll_x: Option<f32>,
    pub scroll_y: Option<f32>,
    pub form_draft: Option<String>,
}

/// Persisted node.
#[derive(Archive, Serialize, Deserialize, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PersistedNode {
    /// Stable node identity.
    pub node_id: String,

    /// Typed address — canonical source of the node URL since Stage C.2.
    #[serde(default)]
    pub address: PersistedAddress,

    /// Legacy URL field — written alongside `address` for backward compatibility.
    #[serde(default)]
    pub url: String,

    #[serde(default)]
    pub cached_host: Option<String>,
    pub title: String,
    pub position_x: f32,
    pub position_y: f32,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub tag_presentation: NodeTagPresentationState,
    #[serde(default)]
    pub import_provenance: Vec<NodeImportProvenance>,
    pub is_pinned: bool,
    #[serde(default)]
    pub navigation_memory: NodeNavigationMemory,
    pub thumbnail_png: Option<Vec<u8>>,
    pub thumbnail_width: u32,
    pub thumbnail_height: u32,
    pub favicon_rgba: Option<Vec<u8>>,
    pub favicon_width: u32,
    pub favicon_height: u32,
    pub session_state: Option<PersistedNodeSessionState>,
    /// Optional MIME type hint; drives renderer selection.
    pub mime_hint: Option<String>,
    /// Durable provenance-bearing classification records (Stage A enrichment).
    #[serde(default)]
    pub classifications: Vec<NodeClassification>,
    /// Durable split arrangement annotations for frame-anchor nodes.
    #[serde(default)]
    pub frame_layout_hints: Vec<FrameLayoutHint>,
    /// Durable split-offer suppression for frame-anchor nodes.
    #[serde(default)]
    pub frame_split_offer_suppressed: bool,
}

// ---------------------------------------------------------------------------
// Edge persistence types
// ---------------------------------------------------------------------------

#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum PersistedEdgeFamily {
    Semantic,
    Traversal,
    Containment,
    Arrangement,
    Imported,
    Provenance,
}

#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum PersistedSemanticSubKind {
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
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum PersistedContainmentSubKind {
    UrlPath,
    Domain,
    FileSystem,
    UserFolder,
    ClipSource,
    NotebookSection,
    CollectionMember,
}

#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum PersistedArrangementSubKind {
    FrameMember,
    TileGroup,
    SplitPair,
    TabNeighbor,
    ActiveTab,
    PinnedInFrame,
}

#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum PersistedImportedSubKind {
    BookmarkFolder,
    HistoryImport,
    SessionImport,
    RssMembership,
    FileSystemImport,
    ArchiveMembership,
    SharedCollection,
}

#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum PersistedProvenanceSubKind {
    ClippedFrom,
    ExcerptedFrom,
    SummarizedFrom,
    TranslatedFrom,
    RewrittenFrom,
    GeneratedFrom,
    ExtractedFrom,
    ImportedFromSource,
}

#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Default,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub struct PersistedSemanticEdgeData {
    #[serde(default)]
    pub sub_kinds: Vec<PersistedSemanticSubKind>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub agent_decay_progress: Option<f32>,
}

#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub struct PersistedTraversalRecord {
    pub timestamp_ms: u64,
    pub trigger: PersistedNavigationTrigger,
}

#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub struct PersistedTraversalMetrics {
    pub total_navigations: u64,
    pub forward_navigations: u64,
    pub backward_navigations: u64,
    pub last_navigated_at: Option<u64>,
}

#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub struct PersistedTraversalEdgeData {
    #[serde(default)]
    pub traversals: Vec<PersistedTraversalRecord>,
    #[serde(default)]
    pub metrics: PersistedTraversalMetrics,
}

#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub struct PersistedContainmentEdgeData {
    #[serde(default)]
    pub sub_kinds: Vec<PersistedContainmentSubKind>,
}

#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub struct PersistedArrangementEdgeData {
    #[serde(default)]
    pub sub_kinds: Vec<PersistedArrangementSubKind>,
}

#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub struct PersistedImportedEdgeData {
    #[serde(default)]
    pub sub_kinds: Vec<PersistedImportedSubKind>,
}

#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub struct PersistedProvenanceEdgeData {
    #[serde(default)]
    pub sub_kinds: Vec<PersistedProvenanceSubKind>,
}

#[derive(
    Archive, Serialize, Deserialize, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum PersistedEdgeAssertion {
    Semantic {
        sub_kind: PersistedSemanticSubKind,
        label: Option<String>,
        agent_decay_progress: Option<f32>,
    },
    Containment {
        sub_kind: PersistedContainmentSubKind,
    },
    Arrangement {
        sub_kind: PersistedArrangementSubKind,
    },
    Imported {
        sub_kind: PersistedImportedSubKind,
    },
    Provenance {
        sub_kind: PersistedProvenanceSubKind,
    },
}

#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum PersistedRelationSelector {
    Family(PersistedEdgeFamily),
    Semantic(PersistedSemanticSubKind),
    Containment(PersistedContainmentSubKind),
    Arrangement(PersistedArrangementSubKind),
    Imported(PersistedImportedSubKind),
    Provenance(PersistedProvenanceSubKind),
}

/// Persisted traversal trigger classification.
#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum PersistedNavigationTrigger {
    Unknown,
    LinkClick,
    Back,
    Forward,
    AddressBarEntry,
    PanePromotion,
    Programmatic,
}

/// Persisted edge.
#[derive(Archive, Serialize, Deserialize, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PersistedEdge {
    pub from_node_id: String,
    pub to_node_id: String,
    #[serde(default)]
    pub families: Vec<PersistedEdgeFamily>,
    #[serde(default)]
    pub semantic: Option<PersistedSemanticEdgeData>,
    #[serde(default)]
    pub traversal: Option<PersistedTraversalEdgeData>,
    #[serde(default)]
    pub containment: Option<PersistedContainmentEdgeData>,
    #[serde(default)]
    pub arrangement: Option<PersistedArrangementEdgeData>,
    #[serde(default)]
    pub imported: Option<PersistedImportedEdgeData>,
    #[serde(default)]
    pub provenance: Option<PersistedProvenanceEdgeData>,
}

/// Full graph snapshot for periodic saves.
#[derive(Archive, Serialize, Deserialize, Clone, Debug)]
pub struct GraphSnapshot {
    pub nodes: Vec<PersistedNode>,
    pub edges: Vec<PersistedEdge>,
    pub import_records: Vec<ImportRecord>,
    pub timestamp_secs: u64,
}

// ---------------------------------------------------------------------------
// Node audit event taxonomy (Slice 63 — promoted from
// `services/persistence/types.rs`)
// ---------------------------------------------------------------------------

/// The kind of node metadata or lifecycle event recorded in an audit log entry.
///
/// Each variant carries only the new value (not the old one). The sequence of
/// audit events in the WAL provides the full history; diffing adjacent entries
/// to recover the "from" value is a query-time operation.
///
/// Promoted to graphshell-core in Slice 63 so it can travel with the rest
/// of the persistence type taxonomy and be referenced by the (eventually
/// portable) `GraphMutation` / `GraphIntent` enums without needing the
/// shell-side `services/persistence` module in scope.
#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug))]
pub enum NodeAuditEventKind {
    /// Node title was changed. Records the new title.
    TitleChanged { new_title: String },
    /// A tag was added to the node.
    Tagged { tag: String },
    /// A tag was removed from the node.
    Untagged { tag: String },
    /// Node was pinned.
    Pinned,
    /// Node was unpinned.
    Unpinned,
    /// Node URL was changed out-of-band (not via NavigateNode navigation).
    /// Used when a node's URL is set directly rather than through navigation.
    UrlChanged { new_url: String },
    /// A viewer or workflow recorded a notable node-scoped action.
    ActionRecorded { action: String, detail: String },
    /// Node was tombstoned (soft-deleted).
    Tombstoned,
    /// Node was restored from tombstone state.
    Restored,
}
