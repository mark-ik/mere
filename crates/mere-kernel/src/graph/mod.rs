/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Graph data structures for the spatial browser.
//!
//! Core structures:
//! - `Graph`: Main graph container backed by petgraph::StableGraph
//! - `Node`: Webpage node with position, velocity, and metadata
//! - `EdgePayload`: Edge semantics and traversal events between nodes
//!
//! This module is WASM-clean: it must compile to `wasm32-unknown-unknown`.

use euclid::default::{Point2D, Vector2D};
// `MemoryEntryPrivacy` / `OwnerScopedMemory` / `GraphMemorySnapshot` /
// `MemoryTransitionKind` were used by the node-history types that
// moved to `history.rs` (2026-05-11 decomposition pass); `Graph` itself
// doesn't reach into node-lineage directly.
use petgraph::algo::{astar, dijkstra, has_path_connecting, kosaraju_scc};
#[cfg(test)]
use petgraph::stable_graph::NodeIndex;
use petgraph::stable_graph::StableGraph;
use petgraph::visit::UndirectedAdaptor;
use petgraph::visit::{EdgeRef, IntoEdgeReferences};
use petgraph::{Directed, Direction};
use rkyv::{Archive, Archived, Deserialize, Place, Resolver, Serialize, rancor::Fallible};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub use node_lineage::TransitionKind as NodeHistoryTransitionKind;

use crate::address::{Address, AddressKind, address_from_url, cached_host_from_url, detect_mime};
use crate::persistence::{
    GraphSnapshot, PersistedAddress, PersistedArrangementEdgeData, PersistedArrangementSubKind,
    PersistedContainmentEdgeData, PersistedContainmentSubKind, PersistedEdge, PersistedEdgeFamily,
    PersistedImportedEdgeData, PersistedImportedSubKind, PersistedNavigationTrigger, PersistedNode,
    PersistedNodeSessionState, PersistedProvenanceEdgeData, PersistedProvenanceSubKind,
    PersistedSemanticEdgeData, PersistedSemanticSubKind, PersistedTraversalEdgeData,
    PersistedTraversalMetrics, PersistedTraversalRecord,
};
#[allow(unused_imports)] // Some used only in #[cfg(test)]
use crate::types::{
    ClassificationProvenance, ClassificationScheme, ClassificationStatus, DominantEdge,
    FrameLayoutHint, FrameLayoutNodeId, ImportRecord, ImportRecordMembership, NodeClassification,
    NodeImportProvenance, NodeImportRecordSummary, NodeTagPresentationState, SplitOrientation,
    format_imported_at_secs,
};

pub mod apply;
pub mod edge_payload;
pub mod edge_taxonomy;
pub mod facet_projection;
pub mod filter;
pub mod history;
pub mod identity;
pub mod import_records;
pub mod node;
pub mod node_props;

// Identity types and rkyv archive helpers extracted to `identity.rs`
// per the 2026-04-30 renderer plan §6.4 decomposition target. Public
// types are re-exported at this module path so external callers
// (`mere_kernel::graph::NodeKey`, etc.) continue to resolve;
// the rkyv `with = ...` archive helpers are crate-internal and used
// only by struct field annotations in this file.
pub(crate) use identity::UuidAsBytes;
pub use identity::{EdgeKey, GraphDirection, GraphIndex, GraphViewId, NodeKey};

// Node + NodeLifecycle extracted to `node.rs` per the same
// decomposition target. Re-exported so `mere_kernel::graph::Node`
// continues to resolve.
pub use node::{Node, NodeLifecycle};

// Node navigation history extracted to `history.rs` (2026-05-11
// kernel-mod decomposition pass). Re-exported so external callers
// (`mere_kernel::graph::NodeNavigationMemory`, etc.) keep resolving.
pub use history::{
    NodeHistoryBranchAlternative, NodeHistoryBranchProjection, NodeHistoryBranchVisit,
    NodeHistoryOwner, NodeHistoryProjection, NodeHistorySemanticSummary, NodeNavigationMemory,
};

// Edge taxonomy (family/sub-kind enums, family-specific data structs)
// and `EdgePayload` extracted to their own modules (2026-05-11
// kernel-mod decomposition pass). Stage 4 of the 2026-05-11
// relation-taxonomy plan removed `EdgeType` and `EdgeKind`; reads go
// through [`RelationKind`] + [`RelationSelector`], writes through
// [`EdgeAssertion`].
pub use edge_payload::EdgePayload;
pub use edge_taxonomy::{
    ArrangementData, ArrangementSubKind, ContainmentData, ContainmentSubKind, EdgeAssertion,
    EdgeFamily, EdgeMetrics, ImportedData, ImportedSubKind, NavigationTrigger, ProvenanceData,
    ProvenanceSubKind, RelationDurability, RelationKind, RelationSelector, SemanticData,
    SemanticSubKind, Traversal, TraversalData,
};

