// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::MemberId;
use crate::graphlet::GraphletId;
use crate::lens::ProjectionLens;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// The tree's parent-child structure, derived from graph edges.
/// This is NOT spatial layout — it's semantic grouping.
///
/// Placement rules:
/// - Traversal: child of source node ("opened B from A" -> B is child of A)
/// - Manual add: sibling of connection point (same parent as the node
///   you were looking at when you added it)
/// - Derived (graphlet computation): sibling of connection point,
///   or child of graphlet anchor if no specific connection
/// - AgentDerived: sibling of source, pending user accept
/// - Anchor: root
/// - Restored: original position from persistence
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct TreeTopology<N: MemberId> {
    parent: HashMap<N, N>,
    children: HashMap<N, Vec<N>>,
    roots: Vec<N>,
    insertion_order: Vec<N>,
}

impl<N: MemberId> Default for TreeTopology<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N: MemberId> TreeTopology<N> {
    pub fn new() -> Self {
        Self {
            parent: HashMap::new(),
            children: HashMap::new(),
            roots: Vec::new(),
            insertion_order: Vec::new(),
        }
    }

    /// Attach a member as a child of the given parent.
    /// Returns `false` (no-op) if child == parent, child is already placed,
    /// or **parent does not exist in the topology**.
    pub fn attach_child(&mut self, child: N, parent: &N) -> bool {
        if child == *parent || self.contains(&child) || !self.contains(parent) {
            return false;
        }
        self.parent.insert(child.clone(), parent.clone());
        self.children
            .entry(parent.clone())
            .or_default()
            .push(child.clone());
        if !self.insertion_order.contains(&child) {
            self.insertion_order.push(child);
        }
        true
    }

    /// Attach a member as a sibling of the given node (same parent).
    /// If `sibling_of` is a root, the new member becomes a root too.
    /// Returns `false` (no-op) if `sibling_of` does not exist in the topology.
    pub fn attach_sibling(&mut self, member: N, sibling_of: &N) -> bool {
        if !self.contains(sibling_of) {
            return false;
        }
        if let Some(parent) = self.parent.get(sibling_of).cloned() {
            self.attach_child(member, &parent)
        } else {
            // sibling_of is a root — new member also becomes a root
            self.attach_root(member)
        }
    }

    /// Attach a member as a root node. No-op if already in the topology.
    pub fn attach_root(&mut self, member: N) -> bool {
        if self.contains(&member) {
            return false;
        }
        self.roots.push(member.clone());
        self.insertion_order.push(member);
        true
    }

    /// Move a member to be a child of a new parent.
    /// Returns `false` (no-op) if the move would create a cycle
    /// (new_parent is a descendant of member), member == new_parent,
    /// or either node does not exist in the topology.
    pub fn reparent(&mut self, member: &N, new_parent: &N) -> bool {
        if member == new_parent {
            return false;
        }
        if !self.contains(member) || !self.contains(new_parent) {
            return false;
        }
        // Cycle check: new_parent must not be a descendant of member.
        if self.is_ancestor_of(member, new_parent) {
            return false;
        }
        self.detach_from_parent(member);
        self.parent.insert(member.clone(), new_parent.clone());
        self.children
            .entry(new_parent.clone())
            .or_default()
            .push(member.clone());
        true
    }

    /// Detach a member and its subtree from the topology.
    /// Returns the detached subtree members (depth-first order).
    pub fn detach(&mut self, member: &N) -> Vec<N> {
        let subtree = self.descendants_inclusive(member);
        self.detach_from_parent(member);

        for node in &subtree {
            self.parent.remove(node);
            self.children.remove(node);
            self.insertion_order.retain(|n| n != node);
        }

        subtree
    }

    /// Re-attach an existing topology member as a child of a new parent.
    /// Unlike `attach_child`, this does not require the child to be absent
    /// from the topology — it's used after `detach_single` to reconnect
    /// orphaned children to a new parent. The child's subtree is preserved.
    pub fn reattach_child(&mut self, child: N, parent: &N) {
        // Set parent pointer.
        self.parent.insert(child.clone(), parent.clone());
        // Add to parent's children list.
        self.children
            .entry(parent.clone())
            .or_default()
            .push(child.clone());
        // Ensure the child is in insertion_order (it should already be).
        if !self.insertion_order.contains(&child) {
            self.insertion_order.push(child);
        }
    }

