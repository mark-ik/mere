// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Convenience query functions over `GraphTree`.
//!
//! These are thin wrappers that combine topology + membership queries
//! into common patterns. They exist so callers don't need to reach
//! through `tree.topology()` for everyday questions.

use crate::MemberId;
use crate::member::Lifecycle;
use crate::tree::GraphTree;

/// Members filtered by lifecycle.
pub fn members_by_lifecycle<'a, N: MemberId>(
    tree: &'a GraphTree<N>,
    lifecycle: Lifecycle,
) -> Vec<&'a N> {
    tree.members()
        .filter(|(_, entry)| entry.lifecycle == lifecycle)
        .map(|(id, _)| id)
        .collect()
}

/// Active members in topology order (depth-first from roots).
pub fn active_in_tree_order<N: MemberId>(tree: &GraphTree<N>) -> Vec<N> {
    let mut result = Vec::new();
    collect_by_lifecycle_dfs(
        tree,
        tree.topology().roots(),
        Lifecycle::Active,
        &mut result,
    );
    result
}

/// Warm members in topology order.
pub fn warm_in_tree_order<N: MemberId>(tree: &GraphTree<N>) -> Vec<N> {
    let mut result = Vec::new();
    collect_by_lifecycle_dfs(tree, tree.topology().roots(), Lifecycle::Warm, &mut result);
    result
}

/// All visible members (Active + Warm) in topology order.
pub fn visible_in_tree_order<N: MemberId>(tree: &GraphTree<N>) -> Vec<N> {
    let mut result = Vec::new();
    collect_visible_dfs(tree, tree.topology().roots(), &mut result);
    result
}

/// Find the nearest active ancestor of a member.
pub fn nearest_active_ancestor<'a, N: MemberId>(
    tree: &'a GraphTree<N>,
    member: &N,
) -> Option<&'a N> {
    for ancestor in tree.topology().ancestors(member) {
        if let Some(entry) = tree.get(&ancestor) {
            if entry.lifecycle == Lifecycle::Active {
                // Return the ancestor from the tree's own storage
                return tree
                    .members()
                    .find(|(id, _)| **id == ancestor)
                    .map(|(id, _)| id);
            }
        }
    }
    None
}

/// Find the next sibling that is active, or None.
pub fn next_active_sibling<'a, N: MemberId>(tree: &'a GraphTree<N>, member: &N) -> Option<&'a N> {
    let siblings = tree.topology().siblings(member);
    for sibling in &siblings {
        if let Some(entry) = tree.get(sibling) {
            if entry.lifecycle == Lifecycle::Active {
                return tree
                    .members()
                    .find(|(id, _)| *id == sibling)
                    .map(|(id, _)| id);
            }
        }
    }
    None
}

/// Count members at each depth level.
pub fn depth_histogram<N: MemberId>(tree: &GraphTree<N>) -> Vec<usize> {
    let mut histogram = Vec::new();
    for (id, _) in tree.members() {
        let depth = tree.topology().depth_of(id);
        if depth >= histogram.len() {
            histogram.resize(depth + 1, 0);
        }
        histogram[depth] += 1;
    }
    histogram
}

// --- Private helpers ---

fn collect_by_lifecycle_dfs<N: MemberId>(
    tree: &GraphTree<N>,
    nodes: &[N],
    lifecycle: Lifecycle,
    result: &mut Vec<N>,
) {
    for node in nodes {
        if let Some(entry) = tree.get(node) {
            if entry.lifecycle == lifecycle {
                result.push(node.clone());
            }
        }
        let children = tree.topology().children_of(node).to_vec();
        collect_by_lifecycle_dfs(tree, &children, lifecycle, result);
    }
}

fn collect_visible_dfs<N: MemberId>(tree: &GraphTree<N>, nodes: &[N], result: &mut Vec<N>) {
    for node in nodes {
        if let Some(entry) = tree.get(node) {
            if entry.is_visible_in_pane() {
                result.push(node.clone());
            }
        }
        let children = tree.topology().children_of(node).to_vec();
        collect_visible_dfs(tree, &children, result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::member::Provenance;

    fn make_tree() -> GraphTree<u64> {
        let mut tree = GraphTree::new(
            crate::layout::LayoutMode::TreeStyleTabs,
            crate::lens::ProjectionLens::Traversal,
        );
        tree.apply(crate::nav::NavAction::Attach {
            member: 1,
            provenance: Provenance::Anchor,
        });
        tree.apply(crate::nav::NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(crate::nav::NavAction::Attach {
            member: 3,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        // Set lifecycles
        tree.apply(crate::nav::NavAction::SetLifecycle(1, Lifecycle::Active));
        tree.apply(crate::nav::NavAction::SetLifecycle(2, Lifecycle::Active));
        tree.apply(crate::nav::NavAction::SetLifecycle(3, Lifecycle::Warm));
        tree
    }

    #[test]
    fn active_members_in_order() {
        let tree = make_tree();
        let active = active_in_tree_order(&tree);
        assert_eq!(active, vec![1, 2]);
    }

    #[test]
    fn visible_members_in_order() {
        let tree = make_tree();
        let visible = visible_in_tree_order(&tree);
        assert_eq!(visible, vec![1, 2, 3]);
    }

    #[test]
    fn depth_histogram_works() {
        let tree = make_tree();
        let hist = depth_histogram(&tree);
        assert_eq!(hist, vec![1, 2]); // 1 root, 2 at depth 1
    }
}
