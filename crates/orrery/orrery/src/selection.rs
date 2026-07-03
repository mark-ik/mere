/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Selection, tagging, semantic relations, and node-state setters.

use kernel::graph::Graph;
use kernel::graph::apply::{GraphDelta, GraphDeltaResult, apply_graph_delta};

use super::*;

/// Retract relations through the sanctioned delta path, unwrapping the count.
/// (Write-path migration, 2026-07-01.)
fn retract_via_delta(
    graph: &mut Graph,
    from: NodeKey,
    to: NodeKey,
    selector: RelationSelector,
) -> usize {
    match apply_graph_delta(graph, GraphDelta::RetractRelations { from, to, selector }) {
        GraphDeltaResult::EdgesRemoved(n) => n,
        _ => 0,
    }
}

impl Orrery {
    /// Replace the selection with just `key` (clearing any selected nodes/edges).
    pub(crate) fn select_only(&mut self, key: NodeKey) {
        self.selected.clear();
        self.selected_edges.clear();
        self.selected.insert(key);
    }

    /// Select the existing node with `url` (URL identity), if present, without
    /// adding one. Returns whether a node was found and focused. The host calls
    /// this to restore the focused node from persisted view-intent.
    pub fn select_by_url(&mut self, url: &str) -> bool {
        if let Some((key, _)) = self.graph.get_node_by_url(url) {
            self.select_only(key);
            true
        } else {
            false
        }
    }

    /// Toggle `member` in or out of the selection (a multi-select add), keeping the
    /// rest selected — the member-keyed twin of the canvas's Shift-click. Clears the
    /// edge selection (matching that gesture) so a mixed node+edge selection can't
    /// confuse the pairwise relate. Returns `false` if the member is not in the
    /// graph. Selection is read live at frame time, so no reconcile is needed.
    pub fn toggle_select_member(&mut self, member: uuid::Uuid) -> bool {
        let Some(key) = self.graph.get_node_key_by_id(member) else {
            return false;
        };
        if !self.selected.remove(&key) {
            self.selected.insert(key);
        }
        self.selected_edges.clear();
        true
    }

    /// Remove the single focused node from the session graph (if exactly one is
    /// selected), returning its member id (the kernel node UUID) so the host can
    /// reap any activation for it. Clears the selection and reconciles the physics
    /// + node pool to the smaller graph. Returns `None` when zero or many nodes
    /// are selected, leaving the graph untouched. The host calls this on a
    /// delete-node gesture; deactivation (reaping the actor) is the host's job.
    pub fn remove_focused(&mut self) -> Option<uuid::Uuid> {
        if self.selected.len() != 1 {
            return None;
        }
        let key = *self.selected.iter().next()?;
        let id = self.graph.get_node(key)?.id;
        let _ = apply_graph_delta(&mut self.graph, GraphDelta::RemoveNode { key });
        self.selected.clear();
        self.selected_edges.clear();
        self.reconcile_derived();
        Some(id)
    }

    /// Clear the node + edge selection (focus) without removing anything from
    /// the graph. The host calls this to drop focus — e.g. closing the last
    /// workbench tile returns to the graph with nothing focused, so the node
    /// deactivates instead of its Cartography preview re-activating it.
    /// (Card-system plan, Phase 1.)
    pub fn clear_selection(&mut self) {
        if self.selected.is_empty() && self.selected_edges.is_empty() {
            return;
        }
        self.selected.clear();
        self.selected_edges.clear();
        self.reconcile_derived();
    }

    /// The single focused node key, for layout strategies that center on a selection
    /// (radial). `None` when zero or many nodes are selected, so a focus-driven layout
    /// stays well-defined (it no-ops rather than picking an arbitrary node from a
    /// multi-selection). (Layout picker — radial.)
    pub fn focused_key(&self) -> Option<NodeKey> {
        match self.selected.len() {
            1 => self.selected.iter().copied().next(),
            _ => None,
        }
    }