    /// Detach a single node from the topology WITHOUT removing its subtree.
    /// Children of the detached node become disconnected from their parent
    /// but remain in the topology's children/parent maps — the caller is
    /// responsible for reparenting or promoting them.
    pub fn detach_single(&mut self, member: &N) {
        // Remove parent pointer and unlink from parent's children list.
        self.detach_from_parent(member);
        // Remove from insertion_order.
        self.insertion_order.retain(|n| n != member);
        // Clear this node's children list (children keep their parent pointers
        // which now point to a removed node — caller must fix them).
        if let Some(children) = self.children.remove(member) {
            // Clear the dangling parent pointers on the children.
            for child in &children {
                self.parent.remove(child);
            }
        }
    }

    /// Promote a member to root status. Used after `detach_single` removes
    /// a parent — the orphaned child needs to become a root.
    ///
    /// No-op if the member is already a root or doesn't exist in the topology.
    pub fn promote_to_root(&mut self, member: &N) {
        if !self.insertion_order.contains(member) {
            return;
        }
        // Remove existing parent pointer if any.
        if let Some(parent) = self.parent.remove(member) {
            if let Some(siblings) = self.children.get_mut(&parent) {
                siblings.retain(|n| n != member);
            }
        }
        // Add to roots if not already there.
        if !self.roots.contains(member) {
            self.roots.push(member.clone());
        }
    }

    /// Reorder children of a parent node.
    pub fn reorder_children(&mut self, parent: &N, new_order: Vec<N>) {
        if let Some(children) = self.children.get_mut(parent) {
            // Only keep children that are actually children of this parent
            let valid: std::collections::HashSet<&N> = children.iter().collect();
            let mut reordered: Vec<N> = new_order
                .into_iter()
                .filter(|n| valid.contains(n))
                .collect();

            // Re-add any active children that were omitted from `new_order`
            // to ensure we never orphan members from the topology.
            let explicit: std::collections::HashSet<N> = reordered.iter().cloned().collect();
            for child in children.iter() {
                if !explicit.contains(child) {
                    reordered.push(child.clone());
                }
            }

            *children = reordered;
        }
    }

    /// Get the parent of a member, if any.
    pub fn parent_of(&self, member: &N) -> Option<&N> {
        self.parent.get(member)
    }