/// Traversal archive payload emitted when dissolving a node.
#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub struct DissolvedTraversalRecord {
    #[rkyv(with = UuidAsBytes)]
    pub from_node_id: Uuid,
    #[rkyv(with = UuidAsBytes)]
    pub to_node_id: Uuid,
    pub traversals: Vec<Traversal>,
}

fn normalize_import_record_memberships(memberships: &mut Vec<ImportRecordMembership>) {
    memberships.sort_by(|left, right| {
        left.node_id
            .cmp(&right.node_id)
            .then_with(|| left.suppressed.cmp(&right.suppressed))
    });
    let mut deduped: Vec<ImportRecordMembership> = Vec::with_capacity(memberships.len());
    for membership in memberships.drain(..) {
        if let Some(existing) = deduped.last_mut()
            && existing.node_id == membership.node_id
        {
            existing.suppressed &= membership.suppressed;
            continue;
        }
        deduped.push(membership);
    }
    *memberships = deduped;
}

pub(crate) fn normalize_import_records(import_records: &mut Vec<ImportRecord>) {
    let mut merged = BTreeMap::<String, ImportRecord>::new();
    for mut record in import_records.drain(..) {
        normalize_import_record_memberships(&mut record.memberships);
        if record.record_id.trim().is_empty() {
            continue;
        }
        let entry = merged
            .entry(record.record_id.clone())
            .or_insert_with(|| ImportRecord {
                record_id: record.record_id.clone(),
                source_id: record.source_id.clone(),
                source_label: record.source_label.clone(),
                imported_at_secs: record.imported_at_secs,
                memberships: Vec::new(),
            });
        if entry.source_id.is_empty() {
            entry.source_id = record.source_id.clone();
        }
        if entry.source_label.is_empty() {
            entry.source_label = record.source_label.clone();
        }
        if entry.imported_at_secs == 0 {
            entry.imported_at_secs = record.imported_at_secs;
        } else if record.imported_at_secs != 0 {
            entry.imported_at_secs = entry.imported_at_secs.min(record.imported_at_secs);
        }
        entry.memberships.extend(record.memberships);
    }
    *import_records = merged.into_values().collect();
    for record in import_records.iter_mut() {
        normalize_import_record_memberships(&mut record.memberships);
    }
    import_records.sort_by(|left, right| {
        left.source_label
            .cmp(&right.source_label)
            .then_with(|| left.imported_at_secs.cmp(&right.imported_at_secs))
            .then_with(|| left.record_id.cmp(&right.record_id))
    });
}

pub(crate) fn current_unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}


// Node, NodeLifecycle, and impl Node moved to `node.rs` per the
// 2026-04-30 renderer plan §6.4 decomposition target. Re-exported
// here so external paths (`mere_kernel::graph::Node`, etc.)
// resolve unchanged.

