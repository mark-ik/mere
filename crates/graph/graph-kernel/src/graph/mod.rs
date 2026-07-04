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
    FrameLayoutHint, FrameLayoutNodeId, GraphScope, ImportRecord, ImportRecordMembership,
    NodeClassification, NodeImportProvenance, NodeImportRecordSummary, NodeTagPresentationState,
    SplitOrientation, format_imported_at_secs,
};

pub mod apply;
pub mod capture;
/// Cross-graph node copy (tear-out fork): mints a node in this graph from a
/// donor node in another graph, recording cross-graph derivation provenance.
pub mod cross_graph;
pub mod edge_data;
pub mod edge_payload;
pub mod edge_taxonomy;
pub mod facet_projection;
pub mod filter;
/// Test-fixture escape hatch over the `pub(crate)` mutators (write-path
/// migration). Feature-gated: enable `fixtures` in dev-dependencies only.
#[cfg(feature = "fixtures")]
pub mod fixtures;
pub mod history;
pub mod identity;
pub mod import_records;
pub mod node;
pub mod node_props;

// Field system (2026-05-31, field-system step 3): Field/Coupling kernel-truth
// primitives + the portable field AST. A parallel keyed store on `Graph` lands
// in Phase 1 — these are not petgraph node weights or `EdgePayload` sidecars.
// See `2026-05-31_field_coupling_kernel_primitive_plan.md`.
pub mod coupling;
pub mod edge_path;
pub mod field;
pub mod field_ast;
/// Graph mutators + queries for the field layer (Phase 1): the parallel keyed
/// store on `Graph` + selector evaluation.
mod field_ops;

// Identity types and rkyv archive helpers extracted to `identity.rs`
// per the 2026-04-30 renderer plan §6.4 decomposition target. Public
// types are re-exported at this module path so external callers
// (`kernel::graph::NodeKey`, etc.) continue to resolve;
// the rkyv `with = ...` archive helpers are crate-internal and used
// only by struct field annotations in this file.
pub(crate) use identity::UuidAsBytes;
pub use identity::{EdgeKey, GraphDirection, GraphIndex, GraphViewId, NodeKey};

// Node + NodeLifecycle extracted to `node.rs` per the same
// decomposition target. Re-exported so `kernel::graph::Node`
// continues to resolve.
pub use node::{Node, NodeLifecycle};

// Node navigation history extracted to `history.rs` (2026-05-11
// kernel-mod decomposition pass). Re-exported so external callers
// (`kernel::graph::NodeNavigationMemory`, etc.) keep resolving.
pub use history::{
    NodeHistoryBranchAlternative, NodeHistoryBranchProjection, NodeHistoryBranchVisit,
    NodeHistoryOwner, NodeHistoryProjection, NodeHistorySemanticSummary, RecentVisit,
    SharedNavigationMemory,
};

// Edge taxonomy (family/sub-kind enums) and per-family runtime data structs
// extracted to their own modules. edge_taxonomy holds classifiers; edge_data
// holds runtime payload types (Data structs, Traversal, EdgeMetrics, IRI fns).
// Stage 4 of the 2026-05-11 relation-taxonomy plan removed `EdgeType` and
// `EdgeKind`; reads go through [`RelationKind`] + [`RelationSelector`], writes
// through [`EdgeAssertion`].
pub use capture::{
    CapturedDelta, GraphTableStats, replay_captured_deltas, set_captured_delta_hook,
};
pub use edge_data::{
    ArrangementData, ContainmentData, EdgeMetrics, ImportedData, ProvenanceData, REL_VOCAB,
    SemanticData, SemanticStatement, Traversal, TraversalData, predicate_iri, sub_kind_from_iri,
};
pub use edge_payload::EdgePayload;
pub use edge_taxonomy::{
    ArrangementSubKind, ContainmentSubKind, EdgeAssertion, EdgeFamily, ImportedSubKind,
    NavigationTrigger, ProvenanceSubKind, RelationDurability, RelationKind, RelationSelector,
    SemanticSubKind,
};