    /// Get children of a member.
    pub fn children_of(&self, member: &N) -> &[N] {
        self.children
            .get(member)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get root nodes.
    pub fn roots(&self) -> &[N] {
        &self.roots
    }

    /// Get insertion order.
    pub fn insertion_order(&self) -> &[N] {
        &self.insertion_order
    }

    /// Check if a member exists in the topology.
    pub fn contains(&self, member: &N) -> bool {
        self.roots.contains(member) || self.parent.contains_key(member)
    }

    /// Returns true if the member has children.
    pub fn has_children(&self, member: &N) -> bool {
        self.children.get(member).is_some_and(|c| !c.is_empty())
    }

    /// Depth of a member (0 for roots).
    pub fn depth_of(&self, member: &N) -> usize {
        let mut depth = 0;
        let mut current = member;
        while let Some(p) = self.parent.get(current) {
            depth += 1;
            current = p;
        }
        depth
    }

    /// All descendants of a member (not including the member itself).
    pub fn descendants(&self, member: &N) -> Vec<N> {
        let mut result = Vec::new();
        self.collect_descendants(member, &mut result);
        result
    }

    /// All ancestors of a member (parent, grandparent, ..., root).
    pub fn ancestors(&self, member: &N) -> Vec<N> {
        let mut result = Vec::new();
        let mut current = member;
        while let Some(p) = self.parent.get(current) {
            result.push(p.clone());
            current = p;
        }
        result
    }

    /// Siblings of a member (same parent, excluding self).
    pub fn siblings(&self, member: &N) -> Vec<N> {
        if let Some(parent) = self.parent.get(member) {
            self.children_of(parent)
                .iter()
                .filter(|n| *n != member)
                .cloned()
                .collect()
        } else {
            // Root siblings
            self.roots
                .iter()
                .filter(|n| *n != member)
                .cloned()
                .collect()
        }
    }

    /// Depth-first walk respecting expansion state.
    pub fn visible_walk<'a>(
        &'a self,
        expanded: &'a HashSet<N>,
        _lens: &ProjectionLens,
    ) -> Vec<TreeRow<'a, N>> {
        let mut rows = Vec::new();
        for root in &self.roots {
            self.walk_node(root, 0, expanded, &mut rows);
        }
        rows
    }

    /// Total number of members in the topology.
    pub fn len(&self) -> usize {
        // Roots that also appear as children are counted once via insertion_order
        self.insertion_order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.insertion_order.is_empty()
    }

    // --- Private helpers ---

    fn detach_from_parent(&mut self, member: &N) {
        if let Some(parent) = self.parent.remove(member) {
            if let Some(siblings) = self.children.get_mut(&parent) {
                siblings.retain(|n| n != member);
            }
        } else {
            self.roots.retain(|n| n != member);
        }
    }

    fn descendants_inclusive(&self, member: &N) -> Vec<N> {
        let mut result = vec![member.clone()];
        self.collect_descendants(member, &mut result);
        result
    }

    fn collect_descendants(&self, member: &N, result: &mut Vec<N>) {
        if let Some(children) = self.children.get(member) {
            for child in children {
                result.push(child.clone());
                self.collect_descendants(child, result);
            }
        }
    }

    fn walk_node<'a>(
        &'a self,
        member: &'a N,
        depth: usize,
        expanded: &HashSet<N>,
        rows: &mut Vec<TreeRow<'a, N>>,
    ) {
        let has_children = self.has_children(member);
        let is_expanded = expanded.contains(member);
        let is_last = self.is_last_sibling(member);

        rows.push(TreeRow {
            member,
            depth,
            is_expanded: is_expanded && has_children,
            has_children,
            is_last_sibling: is_last,
            graphlet_id: None, // Filled in by GraphTree when walking
        });

        if has_children && is_expanded {
            for child in self.children_of(member) {
                self.walk_node(child, depth + 1, expanded, rows);
            }
        }
    }

    fn is_last_sibling(&self, member: &N) -> bool {
        if let Some(parent) = self.parent.get(member) {
            self.children_of(parent).last() == Some(member)
        } else {
            self.roots.last() == Some(member)
        }
    }

    /// Returns true if `ancestor` is an ancestor of `descendant`
    /// (or if they are the same node).
    fn is_ancestor_of(&self, ancestor: &N, descendant: &N) -> bool {
        if ancestor == descendant {
            return true;
        }
        let mut current = descendant;
        while let Some(p) = self.parent.get(current) {
            if p == ancestor {
                return true;
            }
            current = p;
        }
        false
    }

    /// Debug-mode invariant check. Panics if the topology is inconsistent.
    /// Call after mutations in tests or with `debug_assert!`.
    pub fn assert_invariants(&self) {
        // 1. Every child's parent pointer matches the children map
        for (child, parent) in &self.parent {
            let children = self.children_of(parent);
            assert!(
                children.contains(child),
                "child {:?} claims parent {:?}, but parent's children list doesn't contain it",
                child,
                parent
            );
        }

        // 2. Every entry in children maps has a matching parent pointer
        for (parent, children) in &self.children {
            for child in children {
                assert_eq!(
                    self.parent.get(child),
                    Some(parent),
                    "parent {:?} lists child {:?}, but child's parent pointer is {:?}",
                    parent,
                    child,
                    self.parent.get(child)
                );
            }
        }

        // 3. Roots have no parent
        for root in &self.roots {
            assert!(
                !self.parent.contains_key(root),
                "root {:?} has a parent pointer",
                root
            );
        }

        // 4. Every member without a parent should be a root
        for node in &self.insertion_order {
            if !self.parent.contains_key(node) {
                assert!(
                    self.roots.contains(node),
                    "node {:?} has no parent but is not a root",
                    node
                );
            }
        }

        // 5. No duplicate children
        for (parent, children) in &self.children {
            let unique: HashSet<&N> = children.iter().collect();
            assert_eq!(
                unique.len(),
                children.len(),
                "parent {:?} has duplicate children",
                parent
            );
        }

        // 6. No duplicate roots
        let unique_roots: HashSet<&N> = self.roots.iter().collect();
        assert_eq!(unique_roots.len(), self.roots.len(), "duplicate roots");

        // 7. Every node in insertion_order is reachable from a root
        let reachable = self.all_reachable_from_roots();
        for node in &self.insertion_order {
            assert!(
                reachable.contains(node),
                "node {:?} is in insertion_order but not reachable from any root",
                node
            );
        }

        // 8. Every reachable node is in insertion_order
        for node in &reachable {
            assert!(
                self.insertion_order.contains(node),
                "node {:?} is reachable from roots but missing from insertion_order",
                node
            );
        }
    }

    /// Collect all nodes reachable from roots via depth-first traversal.
    fn all_reachable_from_roots(&self) -> HashSet<N> {
        let mut reachable = HashSet::new();
        for root in &self.roots {
            self.collect_reachable(root, &mut reachable);
        }
        reachable
    }

    fn collect_reachable(&self, node: &N, reachable: &mut HashSet<N>) {
        if !reachable.insert(node.clone()) {
            return; // already visited
        }
        if let Some(children) = self.children.get(node) {
            for child in children {
                self.collect_reachable(child, reachable);
            }
        }
    }
}

