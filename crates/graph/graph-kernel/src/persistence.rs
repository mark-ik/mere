/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Serializable types for graph persistence.
//!
//! These types define the portable snapshot and edge schema shared by all hosts.
//! Storage adapters (fjall, IndexedDB, etc.) live in the host, not here.

use rkyv::{Archive, Deserialize, Serialize};

// Field-layer DTOs live in their own file (per-file ceiling) but belong to this
// module's vocabulary; re-export so `crate::persistence::PersistedField` resolves.
pub use crate::persistence_fields::{
    PersistedCoupling, PersistedCouplingResponse, PersistedField, PersistedFieldExtent,
    PersistedFieldLifecycle, PersistedNodeSelector,
};

// Edge persistence types live in their own file (per-file ceiling); re-exported
// here so `crate::persistence::PersistedEdge` etc. continue to resolve.
pub use crate::persistence_edge::{
    PersistedArrangementEdgeData, PersistedArrangementSubKind, PersistedContainmentEdgeData,
    PersistedContainmentSubKind, PersistedEdge, PersistedEdgeAssertion, PersistedEdgeFamily,
    PersistedImportedEdgeData, PersistedImportedSubKind, PersistedNavigationTrigger,
    PersistedProvenanceEdgeData, PersistedProvenanceSubKind, PersistedRelationSelector,
    PersistedSemanticEdgeData, PersistedSemanticStatement, PersistedSemanticSubKind,
    PersistedTraversalEdgeData, PersistedTraversalMetrics, PersistedTraversalRecord,
};

use crate::graph::SharedNavigationMemory;
use crate::types::{
    FrameLayoutHint, ImportRecord, NodeClassification, NodeDerivation, NodeImportProvenance,
    NodeProperty, NodeTagPresentationState,
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
    #[serde(default)]
    pub last_visited_ms: Option<u64>,
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
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub tag_presentation: NodeTagPresentationState,
    #[serde(default)]
    pub import_provenance: Vec<NodeImportProvenance>,
    pub is_pinned: bool,
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
    /// Open literal properties (non-curated literals preserved on ingest).
    #[serde(default)]
    pub properties: Vec<NodeProperty>,
    /// Cross-graph derivation provenance (copied/forked from another graph).
    #[serde(default)]
    pub derivations: Vec<NodeDerivation>,
    /// Inline authored content body — a knot note's djot source. `#[serde(default)]`
    /// so snapshots written before the inline body load with `None`. (Djot editor
    /// reframe, slice 3.)
    #[serde(default)]
    pub body: Option<String>,
    /// The app-launch session number this node was last navigated in. `0` (the
    /// `#[serde(default)]` for snapshots written before this field existed) means
    /// "never stamped" — by-sessions eviction treats that as undated, never evicted.
    /// (Alembic B5.)
    #[serde(default)]
    pub last_session_visited: u64,
}

/// Full graph snapshot for periodic saves.
///
/// Carries both `rkyv` derives (for compact binary persistence —
/// the original intent) and `serde` derives (added 2026-05-11 so
/// `host-runtime::session_graph_store` can persist sessions
/// as hand-inspectable JSON). All sub-types (`PersistedNode`,
/// `PersistedEdge`, `ImportRecord`) already carry both.
#[derive(Archive, Serialize, Deserialize, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GraphSnapshot {
    pub nodes: Vec<PersistedNode>,
    pub edges: Vec<PersistedEdge>,
    pub import_records: Vec<ImportRecord>,
    pub timestamp_secs: u64,
    /// Field-layer truth (field-system Phase 2). `#[serde(default)]` so snapshots
    /// written before the field layer load with an empty one.
    #[serde(default)]
    pub fields: Vec<PersistedField>,
    #[serde(default)]
    pub couplings: Vec<PersistedCoupling>,
    /// The graph's shared navigation history (one visit space, owner per node —
    /// the (b) anchor design). `#[serde(default)]` so snapshots written before the
    /// shared-history migration (per-node `navigation_memory`, now ignored) load
    /// with an empty history rather than failing.
    #[serde(default)]
    pub navigation: SharedNavigationMemory,
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
/// Promoted to kernel in Slice 63 so it can travel with the rest
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
