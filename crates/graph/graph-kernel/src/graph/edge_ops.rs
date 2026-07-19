// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Edge operations — assert, replay, dissolve, retract, traversal
//! recording.
//!
//! Extracted from `graph/mod.rs` per the 2026-05-11 kernel
//! decomposition pass. Stage 4 of the 2026-05-11 relation-taxonomy
//! plan removed the legacy `add_edge` / `remove_edges` /
//! `replay_add_edge_by_ids` / `replay_remove_edges_by_ids` paths and
//! their `EdgeType`-shaped helpers; everything goes through
//! [`EdgeAssertion`] / [`RelationSelector`] now.

use euclid::default::Point2D;
use petgraph::Direction;
use petgraph::visit::{EdgeRef, IntoEdgeReferences};
use uuid::Uuid;

use crate::types::GraphScope;

use super::edge_data::{SemanticStatementSpec, StatementAssert, SemanticStatement};
use super::edge_data::Traversal;
use super::edge_payload::EdgePayload;
use super::edge_taxonomy::{EdgeAssertion, RelationSelector, SemanticSubKind};
use super::identity::{EdgeKey, NodeKey};
use super::{DissolvedTraversalRecord, Graph};

impl Graph {
    pub(crate) fn assert_relation(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        assertion: EdgeAssertion,
    ) -> Option<EdgeKey> {
        if let EdgeAssertion::Semantic {
            sub_kind,
            label,
            decay_progress: _,
        } = assertion
        {
            return self.assert_semantic_relation_in_scope(
                from,
                to,
                sub_kind,
                label,
                GraphScope::Default,
            );
        }
        if !self.inner.contains_node(from) || !self.inner.contains_node(to) {
            return None;
        }
        if let Some(edge_key) = self.find_edge_key(from, to) {
            // Existing edge: assert onto its payload. A real change (a new relation on the pair —
            // a multiplicity bump a weighted signal reads) advances the revision.
            let changed = {
                let payload = self.inner.edge_mut(edge_key)?;
                payload.assert_relation(assertion)
            };
            if changed {
                self.bump_revision();
                return Some(edge_key);
            }
            return None;
        }
        let mut payload = EdgePayload::new();
        if !payload.assert_relation(assertion) {
            return None;
        }
        let edge_key = self.inner.connect(from, to, payload);
        self.bump_revision();
        Some(edge_key)
    }

    pub(crate) fn assert_semantic_relation_in_scope(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        sub_kind: SemanticSubKind,
        label: Option<String>,
        graph_scope: GraphScope,
    ) -> Option<EdgeKey> {
        if !self.inner.contains_node(from) || !self.inner.contains_node(to) {
            return None;
        }
        let edge_key = self
            .find_edge_key(from, to)
            .unwrap_or_else(|| self.inner.connect(from, to, EdgePayload::new()));
        let changed = {
            let payload = self.inner.edge_mut(edge_key)?;
            payload.assert_semantic_relation_in_scope(sub_kind, label, graph_scope)
        };
        if changed {
            self.bump_revision();
            return Some(edge_key);
        }
        None
    }

    /// Assert an **open predicate** semantic relation from `from` to `to`,
    /// identified by an IRI rather than a closed [`SemanticSubKind`]. Creates the
    /// pair-local edge bucket if absent and appends one open semantic statement to
    /// it; an edge carrying only such a statement still reports the `Semantic`
    /// family. Returns the edge, or `None` if either endpoint is missing or the
    /// same open predicate statement already exists. Write path for raw web
    /// predicates (linked-data ingest / knot `rel`s outside Mere's vocabulary).
    pub(crate) fn assert_semantic_predicate(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        predicate: String,
    ) -> Option<EdgeKey> {
        self.assert_semantic_predicate_in_scope(from, to, predicate, GraphScope::Default)
    }

