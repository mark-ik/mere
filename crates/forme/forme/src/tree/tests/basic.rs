// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;

#[test]
fn new_tree_is_empty() {
    let tree = GraphTree::<u64>::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);
    assert_eq!(tree.member_count(), 0);
    assert!(tree.active().is_none());
    assert_eq!(tree.layout_mode(), LayoutMode::TreeStyleTabs);
}

#[test]
fn attach_traversal_creates_child() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::Attach {
        member: 2,
        provenance: Provenance::Traversal {
            source: 1,
            edge_kind: None,
        },
    });

    assert_eq!(tree.member_count(), 2);
    assert_eq!(tree.parent_of(&2), Some(&1));
    assert_eq!(tree.children_of(&1), &[2]);
}

#[test]
fn attach_manual_creates_sibling() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::Attach {
        member: 2,
        provenance: Provenance::Traversal {
            source: 1,
            edge_kind: None,
        },
    });
    tree.apply(NavAction::Attach {
        member: 3,
        provenance: Provenance::Manual {
            source: Some(2),
            context: None,
        },
    });

    // 3 should be sibling of 2 (child of 1)
    assert_eq!(tree.parent_of(&3), Some(&1));
}

#[test]
fn activate_and_dismiss() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });

    let result = tree.apply(NavAction::Activate(1));
    assert!(result.session_changed);
    assert_eq!(tree.active(), Some(&1));
    assert!(tree.get(&1).unwrap().is_active());

    let result = tree.apply(NavAction::Dismiss(1));
    assert!(result.session_changed);
    assert!(tree.active().is_none());
    assert!(tree.get(&1).unwrap().is_cold());
}

#[test]
fn toggle_expand() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });

    assert!(!tree.is_expanded(&1));
    tree.apply(NavAction::ToggleExpand(1));
    assert!(tree.is_expanded(&1));
    tree.apply(NavAction::ToggleExpand(1));
    assert!(!tree.is_expanded(&1));
}

#[test]
fn reveal_expands_ancestors() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::Attach {
        member: 2,
        provenance: Provenance::Traversal {
            source: 1,
            edge_kind: None,
        },
    });
    tree.apply(NavAction::Attach {
        member: 3,
        provenance: Provenance::Traversal {
            source: 2,
            edge_kind: None,
        },
    });

    assert!(!tree.is_expanded(&1));
    assert!(!tree.is_expanded(&2));

    tree.apply(NavAction::Reveal(3));

    assert!(tree.is_expanded(&1));
    assert!(tree.is_expanded(&2));
}

#[test]
fn detach_recursive() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::Attach {
        member: 2,
        provenance: Provenance::Traversal {
            source: 1,
            edge_kind: None,
        },
    });
    tree.apply(NavAction::Attach {
        member: 3,
        provenance: Provenance::Traversal {
            source: 2,
            edge_kind: None,
        },
    });

    let result = tree.apply(NavAction::Detach {
        member: 2,
        recursive: true,
    });
    assert!(result.structure_changed);
    assert_eq!(tree.member_count(), 1);
    assert!(!tree.contains(&2));
    assert!(!tree.contains(&3));
}

#[test]
fn detach_non_recursive_reparents_children() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::Attach {
        member: 2,
        provenance: Provenance::Traversal {
            source: 1,
            edge_kind: None,
        },
    });
    tree.apply(NavAction::Attach {
        member: 3,
        provenance: Provenance::Traversal {
            source: 2,
            edge_kind: None,
        },
    });

    tree.apply(NavAction::Detach {
        member: 2,
        recursive: false,
    });

    assert_eq!(tree.member_count(), 2);
    assert!(!tree.contains(&2));
    // 3 should have been reparented to 1
    assert!(tree.contains(&3));
}

