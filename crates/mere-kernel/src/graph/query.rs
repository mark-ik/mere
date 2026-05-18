/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Graph query + traversal — node / edge lookups, neighbor
//! iteration, BFS / shortest-path / connected-components / SCC, and
//! the derived-edge views (semantic / arrangement / containment).
//!
//! Extracted from `graph/mod.rs` per the 2026-05-11 kernel
//! decomposition pass. These are the read-only and read-mostly
//! methods on `Graph`; the mutating counterparts live in `mod.rs`
//! (node-property setters), `edge_ops.rs` (edge mutators), and
//! `snapshot.rs` (serialization).

use std::collections::{HashMap, HashSet, VecDeque};

use petgraph::Direction;
use petgraph::algo::{astar, dijkstra, has_path_connecting, kosaraju_scc};
use petgraph::visit::{EdgeRef, IntoEdgeReferences, UndirectedAdaptor};
use uuid::Uuid;

use super::edge_taxonomy::{
    ContainmentSubKind, EdgeAssertion, RelationSelector,
};
use super::identity::{EdgeKey, NodeKey};
use super::node::Node;
use super::snapshot::containment_parent_url;
use super::{
    ArrangementEdgeView, ContainmentEdgeView, Graph, RelationView, SemanticEdgeView,
};

impl Graph {
    /// Get a node by key
    pub fn get_node(&self, key: NodeKey) -> Option<&Node> {
        self.inner.node_weight(key)
    }

    /// Get a mutable node by key.
    pub fn get_node_mut(&mut self, key: NodeKey) -> Option<&mut Node> {
        self.inner.node_weight_mut(key)
    }

    /// Find a node whose AddressClaims include the given address (any
    /// `AddressRole`). Returns an arbitrary matching node when multiple
    /// nodes share the address — the caller is expected to handle
    /// duplicates explicitly when relevant.
    ///
    /// Canonical address lookup per the [node identity + duplicates
    /// brief](https://github.com/mark-ik/mere/blob/main/design_docs/mere_docs/research/2026-05-18_node_identity_and_duplicates_brief.md);
    /// supersedes the URL-string-keyed `get_node_by_url` (which is
    /// retained as a transitional shim — see below).
    pub fn find_node_by_address(&self, address: &crate::address::Address) -> Option<(NodeKey, &Node)> {
        let url = address.as_url_str();
        let key = self.url_to_nodes.get(url)?.last().copied()?;
        Some((key, self.inner.node_weight(key)?))
    }

    /// Transitional shim — delegates to [`Graph::find_node_by_address`]
    /// after building an `Address` from the URL string. Kept while
    /// Phase 2 of the node-identity rollout updates host call sites; it
    /// will be removed when the host helpers (`find_node_by_address` /
    /// `create_node_for_address` in `host_helpers`) land.
    pub fn get_node_by_url(&self, url: &str) -> Option<(NodeKey, &Node)> {
        let address = crate::address::address_from_url(url);
        self.find_node_by_address(&address)
    }

    /// Get all node keys currently mapped to a URL.
    pub fn get_nodes_by_url(&self, url: &str) -> Vec<NodeKey> {
        self.url_to_nodes.get(url).cloned().unwrap_or_default()
    }

    /// Get a node by UUID.
    pub fn get_node_by_id(&self, id: Uuid) -> Option<(NodeKey, &Node)> {
        let key = *self.id_to_node.get(&id)?;
        Some((key, self.inner.node_weight(key)?))
    }

    /// Get node key by UUID.
    pub fn get_node_key_by_id(&self, id: Uuid) -> Option<NodeKey> {
        self.id_to_node.get(&id).copied()
    }

    /// Iterate over all nodes as (key, node) pairs
    pub fn nodes(&self) -> impl Iterator<Item = (NodeKey, &Node)> {
        self.inner
            .node_indices()
            .map(move |idx| (idx, &self.inner[idx]))
    }