    /// Assert a semantic relation of `sub_kind` between exactly two selected
    /// nodes — the user-initiated edge-creation gesture the rich kernel taxonomy
    /// always supported but the UI never reached. The pair is ordered by node
    /// UUID so a symmetric relation is reproducible; the edge is created or
    /// merged (idempotent per sub-kind) via [`Graph::assert_relation`]. Returns
    /// `true` when an edge was asserted, `false` for any selection that is not a
    /// clean pair. The springs / drawn edges refresh on the next reconcile.
    pub fn assert_selected_relation(&mut self, sub_kind: SemanticSubKind) -> bool {
        if self.selected.len() != 2 {
            return false;
        }
        let mut pair: Vec<NodeKey> = self.selected.iter().copied().collect();
        pair.sort_by_key(|k| self.graph.get_node(*k).map(|n| n.id));
        // `assert_relation` returns `None` for a no-op re-assert (the sub-kind is
        // already present), so we don't gate success on its return: for a clean
        // pair the relation is present afterwards either way, which is what
        // "relate these two" means. Reconcile rebuilds edges / springs.
        let _ = apply_graph_delta(
            &mut self.graph,
            GraphDelta::AssertRelation {
                from: pair[0],
                to: pair[1],
                assertion: EdgeAssertion::Semantic {
                    sub_kind,
                    label: None,
                    decay_progress: None,
                },
            },
        );
        self.reconcile_derived();
        true
    }

    /// Assert a semantic relation between two stable graph members. This is the
    /// card/detail twin of [`assert_selected_relation`]: the roster Link Card has
    /// explicit endpoints, so it should not need to perturb selection first.
    pub fn assert_relation_between_members(
        &mut self,
        from_id: uuid::Uuid,
        to_id: uuid::Uuid,
        sub_kind: SemanticSubKind,
    ) -> bool {
        let Some(from) = self.graph.get_node_key_by_id(from_id) else {
            return false;
        };
        let Some(to) = self.graph.get_node_key_by_id(to_id) else {
            return false;
        };
        let _ = apply_graph_delta(
            &mut self.graph,
            GraphDelta::AssertRelation {
                from,
                to,
                assertion: EdgeAssertion::Semantic {
                    sub_kind,
                    label: None,
                    decay_progress: None,
                },
            },
        );
        self.reconcile_derived();
        true
    }

    /// Insert `tag` on every selected node — the user-initiated tagging gesture
    /// (the context menu's "Add tag…"). Trims the tag; an empty tag or empty
    /// selection is a no-op. Returns how many nodes newly gained the tag (an
    /// already-tagged node counts 0). Tags are node truth the host persists; they
    /// do not affect layout, so no reconcile is needed.
    pub fn tag_selected(&mut self, tag: &str) -> usize {
        let tag = tag.trim();
        if tag.is_empty() {
            return 0;
        }
        let keys: Vec<NodeKey> = self.selected.iter().copied().collect();
        let mut tagged = 0;
        for key in keys {
            if matches!(
                apply_graph_delta(
                    &mut self.graph,
                    GraphDelta::InsertNodeTag {
                        key,
                        tag: tag.to_string()
                    },
                ),
                GraphDeltaResult::NodeMetadataUpdated(true)
            ) {
                tagged += 1;
            }
        }
        tagged
    }

    /// Insert `tag` on the node addressed by `url`, if present — a by-url tagging gesture (the
    /// Alembic "keep" one-click promote, distinct from the selection-based [`tag_selected`]).
    /// Returns whether the node newly gained the tag. Tags are node truth the host persists.
    pub fn tag_node_by_url(&mut self, url: &str, tag: &str) -> bool {
        let Some(key) = self.graph.get_node_by_url(url).map(|(k, _)| k) else {
            return false;
        };
        matches!(
            apply_graph_delta(
                &mut self.graph,
                GraphDelta::InsertNodeTag {
                    key,
                    tag: tag.to_string()
                },
            ),
            GraphDeltaResult::NodeMetadataUpdated(true)
        )
    }