// Field-system truth types (2026-05-31). Field/Coupling form a parallel field
// layer beside the node/edge graph; aether reads them and evaluates.
pub use coupling::{COUPLING_VOCAB, Coupling, CouplingResponse, NodeSelector};
pub use edge_path::{EdgePath, EdgePathRule};
pub use field::{CouplingId, Field, FieldDefinition, FieldExtent, FieldId, FieldLifecycle};
pub use field_ast::{Falloff, ScalarField, VectorField};

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
// here so external paths (`kernel::graph::Node`, etc.)
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
    /// The underlying petgraph stable graph. `pub(crate)` to enforce the single-write-path boundary
    /// at the type level: every topology mutation must go through a `Graph` method so it advances
    /// [`revision`](Self::revision); direct `inner` mutation from outside the kernel would bypass the
    /// bump and stale the structural caches. No external code accesses it (verified). (Graph signals.)
    pub(crate) inner: StableGraph<Node, EdgePayload, Directed>,

    /// URL to node mapping for lookup (supports duplicate URLs).
    pub(crate) url_to_nodes: HashMap<String, Vec<NodeKey>>,

    /// Stable UUID to node mapping.
    pub(crate) id_to_node: HashMap<Uuid, NodeKey>,

    /// Durable imported relation truth; node provenance is derived from this.
    pub(crate) import_records: Vec<ImportRecord>,

    /// Field-layer truth (field-system step 3, Phase 1): a parallel keyed store
    /// beside the node/edge petgraph. A coupling targets a *selector* over nodes,
    /// not a node→node edge, so fields/couplings cannot ride the petgraph; they
    /// are content truth that `aether` reads and evaluates (derived).
    pub(crate) fields: HashMap<FieldId, Field>,
    pub(crate) couplings: HashMap<CouplingId, Coupling>,

    /// The graph's whole navigation history: one shared visit space, one owner
    /// per node (the (b) anchor design). In-place navigation extends a node's own
    /// path; a branch node is spawned anchored under its origin's current visit.
    pub(crate) nav: SharedNavigationMemory,

    /// A monotonic **structural revision**, bumped by [`bump_revision`](Self::bump_revision)
    /// whenever the node/edge structure or a pair's relation set changes (add/remove node, add/
    /// remove edge, assert/retract a relation). It does NOT bump on a re-visit's traversal append
    /// or on pure content edits (title/url/tags), since those leave the structure a derived signal
    /// reads unchanged. The single source of truth any consumer gates a cache on — community,
    /// arrangements, importance — so a recompute happens once per real change, not per frame.
    /// (Graph signals — the universal cache key.)
    revision: u64,

    /// The current app-launch session number, set once by the host via
    /// [`set_current_session`](Self::set_current_session) right after construction/
    /// restore. `0` (the default) means "not wired" — [`navigate_node`](Self::navigate_node)
    /// then stamps nothing, so by-sessions eviction sees every node as undated until the
    /// host opts in. Not part of the persisted snapshot: it is per-launch host state, not
    /// graph truth. (Alembic B5 — by-sessions eviction.)
    current_session: u64,
}

impl Graph {
    /// Create a new empty graph
    pub fn new() -> Self {
        Self {
            inner: StableGraph::new(),
            url_to_nodes: HashMap::new(),
            id_to_node: HashMap::new(),
            import_records: Vec::new(),
            fields: HashMap::new(),
            couplings: HashMap::new(),
            nav: SharedNavigationMemory::empty(),
            revision: 0,
            current_session: 0,
        }
    }

    /// Set the current app-launch session number (Alembic B5). The host calls this once,
    /// right after construction or snapshot-restore, with its persisted-and-incremented
    /// session counter; every [`navigate_node`](Self::navigate_node) call afterwards stamps
    /// the visited node's [`Node::last_session_visited`] with this value.
    pub fn set_current_session(&mut self, session: u64) {
        self.current_session = session;
    }