/// A row in the visible tree walk.
#[derive(Clone, Debug)]
pub struct TreeRow<'a, N: MemberId> {
    pub member: &'a N,
    pub depth: usize,
    pub is_expanded: bool,
    pub has_children: bool,
    pub is_last_sibling: bool,
    pub graphlet_id: Option<GraphletId>,
}

/// How derived members are placed in the topology.
#[derive(Clone, Debug)]
pub enum PlacementPolicy {
    /// Child of the node they're connected to.
    ChildOfConnection,
    /// Sibling of the node they're connected to (same parent).
    SiblingOfConnection,
    /// Child of the graphlet anchor.
    ChildOfAnchor,
}

/// Derive a `TreeTopology` from a petgraph graph by walking selected edges
/// outward from the given root nodes (BFS). Only edges accepted by `selector`
/// are followed. Each discovered node is placed according to `policy`.
///
/// Nodes reachable from multiple roots are assigned to whichever root's BFS
/// reaches them first. Unreachable nodes are ignored.
#[cfg(feature = "petgraph")]
pub fn derive_topology<N, E, Ix>(
    graph: &petgraph::Graph<N, E, petgraph::Directed, Ix>,
    roots: &[petgraph::graph::NodeIndex<Ix>],
    selector: impl Fn(&E) -> bool,
    policy: &PlacementPolicy,
) -> TreeTopology<N>
where
    N: MemberId,
    Ix: petgraph::graph::IndexType,
{
    use petgraph::visit::EdgeRef;
    use std::collections::VecDeque;

    let mut topo = TreeTopology::new();
    let mut visited: HashSet<petgraph::graph::NodeIndex<Ix>> = HashSet::new();

    // Map from NodeIndex to the node weight (our MemberId)
    let weight = |idx: petgraph::graph::NodeIndex<Ix>| -> &N { &graph[idx] };

    let mut queue: VecDeque<petgraph::graph::NodeIndex<Ix>> = VecDeque::new();

    // Seed with roots
    for &root_idx in roots {
        if visited.insert(root_idx) {
            topo.attach_root(weight(root_idx).clone());
            queue.push_back(root_idx);
        }
    }

    // BFS
    while let Some(current_idx) = queue.pop_front() {
        for edge in graph.edges(current_idx) {
            if !selector(edge.weight()) {
                continue;
            }
            let target_idx = edge.target();
            if !visited.insert(target_idx) {
                continue;
            }

            let target = weight(target_idx).clone();
            let current = weight(current_idx).clone();

            match policy {
                PlacementPolicy::ChildOfConnection => {
                    topo.attach_child(target, &current);
                }
                PlacementPolicy::SiblingOfConnection => {
                    topo.attach_sibling(target, &current);
                }
                PlacementPolicy::ChildOfAnchor => {
                    // Find the root ancestor of current, attach as child of that
                    let mut ancestor = current.clone();
                    while let Some(p) = topo.parent_of(&ancestor) {
                        ancestor = p.clone();
                    }
                    topo.attach_child(target, &ancestor);
                }
            }

            queue.push_back(target_idx);
        }
    }

    topo
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_and_query() {
        let mut topo = TreeTopology::<u64>::new();
        assert!(topo.attach_root(1));
        assert!(topo.attach_child(2, &1));
        assert!(topo.attach_child(3, &1));
        assert!(topo.attach_child(4, &2));

        assert_eq!(topo.roots(), &[1]);
        assert_eq!(topo.children_of(&1), &[2, 3]);
        assert_eq!(topo.children_of(&2), &[4]);
        assert_eq!(topo.parent_of(&4), Some(&2));
        assert_eq!(topo.depth_of(&4), 2);
        assert_eq!(topo.depth_of(&1), 0);
        topo.assert_invariants();
    }

    #[test]
    fn siblings() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);
        topo.attach_child(2, &1);
        topo.attach_child(3, &1);

        assert_eq!(topo.siblings(&2), vec![3]);
        assert_eq!(topo.siblings(&3), vec![2]);
        topo.assert_invariants();
    }

    #[test]
    fn attach_sibling() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);
        topo.attach_child(2, &1);
        topo.attach_sibling(3, &2); // should become child of 1

        assert_eq!(topo.children_of(&1), &[2, 3]);
        assert_eq!(topo.parent_of(&3), Some(&1));
        topo.assert_invariants();
    }

    #[test]
    fn attach_sibling_of_root() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);
        topo.attach_sibling(2, &1);

        assert_eq!(topo.roots(), &[1, 2]);
        topo.assert_invariants();
    }

    #[test]
    fn detach_subtree() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);
        topo.attach_child(2, &1);
        topo.attach_child(3, &2);
        topo.attach_child(4, &2);

        let detached = topo.detach(&2);
        assert_eq!(detached, vec![2, 3, 4]);
        assert_eq!(topo.children_of(&1), &[] as &[u64]);
        assert!(!topo.contains(&2));
        topo.assert_invariants();
    }

    #[test]
    fn reparent() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);
        topo.attach_root(2);
        topo.attach_child(3, &1);

        assert!(topo.reparent(&3, &2));
        assert_eq!(topo.children_of(&1), &[] as &[u64]);
        assert_eq!(topo.children_of(&2), &[3]);
        assert_eq!(topo.parent_of(&3), Some(&2));
        topo.assert_invariants();
    }

    #[test]
    fn visible_walk_respects_expansion() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);
        topo.attach_child(2, &1);
        topo.attach_child(3, &2);
        topo.attach_child(4, &1);

        // Only root expanded — children of 2 hidden
        let mut expanded = HashSet::new();
        expanded.insert(1u64);
        let rows = topo.visible_walk(&expanded, &ProjectionLens::Traversal);
        let ids: Vec<&u64> = rows.iter().map(|r| r.member).collect();
        assert_eq!(ids, vec![&1, &2, &4]);

        // Expand 2 as well
        expanded.insert(2);
        let rows = topo.visible_walk(&expanded, &ProjectionLens::Traversal);
        let ids: Vec<&u64> = rows.iter().map(|r| r.member).collect();
        assert_eq!(ids, vec![&1, &2, &3, &4]);
    }

    #[test]
    fn ancestors_and_descendants() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);
        topo.attach_child(2, &1);
        topo.attach_child(3, &2);

        assert_eq!(topo.ancestors(&3), vec![2, 1]);
        assert_eq!(topo.descendants(&1), vec![2, 3]);
    }

    #[test]
    fn reorder_children() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);
        topo.attach_child(2, &1);
        topo.attach_child(3, &1);
        topo.attach_child(4, &1);

        topo.reorder_children(&1, vec![4, 2, 3]);
        assert_eq!(topo.children_of(&1), &[4, 2, 3]);
        topo.assert_invariants();
    }

    // --- Invariant enforcement tests ---

    #[test]
    fn reparent_rejects_cycle() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);
        topo.attach_child(2, &1);
        topo.attach_child(3, &2);

        // Trying to make 1 a child of 3 would create 1→2→3→1 cycle
        assert!(!topo.reparent(&1, &3));
        // Tree unchanged
        assert_eq!(topo.roots(), &[1]);
        assert_eq!(topo.parent_of(&2), Some(&1));
        topo.assert_invariants();
    }

    #[test]
    fn reparent_rejects_self() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);

        assert!(!topo.reparent(&1, &1));
        topo.assert_invariants();
    }

    #[test]
    fn attach_child_rejects_self_parent() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);

        assert!(!topo.attach_child(1, &1));
        topo.assert_invariants();
    }

    #[test]
    fn attach_child_rejects_duplicate() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);
        assert!(topo.attach_child(2, &1));
        // Second attach of same node is rejected
        assert!(!topo.attach_child(2, &1));
        assert_eq!(topo.children_of(&1), &[2]);
        topo.assert_invariants();
    }

    #[test]
    fn attach_root_rejects_duplicate() {
        let mut topo = TreeTopology::<u64>::new();
        assert!(topo.attach_root(1));
        assert!(!topo.attach_root(1));
        assert_eq!(topo.roots(), &[1]);
        topo.assert_invariants();
    }

    #[test]
    fn attach_child_rejects_missing_parent() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);
        // Parent 99 doesn't exist — should reject
        assert!(!topo.attach_child(2, &99));
        assert!(!topo.contains(&2));
        topo.assert_invariants();
    }

    #[test]
    fn attach_sibling_rejects_missing_reference() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);
        // sibling_of 99 doesn't exist — should reject
        assert!(!topo.attach_sibling(2, &99));
        assert!(!topo.contains(&2));
        topo.assert_invariants();
    }

    #[test]
    fn reparent_rejects_missing_parent() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);
        // new_parent 99 doesn't exist
        assert!(!topo.reparent(&1, &99));
        assert_eq!(topo.roots(), &[1]);
        topo.assert_invariants();
    }

    #[test]
    fn reparent_rejects_missing_member() {
        let mut topo = TreeTopology::<u64>::new();
        topo.attach_root(1);
        // member 99 doesn't exist
        assert!(!topo.reparent(&99, &1));
        topo.assert_invariants();
    }

    #[cfg(feature = "petgraph")]
    mod petgraph_tests {
        use super::*;
        use crate::topology::derive_topology;

        #[test]
        fn derive_child_of_connection() {
            let mut graph = petgraph::Graph::<u64, &str>::new();
            let a = graph.add_node(1);
            let b = graph.add_node(2);
            let c = graph.add_node(3);
            let d = graph.add_node(4);

            graph.add_edge(a, b, "traversal");
            graph.add_edge(b, c, "traversal");
            graph.add_edge(a, d, "unrelated");

            let topo = derive_topology(
                &graph,
                &[a],
                |e| *e == "traversal",
                &PlacementPolicy::ChildOfConnection,
            );

            assert_eq!(topo.roots(), &[1]);
            assert_eq!(topo.children_of(&1), &[2]);
            assert_eq!(topo.children_of(&2), &[3]);
            // 4 is not reachable via "traversal" edges
            assert!(!topo.contains(&4));
            topo.assert_invariants();
        }

        #[test]
        fn derive_all_edges() {
            let mut graph = petgraph::Graph::<u64, &str>::new();
            let a = graph.add_node(1);
            let b = graph.add_node(2);
            let c = graph.add_node(3);

            graph.add_edge(a, b, "traversal");
            graph.add_edge(a, c, "manual");

            let topo = derive_topology(&graph, &[a], |_| true, &PlacementPolicy::ChildOfConnection);

            assert_eq!(topo.roots(), &[1]);
            assert!(topo.contains(&2));
            assert!(topo.contains(&3));
            assert_eq!(topo.parent_of(&2), Some(&1));
            assert_eq!(topo.parent_of(&3), Some(&1));
            topo.assert_invariants();
        }

        #[test]
        fn derive_sibling_policy() {
            let mut graph = petgraph::Graph::<u64, &str>::new();
            let a = graph.add_node(1);
            let b = graph.add_node(2);
            let c = graph.add_node(3);

            graph.add_edge(a, b, "link");
            graph.add_edge(b, c, "link");

            let topo = derive_topology(
                &graph,
                &[a],
                |_| true,
                &PlacementPolicy::SiblingOfConnection,
            );

            // 2 is sibling of root 1, so also a root
            // 3 is sibling of 2, so also a root
            assert!(topo.roots().contains(&1));
            assert!(topo.roots().contains(&2));
            assert!(topo.roots().contains(&3));
            topo.assert_invariants();
        }
    }
}