    pub(crate) fn assert_semantic_predicate_in_scope(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        predicate: String,
        graph_scope: GraphScope,
    ) -> Option<EdgeKey> {
        if !self.inner.contains_node(from) || !self.inner.contains_node(to) {
            return None;
        }
        let edge_key = self
            .find_edge_key(from, to)
            .unwrap_or_else(|| self.inner.connect(from, to, EdgePayload::new()));
        let changed = {
            let payload = self.inner.edge_mut(edge_key)?;
            payload.assert_semantic_predicate_in_scope(predicate, graph_scope)
        };
        if changed {
            self.bump_revision();
            return Some(edge_key);
        }
        None
    }

    /// Replay helper: add node only if UUID is not already present.
    pub(crate) fn replay_add_node_with_id_if_missing(
        &mut self,
        id: Uuid,
        url: String,
        position: Point2D<f32>,
    ) -> Option<NodeKey> {
        if self.get_node_key_by_id(id).is_some() {
            return None;
        }
        Some(self.add_node_with_id(id, url, position))
    }

    /// Replay helper: remove node by stable UUID.
    pub(crate) fn replay_remove_node_by_id(&mut self, node_id: Uuid) -> bool {
        let Some(key) = self.get_node_key_by_id(node_id) else {
            return false;
        };
        self.remove_node(key)
    }

    pub(crate) fn replay_retract_relations_by_ids(
        &mut self,
        from_id: Uuid,
        to_id: Uuid,
        selector: RelationSelector,
    ) -> usize {
        let Some(from_key) = self.get_node_key_by_id(from_id) else {
            return 0;
        };
        let Some(to_key) = self.get_node_key_by_id(to_id) else {
            return 0;
        };
        self.retract_relations(from_key, to_key, selector)
    }

    pub(crate) fn replay_assert_relation_by_ids(
        &mut self,
        from_id: Uuid,
        to_id: Uuid,
        assertion: EdgeAssertion,
    ) -> Option<EdgeKey> {
        let from_key = self.get_node_key_by_id(from_id)?;
        let to_key = self.get_node_key_by_id(to_id)?;
        self.assert_relation(from_key, to_key, assertion)
    }

    pub(crate) fn replay_set_edge_semantic_predicate_by_ids(
        &mut self,
        from_id: Uuid,
        to_id: Uuid,
        predicate: Option<String>,
    ) -> bool {
        let Some(from_key) = self.get_node_key_by_id(from_id) else {
            return false;
        };
        let Some(to_key) = self.get_node_key_by_id(to_id) else {
            return false;
        };
        let Some(edge_key) = self.find_edge_key(from_key, to_key) else {
            return false;
        };
        self.set_edge_semantic_predicate(edge_key, predicate)
    }

    pub(crate) fn replay_assert_semantic_predicate_by_ids(
        &mut self,
        from_id: Uuid,
        to_id: Uuid,
        predicate: String,
    ) -> Option<EdgeKey> {
        let from_key = self.get_node_key_by_id(from_id)?;
        let to_key = self.get_node_key_by_id(to_id)?;
        self.assert_semantic_predicate(from_key, to_key, predicate)
    }

    pub(crate) fn replay_append_traversal_by_ids(
        &mut self,
        from_id: Uuid,
        to_id: Uuid,
        trigger: super::edge_taxonomy::NavigationTrigger,
        timestamp_ms: u64,
    ) -> bool {
        let Some(from_key) = self.get_node_key_by_id(from_id) else {
            return false;
        };
        let Some(to_key) = self.get_node_key_by_id(to_id) else {
            return false;
        };
        self.append_traversal(from_key, to_key, trigger, Some(timestamp_ms))
    }