    /// The current structural revision (see [`revision`](Self::revision)). A consumer captures it
    /// alongside a derived result and recomputes only when it has advanced. (Graph signals.)
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Advance the structural revision. Called by the topology/relation mutators on a real change.
    /// (Graph signals — the universal cache key.)
    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    // Single-write-path boundary (Phase 6.5 — ENFORCED as of the 2026-07-01
    // write-path migration). Every primitive durable mutator on `Graph` is
    // `pub(crate)`; external code routes through `apply::apply_graph_delta`
    // (the one funnel a future recording hook — the event log — instruments).
    // The four writer classes:
    //   1. Primitive durable mutations (topology, relations, nav history, tags,
    //      titles, body, media, classifications, properties, predicates,
    //      fields/couplings, frame hints, import records) — `pub(crate)`,
    //      reached externally only via `GraphDelta`.
    //   2. Compound kernel operations stay `pub`: `from_snapshot`,
    //      `cross_graph::copy_*` — kernel-authored multi-step ops over
    //      non-delta-able inputs (`&Graph` donors); the "trusted writers".
    //   3. Transient/runtime state stays `pub`, exempt by design:
    //      `set_node_position` / `set_node_projected_position` (physics/view —
    //      positions are not graph truth), `set_current_session` (per-launch
    //      host wiring), `set_node_lifecycle` (webview runtime state).
    //   4. Test fixtures: the `fixtures` cargo feature re-exposes the raw
    //      mutators via `graph::fixtures::GraphFixtures` — dev-dependencies
    //      only, never a production enable.

    /// The fixed namespace for the deterministic node id (see
    /// [`node_namespace_id`](Self::node_namespace_id)). Fixed forever: changing it
    /// would renumber every node derived from it.
    const NODE_NAMESPACE: Uuid = Uuid::from_u128(0x6D65_7265_4E4F_4445_6E61_6D65_7370_6163);

    /// The deterministic node UUID for `url`: a name-based UUIDv5 under
    /// [`NODE_NAMESPACE`](Self::NODE_NAMESPACE). Two hosts that materialize the
    /// same `url` mint the same id, so a federated merge needs no identity
    /// reconciliation. The linked-data ingest layer uses this for a document's
    /// `@id`, where the URL *is* the identity. Raw [`add_node`] keeps a fresh
    /// random id, because the kernel treats a node's address as a property and
    /// lets several nodes share one (see `get_nodes_by_url`).
    pub fn node_namespace_id(url: &str) -> Uuid {
        Uuid::new_v5(&Self::NODE_NAMESPACE, url.as_bytes())
    }