    /// Remove `tag` from the node addressed by `url`, if present (the Alembic "release"
    /// one-click demote). Returns whether it was removed. A node still tagged otherwise stays
    /// long-term (Saved) — only this tag is touched.
    pub fn untag_node_by_url(&mut self, url: &str, tag: &str) -> bool {
        let Some(key) = self.graph.get_node_by_url(url).map(|(k, _)| k) else {
            return false;
        };
        matches!(
            apply_graph_delta(
                &mut self.graph,
                GraphDelta::RemoveNodeTag {
                    key,
                    tag: tag.to_string()
                },
            ),
            GraphDeltaResult::NodeMetadataUpdated(true)
        )
    }

    /// Retract the user-asserted semantic relation(s) on the selected edge(s) —
    /// a true removal, not the display-only [`hide_selected_edges`]. Scoped to the
    /// `Semantic` family, so navigation / provenance history on the same edge
    /// survives; an edge left with no families is garbage-collected by the kernel.
    /// Returns how many relations were retracted, and clears the edge selection.
    pub fn retract_selected_relation(&mut self) -> usize {
        let mut removed = 0;
        // Symmetric with `assert_selected_relation`: a two-node selection retracts
        // the relation between the pair (either stored direction), so `>unrelate`
        // mirrors `>relate` on the same gesture.
        if self.selected.len() == 2 {
            let mut pair: Vec<NodeKey> = self.selected.iter().copied().collect();
            pair.sort_by_key(|k| self.graph.get_node(*k).map(|n| n.id));
            removed += self.retract_semantic_between(pair[0], pair[1]);
            removed += self.retract_semantic_between(pair[1], pair[0]);
        }
        // Also retract any directly-selected relation cells (the click-an-edge path).
        for cell in self.selected_edges.drain().collect::<Vec<_>>() {
            if matches!(cell.selector, RelationSelector::Semantic(_)) {
                removed += retract_via_delta(&mut self.graph, cell.from, cell.to, cell.selector);
            }
        }
        if removed > 0 {
            self.reconcile_derived();
        }
        removed
    }

    /// Retract every semantic relation on the directed edge `a -> b`. The `Family`
    /// selector is read-only (not retractable), so enumerate the edge's semantic
    /// sub-kinds and retract each — the user-meaning relations go, while traversal
    /// / provenance history on the same edge survives (an edge left with no
    /// families is garbage-collected by the kernel). Returns how many were removed.
    pub(crate) fn retract_semantic_between(&mut self, a: NodeKey, b: NodeKey) -> usize {
        let sub_kinds: Vec<SemanticSubKind> = self
            .graph
            .find_edge_key(a, b)
            .and_then(|k| self.graph.get_edge(k))
            .and_then(|p| p.semantic_data())
            .map(|s| s.sub_kinds.iter().copied().collect())
            .unwrap_or_default();
        let mut removed = 0;
        for sk in sub_kinds {
            removed += retract_via_delta(&mut self.graph, a, b, RelationSelector::Semantic(sk));
        }
        removed
    }

    /// Retract a specific relation selector between two stable graph members.
    /// Used by the roster Link Card; unlike `retract_selected_relation`, it can
    /// target one semantic cell without first selecting an edge on the canvas.
    pub fn retract_relation_between_members(
        &mut self,
        from_id: uuid::Uuid,
        to_id: uuid::Uuid,
        selector: RelationSelector,
    ) -> usize {
        let Some(from) = self.graph.get_node_key_by_id(from_id) else {
            return 0;
        };
        let Some(to) = self.graph.get_node_key_by_id(to_id) else {
            return 0;
        };
        let removed = retract_via_delta(&mut self.graph, from, to, selector);
        if removed > 0 {
            self.reconcile_derived();
        }
        removed
    }

