/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Per-node navigation history.
//!
//! Carries each node's webview navigation history projection over the
//! shared `graph_memory::GraphMemory` substrate. The history is
//! addressable per-owner (one node has one `Primary` owner today);
//! projections expose a linear-history view, a branching-history view,
//! and a coarse semantic summary used by intelligence layers.
//!
//! Extracted from `graph/mod.rs` per the 2026-05-11 kernel-mod
//! decomposition pass — the kernel's `graph/mod.rs` was 5102 LOC and
//! well over the 600-LOC ceiling. Earlier work moved
//! identity types to `identity.rs` and `Node` / `NodeLifecycle` to
//! `node.rs`; node history was already documented as the next natural
//! split target in `node.rs`'s module header.

use graph_memory::{
    EntryPrivacy as MemoryEntryPrivacy, GraphMemory as OwnerScopedMemory, GraphMemorySnapshot,
    TransitionKind as MemoryTransitionKind,
};
use rkyv::{Archive, Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
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
    Primary,
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
pub struct NodeNavigationMemory {
    snapshot: GraphMemorySnapshot<String, String, NodeHistoryOwner, ()>,
}

impl Default for NodeNavigationMemory {
    fn default() -> Self {
        Self::empty()
    }
}

impl NodeNavigationMemory {
    pub fn empty() -> Self {
        Self {
            snapshot: GraphMemorySnapshot {
                entries: Vec::new(),
                visits: Vec::new(),
                owners: Vec::new(),
            },
        }
    }

    pub fn from_linear_history(entries: Vec<String>, current_index: usize) -> Self {
        if entries.is_empty() {
            return Self::empty();
        }

        let mut memory = OwnerScopedMemory::<String, String, NodeHistoryOwner, ()>::new();
        let owner = memory.ensure_owner(NodeHistoryOwner::Primary, None);
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

        let clamped_index = current_index.min(entries.len().saturating_sub(1));
        let steps_back = entries
            .len()
            .saturating_sub(1)
            .saturating_sub(clamped_index);
        if steps_back > 0 {
            let _ = memory.back(owner, steps_back, entries.len() as u64);
        }

        Self {
            snapshot: memory.to_snapshot(),
        }
    }

    pub fn projection(&self) -> NodeHistoryProjection {
        let memory = OwnerScopedMemory::<String, String, NodeHistoryOwner, ()>::from_snapshot(
            self.snapshot.clone(),
        );
        let Some(owner) = memory.owner_id_by_identity(&NodeHistoryOwner::Primary) else {
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

    pub fn branch_projection(&self) -> NodeHistoryBranchProjection {
        let memory = OwnerScopedMemory::<String, String, NodeHistoryOwner, ()>::from_snapshot(
            self.snapshot.clone(),
        );
        let Some(owner) = memory.owner_id_by_identity(&NodeHistoryOwner::Primary) else {
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

    pub fn current_url(&self) -> Option<String> {
        let projection = self.projection();
        projection.entries.get(projection.current_index).cloned()
    }

    pub fn semantic_summary(&self) -> NodeHistorySemanticSummary {
        let memory = OwnerScopedMemory::<String, String, NodeHistoryOwner, ()>::from_snapshot(
            self.snapshot.clone(),
        );
        let Some(owner) = memory.owner_id_by_identity(&NodeHistoryOwner::Primary) else {
            return NodeHistorySemanticSummary::default();
        };

        let current_url = memory
            .current_entry_of_owner(owner)
            .and_then(|entry_id| memory.entry(entry_id).map(|entry| entry.payload.clone()));
        let last_visit_at_ms = memory
            .visits()
            .map(|(_, visit)| visit.created_at_ms)
            .max()
            .filter(|timestamp| *timestamp > 0);

        NodeHistorySemanticSummary {
            current_url,
            last_visit_at_ms,
            visit_count: memory.visit_count(),
        }
    }

    pub fn replace_linear_history(&mut self, entries: Vec<String>, current_index: usize) {
        if entries.is_empty() {
            *self = Self::empty();
            return;
        }

        let mut memory = OwnerScopedMemory::<String, String, NodeHistoryOwner, ()>::from_snapshot(
            self.snapshot.clone(),
        );
        let owner = memory.ensure_owner(NodeHistoryOwner::Primary, None);
        let existing_visits = memory
            .linear_history_visits_of_owner(owner)
            .unwrap_or_default();
        let existing_entries = existing_visits
            .iter()
            .filter_map(|visit_id| {
                let visit = memory.visit(*visit_id)?;
                let entry = memory.entry(visit.entry)?;
                Some(entry.payload.clone())
            })
            .collect::<Vec<_>>();

        if existing_entries.first() != entries.first() {
            *self = Self::from_linear_history(entries, current_index);
            return;
        }

        let mut path = vec![existing_visits[0]];
        let mut parent = existing_visits[0];

        for (idx, url) in entries.iter().enumerate().skip(1) {
            let entry_id = memory.resolve_or_create_entry(
                url.clone(),
                url.clone(),
                idx as u64,
                MemoryEntryPrivacy::LocalOnly,
            );
            let reusable_child = memory.visit(parent).and_then(|visit| {
                visit.children.iter().copied().find(|child_id| {
                    memory
                        .visit(*child_id)
                        .is_some_and(|child| child.entry == entry_id)
                })
            });

            let child_id = if let Some(child_id) = reusable_child {
                child_id
            } else {
                let parent_index = path.len().saturating_sub(1);
                if memory
                    .rebind_owner_to_path(owner, &path, parent_index, idx as u64)
                    .is_err()
                {
                    *self = Self::from_linear_history(entries, current_index);
                    return;
                }
                let transition = if idx == 0 {
                    MemoryTransitionKind::UrlTyped
                } else {
                    MemoryTransitionKind::Unknown
                };
                let Ok(child_id) = memory.visit_entry(owner, entry_id, (), transition, idx as u64)
                else {
                    *self = Self::from_linear_history(entries, current_index);
                    return;
                };
                child_id
            };

            path.push(child_id);
            parent = child_id;
        }

        if memory
            .rebind_owner_to_path(owner, &path, current_index, entries.len() as u64)
            .is_err()
        {
            *self = Self::from_linear_history(entries, current_index);
            return;
        }

        self.snapshot = memory.to_snapshot();
    }

    pub fn snapshot(&self) -> &GraphMemorySnapshot<String, String, NodeHistoryOwner, ()> {
        &self.snapshot
    }
}
