/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Navigation history — one **shared** visit space for the whole graph.
//!
//! Every node is an `Owner` within a single `stemma::Stemma`, keyed by
//! the node's UUID. A node's own browse history is its owner's path (within-node
//! back/forward); a node minted by an "open in new node" gesture is **spawned**
//! with its creator set to the origin node, so its first visit attaches under the
//! origin's *current* visit — the navigated-from anchor is correct-by-construction
//! and within-node + cross-node lineage live in one tree (the (b) anchor design,
//! 2026-06-06; see the node-navigation-lineage plan).
//!
//! The store keeps the serializable [`GraphMemorySnapshot`] and rehydrates the
//! live `GraphMemory` (a non-serializable `SlotMap`) per operation — the same
//! pattern the per-node predecessor used, now over one shared space. (A live
//! in-memory copy with custom (de)serialization is the future optimization if the
//! per-op rehydrate cost matters.)
//!
//! Extracted from `graph/mod.rs` per the 2026-05-11 kernel-mod decomposition pass.

use stemma::{
    EntryPrivacy as MemoryEntryPrivacy, Stemma as OwnerScopedMemory,
    StemmaSnapshot as GraphMemorySnapshot, TransitionKind as MemoryTransitionKind,
};
use rkyv::{Archive, Deserialize, Serialize};
use uuid::Uuid;

/// Owner identity in the shared visit space: one owner per graph node, keyed by
/// the node's UUID (string form — rkyv/serde-clean without a uuid feature). The
/// enum (rather than a bare `String`) keeps the identity namespaced and leaves
/// room for non-node owners later.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq, Eq))]
pub enum NodeHistoryOwner {
    Node(String),
}