    /// Whether any relation cell is currently selected (the host routes a `Delete` to edge
    /// retraction when so, else to node deletion).
    pub fn has_selected_edges(&self) -> bool {
        !self.selected_edges.is_empty()
    }

    /// Hide the currently-selected relation cells, and clear the selection. Relation-cell
    /// truth persists (display-only); the spring for each newly-hidden cell relaxes in
    /// this instance only (swatch-primitive P5).
    pub fn hide_selected_edges(&mut self) -> usize {
        let mut count = 0;
        for cell in self.selected_edges.drain().collect::<Vec<_>>() {
            if self.hidden_edges.insert(cell) {
                count += 1;
            }
        }
        if count > 0 {
            self.resync_edge_springs();
        }
        count
    }

    /// Whether every live relation cell in the endpoint bundle is hidden.
    pub fn edge_between_members_hidden(&self, from_id: uuid::Uuid, to_id: uuid::Uuid) -> bool {
        self.edge_cells_between_members(from_id, to_id)
            .is_some_and(|cells| {
                !cells.is_empty() && cells.iter().all(|cell| self.hidden_edges.contains(cell))
            })
    }

    /// Whether one directed relation cell between two graph members is hidden.
    pub fn relation_between_members_hidden(
        &self,
        from_id: uuid::Uuid,
        to_id: uuid::Uuid,
        selector: RelationSelector,
    ) -> bool {
        self.edge_cell_between_members(from_id, to_id, selector)
            .is_some_and(|cell| self.hidden_edges.contains(&cell))
    }

    /// Hide every live relation cell in the endpoint bundle between two graph members.
    pub fn hide_edge_between_members(&mut self, from_id: uuid::Uuid, to_id: uuid::Uuid) -> bool {
        let Some(cells) = self.edge_cells_between_members(from_id, to_id) else {
            return false;
        };
        let changed = cells.into_iter().fold(false, |changed, cell| {
            self.hidden_edges.insert(cell) || changed
        });
        if changed {
            self.resync_edge_springs();
        }
        changed
    }

    /// Hide one directed relation cell between two graph members.
    pub fn hide_relation_between_members(
        &mut self,
        from_id: uuid::Uuid,
        to_id: uuid::Uuid,
        selector: RelationSelector,
    ) -> bool {
        let changed = self
            .edge_cell_between_members(from_id, to_id, selector)
            .is_some_and(|cell| self.hidden_edges.insert(cell));
        if changed {
            self.resync_edge_springs();
        }
        changed
    }

    /// Reveal every hidden relation cell in the endpoint bundle between two graph members.
    pub fn show_edge_between_members(&mut self, from_id: uuid::Uuid, to_id: uuid::Uuid) -> bool {
        let Some(pair) = self.edge_pair_between_members(from_id, to_id) else {
            return false;
        };
        let before = self.hidden_edges.len();
        self.hidden_edges
            .retain(|cell| cell.endpoint_pair() != pair);
        let changed = self.hidden_edges.len() != before;
        if changed {
            self.resync_edge_springs();
        }
        changed
    }

    /// Reveal one directed relation cell between two graph members.
    pub fn show_relation_between_members(
        &mut self,
        from_id: uuid::Uuid,
        to_id: uuid::Uuid,
        selector: RelationSelector,
    ) -> bool {
        let (Some(from), Some(to)) = (
            self.graph.get_node_key_by_id(from_id),
            self.graph.get_node_key_by_id(to_id),
        ) else {
            return false;
        };
        let changed = self.hidden_edges.remove(&EdgeCell { from, to, selector });
        if changed {
            self.resync_edge_springs();
        }
        changed
    }

    /// Hidden display-only endpoint bundles as stable graph member ids.
    pub fn hidden_edge_member_pairs(&self) -> Vec<(uuid::Uuid, uuid::Uuid)> {
        self.hidden_edges
            .iter()
            .filter_map(|cell| {
                let (a, b) = cell.endpoint_pair();
                Some((self.graph.get_node(a)?.id, self.graph.get_node(b)?.id))
            })
            .collect()
    }

