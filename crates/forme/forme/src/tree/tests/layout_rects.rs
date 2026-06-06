// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use super::make_layout_tree;

#[test]
fn tree_style_tabs_single_pane() {
    let tree = make_layout_tree(LayoutMode::TreeStyleTabs);
    let rect = Rect::new(0.0, 0.0, 800.0, 600.0);
    let result = tree.compute_layout(rect);

    // Active member gets the full rect
    assert_eq!(result.pane_rects.len(), 1);
    let pane = result.pane_rects.get(&1).expect("active member rect");
    assert_eq!(pane.w, 800.0);
    assert_eq!(pane.h, 600.0);

    // Tree rows populated (sidebar)
    assert!(!result.tree_rows.is_empty());
}

#[test]
fn flat_tabs_single_pane() {
    let tree = make_layout_tree(LayoutMode::FlatTabs);
    let rect = Rect::new(0.0, 0.0, 800.0, 600.0);
    let result = tree.compute_layout(rect);

    // Active member gets the full rect
    assert_eq!(result.pane_rects.len(), 1);
    assert!(result.pane_rects.contains_key(&1));

    // Tab order includes visible members (Active + Warm)
    assert_eq!(result.tab_order.len(), 3);
}

#[test]
fn flat_tabs_no_active_empty_rects() {
    let mut tree = GraphTree::new(LayoutMode::FlatTabs, ProjectionLens::Traversal);
    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    // Member is Cold (default), no active set
    let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
    assert!(result.pane_rects.is_empty());
}

#[test]
fn split_panes_divides_space() {
    let tree = make_layout_tree(LayoutMode::SplitPanes);
    let rect = Rect::new(0.0, 0.0, 900.0, 600.0);
    let result = tree.compute_layout(rect);

    // 3 visible members (1=Active, 2=Active, 3=Warm) get rects.
    // Topology: 1 is root with children [2, 3].
    // Layout: root(H) → container(V for member 1's subtree) → [leaf(1), leaf(2), leaf(3)]
    // All 3 share the vertical space; each gets full width.
    assert_eq!(result.pane_rects.len(), 3);

    // All rects are within the available area
    for (_, r) in &result.pane_rects {
        assert!(r.x >= 0.0);
        assert!(r.y >= 0.0);
        assert!(r.x + r.w <= 900.1); // small epsilon for float
        assert!(r.y + r.h <= 600.1);
        assert!(r.w > 0.0);
        assert!(r.h > 0.0);
    }

    // With nested layout, member 1's subtree is a V container.
    // All 3 leaves get full width, split height equally.
    let total_height: f32 = result.pane_rects.values().map(|r| r.h).sum();
    assert!(
        (total_height - 600.0).abs() < 1.0,
        "expected total height ~600, got {}",
        total_height
    );
    for (_, r) in &result.pane_rects {
        assert!(
            (r.w - 900.0).abs() < 1.0,
            "expected full width ~900, got {}",
            r.w
        );
    }
}

#[test]
fn split_panes_non_overlapping() {
    let tree = make_layout_tree(LayoutMode::SplitPanes);
    let result = tree.compute_layout(Rect::new(0.0, 0.0, 900.0, 600.0));

    let rects: Vec<&Rect> = result.pane_rects.values().collect();
    for i in 0..rects.len() {
        for j in (i + 1)..rects.len() {
            let a = rects[i];
            let b = rects[j];
            // No overlap: one must be fully left, right, above, or below the other
            let no_overlap = a.x + a.w <= b.x + 0.1
                || b.x + b.w <= a.x + 0.1
                || a.y + a.h <= b.y + 0.1
                || b.y + b.h <= a.y + 0.1;
            assert!(no_overlap, "panes overlap: {:?} and {:?}", a, b);
        }
    }
}

#[test]
fn split_panes_respects_min_width() {
    let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
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
    tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));

    // Set min_width on member 1
    tree.get_mut(&1).unwrap().layout_override = Some(crate::member::LayoutOverride {
        min_width: Some(400.0),
        min_height: None,
        flex_grow: Some(1.0),
        flex_shrink: Some(0.0), // don't shrink below min
        preferred_split: None,
        split_ratio: None,
    });

    let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
    let r1 = result.pane_rects.get(&1).expect("member 1 rect");
    assert!(r1.w >= 399.0, "min_width not respected: {}", r1.w);
}