    /// Add a new node to the graph.
    ///
    /// Not available on `wasm32`; use [`add_node_with_id`] with a host-provided
    /// UUID (e.g. from [`node_namespace_id`](Self::node_namespace_id)) instead.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn add_node(&mut self, url: String, position: Point2D<f32>) -> NodeKey {
        self.add_node_with_id(Uuid::new_v4(), url, position)
    }

    /// Add a node with a pre-existing UUID.
    pub(crate) fn add_node_with_id(
        &mut self,
        id: Uuid,
        url: String,
        position: Point2D<f32>,
    ) -> NodeKey {
        let now = std::time::SystemTime::now();
        let primary_address = address_from_url(&url);
        let key = self.inner.add_node(Node {
            id,
            title: url.clone(),
            cached_host: cached_host_from_url(&url),
            body: None,
            position,
            velocity: Vector2D::zero(),
            tags: HashSet::new(),
            tag_presentation: NodeTagPresentationState::default(),
            import_provenance: Vec::new(),
            classifications: Vec::new(),
            derivations: Vec::new(),
            properties: Vec::new(),
            is_pinned: false,
            last_visited: now,
            last_session_visited: 0,
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
            addresses: vec![crate::address::AddressClaim::primary(primary_address)],
            frame_layout_hints: Vec::new(),
            frame_split_offer_suppressed: false,
            lifecycle: NodeLifecycle::Cold,
        });

        self.url_to_nodes.entry(url).or_default().push(key);
        self.id_to_node.insert(id, key);
        self.bump_revision();
        key
    }

    /// Remove a node and all its connected edges
    pub(crate) fn remove_node(&mut self, key: NodeKey) -> bool {
        if let Some(node) = self.inner.remove_node(key) {
            self.id_to_node.remove(&node.id);
            // The node's navigation owner is intentionally *kept* in the shared
            // visit space: a node branched from it still anchors to its visits, so
            // erasing the lineage on node removal would orphan those anchors. The
            // owner is simply never queried once the node is gone (pruning belongs
            // to a future GC / `eidetic` scope, not graph removal).
            // Deregister every URL claim this node carried — Primary plus
            // any aliases. (Aliases are not yet wired into add paths but the
            // index handles them when they land.)
            for claim in &node.addresses {
                self.remove_url_mapping(claim.address.as_url_str(), key);
            }
            let removed_id = node.id.to_string();
            for record in &mut self.import_records {
                record
                    .memberships
                    .retain(|membership| membership.node_id != removed_id);
            }
            self.import_records
                .retain(|record| !record.memberships.is_empty());
            self.bump_revision();
            true
        } else {
            false
        }
    }

    /// Update a node's Primary URL, maintaining the url_to_nodes index.
    /// Returns the old URL, or None if the node doesn't exist.
    ///
    /// Operates on the Primary claim only; aliases (when supported) stay
    /// attached. To mutate aliases, use dedicated alias methods (future).
    pub(crate) fn update_node_url(&mut self, key: NodeKey, new_url: String) -> Option<String> {
        let node = self.inner.node_weight_mut(key)?;
        let old_url = node.primary_address().as_url_str().to_string();
        node.cached_host = cached_host_from_url(&new_url);
        // A navigation to a different host invalidates the favicon (it was the old
        // site's icon); clear it so a stale favicon does not linger on the tile until
        // the new one loads. A same-host path change keeps it. (Favicon-on-tile.)
        if cached_host_from_url(&old_url) != node.cached_host {
            node.favicon_rgba = None;
            node.favicon_width = 0;
            node.favicon_height = 0;
        }
        // Replace the Primary claim's address; aliases (if any) are
        // preserved.
        let new_primary_address = address_from_url(&new_url);
        for claim in node.addresses.iter_mut() {
            if claim.is_primary() {
                claim.address = new_primary_address.clone();
                break;
            }
        }
        self.remove_url_mapping(&old_url, key);
        self.url_to_nodes.entry(new_url).or_default().push(key);
        Some(old_url)
    }

    /// Navigate `key` in place to `url`: record the visit in the node's own
    /// browse history (a forward-fork if the cursor had stepped back) and update
    /// its Primary URL. No new node and no edge — the node is a browsing surface
    /// whose content changes; the graph shape does not. Cross-node lineage
    /// (the navigated-from relation) is a separate, explicit edge.
    pub(crate) fn navigate_node(&mut self, key: NodeKey, url: &str) {
        let at_ms = Self::epoch_ms();
        if let Some(id) = self.inner.node_weight(key).map(|n| n.id) {
            self.nav
                .record_visit(id, url, node_lineage::TransitionKind::UrlTyped, at_ms);
        }
        if let Some(node) = self.inner.node_weight_mut(key) {
            node.last_session_visited = self.current_session;
        }
        self.update_node_url(key, url.to_string());
    }

    /// Anchor a freshly-minted `child` node's history under `parent`'s current
    /// visit — the navigated-from anchor. Call **before** the child's first
    /// [`navigate_node`](Self::navigate_node) so that first visit attaches there in
    /// the shared lineage tree (the (b) cross-node anchor; the branch-mint path).
    pub(crate) fn branch_history(&mut self, child: NodeKey, parent: NodeKey) {
        let (Some(child_id), Some(parent_id)) = (
            self.inner.node_weight(child).map(|n| n.id),
            self.inner.node_weight(parent).map(|n| n.id),
        ) else {
            return;
        };
        self.nav.spawn(child_id, parent_id);
    }

    /// Step `key` back one visit in its own history, updating its Primary URL to
    /// the revealed page. Returns the new URL, or `None` if already at the root.
    pub(crate) fn node_history_back(&mut self, key: NodeKey) -> Option<String> {
        let at_ms = Self::epoch_ms();
        let id = self.inner.node_weight(key)?.id;
        let url = self.nav.back(id, at_ms)?;
        self.update_node_url(key, url.clone());
        Some(url)
    }

    /// Step `key` forward one visit in its own history. Returns the new URL, or
    /// `None` if already at the tip.
    pub(crate) fn node_history_forward(&mut self, key: NodeKey) -> Option<String> {
        let at_ms = Self::epoch_ms();
        let id = self.inner.node_weight(key)?.id;
        let url = self.nav.forward(id, at_ms)?;
        self.update_node_url(key, url.clone());
        Some(url)
    }

    /// Whether `key`'s within-node history can step back (toolbar gating).
    pub fn node_can_back(&self, key: NodeKey) -> bool {
        self.inner
            .node_weight(key)
            .is_some_and(|n| self.nav.can_back(n.id))
    }

    /// Whether `key`'s within-node history can step forward (toolbar gating).
    pub fn node_can_forward(&self, key: NodeKey) -> bool {
        self.inner
            .node_weight(key)
            .is_some_and(|n| self.nav.can_forward(n.id))
    }

    /// `key`'s current page (its history cursor's URL), if any.
    pub fn node_current_url(&self, key: NodeKey) -> Option<String> {
        let id = self.inner.node_weight(key)?.id;
        self.nav.current_url(id)
    }

    /// `key`'s linear-history projection (active path + cursor).
    pub fn node_history_projection(&self, key: NodeKey) -> NodeHistoryProjection {
        match self.inner.node_weight(key) {
            Some(node) => self.nav.projection(node.id),
            None => NodeHistoryProjection {
                entries: Vec::new(),
                current_index: 0,
            },
        }
    }

    /// `key`'s branching-history projection (visit tree with alternates).
    pub fn node_history_branch_projection(&self, key: NodeKey) -> NodeHistoryBranchProjection {
        self.inner
            .node_weight(key)
            .map(|node| self.nav.branch_projection(node.id))
            .unwrap_or_default()
    }

    /// A coarse semantic summary of `key`'s history.
    pub fn node_history_semantic_summary(&self, key: NodeKey) -> NodeHistorySemanticSummary {
        self.inner
            .node_weight(key)
            .map(|node| self.nav.semantic_summary(node.id))
            .unwrap_or_default()
    }

    /// The graph's recently-visited nodes, newest first, capped at `limit` — the
    /// graph-wide "recent" projection over the shared visit space (gloss; the
    /// lineage pane). Delegates to [`SharedNavigationMemory::recent_visited`].
    pub fn recent_visited(&self, limit: usize) -> Vec<RecentVisit> {
        self.nav.recent_visited(limit)
    }

    /// Milliseconds since the Unix epoch, for visit timestamps (live-app clock).
    fn epoch_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    pub(crate) fn recompute_cached_hosts(&mut self) {
        for node in self.inner.node_weights_mut() {
            node.cached_host = cached_host_from_url(node.primary_address().as_url_str());
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

// `node_display_label`: a human-meaningful label derived from a node's title /
// linked-data role / @type / address, for the canvas tiles + roster (so an
// anonymous ingested entity reads as "publisher" / "Organization", not a raw
// `urn:mere:bnode:` string).
pub mod display;

// Snapshot serialization (`to_snapshot` / `from_snapshot`), the rkyv
// trait impls that delegate through them, `containment_parent_url`,
// and `impl Default for Graph` live in `graph/snapshot.rs`
// (2026-05-11 decomposition pass).
pub mod snapshot;

// Tests live in `graph/tests.rs` to keep this file under the
// 600-LOC ceiling (kernel decomposition pass 2026-05-11).
#[cfg(test)]
mod tests;