    /// Dissolve helper: collect traversals from all incident edges and remove the node.
    pub(crate) fn dissolve_remove_node_collect_traversals(
        &mut self,
        key: NodeKey,
    ) -> Option<Vec<DissolvedTraversalRecord>> {
        let _ = self.get_node(key)?;

        let mut records = Vec::new();
        let inner = self.inner.inner();
        for edge in inner
            .edges_directed(key, Direction::Outgoing)
            .chain(inner.edges_directed(key, Direction::Incoming))
        {
            if edge.weight().traversals().is_empty() {
                continue;
            }

            let from_node = self.get_node(edge.source())?;
            let to_node = self.get_node(edge.target())?;
            records.push(DissolvedTraversalRecord {
                from_node_id: from_node.id,
                to_node_id: to_node.id,
                traversals: edge.weight().traversals().to_vec(),
            });
        }

        let _ = self.remove_node(key);
        Some(records)
    }

    /// Collect traversals from all incident edges without mutating graph state.
    pub fn collect_node_traversals(&self, key: NodeKey) -> Option<Vec<DissolvedTraversalRecord>> {
        let _ = self.get_node(key)?;

        let mut records = Vec::new();
        let inner = self.inner.inner();
        for edge in inner
            .edges_directed(key, Direction::Outgoing)
            .chain(inner.edges_directed(key, Direction::Incoming))
        {
            if edge.weight().traversals().is_empty() {
                continue;
            }

            let from_node = self.get_node(edge.source())?;
            let to_node = self.get_node(edge.target())?;
            records.push(DissolvedTraversalRecord {
                from_node_id: from_node.id,
                to_node_id: to_node.id,
                traversals: edge.weight().traversals().to_vec(),
            });
        }

        Some(records)
    }

    /// Statement-aware assert (petgraph-RDF Phase 1 write API): assert one
    /// semantic statement with full per-statement metadata on the `(from, to)`
    /// pair bucket, creating the bucket edge if absent. Content-dedup returns
    /// the EXISTING fact handle (updating its metadata in place); a genuine
    /// change bumps the revision. `None` only when an endpoint is missing.
    pub fn assert_semantic_statement(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        spec: SemanticStatementSpec,
    ) -> Option<(EdgeKey, StatementAssert)> {
        if !self.inner.contains_node(from) || !self.inner.contains_node(to) {
            return None;
        }
        let edge_key = self
            .find_edge_key(from, to)
            .unwrap_or_else(|| self.inner.connect(from, to, EdgePayload::new()));
        let outcome = {
            let payload = self.inner.edge_mut(edge_key)?;
            payload.assert_semantic_statement(spec)
        };
        if outcome.changed {
            self.bump_revision();
        }
        Some((edge_key, outcome))
    }

    /// Assert a semantic statement whose id is ALREADY minted — the re-ingest /
    /// round-trip path (a reifier carried the fact handle, and preserving it is
    /// what makes `RDF -> kernel -> RDF` id-stable). Same content-dedup as the
    /// persisted-snapshot load; endpoints must exist.
    pub fn assert_persisted_semantic_statement(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        statement: SemanticStatement,
    ) -> Option<EdgeKey> {
        if !self.inner.contains_node(from) || !self.inner.contains_node(to) {
            return None;
        }
        let edge_key = self
            .find_edge_key(from, to)
            .unwrap_or_else(|| self.inner.connect(from, to, EdgePayload::new()));
        let changed = {
            let payload = self.inner.edge_mut(edge_key)?;
            payload.push_persisted_semantic_statement(statement)
        };
        if changed {
            self.bump_revision();
        }
        Some(edge_key)
    }

    /// Precise retract by fact handle (the id `assert_semantic_statement`
    /// returned, also carried on every projected reifier). Removes the
    /// statement from the pair bucket; an emptied payload removes the petgraph
    /// edge itself, so "there is an edge A -> B" keeps meaning "at least one
    /// fact or other family payload exists on the pair".
    pub fn retract_semantic_statement(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        statement_id: &str,
    ) -> bool {
        let Some(edge_key) = self.find_edge_key(from, to) else {
            return false;
        };
        let (removed, now_empty) = {
            let Some(payload) = self.inner.edge_mut(edge_key) else {
                return false;
            };
            let removed = payload.retract_semantic_statement(statement_id);
            (removed, removed && payload.is_empty())
        };
        if now_empty {
            let _ = self.inner.disconnect(edge_key);
        }
        if removed {
            self.bump_revision();
        }
        removed
    }

