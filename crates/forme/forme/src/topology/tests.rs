// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;

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