#[test]
fn split_panes_vertical_direction() {
    let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
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
    tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));

    // Set vertical split
    tree.get_mut(&1).unwrap().layout_override = Some(crate::member::LayoutOverride {
        min_width: None,
        min_height: None,
        flex_grow: None,
        flex_shrink: None,
        preferred_split: Some(crate::member::SplitDirection::Vertical),
        split_ratio: None,
    });

    let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));

    // Vertical split: both panes should have full width, split height
    let r1 = result.pane_rects.get(&1).expect("member 1 rect");
    let r2 = result.pane_rects.get(&2).expect("member 2 rect");
    assert!(
        (r1.w - 800.0).abs() < 1.0,
        "expected full width, got {}",
        r1.w
    );
    assert!(
        (r2.w - 800.0).abs() < 1.0,
        "expected full width, got {}",
        r2.w
    );
    assert!((r1.h + r2.h - 600.0).abs() < 1.0);
}

#[test]
fn layout_tab_order_stable() {
    let tree = make_layout_tree(LayoutMode::FlatTabs);
    let result1 = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
    let result2 = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));

    let ids1: Vec<u64> = result1.tab_order.iter().map(|t| t.member).collect();
    let ids2: Vec<u64> = result2.tab_order.iter().map(|t| t.member).collect();
    assert_eq!(ids1, ids2, "tab order should be stable across calls");
}

#[test]
fn tree_rows_respect_expansion() {
    let mut tree = make_layout_tree(LayoutMode::TreeStyleTabs);
    // Initially root is expanded, children visible
    let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_eq!(result.tree_rows.len(), 3); // root + 2 children

    // Collapse root
    tree.apply(NavAction::ToggleExpand(1));
    let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_eq!(result.tree_rows.len(), 1); // just root
}

// --- Orphan prevention tests (Phase A correctness hardening) ---

#[test]
fn traversal_attach_with_missing_source_falls_back_to_root() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    // Attach member 2 with traversal from source 99 which doesn't exist
    tree.apply(NavAction::Attach {
        member: 2u64,
        provenance: Provenance::Traversal {
            source: 99,
            edge_kind: None,
        },
    });

    // Member should still be attached — as a root, not orphaned
    assert!(tree.contains(&2));
    assert!(tree.topology().roots().contains(&2));
    assert!(tree.topology().parent_of(&2).is_none());
    tree.topology().assert_invariants();
}

#[test]
fn manual_attach_with_missing_source_falls_back_to_root() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Manual {
            source: Some(99), // doesn't exist
            context: None,
        },
    });

    assert!(tree.contains(&1));
    assert!(tree.topology().roots().contains(&1));
    tree.topology().assert_invariants();
}

#[test]
fn derived_attach_with_missing_connection_falls_back_to_root() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    tree.apply(NavAction::Attach {
        member: 5u64,
        provenance: Provenance::Derived {
            connection: Some(42), // doesn't exist
            derivation: "test".to_string(),
        },
    });

    assert!(tree.contains(&5));
    assert!(tree.topology().roots().contains(&5));
    tree.topology().assert_invariants();
}

#[test]
fn all_members_reachable_after_mixed_attaches() {
    let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

    // Valid chain
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

    // Attach with missing source — should become root
    tree.apply(NavAction::Attach {
        member: 3,
        provenance: Provenance::Traversal {
            source: 99,
            edge_kind: None,
        },
    });

    // Every member must appear in visible_rows when all are expanded
    tree.apply(NavAction::ToggleExpand(1));
    tree.apply(NavAction::ToggleExpand(2));
    tree.apply(NavAction::ToggleExpand(3));

    let rows = tree.visible_rows();
    let row_ids: Vec<u64> = rows.iter().map(|r| r.member).collect();
    assert!(row_ids.contains(&1));
    assert!(row_ids.contains(&2));
    assert!(row_ids.contains(&3));

    tree.topology().assert_invariants();
}

// --- Nested layout tests (Phase E/G) ---

