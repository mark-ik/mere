/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

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

use super::edge_payload::EdgePayload;
use super::edge_taxonomy::{EdgeAssertion, RelationSelector, Traversal};
use super::identity::{EdgeKey, NodeKey};
use super::{DissolvedTraversalRecord, Graph};

impl Graph {
    pub fn assert_relation(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        assertion: EdgeAssertion,
    ) -> Option<EdgeKey> {
        if !self.inner.contains_node(from) || !self.inner.contains_node(to) {
            return None;
        }
        if let Some(edge_key) = self.find_edge_key(from, to) {
            let payload = self.inner.edge_weight_mut(edge_key)?;
            return payload.assert_relation(assertion).then_some(edge_key);
        }
        let mut payload = EdgePayload::new();
        if !payload.assert_relation(assertion) {
            return None;
        }
        Some(self.inner.add_edge(from, to, payload))
    }

    /// Replay helper: add node only if UUID is not already present.
    pub fn replay_add_node_with_id_if_missing(
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
    pub fn replay_remove_node_by_id(&mut self, node_id: Uuid) -> bool {
        let Some(key) = self.get_node_key_by_id(node_id) else {
            return false;
        };
        self.remove_node(key)
    }

    pub fn replay_retract_relations_by_ids(
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

    pub fn replay_assert_relation_by_ids(
        &mut self,
        from_id: Uuid,
        to_id: Uuid,
        assertion: EdgeAssertion,
    ) -> Option<EdgeKey> {
        let from_key = self.get_node_key_by_id(from_id)?;
        let to_key = self.get_node_key_by_id(to_id)?;
        self.assert_relation(from_key, to_key, assertion)
    }

    /// Dissolve helper: collect traversals from all incident edges and remove the node.
    pub fn dissolve_remove_node_collect_traversals(
        &mut self,
        key: NodeKey,
    ) -> Option<Vec<DissolvedTraversalRecord>> {
        let _ = self.get_node(key)?;

        let mut records = Vec::new();
        for edge in self
            .inner
            .edges_directed(key, Direction::Outgoing)
            .chain(self.inner.edges_directed(key, Direction::Incoming))
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
        for edge in self
            .inner
            .edges_directed(key, Direction::Outgoing)
            .chain(self.inner.edges_directed(key, Direction::Incoming))
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

    pub fn retract_relations(
        &mut self,
        from: NodeKey,
        to: NodeKey,
        selector: RelationSelector,
    ) -> usize {
        let edge_ids: Vec<EdgeKey> = self
            .inner
            .edge_references()
            .filter(|edge| {
                edge.source() == from && edge.target() == to && edge.weight().has_relation(selector)
            })
            .map(|edge| edge.id())
            .collect();

        let mut removed = 0usize;
        let mut edges_to_delete = Vec::new();
        for edge_id in edge_ids {
            if let Some(payload) = self.inner.edge_weight_mut(edge_id)
                && payload.retract_relation(selector)
            {
                removed += 1;
                if payload.is_empty() {
                    edges_to_delete.push(edge_id);
                }
            }
        }
        for edge_id in edges_to_delete {
            let _ = self.inner.remove_edge(edge_id);
        }
        removed
    }

    /// Get a mutable edge payload by key.
    pub fn get_edge_mut(&mut self, key: EdgeKey) -> Option<&mut EdgePayload> {
        self.inner.edge_weight_mut(key)
    }

    /// Get an edge payload by key.
    pub fn get_edge(&self, key: EdgeKey) -> Option<&EdgePayload> {
        self.inner.edge_weight(key)
    }

    /// Find the first directed edge key between two nodes.
    pub fn find_edge_key(&self, from: NodeKey, to: NodeKey) -> Option<EdgeKey> {
        self.inner.find_edge(from, to)
    }

    /// Append a traversal event to an existing edge, or create an edge carrying the traversal.
    pub fn push_traversal(&mut self, from: NodeKey, to: NodeKey, traversal: Traversal) -> bool {
        if from == to || !self.inner.contains_node(from) || !self.inner.contains_node(to) {
            return false;
        }
        if let Some(edge_key) = self.find_edge_key(from, to)
            && let Some(payload) = self.inner.edge_weight_mut(edge_key)
        {
            payload.push_traversal(traversal);
            return true;
        }
        let mut payload = EdgePayload::new();
        payload.push_traversal(traversal);
        let _ = self.inner.add_edge(from, to, payload);
        true
    }

    /// Append a traversal event by `(trigger, timestamp_ms?)`.
    /// Thin wrapper over [`Self::push_traversal`] matching the
    /// 2026-05-11 relation-taxonomy plan §7 signature — callers
    /// that just have a `NavigationTrigger` don't need to build a
    /// `Traversal` struct themselves. `timestamp_ms = None` stamps
    /// now-via-`Traversal::now`; `Some(t)` uses the supplied
    /// timestamp (importers, replay).
    pub fn append_traversal(
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