    pub(crate) fn retract_relations(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        selector: RelationSelector,
    ) -> usize {
        let edge_ids: Vec<EdgeKey> = self
            .inner
            .inner()
            .edge_references()
            .filter(|edge| {
                edge.source() == from && edge.target() == to && edge.weight().has_relation(selector)
            })
            .map(|edge| edge.id())
            .collect();

        let mut removed = 0usize;
        let mut edges_to_delete = Vec::new();
        for edge_id in edge_ids {
            if let Some(payload) = self.inner.edge_mut(edge_id)
                && payload.retract_relation(selector)
            {
                removed += 1;
                if payload.is_empty() {
                    edges_to_delete.push(edge_id);
                }
            }
        }
        for edge_id in edges_to_delete {
            let _ = self.inner.disconnect(edge_id);
        }
        if removed > 0 {
            self.bump_revision();
        }
        removed
    }

    /// Get a mutable edge payload by key.
    pub(crate) fn get_edge_mut(&mut self, key: EdgeKey) -> Option<&mut EdgePayload> {
        self.inner.edge_mut(key)
    }

    /// Set (or clear) the canonical semantic-predicate IRI on an existing edge.
    /// Returns whether the edge exists. The sanctioned write path for a payload's
    /// predicate — the linked-data ingest and inker statements previously reached
    /// it through `get_edge_mut` (write-path migration, 2026-07-01). A content
    /// annotation on an existing edge, not a structural change, so the revision
    /// holds (same rule as title/tag edits).
    pub(crate) fn set_edge_semantic_predicate(
        &mut self,
        key: EdgeKey,
        predicate: Option<String>,
    ) -> bool {
        let Some(payload) = self.inner.edge_mut(key) else {
            return false;
        };
        payload.set_semantic_predicate(predicate);
        true
    }

    /// Get an edge payload by key.
    pub fn get_edge(&self, key: EdgeKey) -> Option<&EdgePayload> {
        self.inner.edge(key)
    }

    /// Find the first directed edge key between two nodes.
    pub fn find_edge_key(&self, from: NodeKey, to: NodeKey) -> Option<EdgeKey> {
        self.inner.inner().find_edge(from, to)
    }

    /// Append a traversal event to an existing edge, or create an edge carrying the traversal.
    pub(crate) fn push_traversal(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        traversal: Traversal,
    ) -> bool {
        if from == to || !self.inner.contains_node(from) || !self.inner.contains_node(to) {
            return false;
        }
        if let Some(edge_key) = self.find_edge_key(from, to)
            && let Some(payload) = self.inner.edge_mut(edge_key)
        {
            // Existing edge: a re-visit appends a traversal but does not change the structure, so
            // the revision holds (navigation re-visits must not churn the structural caches).
            payload.push_traversal(traversal);
            return true;
        }
        let mut payload = EdgePayload::new();
        payload.push_traversal(traversal);
        let _ = self.inner.connect(from, to, payload);
        self.bump_revision();
        true
    }

    /// Append a traversal event by `(trigger, timestamp_ms?)`.
    /// Thin wrapper over [`Self::push_traversal`] matching the
    /// 2026-05-11 relation-taxonomy plan §7 signature — callers
    /// that just have a `NavigationTrigger` don't need to build a
    /// `Traversal` struct themselves. `timestamp_ms = None` stamps
    /// now-via-`Traversal::now`; `Some(t)` uses the supplied
    /// timestamp (importers, replay).
    pub(crate) fn append_traversal(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        trigger: super::edge_taxonomy::NavigationTrigger,
        timestamp_ms: Option<u64>,
    ) -> bool {
        let traversal = match timestamp_ms {
            Some(timestamp_ms) => Traversal {
                timestamp_ms,
                trigger,
            },
            None => Traversal::now(trigger),
        };
        self.push_traversal(from, to, traversal)
    }
}