impl NodeHistoryOwner {
    fn of(node: Uuid) -> Self {
        NodeHistoryOwner::Node(node.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeHistoryProjection {
    pub entries: Vec<String>,
    pub current_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeHistoryBranchAlternative {
    pub url: String,
    pub transition: Option<MemoryTransitionKind>,
    pub at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeHistoryBranchVisit {
    pub url: String,
    pub transition: Option<MemoryTransitionKind>,
    pub at_ms: u64,
    pub is_current: bool,
    pub alternate_children: Vec<NodeHistoryBranchAlternative>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodeHistoryBranchProjection {
    pub visits: Vec<NodeHistoryBranchVisit>,
    pub current_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodeHistorySemanticSummary {
    pub current_url: Option<String>,
    pub last_visit_at_ms: Option<u64>,
    pub visit_count: usize,
}

/// One node's most-recent visit, for the graph-wide "recently visited" projection
/// (gloss recent-nodes; the lineage pane). A row of [`recent_visited`](SharedNavigationMemory::recent_visited).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentVisit {
    pub node: Uuid,
    pub url: String,
    pub last_visit_at_ms: u64,
}

type Memory = OwnerScopedMemory<String, String, NodeHistoryOwner, ()>;
type Snapshot = GraphMemorySnapshot<String, String, NodeHistoryOwner, ()>;

/// The graph's whole navigation history: one shared visit space, one owner per
/// node. Methods key by node [`Uuid`]; an absent owner reads as empty history and
/// is created lazily on the node's first [`record_visit`](Self::record_visit) (or
/// eagerly via [`spawn`](Self::spawn) for an anchored branch).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct SharedNavigationMemory {
    snapshot: Snapshot,
}

impl Default for SharedNavigationMemory {
    fn default() -> Self {
        Self::empty()
    }
}

impl SharedNavigationMemory {
    pub fn empty() -> Self {
        Self {
            snapshot: GraphMemorySnapshot {
                entries: Vec::new(),
                visits: Vec::new(),
                owners: Vec::new(),
            },
        }
    }

    /// Wrap an existing snapshot (snapshot restore).
    pub fn from_snapshot(snapshot: Snapshot) -> Self {
        Self { snapshot }
    }

    /// The serializable snapshot, for persistence.
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub fn owner_count(&self) -> usize {
        self.snapshot.owners.len()
    }

    pub fn entry_count(&self) -> usize {
        self.snapshot.entries.len()
    }

    pub fn visit_count(&self) -> usize {
        self.snapshot.visits.len()
    }

    fn hydrate(&self) -> Memory {
        Memory::from_snapshot(self.snapshot.clone())
    }

    /// Ensure `node` has an owner (no creator — a root history). Lazy: also done
    /// by [`record_visit`]; explicit calls are harmless (idempotent).
    pub fn ensure(&mut self, node: Uuid) {
        let mut memory = self.hydrate();
        memory.ensure_owner(NodeHistoryOwner::of(node), None);
        self.snapshot = memory.to_snapshot();
    }

    /// Spawn `child`'s owner anchored under `parent`'s **current visit** (the
    /// navigated-from point), via node-lineage's creator machinery. Call **before**
    /// the child's first [`record_visit`] so that first visit attaches under the
    /// parent's current visit in the shared tree. A no-op-ish fallback to a root
    /// owner if `parent` has no owner yet.
    pub fn spawn(&mut self, child: Uuid, parent: Uuid) {
        let mut memory = self.hydrate();
        let creator = memory.owner_id_by_identity(&NodeHistoryOwner::of(parent));
        memory.ensure_owner(NodeHistoryOwner::of(child), creator);
        self.snapshot = memory.to_snapshot();
    }

    /// Record navigating `node` to `url` as a new current visit. Forward-fork:
    /// navigating after [`back`](Self::back) branches off the current visit and
    /// preserves the prior forward path. Entries dedup by URL; each navigation is
    /// a distinct visit. Creates the owner (root) if absent.
    pub fn record_visit(
        &mut self,
        node: Uuid,
        url: &str,
        transition: MemoryTransitionKind,
        at_ms: u64,
    ) {
        let mut memory = self.hydrate();
        let owner = memory.ensure_owner(NodeHistoryOwner::of(node), None);
        let entry = memory.resolve_or_create_entry(
            url.to_string(),
            url.to_string(),
            at_ms,
            MemoryEntryPrivacy::LocalOnly,
        );
        let _ = memory.visit_entry(owner, entry, (), transition, at_ms);
        self.snapshot = memory.to_snapshot();
    }

    /// Move `node`'s cursor back one visit. Returns the revealed URL, or `None` at
    /// the root (cursor did not move).
    pub fn back(&mut self, node: Uuid, at_ms: u64) -> Option<String> {
        self.step(node, true, at_ms)
    }

    /// Move `node`'s cursor forward one visit. Returns the revealed URL, or `None`
    /// at the tip.
    pub fn forward(&mut self, node: Uuid, at_ms: u64) -> Option<String> {
        self.step(node, false, at_ms)
    }

    fn step(&mut self, node: Uuid, backward: bool, at_ms: u64) -> Option<String> {
        let mut memory = self.hydrate();
        let owner = memory.owner_id_by_identity(&NodeHistoryOwner::of(node))?;
        let moved = if backward {
            memory.back(owner, 1, at_ms)
        } else {
            memory.forward(owner, 1, at_ms)
        }
        .ok()
        .flatten()?;
        let url = memory
            .visit(moved)
            .and_then(|visit| memory.entry(visit.entry))
            .map(|entry| entry.payload.clone());
        self.snapshot = memory.to_snapshot();
        url
    }

    /// `node`'s linear-history projection (active path + cursor).
    pub fn projection(&self, node: Uuid) -> NodeHistoryProjection {
        let memory = self.hydrate();
        let Some(owner) = memory.owner_id_by_identity(&NodeHistoryOwner::of(node)) else {
            return NodeHistoryProjection {
                entries: Vec::new(),
                current_index: 0,
            };
        };
        let entries = memory
            .linear_history_entries_of_owner(owner)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|entry_id| memory.entry(entry_id).map(|entry| entry.payload.clone()))
            .collect::<Vec<_>>();
        let current_index = memory
            .current_index_of_owner(owner)
            .ok()
            .flatten()
            .unwrap_or(0);
        NodeHistoryProjection {
            entries,
            current_index,
        }
    }

    /// `node`'s branching-history projection (visit tree with alternates).
    pub fn branch_projection(&self, node: Uuid) -> NodeHistoryBranchProjection {
        let memory = self.hydrate();
        let Some(owner) = memory.owner_id_by_identity(&NodeHistoryOwner::of(node)) else {
            return NodeHistoryBranchProjection::default();
        };
        let Ok(branch) = memory.owner_branch_projection(owner) else {
            return NodeHistoryBranchProjection::default();
        };
        NodeHistoryBranchProjection {
            visits: branch
                .visits
                .into_iter()
                .map(|visit| NodeHistoryBranchVisit {
                    url: visit.payload,
                    transition: visit.transition,
                    at_ms: visit.at_ms,
                    is_current: visit.is_current,
                    alternate_children: visit
                        .alternate_children
                        .into_iter()
                        .map(|child| NodeHistoryBranchAlternative {
                            url: child.payload,
                            transition: child.transition,
                            at_ms: child.at_ms,
                        })
                        .collect(),
                })
                .collect(),
            current_index: branch.current_index,
        }
    }

    /// A coarse semantic summary of `node`'s history, for intelligence layers.
    pub fn semantic_summary(&self, node: Uuid) -> NodeHistorySemanticSummary {
        let memory = self.hydrate();
        let Some(owner) = memory.owner_id_by_identity(&NodeHistoryOwner::of(node)) else {
            return NodeHistorySemanticSummary::default();
        };
        let current_url = memory
            .current_entry_of_owner(owner)
            .and_then(|entry_id| memory.entry(entry_id).map(|entry| entry.payload.clone()));
        // Last visit timestamp, scoped to this owner's visits.
        let last_visit_at_ms = memory
            .linear_history_visits_of_owner(owner)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|visit_id| memory.visit(visit_id).map(|v| v.created_at_ms))
            .max()
            .filter(|timestamp| *timestamp > 0);
        let visit_count = memory
            .linear_history_visits_of_owner(owner)
            .map(|v| v.len())
            .unwrap_or(0);
        NodeHistorySemanticSummary {
            current_url,
            last_visit_at_ms,
            visit_count,
        }
    }

    /// The graph's recently-visited nodes, newest first, capped at `limit`. A pure
    /// projection over the one shared visit space (R0 shared-projection): each node
    /// (owner) contributes the url + timestamp of its latest visit; nodes never
    /// navigated are omitted. This is the graph-wide "recent" the forest composes
    /// into — no separate recency list. (gloss recent-nodes; the lineage pane.)
    pub fn recent_visited(&self, limit: usize) -> Vec<RecentVisit> {
        let memory = self.hydrate();
        let mut recent: Vec<RecentVisit> = memory
            .owners()
            .filter_map(|(_, owner)| {
                let NodeHistoryOwner::Node(node_str) = &owner.identity;
                let node = Uuid::parse_str(node_str).ok()?;
                // The owner's latest visit (max created_at over all its visits), so
                // the timestamp is "last touched", robust to back/forward stepping.
                let (entry_id, at_ms) = owner
                    .owned_visits
                    .iter()
                    .filter_map(|vid| {
                        memory
                            .visit(*vid)
                            .map(|visit| (visit.entry, visit.created_at_ms))
                    })
                    .max_by_key(|(_, at)| *at)?;
                let url = memory.entry(entry_id).map(|entry| entry.payload.clone())?;
                Some(RecentVisit {
                    node,
                    url,
                    last_visit_at_ms: at_ms,
                })
            })
            .collect();
        recent.sort_by(|a, b| b.last_visit_at_ms.cmp(&a.last_visit_at_ms));
        recent.truncate(limit);
        recent
    }

    /// `node`'s current page (its cursor's URL), if any.
    pub fn current_url(&self, node: Uuid) -> Option<String> {
        let projection = self.projection(node);
        projection.entries.get(projection.current_index).cloned()
    }

    /// Whether [`back`](Self::back) would move `node`'s cursor.
    pub fn can_back(&self, node: Uuid) -> bool {
        self.projection(node).current_index > 0
    }

    /// Whether [`forward`](Self::forward) would move `node`'s cursor.
    pub fn can_forward(&self, node: Uuid) -> bool {
        let projection = self.projection(node);
        projection.current_index + 1 < projection.entries.len()
    }

    /// Drop `node`'s owner and its visits (on node removal).
    pub fn remove(&mut self, node: Uuid) {
        let mut memory = self.hydrate();
        if let Some(owner) = memory.owner_id_by_identity(&NodeHistoryOwner::of(node)) {
            let _ = memory.delete_owner(owner);
            self.snapshot = memory.to_snapshot();
        }
    }

    /// Seed `node`'s linear history from `entries` with `current_index` current
    /// (a fresh root owner). Used by tests and any linear-history import path.
    pub fn seed_linear(&mut self, node: Uuid, entries: Vec<String>, current_index: usize) {
        if entries.is_empty() {
            return;
        }
        let mut memory = self.hydrate();
        let owner = memory.ensure_owner(NodeHistoryOwner::of(node), None);
        for (idx, url) in entries.iter().enumerate() {
            let entry = memory.resolve_or_create_entry(
                url.clone(),
                url.clone(),
                idx as u64,
                MemoryEntryPrivacy::LocalOnly,
            );
            let transition = if idx == 0 {
                MemoryTransitionKind::UrlTyped
            } else {
                MemoryTransitionKind::Unknown
            };
            let _ = memory.visit_entry(owner, entry, (), transition, idx as u64);
        }
        let clamped = current_index.min(entries.len().saturating_sub(1));
        let steps_back = entries.len().saturating_sub(1).saturating_sub(clamped);
        if steps_back > 0 {
            let _ = memory.back(owner, steps_back, entries.len() as u64);
        }
        self.snapshot = memory.to_snapshot();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(x: u128) -> Uuid {
        Uuid::from_u128(x)
    }

    #[test]
    fn record_visit_back_forward_and_forward_fork() {
        let node = n(1);
        let mut m = SharedNavigationMemory::empty();
        m.record_visit(node, "a", MemoryTransitionKind::UrlTyped, 1);
        m.record_visit(node, "b", MemoryTransitionKind::LinkClick, 2);
        m.record_visit(node, "c", MemoryTransitionKind::LinkClick, 3);
        assert_eq!(m.current_url(node).as_deref(), Some("c"));
        assert!(m.can_back(node));
        assert!(!m.can_forward(node));

        assert_eq!(m.back(node, 4).as_deref(), Some("b"));
        assert_eq!(m.back(node, 5).as_deref(), Some("a"));
        assert_eq!(m.back(node, 6), None, "at the root, back does not move");
        assert!(!m.can_back(node));
        assert!(m.can_forward(node));
        assert_eq!(m.forward(node, 7).as_deref(), Some("b"));

        // Forward-fork: from b, navigate to d. c is preserved as an alternate.
        m.record_visit(node, "d", MemoryTransitionKind::UrlTyped, 8);
        assert_eq!(m.current_url(node).as_deref(), Some("d"));
        let branch = m.branch_projection(node);
        let b_visit = branch
            .visits
            .iter()
            .find(|v| v.url == "b")
            .expect("b on the active path");
        assert!(
            b_visit.alternate_children.iter().any(|alt| alt.url == "c"),
            "c is preserved as an alternate branch off b after the forward-fork",
        );
    }

    #[test]
    fn nodes_have_independent_histories_in_one_space() {
        let (a, b) = (n(1), n(2));
        let mut m = SharedNavigationMemory::empty();
        m.record_visit(a, "a1", MemoryTransitionKind::UrlTyped, 1);
        m.record_visit(a, "a2", MemoryTransitionKind::LinkClick, 2);
        m.record_visit(b, "b1", MemoryTransitionKind::UrlTyped, 3);
        assert_eq!(m.current_url(a).as_deref(), Some("a2"));
        assert_eq!(m.current_url(b).as_deref(), Some("b1"));
        assert!(
            m.can_back(a) && !m.can_back(b),
            "each node walks only its own path"
        );
    }

    #[test]
    fn spawned_child_anchors_under_the_parents_current_visit() {
        let (parent, child) = (n(1), n(2));
        let mut m = SharedNavigationMemory::empty();
        m.record_visit(parent, "p1", MemoryTransitionKind::UrlTyped, 1);
        m.record_visit(parent, "p2", MemoryTransitionKind::LinkClick, 2); // parent current = p2
        // Branch: spawn child anchored at the parent, then its first visit.
        m.spawn(child, parent);
        m.record_visit(child, "c1", MemoryTransitionKind::UrlTyped, 3);
        assert_eq!(m.current_url(child).as_deref(), Some("c1"));
        // The child is its own history; the parent is unchanged (still at p2).
        assert_eq!(m.current_url(parent).as_deref(), Some("p2"));
        assert!(!m.can_back(child), "the child's first visit is its root");
    }

    #[test]
    fn revisiting_a_url_is_a_distinct_visit() {
        let node = n(1);
        let mut m = SharedNavigationMemory::empty();
        m.record_visit(node, "a", MemoryTransitionKind::UrlTyped, 1);
        m.record_visit(node, "a", MemoryTransitionKind::Reload, 2);
        assert_eq!(
            m.projection(node).entries,
            vec!["a".to_string(), "a".to_string()]
        );
    }

    #[test]
    fn seed_linear_and_remove() {
        let node = n(1);
        let mut m = SharedNavigationMemory::empty();
        m.seed_linear(node, vec!["a".into(), "b".into(), "c".into()], 1);
        assert_eq!(
            m.current_url(node).as_deref(),
            Some("b"),
            "cursor at the seeded index"
        );
        assert!(m.can_back(node) && m.can_forward(node));
        m.remove(node);
        assert_eq!(m.current_url(node), None, "owner dropped on remove");
    }

    #[test]
    fn recent_visited_rolls_the_forest_up_by_last_visit_newest_first() {
        let (a, b, c) = (n(1), n(2), n(3));
        let mut m = SharedNavigationMemory::empty();
        m.record_visit(a, "a1", MemoryTransitionKind::UrlTyped, 10);
        m.record_visit(b, "b1", MemoryTransitionKind::UrlTyped, 30);
        m.record_visit(c, "c1", MemoryTransitionKind::UrlTyped, 20);
        m.record_visit(a, "a2", MemoryTransitionKind::LinkClick, 40); // a's last touch is now newest

        let recent = m.recent_visited(10);
        assert_eq!(recent.len(), 3, "every visited node appears once");
        assert_eq!(recent[0].node, a, "a (last visit 40) is newest");
        assert_eq!(recent[0].url, "a2", "its latest url, not its first");
        assert_eq!(recent[0].last_visit_at_ms, 40);
        assert_eq!(recent[1].node, b, "b (30) before c (20)");
        assert_eq!(recent[2].node, c);

        // `limit` caps the list to the most-recent N.
        let top = m.recent_visited(1);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].node, a);

        // A fresh space has no recents.
        assert!(SharedNavigationMemory::empty().recent_visited(5).is_empty());
    }
}