    /// Whether the pair has at least one visible live relation cell.
    pub(crate) fn edge_pair_has_visible_relation(&self, a: NodeKey, b: NodeKey) -> bool {
        self.edge_cells_between_pair(a, b)
            .into_iter()
            .any(|cell| !self.hidden_edges.contains(&cell))
    }

    fn edge_pair_between_members(
        &self,
        from_id: uuid::Uuid,
        to_id: uuid::Uuid,
    ) -> Option<(NodeKey, NodeKey)> {
        let from = self.graph.get_node_key_by_id(from_id)?;
        let to = self.graph.get_node_key_by_id(to_id)?;
        Some(if from <= to { (from, to) } else { (to, from) })
    }

    fn edge_cell_between_members(
        &self,
        from_id: uuid::Uuid,
        to_id: uuid::Uuid,
        selector: RelationSelector,
    ) -> Option<EdgeCell> {
        let from = self.graph.get_node_key_by_id(from_id)?;
        let to = self.graph.get_node_key_by_id(to_id)?;
        let cell = EdgeCell { from, to, selector };
        self.graph
            .relations()
            .any(|relation| {
                relation.from == from
                    && relation.to == to
                    && crate::edge_cells::selector_for_relation_kind(relation.kind) == selector
            })
            .then_some(cell)
    }

    fn edge_cells_between_members(
        &self,
        from_id: uuid::Uuid,
        to_id: uuid::Uuid,
    ) -> Option<Vec<EdgeCell>> {
        let from = self.graph.get_node_key_by_id(from_id)?;
        let to = self.graph.get_node_key_by_id(to_id)?;
        Some(self.edge_cells_between_pair(from, to))
    }

    fn edge_cells_between_pair(&self, a: NodeKey, b: NodeKey) -> Vec<EdgeCell> {
        let pair = if a <= b { (a, b) } else { (b, a) };
        self.graph
            .relations()
            .filter_map(|relation| {
                let rel_pair = if relation.from <= relation.to {
                    (relation.from, relation.to)
                } else {
                    (relation.to, relation.from)
                };
                (rel_pair == pair).then_some(crate::edge_cells::edge_cell_for_relation(
                    relation.from,
                    relation.to,
                    relation.kind,
                ))
            })
            .collect()
    }

    /// Reveal every hidden edge. Returns how many were shown.
    pub fn show_all_edges(&mut self) -> usize {
        let count = self.hidden_edges.len();
        self.hidden_edges.clear();
        if count > 0 {
            self.resync_edge_springs();
        }
        count
    }

    /// Re-sync the physics spring topology to the current visible relation-cell set and
    /// give it a quick settle nudge. Hiding/showing a cell should relax/restore its own
    /// spring in this instance right away, not wait for an unrelated graph mutation to
    /// reconcile it. (Swatch-primitive P5 — hiding relaxes the spring in that instance
    /// only; graph truth, membership, and every other instance are unaffected.)
    fn resync_edge_springs(&mut self) {
        self.physics
            .sync_edges(visible_relation_edges(&self.graph, &self.hidden_edges));
        self.settle_physics(SETTLE_TICKS / 3);
    }

    /// Set the per-node activation states the orrery colors its on-screen nodes
    /// by, keyed by node UUID (the host's member id); the orrery resolves each to
    /// its `NodeKey`. The host recomputes + pushes this as the actor pool / content
    /// cache change; a node absent from `states` colors as [`NodeState::Idle`].
    pub fn set_node_states(&mut self, states: HashMap<uuid::Uuid, NodeState>) {
        self.node_states = states
            .into_iter()
            .filter_map(|(id, state)| self.graph.get_node_by_id(id).map(|(key, _)| (key, state)))
            .collect();
    }
}
