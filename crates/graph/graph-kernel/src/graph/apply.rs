// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

use euclid::default::Point2D;
use chartulary::stemma::TransitionKind;
use uuid::Uuid;

use super::{
    Coupling, CouplingId, EdgeAssertion, EdgeKey, Field, FieldId, FrameLayoutHint, Graph,
    NavigationTrigger, NodeKey, RelationSelector, SemanticSubKind,
    capture::{
        CapturedDelta, coupling_from_persisted, field_from_persisted,
        persisted_coupling_from_coupling, persisted_field_from_field, record_captured_delta,
    },
};
use crate::persistence::{PersistedCoupling, PersistedField};
use crate::types::{
    BadgeIcon, ClassificationScheme, ClassificationStatus, GraphScope, ImportRecord,
    NodeClassification, NodeDerivation, NodeImportProvenance, NodeProperty,
};

#[derive(Debug, Clone)]
pub enum GraphDelta {
    AddNode {
        id: Option<Uuid>,
        url: String,
        position: Point2D<f32>,
    },
    AssertRelation {
        from: NodeKey,
        to: NodeKey,
        assertion: EdgeAssertion,
    },
    RemoveNode {
        key: NodeKey,
    },
    ReplayAddNodeWithIdIfMissing {
        id: Uuid,
        url: String,
        position: Point2D<f32>,
    },
    ReplayAssertRelationByIds {
        from_id: Uuid,
        to_id: Uuid,
        assertion: EdgeAssertion,
    },
    ReplayRemoveNodeById {
        node_id: Uuid,
    },
    ReplayRetractRelationsByIds {
        from_id: Uuid,
        to_id: Uuid,
        selector: RelationSelector,
    },
    ReplayAppendTraversalByIds {
        from_id: Uuid,
        to_id: Uuid,
        trigger: NavigationTrigger,
        timestamp_ms: u64,
    },
    ReplaySetNodeTitleById {
        node_id: Uuid,
        title: String,
    },
    ReplaySetNodeUrlById {
        node_id: Uuid,
        new_url: String,
    },
    ReplaySetNodeThumbnailById {
        node_id: Uuid,
        png_bytes: Vec<u8>,
        width: u32,
        height: u32,
    },
    ReplaySetNodeFaviconById {
        node_id: Uuid,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    },
    ReplaySetNodeMimeHintById {
        node_id: Uuid,
        mime_hint: Option<String>,
    },
    ReplaySetNodeNestedById {
        node_id: Uuid,
        nested: Option<String>,
    },
    ReplaySetNodePinnedById {
        node_id: Uuid,
        is_pinned: bool,
    },
    ReplayInsertNodeTagById {
        node_id: Uuid,
        tag: String,
    },
    ReplayRemoveNodeTagById {
        node_id: Uuid,
        tag: String,
    },
    ReplaySetNodeBodyById {
        node_id: Uuid,
        body: Option<String>,
    },
    ReplayNavigateNodeById {
        node_id: Uuid,
        url: String,
        transition: TransitionKind,
        timestamp_ms: u64,
        last_session_visited: u64,
    },
    ReplayBranchHistoryByIds {
        child_id: Uuid,
        parent_id: Uuid,
    },
    ReplayNodeHistoryBackById {
        node_id: Uuid,
        timestamp_ms: u64,
    },
    ReplayNodeHistoryForwardById {
        node_id: Uuid,
        timestamp_ms: u64,
    },
    ReplayAppendNodePropertyById {
        node_id: Uuid,
        property: NodeProperty,
    },
    ReplayAddNodeClassificationById {
        node_id: Uuid,
        classification: NodeClassification,
    },
    ReplayRemoveNodeClassificationById {
        node_id: Uuid,
        scheme: ClassificationScheme,
        value: String,
    },
    ReplaySetNodeClassificationStatusById {
        node_id: Uuid,
        scheme: ClassificationScheme,
        value: String,
        status: ClassificationStatus,
    },
    ReplaySetNodePrimaryClassificationById {
        node_id: Uuid,
        scheme: ClassificationScheme,
        value: String,
    },
    ReplayRecordNodeDerivationById {
        node_id: Uuid,
        derivation: NodeDerivation,
    },
    ReplaySetNodeTagIconOverrideById {
        node_id: Uuid,
        tag: String,
        icon: Option<BadgeIcon>,
    },
    ReplaySetEdgeSemanticPredicateByIds {
        from_id: Uuid,
        to_id: Uuid,
        predicate: Option<String>,
    },
    ReplayAssertSemanticPredicateByIds {
        from_id: Uuid,
        to_id: Uuid,
        predicate: String,
    },
    ReplayAppendFrameLayoutHintById {
        node_id: Uuid,
        hint: FrameLayoutHint,
    },
    ReplayRemoveFrameLayoutHintById {
        node_id: Uuid,
        hint_index: usize,
    },
    ReplayMoveFrameLayoutHintById {
        node_id: Uuid,
        from_index: usize,
        to_index: usize,
    },
    ReplaySetFrameSplitOfferSuppressedById {
        node_id: Uuid,
        suppressed: bool,
    },
    ReplayUpdateNodeHistoryById {
        node_id: Uuid,
        entries: Vec<String>,
        current_index: usize,
    },
    ReplaySetImportRecords {
        import_records: Vec<ImportRecord>,
    },
    ReplayTouchNodeLastVisitedById {
        node_id: Uuid,
        timestamp_ms: u64,
    },
    RetractRelations {
        from: NodeKey,
        to: NodeKey,
        selector: RelationSelector,
    },
    /// Append a traversal event to the `from → to` edge (creating the
    /// edge if absent). `timestamp_ms = None` stamps now-via-the-kernel;
    /// `Some(t)` uses the supplied timestamp (importers, replay).
    AppendTraversal {
        from: NodeKey,
        to: NodeKey,
        trigger: NavigationTrigger,
        timestamp_ms: Option<u64>,
    },
    SetNodeTitle {
        key: NodeKey,
        title: String,
    },
    SetNodeUrl {
        key: NodeKey,
        new_url: String,
    },
    SetNodeThumbnail {
        key: NodeKey,
        png_bytes: Vec<u8>,
        width: u32,
        height: u32,
    },
    SetNodeFavicon {
        key: NodeKey,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    },
    SetNodeMimeHint {
        key: NodeKey,
        mime_hint: Option<String>,
    },
    SetNodeNested {
        key: NodeKey,
        nested: Option<codicil::LogId>,
    },
    SetNodePinned {
        key: NodeKey,
        is_pinned: bool,
    },
    AppendFrameLayoutHint {
        key: NodeKey,
        hint: FrameLayoutHint,
    },
    RemoveFrameLayoutHint {
        key: NodeKey,
        hint_index: usize,
    },
    MoveFrameLayoutHint {
        key: NodeKey,
        from_index: usize,
        to_index: usize,
    },
    SetFrameSplitOfferSuppressed {
        key: NodeKey,
        suppressed: bool,
    },
    /// Replace a node's persisted navigation history (entries + current
    /// index). The clamping rule is: `current_index` is treated as `0` when
    /// `entries` is empty, otherwise it is clamped to `entries.len() - 1`.
    /// Sole sanctioned write surface for `Node::history`; the underlying
    /// `Graph` setter is `pub(crate)` so this delta is the only way to
    /// mutate the field from outside `kernel`.
    UpdateNodeHistory {
        key: NodeKey,
        entries: Vec<String>,
        current_index: usize,
    },
    /// Replace the durable import-record table with a normalized whole-table snapshot.
    SetImportRecords {
        import_records: Vec<ImportRecord>,
    },
    /// Delete one durable import record by id.
    DeleteImportRecord {
        record_id: String,
    },
    /// Suppress or unsuppress one node membership within an import record.
    SetImportRecordMembershipSuppressed {
        record_id: String,
        key: NodeKey,
        suppressed: bool,
    },
    /// Replace one node's import provenance; the kernel rebuilds import records from it.
    SetNodeImportProvenance {
        key: NodeKey,
        import_provenance: Vec<NodeImportProvenance>,
    },
    /// Stamp one node's last-visited clock from the kernel clock.
    TouchNodeLastVisited {
        key: NodeKey,
    },
    // --- Write-path migration (2026-07-01): the variants below complete the
    // Phase 6.5 boundary. Every primitive durable mutation shell/runtime code
    // performs now has a delta; the raw mutators are `pub(crate)`. ---
    /// Navigate a node in place: record the visit in its own browse history and
    /// update its Primary URL. No new node, no edge.
    NavigateNode {
        key: NodeKey,
        url: String,
    },
    /// Anchor a freshly-minted `child` node's history under `parent`'s current
    /// visit (the navigated-from anchor; call before the child's first navigate).
    BranchHistory {
        child: NodeKey,
        parent: NodeKey,
    },
    /// Step a node back one visit in its own history (result:
    /// [`GraphDeltaResult::HistoryStepped`] carries the revealed URL).
    NodeHistoryBack {
        key: NodeKey,
    },
    /// Step a node forward one visit in its own history.
    NodeHistoryForward {
        key: NodeKey,
    },
    /// Add a durable semantic tag (also appends to the presentation order).
    InsertNodeTag {
        key: NodeKey,
        tag: String,
    },
    /// Remove a durable semantic tag (also drops its presentation entries).
    RemoveNodeTag {
        key: NodeKey,
        tag: String,
    },
    /// Set (or clear) a node's inline authored content body (knot note source).
    SetNodeBody {
        key: NodeKey,
        body: Option<String>,
    },
    /// Append an open literal property (dedup by exact predicate+value pair).
    AppendNodeProperty {
        key: NodeKey,
        property: NodeProperty,
    },
    /// Add a provenance-bearing classification record (dedup by scheme+value).
    AddNodeClassification {
        key: NodeKey,
        classification: NodeClassification,
    },
    /// Remove a classification record identified by `(scheme, value)`.
    RemoveNodeClassification {
        key: NodeKey,
        scheme: ClassificationScheme,
        value: String,
    },
    /// Update the status of a classification record identified by `(scheme, value)`.
    SetNodeClassificationStatus {
        key: NodeKey,
        scheme: ClassificationScheme,
        value: String,
        status: ClassificationStatus,
    },
    /// Promote one classification record to primary within its scheme.
    SetNodePrimaryClassification {
        key: NodeKey,
        scheme: ClassificationScheme,
        value: String,
    },
    /// Record cross-graph / extraction derivation provenance on a node.
    RecordNodeDerivation {
        key: NodeKey,
        derivation: NodeDerivation,
    },
    /// Set or clear a tag-icon override for a durable user tag.
    SetNodeTagIconOverride {
        key: NodeKey,
        tag: String,
        icon: Option<BadgeIcon>,
    },
    /// Set (or clear) the canonical semantic-predicate IRI on an existing edge.
    SetEdgeSemanticPredicate {
        edge: EdgeKey,
        predicate: Option<String>,
    },
    /// Assert a plain semantic edge carrying a raw predicate IRI (the
    /// unrecognized-predicate ingest path), creating the edge if absent.
    AssertSemanticPredicate {
        from: NodeKey,
        to: NodeKey,
        predicate: String,
    },
    ReplayAddField {
        field: PersistedField,
    },
    ReplayRetireFieldById {
        field_id: String,
    },
    ReplayAddCoupling {
        coupling: PersistedCoupling,
    },
    ReplaySetFieldCouplingStrengthByFieldId {
        field_id: String,
        strength: f32,
    },
    ReplayActivateFieldById {
        field_id: String,
    },
    ReplayRetractCouplingById {
        coupling_id: String,
    },
    /// Add (or replace by id) a field — field-layer truth.
    AddField {
        field: Field,
    },
    /// Retire a field (lifecycle to Retired; couplings stop evaluating).
    RetireField {
        id: FieldId,
    },
    /// Add (or replace by id) a coupling binding a field to a node selector.
    AddCoupling {
        coupling: Coupling,
    },
    /// Set the strength of every coupling bound to `field`.
    SetFieldCouplingStrength {
        field: FieldId,
        strength: f32,
    },
    /// Reactivate a retired field.
    ActivateField {
        id: FieldId,
    },
    /// Remove one coupling binding by id.
    RetractCoupling {
        id: CouplingId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphDeltaResult {
    NodeAdded(NodeKey),
    NodeMaybeAdded(Option<NodeKey>),
    EdgeAdded(Option<EdgeKey>),
    NodeRemoved(bool),
    EdgesRemoved(usize),
    TraversalAppended(bool),
    NodeMetadataUpdated(bool),
    NodeUrlUpdated(Option<String>),
    /// A history back/forward step: the revealed URL, or `None` at the
    /// root/tip (the cursor did not move).
    HistoryStepped(Option<String>),
    /// A field-layer mutation: whether anything actually changed.
    FieldChanged(bool),
    /// An import-record mutation: whether anything actually changed.
    ImportRecordsUpdated(bool),
    /// A mutation with no observable result (e.g. navigate, branch-history).
    Applied,
}

fn capture_resolved_import_records(graph: &Graph) {
    record_captured_delta(&CapturedDelta::ReplaySetImportRecords {
        import_records: graph.import_records().to_vec(),
    });
}

pub fn apply_graph_delta(graph: &mut Graph, delta: GraphDelta) -> GraphDeltaResult {
    match delta {
        GraphDelta::AddNode { id, url, position } => {
            let capture_url = url.clone();
            let capture_position = [position.x, position.y];
            let key = match id {
                Some(id) => graph.add_node_with_id(id, url, position),
                #[cfg(not(target_arch = "wasm32"))]
                None => graph.add_node(url, position),
                #[cfg(target_arch = "wasm32")]
                None => panic!("AddNode without an explicit id is not supported on WASM"),
            };
            if let Some(node) = graph.get_node(key) {
                record_captured_delta(&CapturedDelta::ReplayAddNodeWithIdIfMissing {
                    id: node.id.to_string(),
                    url: capture_url,
                    position: capture_position,
                });
            }
            GraphDeltaResult::NodeAdded(key)
        }
        GraphDelta::AssertRelation {
            from,
            to,
            assertion,
        } => {
            let from_id = graph.get_node(from).map(|node| node.id);
            let to_id = graph.get_node(to).map(|node| node.id);
            let capture_assertion = assertion.clone();
            let edge = graph.assert_relation(from, to, assertion);
            if edge.is_some()
                && let (Some(from_id), Some(to_id)) = (from_id, to_id)
            {
                record_captured_delta(&CapturedDelta::ReplayAssertRelationByIds {
                    from_id: from_id.to_string(),
                    to_id: to_id.to_string(),
                    assertion: capture_assertion,
                });
            }
            GraphDeltaResult::EdgeAdded(edge)
        }
        GraphDelta::RemoveNode { key } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let removed = graph.remove_node(key);
            if removed && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplayRemoveNodeById {
                    node_id: node_id.to_string(),
                });
            }
            GraphDeltaResult::NodeRemoved(removed)
        }
        GraphDelta::ReplayAddNodeWithIdIfMissing { id, url, position } => {
            let capture_url = url.clone();
            let capture_position = [position.x, position.y];
            let added = graph.replay_add_node_with_id_if_missing(id, url, position);
            if added.is_some() {
                record_captured_delta(&CapturedDelta::ReplayAddNodeWithIdIfMissing {
                    id: id.to_string(),
                    url: capture_url,
                    position: capture_position,
                });
            }
            GraphDeltaResult::NodeMaybeAdded(added)
        }
        GraphDelta::ReplayAssertRelationByIds {
            from_id,
            to_id,
            assertion,
        } => {
            let capture_assertion = assertion.clone();
            let edge = graph.replay_assert_relation_by_ids(from_id, to_id, assertion);
            if edge.is_some() {
                record_captured_delta(&CapturedDelta::ReplayAssertRelationByIds {
                    from_id: from_id.to_string(),
                    to_id: to_id.to_string(),
                    assertion: capture_assertion,
                });
            }
            GraphDeltaResult::EdgeAdded(edge)
        }
        GraphDelta::ReplayRemoveNodeById { node_id } => {
            let removed = graph.replay_remove_node_by_id(node_id);
            if removed {
                record_captured_delta(&CapturedDelta::ReplayRemoveNodeById {
                    node_id: node_id.to_string(),
                });
            }
            GraphDeltaResult::NodeRemoved(removed)
        }
        GraphDelta::ReplayRetractRelationsByIds {
            from_id,
            to_id,
            selector,
        } => {
            let removed = graph.replay_retract_relations_by_ids(from_id, to_id, selector);
            if removed > 0 {
                record_captured_delta(&CapturedDelta::ReplayRetractRelationsByIds {
                    from_id: from_id.to_string(),
                    to_id: to_id.to_string(),
                    selector,
                });
            }
            GraphDeltaResult::EdgesRemoved(removed)
        }
        GraphDelta::ReplayAppendTraversalByIds {
            from_id,
            to_id,
            trigger,
            timestamp_ms,
        } => {
            let appended =
                graph.replay_append_traversal_by_ids(from_id, to_id, trigger, timestamp_ms);
            if appended {
                record_captured_delta(&CapturedDelta::ReplayAppendTraversalByIds {
                    from_id: from_id.to_string(),
                    to_id: to_id.to_string(),
                    trigger,
                    timestamp_ms,
                });
            }
            GraphDeltaResult::TraversalAppended(appended)
        }
        GraphDelta::RetractRelations { from, to, selector } => {
            let from_id = graph.get_node(from).map(|node| node.id);
            let to_id = graph.get_node(to).map(|node| node.id);
            let removed = graph.retract_relations(from, to, selector);
            if removed > 0
                && let (Some(from_id), Some(to_id)) = (from_id, to_id)
            {
                record_captured_delta(&CapturedDelta::ReplayRetractRelationsByIds {
                    from_id: from_id.to_string(),
                    to_id: to_id.to_string(),
                    selector,
                });
            }
            GraphDeltaResult::EdgesRemoved(removed)
        }
        GraphDelta::AppendTraversal {
            from,
            to,
            trigger,
            timestamp_ms,
        } => {
            let from_id = graph.get_node(from).map(|node| node.id);
            let to_id = graph.get_node(to).map(|node| node.id);
            let appended = graph.append_traversal(from, to, trigger, timestamp_ms);
            if appended
                && let (Some(from_id), Some(to_id)) = (from_id, to_id)
                && let Some(traversal) = graph
                    .find_edge_key(from, to)
                    .and_then(|edge| graph.get_edge(edge))
                    .and_then(|payload| payload.traversals().last())
                    .copied()
            {
                record_captured_delta(&CapturedDelta::ReplayAppendTraversalByIds {
                    from_id: from_id.to_string(),
                    to_id: to_id.to_string(),
                    trigger: traversal.trigger,
                    timestamp_ms: traversal.timestamp_ms,
                });
            }
            GraphDeltaResult::TraversalAppended(appended)
        }
        GraphDelta::ReplaySetNodeTitleById { node_id, title } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.set_node_title(key, title.clone()));
            if updated {
                record_captured_delta(&CapturedDelta::ReplaySetNodeTitleById {
                    node_id: node_id.to_string(),
                    title,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::SetNodeTitle { key, title } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_title = title.clone();
            let updated = graph.set_node_title(key, title);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplaySetNodeTitleById {
                    node_id: node_id.to_string(),
                    title: capture_title,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplaySetNodeUrlById { node_id, new_url } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .and_then(|key| graph.update_node_url(key, new_url.clone()));
            if updated.is_some() {
                record_captured_delta(&CapturedDelta::ReplaySetNodeUrlById {
                    node_id: node_id.to_string(),
                    new_url,
                });
            }
            GraphDeltaResult::NodeUrlUpdated(updated)
        }
        GraphDelta::SetNodeUrl { key, new_url } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_url = new_url.clone();
            let updated = graph.update_node_url(key, new_url);
            if updated.is_some()
                && let Some(node_id) = node_id
            {
                record_captured_delta(&CapturedDelta::ReplaySetNodeUrlById {
                    node_id: node_id.to_string(),
                    new_url: capture_url,
                });
            }
            GraphDeltaResult::NodeUrlUpdated(updated)
        }
        GraphDelta::ReplaySetNodeThumbnailById {
            node_id,
            png_bytes,
            width,
            height,
        } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.set_node_thumbnail(key, png_bytes.clone(), width, height));
            if updated {
                record_captured_delta(&CapturedDelta::ReplaySetNodeThumbnailById {
                    node_id: node_id.to_string(),
                    png_bytes,
                    width,
                    height,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::SetNodeThumbnail {
            key,
            png_bytes,
            width,
            height,
        } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_png_bytes = png_bytes.clone();
            let updated = graph.set_node_thumbnail(key, png_bytes, width, height);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplaySetNodeThumbnailById {
                    node_id: node_id.to_string(),
                    png_bytes: capture_png_bytes,
                    width,
                    height,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplaySetNodeFaviconById {
            node_id,
            rgba,
            width,
            height,
        } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.set_node_favicon(key, rgba.clone(), width, height));
            if updated {
                record_captured_delta(&CapturedDelta::ReplaySetNodeFaviconById {
                    node_id: node_id.to_string(),
                    rgba,
                    width,
                    height,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::SetNodeFavicon {
            key,
            rgba,
            width,
            height,
        } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_rgba = rgba.clone();
            let updated = graph.set_node_favicon(key, rgba, width, height);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplaySetNodeFaviconById {
                    node_id: node_id.to_string(),
                    rgba: capture_rgba,
                    width,
                    height,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplaySetNodeMimeHintById { node_id, mime_hint } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.set_node_mime_hint(key, mime_hint.clone()));
            if updated {
                record_captured_delta(&CapturedDelta::ReplaySetNodeMimeHintById {
                    node_id: node_id.to_string(),
                    mime_hint,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::SetNodeMimeHint { key, mime_hint } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_mime_hint = mime_hint.clone();
            let updated = graph.set_node_mime_hint(key, mime_hint);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplaySetNodeMimeHintById {
                    node_id: node_id.to_string(),
                    mime_hint: capture_mime_hint,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplaySetNodeNestedById { node_id, nested } => {
            let log = nested.clone().map(codicil::LogId::new);
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.set_node_nested(key, log.clone()));
            if updated {
                record_captured_delta(&CapturedDelta::ReplaySetNodeNestedById {
                    node_id: node_id.to_string(),
                    nested,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::SetNodeNested { key, nested } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_nested = nested.as_ref().map(|log| log.as_str().to_string());
            let updated = graph.set_node_nested(key, nested);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplaySetNodeNestedById {
                    node_id: node_id.to_string(),
                    nested: capture_nested,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplaySetNodePinnedById { node_id, is_pinned } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.set_node_pinned(key, is_pinned));
            if updated {
                record_captured_delta(&CapturedDelta::ReplaySetNodePinnedById {
                    node_id: node_id.to_string(),
                    is_pinned,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::SetNodePinned { key, is_pinned } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let updated = graph.set_node_pinned(key, is_pinned);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplaySetNodePinnedById {
                    node_id: node_id.to_string(),
                    is_pinned,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplayAppendFrameLayoutHintById { node_id, hint } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.append_frame_layout_hint(key, hint.clone()));
            if updated {
                record_captured_delta(&CapturedDelta::ReplayAppendFrameLayoutHintById {
                    node_id: node_id.to_string(),
                    hint,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::AppendFrameLayoutHint { key, hint } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_hint = hint.clone();
            let updated = graph.append_frame_layout_hint(key, hint);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplayAppendFrameLayoutHintById {
                    node_id: node_id.to_string(),
                    hint: capture_hint,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplayRemoveFrameLayoutHintById {
            node_id,
            hint_index,
        } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.remove_frame_layout_hint_at(key, hint_index));
            if updated {
                record_captured_delta(&CapturedDelta::ReplayRemoveFrameLayoutHintById {
                    node_id: node_id.to_string(),
                    hint_index,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::RemoveFrameLayoutHint { key, hint_index } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let updated = graph.remove_frame_layout_hint_at(key, hint_index);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplayRemoveFrameLayoutHintById {
                    node_id: node_id.to_string(),
                    hint_index,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplayMoveFrameLayoutHintById {
            node_id,
            from_index,
            to_index,
        } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.move_frame_layout_hint(key, from_index, to_index));
            if updated {
                record_captured_delta(&CapturedDelta::ReplayMoveFrameLayoutHintById {
                    node_id: node_id.to_string(),
                    from_index,
                    to_index,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::MoveFrameLayoutHint {
            key,
            from_index,
            to_index,
        } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let updated = graph.move_frame_layout_hint(key, from_index, to_index);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplayMoveFrameLayoutHintById {
                    node_id: node_id.to_string(),
                    from_index,
                    to_index,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplaySetFrameSplitOfferSuppressedById {
            node_id,
            suppressed,
        } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.set_frame_split_offer_suppressed(key, suppressed));
            if updated {
                record_captured_delta(&CapturedDelta::ReplaySetFrameSplitOfferSuppressedById {
                    node_id: node_id.to_string(),
                    suppressed,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::SetFrameSplitOfferSuppressed { key, suppressed } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let updated = graph.set_frame_split_offer_suppressed(key, suppressed);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplaySetFrameSplitOfferSuppressedById {
                    node_id: node_id.to_string(),
                    suppressed,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplayUpdateNodeHistoryById {
            node_id,
            entries,
            current_index,
        } => {
            let updated = graph.get_node_key_by_id(node_id).is_some_and(|key| {
                graph.set_node_history_state(key, entries.clone(), current_index)
            });
            if updated {
                record_captured_delta(&CapturedDelta::ReplayUpdateNodeHistoryById {
                    node_id: node_id.to_string(),
                    entries,
                    current_index,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::UpdateNodeHistory {
            key,
            entries,
            current_index,
        } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_entries = entries.clone();
            let updated = graph.set_node_history_state(key, entries, current_index);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplayUpdateNodeHistoryById {
                    node_id: node_id.to_string(),
                    entries: capture_entries,
                    current_index,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplaySetImportRecords { import_records } => {
            let changed = graph.set_import_records(import_records);
            if changed {
                capture_resolved_import_records(graph);
            }
            GraphDeltaResult::ImportRecordsUpdated(changed)
        }
        GraphDelta::ReplayTouchNodeLastVisitedById {
            node_id,
            timestamp_ms,
        } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.set_node_last_visited_at_ms(key, timestamp_ms));
            if updated {
                record_captured_delta(&CapturedDelta::ReplayTouchNodeLastVisitedById {
                    node_id: node_id.to_string(),
                    timestamp_ms,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::NavigateNode { key, url } => {
            let Some(node_id) = graph.get_node(key).map(|node| node.id) else {
                return GraphDeltaResult::Applied;
            };
            let timestamp_ms = Graph::epoch_ms();
            graph
                .nav
                .record_visit(node_id, &url, TransitionKind::UrlTyped, timestamp_ms);
            let last_session_visited = graph.current_session;
            if let Some(node) = graph.inner.node_mut(key) {
                node.last_session_visited = last_session_visited;
            }
            let _ = graph.update_node_url(key, url.clone());
            record_captured_delta(&CapturedDelta::ReplayNavigateNodeById {
                node_id: node_id.to_string(),
                url,
                transition: TransitionKind::UrlTyped,
                timestamp_ms,
                last_session_visited,
            });
            GraphDeltaResult::Applied
        }
        GraphDelta::BranchHistory { child, parent } => {
            if let (Some(child_id), Some(parent_id)) = (
                graph.get_node(child).map(|node| node.id),
                graph.get_node(parent).map(|node| node.id),
            ) {
                graph.nav.spawn(child_id, parent_id);
                record_captured_delta(&CapturedDelta::ReplayBranchHistoryByIds {
                    child_id: child_id.to_string(),
                    parent_id: parent_id.to_string(),
                });
            }
            GraphDeltaResult::Applied
        }
        GraphDelta::NodeHistoryBack { key } => {
            let Some(node_id) = graph.get_node(key).map(|node| node.id) else {
                return GraphDeltaResult::HistoryStepped(None);
            };
            let timestamp_ms = Graph::epoch_ms();
            let stepped = graph.nav.back(node_id, timestamp_ms);
            if let Some(url) = stepped.clone() {
                let _ = graph.update_node_url(key, url);
                record_captured_delta(&CapturedDelta::ReplayNodeHistoryBackById {
                    node_id: node_id.to_string(),
                    timestamp_ms,
                });
            }
            GraphDeltaResult::HistoryStepped(stepped)
        }
        GraphDelta::NodeHistoryForward { key } => {
            let Some(node_id) = graph.get_node(key).map(|node| node.id) else {
                return GraphDeltaResult::HistoryStepped(None);
            };
            let timestamp_ms = Graph::epoch_ms();
            let stepped = graph.nav.forward(node_id, timestamp_ms);
            if let Some(url) = stepped.clone() {
                let _ = graph.update_node_url(key, url);
                record_captured_delta(&CapturedDelta::ReplayNodeHistoryForwardById {
                    node_id: node_id.to_string(),
                    timestamp_ms,
                });
            }
            GraphDeltaResult::HistoryStepped(stepped)
        }
        GraphDelta::ReplayInsertNodeTagById { node_id, tag } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.insert_node_tag(key, tag.clone()));
            if updated {
                record_captured_delta(&CapturedDelta::ReplayInsertNodeTagById {
                    node_id: node_id.to_string(),
                    tag,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::InsertNodeTag { key, tag } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_tag = tag.clone();
            let updated = graph.insert_node_tag(key, tag);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplayInsertNodeTagById {
                    node_id: node_id.to_string(),
                    tag: capture_tag,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplayRemoveNodeTagById { node_id, tag } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.remove_node_tag(key, &tag));
            if updated {
                record_captured_delta(&CapturedDelta::ReplayRemoveNodeTagById {
                    node_id: node_id.to_string(),
                    tag,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::RemoveNodeTag { key, tag } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let updated = graph.remove_node_tag(key, &tag);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplayRemoveNodeTagById {
                    node_id: node_id.to_string(),
                    tag,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplaySetNodeBodyById { node_id, body } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.set_node_body(key, body.clone()));
            if updated {
                record_captured_delta(&CapturedDelta::ReplaySetNodeBodyById {
                    node_id: node_id.to_string(),
                    body,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::SetNodeBody { key, body } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_body = body.clone();
            let updated = graph.set_node_body(key, body);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplaySetNodeBodyById {
                    node_id: node_id.to_string(),
                    body: capture_body,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplayNavigateNodeById {
            node_id,
            url,
            transition,
            timestamp_ms,
            last_session_visited,
        } => {
            let applied = graph.get_node_key_by_id(node_id).is_some_and(|key| {
                graph
                    .nav
                    .record_visit(node_id, &url, transition, timestamp_ms);
                if let Some(node) = graph.inner.node_mut(key) {
                    node.last_session_visited = last_session_visited;
                }
                let _ = graph.update_node_url(key, url.clone());
                true
            });
            if applied {
                record_captured_delta(&CapturedDelta::ReplayNavigateNodeById {
                    node_id: node_id.to_string(),
                    url,
                    transition,
                    timestamp_ms,
                    last_session_visited,
                });
            }
            GraphDeltaResult::Applied
        }
        GraphDelta::ReplayBranchHistoryByIds {
            child_id,
            parent_id,
        } => {
            let applied = match (
                graph.get_node_key_by_id(child_id),
                graph.get_node_key_by_id(parent_id),
            ) {
                (Some(_child), Some(_parent)) => {
                    graph.nav.spawn(child_id, parent_id);
                    true
                }
                _ => false,
            };
            if applied {
                record_captured_delta(&CapturedDelta::ReplayBranchHistoryByIds {
                    child_id: child_id.to_string(),
                    parent_id: parent_id.to_string(),
                });
            }
            GraphDeltaResult::Applied
        }
        GraphDelta::ReplayNodeHistoryBackById {
            node_id,
            timestamp_ms,
        } => {
            let stepped = graph.get_node_key_by_id(node_id).and_then(|key| {
                let url = graph.nav.back(node_id, timestamp_ms)?;
                let _ = graph.update_node_url(key, url.clone());
                Some(url)
            });
            if stepped.is_some() {
                record_captured_delta(&CapturedDelta::ReplayNodeHistoryBackById {
                    node_id: node_id.to_string(),
                    timestamp_ms,
                });
            }
            GraphDeltaResult::HistoryStepped(stepped)
        }
        GraphDelta::ReplayNodeHistoryForwardById {
            node_id,
            timestamp_ms,
        } => {
            let stepped = graph.get_node_key_by_id(node_id).and_then(|key| {
                let url = graph.nav.forward(node_id, timestamp_ms)?;
                let _ = graph.update_node_url(key, url.clone());
                Some(url)
            });
            if stepped.is_some() {
                record_captured_delta(&CapturedDelta::ReplayNodeHistoryForwardById {
                    node_id: node_id.to_string(),
                    timestamp_ms,
                });
            }
            GraphDeltaResult::HistoryStepped(stepped)
        }
        GraphDelta::ReplayAppendNodePropertyById { node_id, property } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.append_node_property(key, property.clone()));
            if updated {
                record_captured_delta(&CapturedDelta::ReplayAppendNodePropertyById {
                    node_id: node_id.to_string(),
                    property,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::AppendNodeProperty { key, property } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_property = property.clone();
            let updated = graph.append_node_property(key, property);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplayAppendNodePropertyById {
                    node_id: node_id.to_string(),
                    property: capture_property,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplayAddNodeClassificationById {
            node_id,
            classification,
        } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.add_node_classification(key, classification.clone()));
            if updated {
                record_captured_delta(&CapturedDelta::ReplayAddNodeClassificationById {
                    node_id: node_id.to_string(),
                    classification,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::AddNodeClassification {
            key,
            classification,
        } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_classification = classification.clone();
            let updated = graph.add_node_classification(key, classification);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplayAddNodeClassificationById {
                    node_id: node_id.to_string(),
                    classification: capture_classification,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplayRemoveNodeClassificationById {
            node_id,
            scheme,
            value,
        } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.remove_node_classification(key, &scheme, &value));
            if updated {
                record_captured_delta(&CapturedDelta::ReplayRemoveNodeClassificationById {
                    node_id: node_id.to_string(),
                    scheme,
                    value,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::RemoveNodeClassification { key, scheme, value } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_scheme = scheme.clone();
            let capture_value = value.clone();
            let updated = graph.remove_node_classification(key, &scheme, &value);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplayRemoveNodeClassificationById {
                    node_id: node_id.to_string(),
                    scheme: capture_scheme,
                    value: capture_value,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplaySetNodeClassificationStatusById {
            node_id,
            scheme,
            value,
            status,
        } => {
            let updated = graph.get_node_key_by_id(node_id).is_some_and(|key| {
                graph.set_node_classification_status(key, &scheme, &value, status.clone())
            });
            if updated {
                record_captured_delta(&CapturedDelta::ReplaySetNodeClassificationStatusById {
                    node_id: node_id.to_string(),
                    scheme,
                    value,
                    status,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::SetNodeClassificationStatus {
            key,
            scheme,
            value,
            status,
        } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_scheme = scheme.clone();
            let capture_value = value.clone();
            let capture_status = status.clone();
            let updated = graph.set_node_classification_status(key, &scheme, &value, status);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplaySetNodeClassificationStatusById {
                    node_id: node_id.to_string(),
                    scheme: capture_scheme,
                    value: capture_value,
                    status: capture_status,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplaySetNodePrimaryClassificationById {
            node_id,
            scheme,
            value,
        } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.set_node_primary_classification(key, &scheme, &value));
            if updated {
                record_captured_delta(&CapturedDelta::ReplaySetNodePrimaryClassificationById {
                    node_id: node_id.to_string(),
                    scheme,
                    value,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::SetNodePrimaryClassification { key, scheme, value } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_scheme = scheme.clone();
            let capture_value = value.clone();
            let updated = graph.set_node_primary_classification(key, &scheme, &value);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplaySetNodePrimaryClassificationById {
                    node_id: node_id.to_string(),
                    scheme: capture_scheme,
                    value: capture_value,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplayRecordNodeDerivationById {
            node_id,
            derivation,
        } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.record_derivation(key, derivation.clone()));
            if updated {
                record_captured_delta(&CapturedDelta::ReplayRecordNodeDerivationById {
                    node_id: node_id.to_string(),
                    derivation,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::RecordNodeDerivation { key, derivation } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_derivation = derivation.clone();
            let updated = graph.record_derivation(key, derivation);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplayRecordNodeDerivationById {
                    node_id: node_id.to_string(),
                    derivation: capture_derivation,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplaySetNodeTagIconOverrideById { node_id, tag, icon } => {
            let updated = graph
                .get_node_key_by_id(node_id)
                .is_some_and(|key| graph.set_node_tag_icon_override(key, &tag, icon.clone()));
            if updated {
                record_captured_delta(&CapturedDelta::ReplaySetNodeTagIconOverrideById {
                    node_id: node_id.to_string(),
                    tag,
                    icon,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::SetNodeTagIconOverride { key, tag, icon } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let capture_tag = tag.clone();
            let capture_icon = icon.clone();
            let updated = graph.set_node_tag_icon_override(key, &tag, icon);
            if updated && let Some(node_id) = node_id {
                record_captured_delta(&CapturedDelta::ReplaySetNodeTagIconOverrideById {
                    node_id: node_id.to_string(),
                    tag: capture_tag,
                    icon: capture_icon,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplaySetEdgeSemanticPredicateByIds {
            from_id,
            to_id,
            predicate,
        } => {
            let updated =
                graph.replay_set_edge_semantic_predicate_by_ids(from_id, to_id, predicate.clone());
            if updated {
                record_captured_delta(&CapturedDelta::ReplaySetEdgeSemanticPredicateByIds {
                    from_id: from_id.to_string(),
                    to_id: to_id.to_string(),
                    predicate,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::ReplayAssertSemanticPredicateByIds {
            from_id,
            to_id,
            predicate,
        } => {
            let edge =
                graph.replay_assert_semantic_predicate_by_ids(from_id, to_id, predicate.clone());
            if edge.is_some() {
                record_captured_delta(&CapturedDelta::ReplayAssertSemanticPredicateByIds {
                    from_id: from_id.to_string(),
                    to_id: to_id.to_string(),
                    predicate,
                });
            }
            GraphDeltaResult::EdgeAdded(edge)
        }
        GraphDelta::SetEdgeSemanticPredicate { edge, predicate } => {
            let endpoints = graph
                .inner
                .inner()
                .edge_endpoints(edge)
                .and_then(|(from, to)| Some((graph.get_node(from)?.id, graph.get_node(to)?.id)));
            let capture_predicate = predicate.clone();
            let updated = graph.set_edge_semantic_predicate(edge, predicate);
            if updated && let Some((from_id, to_id)) = endpoints {
                record_captured_delta(&CapturedDelta::ReplaySetEdgeSemanticPredicateByIds {
                    from_id: from_id.to_string(),
                    to_id: to_id.to_string(),
                    predicate: capture_predicate,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
        GraphDelta::AssertSemanticPredicate {
            from,
            to,
            predicate,
        } => {
            let from_id = graph.get_node(from).map(|node| node.id);
            let to_id = graph.get_node(to).map(|node| node.id);
            let capture_predicate = predicate.clone();
            let edge = graph.assert_semantic_predicate(from, to, predicate);
            if edge.is_some()
                && let (Some(from_id), Some(to_id)) = (from_id, to_id)
            {
                record_captured_delta(&CapturedDelta::ReplayAssertSemanticPredicateByIds {
                    from_id: from_id.to_string(),
                    to_id: to_id.to_string(),
                    predicate: capture_predicate,
                });
            }
            GraphDeltaResult::EdgeAdded(edge)
        }
        GraphDelta::ReplayAddField { field } => {
            let changed = if let Some(field) = field_from_persisted(&field) {
                graph.add_field(field);
                true
            } else {
                false
            };
            if changed {
                record_captured_delta(&CapturedDelta::ReplayAddField { field });
            }
            GraphDeltaResult::FieldChanged(changed)
        }
        GraphDelta::ReplayRetireFieldById { field_id } => {
            let changed = Uuid::parse_str(&field_id)
                .ok()
                .map(FieldId::from_uuid)
                .is_some_and(|id| graph.retire_field(id));
            if changed {
                record_captured_delta(&CapturedDelta::ReplayRetireFieldById { field_id });
            }
            GraphDeltaResult::FieldChanged(changed)
        }
        GraphDelta::ReplayAddCoupling { coupling } => {
            let changed = if let Some(coupling_model) = coupling_from_persisted(&coupling) {
                graph.add_coupling(coupling_model);
                true
            } else {
                false
            };
            if changed {
                record_captured_delta(&CapturedDelta::ReplayAddCoupling { coupling });
            }
            GraphDeltaResult::FieldChanged(changed)
        }
        GraphDelta::ReplaySetFieldCouplingStrengthByFieldId { field_id, strength } => {
            let changed = Uuid::parse_str(&field_id)
                .ok()
                .map(FieldId::from_uuid)
                .is_some_and(|field| graph.set_field_coupling_strength(field, strength));
            if changed {
                record_captured_delta(&CapturedDelta::ReplaySetFieldCouplingStrengthByFieldId {
                    field_id,
                    strength,
                });
            }
            GraphDeltaResult::FieldChanged(changed)
        }
        GraphDelta::ReplayActivateFieldById { field_id } => {
            let changed = Uuid::parse_str(&field_id)
                .ok()
                .map(FieldId::from_uuid)
                .is_some_and(|id| graph.activate_field(id));
            if changed {
                record_captured_delta(&CapturedDelta::ReplayActivateFieldById { field_id });
            }
            GraphDeltaResult::FieldChanged(changed)
        }
        GraphDelta::ReplayRetractCouplingById { coupling_id } => {
            let changed = Uuid::parse_str(&coupling_id)
                .ok()
                .map(CouplingId::from_uuid)
                .is_some_and(|id| graph.retract_coupling(id));
            if changed {
                record_captured_delta(&CapturedDelta::ReplayRetractCouplingById { coupling_id });
            }
            GraphDeltaResult::FieldChanged(changed)
        }
        GraphDelta::AddField { field } => {
            let capture_field = persisted_field_from_field(&field);
            graph.add_field(field);
            record_captured_delta(&CapturedDelta::ReplayAddField {
                field: capture_field,
            });
            GraphDeltaResult::FieldChanged(true)
        }
        GraphDelta::RetireField { id } => {
            let changed = graph.retire_field(id);
            if changed {
                record_captured_delta(&CapturedDelta::ReplayRetireFieldById {
                    field_id: id.as_uuid().to_string(),
                });
            }
            GraphDeltaResult::FieldChanged(changed)
        }
        GraphDelta::AddCoupling { coupling } => {
            let capture_coupling = persisted_coupling_from_coupling(&coupling);
            graph.add_coupling(coupling);
            record_captured_delta(&CapturedDelta::ReplayAddCoupling {
                coupling: capture_coupling,
            });
            GraphDeltaResult::FieldChanged(true)
        }
        GraphDelta::SetFieldCouplingStrength { field, strength } => {
            let changed = graph.set_field_coupling_strength(field, strength);
            if changed {
                record_captured_delta(&CapturedDelta::ReplaySetFieldCouplingStrengthByFieldId {
                    field_id: field.as_uuid().to_string(),
                    strength,
                });
            }
            GraphDeltaResult::FieldChanged(changed)
        }
        GraphDelta::ActivateField { id } => {
            let changed = graph.activate_field(id);
            if changed {
                record_captured_delta(&CapturedDelta::ReplayActivateFieldById {
                    field_id: id.as_uuid().to_string(),
                });
            }
            GraphDeltaResult::FieldChanged(changed)
        }
        GraphDelta::RetractCoupling { id } => {
            let changed = graph.retract_coupling(id);
            if changed {
                record_captured_delta(&CapturedDelta::ReplayRetractCouplingById {
                    coupling_id: id.as_uuid().to_string(),
                });
            }
            GraphDeltaResult::FieldChanged(changed)
        }
        GraphDelta::SetImportRecords { import_records } => {
            let changed = graph.set_import_records(import_records);
            if changed {
                capture_resolved_import_records(graph);
            }
            GraphDeltaResult::ImportRecordsUpdated(changed)
        }
        GraphDelta::DeleteImportRecord { record_id } => {
            let changed = graph.delete_import_record(&record_id);
            if changed {
                capture_resolved_import_records(graph);
            }
            GraphDeltaResult::ImportRecordsUpdated(changed)
        }
        GraphDelta::SetImportRecordMembershipSuppressed {
            record_id,
            key,
            suppressed,
        } => {
            let changed =
                graph.set_import_record_membership_suppressed(&record_id, key, suppressed);
            if changed {
                capture_resolved_import_records(graph);
            }
            GraphDeltaResult::ImportRecordsUpdated(changed)
        }
        GraphDelta::SetNodeImportProvenance {
            key,
            import_provenance,
        } => {
            let changed = graph.set_node_import_provenance(key, import_provenance);
            if changed {
                capture_resolved_import_records(graph);
            }
            GraphDeltaResult::ImportRecordsUpdated(changed)
        }
        GraphDelta::TouchNodeLastVisited { key } => {
            let node_id = graph.get_node(key).map(|node| node.id);
            let updated = graph.touch_node_last_visited_now(key);
            if updated
                && let Some(node_id) = node_id
                && let Some(timestamp_ms) = graph
                    .get_node(key)
                    .and_then(|node| node.last_visited.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_millis() as u64)
            {
                record_captured_delta(&CapturedDelta::ReplayTouchNodeLastVisitedById {
                    node_id: node_id.to_string(),
                    timestamp_ms,
                });
            }
            GraphDeltaResult::NodeMetadataUpdated(updated)
        }
    }
}

// --- Ergonomic wrappers (write-path migration, 2026-07-01) ---
//
// Thin constructors over [`apply_graph_delta`] for the deltas whose result the
// caller almost always unwraps. They keep the funnel — every one routes through
// `apply_graph_delta`, so a future recording hook (the event log) instruments
// exactly one function — while sparing hot call sites the enum-unwrap noise.

/// Add a node (fresh random id when `id` is `None`; not available on wasm
/// without an explicit id) and return its key.
pub fn add_node(
    graph: &mut Graph,
    id: Option<Uuid>,
    url: String,
    position: Point2D<f32>,
) -> NodeKey {
    match apply_graph_delta(graph, GraphDelta::AddNode { id, url, position }) {
        GraphDeltaResult::NodeAdded(key) => key,
        other => unreachable!("AddNode returned {other:?}"),
    }
}

/// Assert a relation between two nodes, returning the edge key (or `None` when
/// either endpoint is gone).
pub fn assert_relation(
    graph: &mut Graph,
    from: NodeKey,
    to: NodeKey,
    assertion: EdgeAssertion,
) -> Option<EdgeKey> {
    match apply_graph_delta(
        graph,
        GraphDelta::AssertRelation {
            from,
            to,
            assertion,
        },
    ) {
        GraphDeltaResult::EdgeAdded(key) => key,
        other => unreachable!("AssertRelation returned {other:?}"),
    }
}

/// Assert a recognized semantic statement in a specific named-graph scope.
pub fn assert_semantic_relation_in_scope(
    graph: &mut Graph,
    from: NodeKey,
    to: NodeKey,
    sub_kind: SemanticSubKind,
    label: Option<String>,
    graph_scope: GraphScope,
) -> Option<EdgeKey> {
    graph.assert_semantic_relation_in_scope(from, to, sub_kind, label, graph_scope)
}

/// Assert an open-predicate semantic statement in a specific named-graph scope.
pub fn assert_semantic_predicate_in_scope(
    graph: &mut Graph,
    from: NodeKey,
    to: NodeKey,
    predicate: String,
    graph_scope: GraphScope,
) -> Option<EdgeKey> {
    graph.assert_semantic_predicate_in_scope(from, to, predicate, graph_scope)
}

/// Step a node back one visit in its own browse history, returning the
/// revealed URL (`None` at the root).
pub fn node_history_back(graph: &mut Graph, key: NodeKey) -> Option<String> {
    match apply_graph_delta(graph, GraphDelta::NodeHistoryBack { key }) {
        GraphDeltaResult::HistoryStepped(url) => url,
        other => unreachable!("NodeHistoryBack returned {other:?}"),
    }
}

/// Step a node forward one visit in its own browse history, returning the
/// revealed URL (`None` at the tip).
pub fn node_history_forward(graph: &mut Graph, key: NodeKey) -> Option<String> {
    match apply_graph_delta(graph, GraphDelta::NodeHistoryForward { key }) {
        GraphDeltaResult::HistoryStepped(url) => url,
        other => unreachable!("NodeHistoryForward returned {other:?}"),
    }
}