    /// Iterate over all stored relations, one row per
    /// (from, to, [`RelationKind`]) tuple. A multi-relation edge
    /// (an `EdgePayload` carrying multiple typed sidecars between
    /// the same node pair) yields multiple rows.
    ///
    /// Canonical read surface per the 2026-05-11 relation-taxonomy
    /// plan §2. Stage 4 removed the legacy `EdgeType`-flavoured
    /// `edges()` iterator that this replaces.
    pub fn relations(&self) -> impl Iterator<Item = RelationView> + '_ {
        use super::edge_taxonomy::RelationKind;
        self.inner.edge_references().flat_map(|edge| {
            let from = edge.source();
            let to = edge.target();
            let payload = edge.weight();
            let mut out: Vec<RelationView> = Vec::new();
            if let Some(sem) = payload.semantic_data() {
                for &sub_kind in &sem.sub_kinds {
                    out.push(RelationView {
                        from,
                        to,
                        kind: RelationKind::Semantic(sub_kind),
                    });
                }
            }
            // Traversal is event-shaped, no sub-kind. Presence of
            // any traversal data (events or metrics) yields one
            // row. Family-aware view callers reach for the
            // typed payload for actual events.
            if let Some(traversal) = payload.traversal_data()
                && (!traversal.traversals.is_empty()
                    || traversal.metrics.total_navigations > 0)
            {
                out.push(RelationView {
                    from,
                    to,
                    kind: RelationKind::Traversal,
                });
            }
            if let Some(containment) = payload.containment_data() {
                for &sub_kind in &containment.sub_kinds {
                    out.push(RelationView {
                        from,
                        to,
                        kind: RelationKind::Containment(sub_kind),
                    });
                }
            }
            if let Some(arrangement) = payload.arrangement_data() {
                for &sub_kind in &arrangement.sub_kinds {
                    out.push(RelationView {
                        from,
                        to,
                        kind: RelationKind::Arrangement(sub_kind),
                    });
                }
            }
            if let Some(imported) = payload.imported_data() {
                for &sub_kind in &imported.sub_kinds {
                    out.push(RelationView {
                        from,
                        to,
                        kind: RelationKind::Imported(sub_kind),
                    });
                }
            }
            if let Some(prov) = payload.provenance_data() {
                for &sub_kind in &prov.sub_kinds {
                    out.push(RelationView {
                        from,
                        to,
                        kind: RelationKind::Provenance(sub_kind),
                    });
                }
            }
            out.into_iter()
        })
    }

    pub fn semantic_edges(&self) -> impl Iterator<Item = SemanticEdgeView> + '_ {
        self.inner.edge_references().flat_map(|edge| {
            let from = edge.source();
            let to = edge.target();
            let payload = edge.weight();
            let label = payload.label().map(str::to_string);
            payload
                .semantic_data()
                .map(|data| {
                    data.sub_kinds
                        .iter()
                        .copied()
                        .map(|sub_kind| SemanticEdgeView {
                            from,
                            to,
                            sub_kind,
                            label: label.clone(),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
                .into_iter()
        })
    }

    pub fn arrangement_edges(&self) -> impl Iterator<Item = ArrangementEdgeView> + '_ {
        self.inner.edge_references().flat_map(|e| {
            let from = e.source();
            let to = e.target();
            e.weight()
                .arrangement_data()
                .map(|data| {
                    data.sub_kinds
                        .iter()
                        .copied()
                        .map(move |sub_kind| ArrangementEdgeView { from, to, sub_kind })
                })
                .into_iter()
                .flatten()
        })
    }

    pub fn containment_edges(&self) -> impl Iterator<Item = ContainmentEdgeView> + '_ {
        self.inner.edge_references().flat_map(|e| {
            let from = e.source();
            let to = e.target();
            e.weight()
                .containment_data()
                .map(|data| {
                    data.sub_kinds
                        .iter()
                        .copied()
                        .map(move |sub_kind| ContainmentEdgeView { from, to, sub_kind })
                })
                .into_iter()
                .flatten()
        })
    }

    /// Rebuild derived containment edges from current node URLs.
    ///
    /// Derived relations are additive/read-only and are never persisted.
    pub fn rebuild_derived_containment_relations(&mut self) {
        let edge_ids: Vec<EdgeKey> = self.inner.edge_indices().collect();
        let mut empty_edges = Vec::new();
        for edge_id in edge_ids {
            if let Some(payload) = self.inner.edge_weight_mut(edge_id) {
                let mut removed_any = false;
                removed_any |= payload.retract_relation(RelationSelector::Containment(
                    ContainmentSubKind::UrlPath,
                ));
                removed_any |= payload.retract_relation(RelationSelector::Containment(
                    ContainmentSubKind::Domain,
                ));
                if removed_any && payload.is_empty() {
                    empty_edges.push(edge_id);
                }
            }
        }
        for edge_id in empty_edges {
            let _ = self.inner.remove_edge(edge_id);
        }

        let mut domain_anchor: HashMap<String, (NodeKey, usize, Uuid)> = HashMap::new();
        for (key, node) in self.nodes() {
            let Ok(parsed) = url::Url::parse(node.url()) else {
                continue;
            };
            let depth = parsed.path_segments().map_or(0, |segments| {
                segments.filter(|segment| !segment.is_empty()).count()
            });

            let Some(host) = parsed.host_str() else {
                continue;
            };
            let host = host.to_ascii_lowercase();
            let candidate = (key, depth, node.id);
            domain_anchor
                .entry(host)
                .and_modify(|current| {
                    if candidate.1 < current.1
                        || (candidate.1 == current.1 && candidate.2 < current.2)
                    {
                        *current = candidate;
                    }
                })
                .or_insert(candidate);
        }

        let mut url_parent_edges = Vec::new();
        let mut domain_edges = Vec::new();
        for (key, node) in self.nodes() {
            let Ok(parsed) = url::Url::parse(node.url()) else {
                continue;
            };

            if let Some(parent_url) = containment_parent_url(&parsed)
                && let Some((parent_key, _)) = self.get_node_by_url(&parent_url)
                && parent_key != key
            {
                url_parent_edges.push((key, parent_key));
            }

            if let Some(host) = parsed.host_str() {
                let host = host.to_ascii_lowercase();
                if let Some((anchor_key, _, _)) = domain_anchor.get(&host)
                    && *anchor_key != key
                {
                    domain_edges.push((key, *anchor_key));
                }
            }
        }

        for (from, to) in url_parent_edges {
            let _ = self.assert_relation(
                from,
                to,
                EdgeAssertion::Containment {
                    sub_kind: ContainmentSubKind::UrlPath,
                },
            );
        }
        for (from, to) in domain_edges {
            let _ = self.assert_relation(
                from,
                to,
                EdgeAssertion::Containment {
                    sub_kind: ContainmentSubKind::Domain,
                },
            );
        }
    }

    /// Iterate outgoing neighbor keys for a node
    pub fn out_neighbors(&self, key: NodeKey) -> impl Iterator<Item = NodeKey> + '_ {
        self.inner.neighbors_directed(key, Direction::Outgoing)
    }

    /// Iterate incoming neighbor keys for a node
    pub fn in_neighbors(&self, key: NodeKey) -> impl Iterator<Item = NodeKey> + '_ {
        self.inner.neighbors_directed(key, Direction::Incoming)
    }

    /// Iterate undirected neighbor keys for a node.
    pub fn neighbors_undirected(&self, key: NodeKey) -> impl Iterator<Item = NodeKey> + '_ {
        self.inner.neighbors_undirected(key)
    }

    /// Undirected neighbors sorted by stable node-key order.
    pub fn neighbors_undirected_sorted(&self, key: NodeKey) -> Vec<NodeKey> {
        let mut neighbors: Vec<NodeKey> = self
            .neighbors_undirected(key)
            .filter(|neighbor| *neighbor != key && self.get_node(*neighbor).is_some())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        neighbors.sort_by_key(|neighbor| neighbor.index());
        neighbors
    }

    /// Seed nodes plus one-hop undirected neighbors for frame import workflows.
    pub fn connected_frame_import_nodes(&self, seeds: &[NodeKey]) -> Vec<NodeKey> {
        let mut out = HashSet::new();
        for seed in seeds {
            if self.get_node(*seed).is_none() {
                continue;
            }
            out.insert(*seed);
            out.extend(self.neighbors_undirected(*seed));
        }
        let mut nodes: Vec<NodeKey> = out
            .into_iter()
            .filter(|key| self.get_node(*key).is_some())
            .collect();
        nodes.sort_by_key(|key| key.index());
        nodes
    }

    /// Connected candidate expansion around a source node with depth annotations.
    ///
    /// `max_depth` currently supports 1 or 2, matching connected-open scope behavior.
    pub fn connected_candidates_with_depth(
        &self,
        source: NodeKey,
        max_depth: u8,
    ) -> Vec<(NodeKey, u8)> {
        if self.get_node(source).is_none() || max_depth == 0 {
            return Vec::new();
        }

        let mut out = Vec::new();
        let mut visited = HashSet::from([source]);

        let depth1 = self.neighbors_undirected_sorted(source);
        for neighbor in depth1 {
            if visited.insert(neighbor) {
                out.push((neighbor, 1));
            }
        }

        if max_depth < 2 {
            return out;
        }

        let depth1_nodes: Vec<NodeKey> = out
            .iter()
            .filter_map(|(node, depth)| (*depth == 1).then_some(*node))
            .collect();
        for depth1_node in depth1_nodes {
            for neighbor in self.neighbors_undirected_sorted(depth1_node) {
                if visited.insert(neighbor) {
                    out.push((neighbor, 2));
                }
            }
        }

        out
    }

    /// Undirected hop distances from `source` using unit edge weights.
    pub fn hop_distances_from(&self, source: NodeKey) -> HashMap<NodeKey, usize> {
        if self.get_node(source).is_none() {
            return HashMap::new();
        }
        dijkstra(&UndirectedAdaptor(&self.inner), source, None, |_| 1_usize)
            .into_iter()
            .collect()
    }

    /// Nodes with no incoming or outgoing edges.
    pub fn orphan_node_keys(&self) -> Vec<NodeKey> {
        self.inner
            .node_indices()
            .filter(|&key| {
                self.inner
                    .edges_directed(key, Direction::Outgoing)
                    .next()
                    .is_none()
                    && self
                        .inner
                        .edges_directed(key, Direction::Incoming)
                        .next()
                        .is_none()
            })
            .collect()
    }

    /// Shortest undirected path between two nodes using unit edge weights.
    pub fn shortest_path(&self, from: NodeKey, to: NodeKey) -> Option<Vec<NodeKey>> {
        if self.get_node(from).is_none() || self.get_node(to).is_none() {
            return None;
        }
        astar(
            &UndirectedAdaptor(&self.inner),
            from,
            |node| node == to,
            |_| 1_usize,
            |_| 0_usize,
        )
        .map(|(_, path)| path)
    }

    /// Reachability in the undirected graph.
    pub fn is_reachable(&self, from: NodeKey, to: NodeKey) -> bool {
        if self.get_node(from).is_none() || self.get_node(to).is_none() {
            return false;
        }
        has_path_connecting(&UndirectedAdaptor(&self.inner), from, to, None)
    }

    /// Weakly connected components (undirected projection).
    pub fn weakly_connected_components(&self) -> Vec<Vec<NodeKey>> {
        let mut visited = HashSet::new();
        let mut components = Vec::new();
        for start in self.inner.node_indices() {
            if !visited.insert(start) {
                continue;
            }
            let mut component = Vec::new();
            let mut stack = vec![start];
            while let Some(current) = stack.pop() {
                component.push(current);
                for neighbor in self.neighbors_undirected(current) {
                    if visited.insert(neighbor) {
                        stack.push(neighbor);
                    }
                }
            }
            components.push(component);
        }
        components
    }

    /// Strongly connected components in the directed graph.
    pub fn strongly_connected_components(&self) -> Vec<Vec<NodeKey>> {
        kosaraju_scc(&self.inner)
    }

    /// Check if a directed edge exists from `from` to `to`
    pub fn has_edge_between(&self, from: NodeKey, to: NodeKey) -> bool {
        self.inner.find_edge(from, to).is_some()
    }

    /// Count of nodes in the graph
    pub fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    /// Count of edges in the graph
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }
}