/// Canonical read-side relation view. One row per (from, to,
/// [`RelationKind`]) — a multi-relation node pair yields multiple
/// rows. Replaces the `EdgeType`-flavoured `EdgeView` removed in
/// stage 4 of the 2026-05-11 relation-taxonomy plan.
///
/// This is the **classifier shape** — it carries no per-relation
/// payload (labels, decay, traversal events). Callers needing
/// payload reach for the typed `EdgePayload` sidecar via the
/// kernel's per-family query methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RelationView {
    pub from: NodeKey,
    pub to: NodeKey,
    pub kind: RelationKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticEdgeView {
    pub from: NodeKey,
    pub to: NodeKey,
    pub sub_kind: SemanticSubKind,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrangementEdgeView {
    pub from: NodeKey,
    pub to: NodeKey,
    pub sub_kind: ArrangementSubKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainmentEdgeView {
    pub from: NodeKey,
    pub to: NodeKey,
    pub sub_kind: ContainmentSubKind,
}

/// Main graph structure backed by petgraph::StableGraph
#[derive(Clone)]
pub struct Graph {
    /// The underlying petgraph stable graph
    pub inner: StableGraph<Node, EdgePayload, Directed>,

    /// URL to node mapping for lookup (supports duplicate URLs).
    pub(crate) url_to_nodes: HashMap<String, Vec<NodeKey>>,

    /// Stable UUID to node mapping.
    pub(crate) id_to_node: HashMap<Uuid, NodeKey>,

    /// Durable imported relation truth; node provenance is derived from this.
    pub(crate) import_records: Vec<ImportRecord>,
}

impl Graph {
    /// Create a new empty graph
    pub fn new() -> Self {
        Self {
            inner: StableGraph::new(),
            url_to_nodes: HashMap::new(),
            id_to_node: HashMap::new(),
            import_records: Vec::new(),
        }
    }

    // Single-write-path boundary (Phase 6.5): graph topology mutators are
    // crate-internal and intended for trusted writers (reducer + persistence
    // replay/recovery). Other runtime/shell code paths should route through
    // reducer intents rather than calling topology mutators directly.

    /// Add a new node to the graph.
    ///
    /// Not available on `wasm32` — use [`add_node_with_id`] with a
    /// host-provided UUID instead.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn add_node(&mut self, url: String, position: Point2D<f32>) -> NodeKey {
        self.add_node_with_id(Uuid::new_v4(), url, position)
    }

    /// Add a node with a pre-existing UUID.
    pub fn add_node_with_id(&mut self, id: Uuid, url: String, position: Point2D<f32>) -> NodeKey {
        let now = std::time::SystemTime::now();
        let key = self.inner.add_node(Node {
            id,
            title: url.clone(),
            cached_host: cached_host_from_url(&url),
            position,
            committed_position: position,
            velocity: Vector2D::zero(),
            tags: HashSet::new(),
            tag_presentation: NodeTagPresentationState::default(),
            import_provenance: Vec::new(),
            classifications: Vec::new(),
            is_pinned: false,
            last_visited: now,
            navigation_memory: NodeNavigationMemory::empty(),
            thumbnail_png: None,
            thumbnail_width: 0,
            thumbnail_height: 0,
            favicon_rgba: None,
            favicon_width: 0,
            favicon_height: 0,
            session_scroll: None,
            session_form_draft: None,
            mime_hint: detect_mime(&url, None),
            viewer_override: None,
            compat_mode: false,
            address: address_from_url(&url),
            frame_layout_hints: Vec::new(),
            frame_split_offer_suppressed: false,
            lifecycle: NodeLifecycle::Cold,
        });

        self.url_to_nodes.entry(url).or_default().push(key);
        self.id_to_node.insert(id, key);
        key
    }

    /// Remove a node and all its connected edges
    pub fn remove_node(&mut self, key: NodeKey) -> bool {
        if let Some(node) = self.inner.remove_node(key) {
            self.id_to_node.remove(&node.id);
            self.remove_url_mapping(node.address.as_url_str(), key);
            let removed_id = node.id.to_string();
            for record in &mut self.import_records {
                record
                    .memberships
                    .retain(|membership| membership.node_id != removed_id);
            }
            self.import_records
                .retain(|record| !record.memberships.is_empty());
            true
        } else {
            false
        }
    }

    /// Update a node's URL, maintaining the url_to_node index.
    /// Returns the old URL, or None if the node doesn't exist.
    pub fn update_node_url(&mut self, key: NodeKey, new_url: String) -> Option<String> {
        let node = self.inner.node_weight_mut(key)?;
        let old_url = node.address.as_url_str().to_string();
        node.cached_host = cached_host_from_url(&new_url);
        node.address = address_from_url(&new_url);
        self.remove_url_mapping(&old_url, key);
        self.url_to_nodes.entry(new_url).or_default().push(key);
        Some(old_url)
    }

    pub fn recompute_cached_hosts(&mut self) {
        for node in self.inner.node_weights_mut() {
            node.cached_host = cached_host_from_url(node.address.as_url_str());
        }
    }


}

// Edge mutators (add_edge, assert_relation, replay_*, dissolve_*,
// remove_edges, retract_relations, get_edge*, push_traversal) live in
// `graph/edge_ops.rs` (2026-05-11 decomposition pass).
pub mod edge_ops;

// Query + traversal (node/edge lookups, neighbor iteration, BFS /
// shortest-path / connected-components / SCC, derived-edge views)
// live in `graph/query.rs` (2026-05-11 decomposition pass).
pub mod query;

// Snapshot serialization (`to_snapshot` / `from_snapshot`), the rkyv
// trait impls that delegate through them, `containment_parent_url`,
// and `impl Default for Graph` live in `graph/snapshot.rs`
// (2026-05-11 decomposition pass).
pub mod snapshot;

// Tests live in `graph/tests.rs` to keep this file under the
// 600-LOC ceiling (kernel decomposition pass 2026-05-11).
#[cfg(test)]
mod tests;