#[test]
fn set_lens_emits_intent() {
    let mut tree = GraphTree::<u64>::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    let result = tree.apply(NavAction::SetLens(ProjectionLens::Containment));
    assert!(result.session_changed);
    assert_eq!(tree.active_lens(), &ProjectionLens::Containment);
    assert!(
        result
            .intents
            .iter()
            .any(|i| matches!(i, TreeIntent::LensChanged(ProjectionLens::Containment)))
    );
}

#[test]
fn cycle_focus_wraps() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::Attach {
        member: 2,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::ToggleExpand(1));
    tree.apply(NavAction::ToggleExpand(2));

    // Select first
    tree.apply(NavAction::Select(1));
    assert_eq!(tree.active(), Some(&1));

    // Cycle next
    tree.apply(NavAction::CycleFocus(FocusDirection::Next));
    assert_eq!(tree.active(), Some(&2));

    // Cycle next wraps to first
    tree.apply(NavAction::CycleFocus(FocusDirection::Next));
    assert_eq!(tree.active(), Some(&1));
}

#[test]
fn graphlet_membership() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::Attach {
        member: 2,
        provenance: Provenance::Traversal {
            source: 1,
            edge_kind: None,
        },
    });

    let graphlet = GraphletRef::new_session(0).with_kind(GraphletKind::Session);
    tree.add_graphlet(graphlet);

    tree.get_mut(&1).unwrap().graphlet_membership.push(0);
    tree.get_mut(&2).unwrap().graphlet_membership.push(0);

    let members = tree.graphlet_members(0);
    assert_eq!(members.len(), 2);
    assert!(tree.graphlet_of(&1).is_some());
}

#[test]
fn serialization_roundtrip() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);
    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::Attach {
        member: 2,
        provenance: Provenance::Traversal {
            source: 1,
            edge_kind: None,
        },
    });
    tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));

    let json = serde_json::to_string(&tree).expect("serialize");
    let restored: GraphTree<u64> = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(restored.member_count(), 2);
    assert_eq!(restored.parent_of(&2), Some(&1));
    assert!(restored.get(&1).unwrap().is_active());
}

#[test]
fn reparent_rejects_cycle_via_nav() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::Attach {
        member: 2,
        provenance: Provenance::Traversal {
            source: 1,
            edge_kind: None,
        },
    });
    tree.apply(NavAction::Attach {
        member: 3,
        provenance: Provenance::Traversal {
            source: 2,
            edge_kind: None,
        },
    });

    // Try to reparent 1 under 3 — would create 1→2→3→1 cycle
    let result = tree.apply(NavAction::Reparent {
        member: 1,
        new_parent: 3,
    });
    assert!(!result.structure_changed);

    // Tree should be unchanged
    assert_eq!(tree.topology().roots(), &[1]);
    assert_eq!(tree.parent_of(&2), Some(&1));
    assert_eq!(tree.parent_of(&3), Some(&2));
    tree.topology().assert_invariants();
}

#[test]
fn duplicate_attach_is_noop() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    let result = tree.apply(NavAction::Attach {
        member: 1,
        provenance: Provenance::Anchor,
    });

    // Should be a no-op
    assert!(!result.structure_changed);
    assert_eq!(tree.member_count(), 1);
    tree.topology().assert_invariants();
}

#[test]
fn actions_on_nonexistent_member_are_noop() {
    let mut tree = GraphTree::<u64>::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    // All of these should be no-ops on an empty tree
    let r = tree.apply(NavAction::Select(99));
    assert!(!r.session_changed);
    let r = tree.apply(NavAction::Activate(99));
    assert!(!r.session_changed);
    let r = tree.apply(NavAction::Dismiss(99));
    assert!(!r.session_changed);
    let r = tree.apply(NavAction::Reveal(99));
    assert!(!r.session_changed);
    let r = tree.apply(NavAction::Detach {
        member: 99,
        recursive: true,
    });
    assert!(!r.structure_changed);
    let r = tree.apply(NavAction::Reparent {
        member: 99,
        new_parent: 100,
    });
    assert!(!r.structure_changed);
}

// --- Layout tests ---

