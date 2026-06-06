// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;

#[test]
fn two_roots_no_children_flat_horizontal_split() {
    // Two independent roots → flat horizontal row.
    let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::Attach {
        member: 2,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
    tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));

    let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_eq!(result.pane_rects.len(), 2);

    let r1 = result.pane_rects.get(&1).unwrap();
    let r2 = result.pane_rects.get(&2).unwrap();
    // Horizontal row: each gets ~400 width, full 600 height.
    assert!((r1.w - 400.0).abs() < 1.0, "expected ~400, got {}", r1.w);
    assert!((r2.w - 400.0).abs() < 1.0, "expected ~400, got {}", r2.w);
    assert!((r1.h - 600.0).abs() < 1.0);
    assert!((r2.h - 600.0).abs() < 1.0);
    // Non-overlapping: r1 ends where r2 starts.
    assert!((r1.x + r1.w - r2.x).abs() < 1.0);
}

#[test]
fn root_with_children_nested_split() {
    // Root A with children [B, C] → A gets pane, B and C get nested sub-panes.
    let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
    tree.apply(NavAction::Attach {
        member: 10u64,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::Attach {
        member: 20,
        provenance: Provenance::Traversal {
            source: 10,
            edge_kind: None,
        },
    });
    tree.apply(NavAction::Attach {
        member: 30,
        provenance: Provenance::Traversal {
            source: 10,
            edge_kind: None,
        },
    });
    tree.apply(NavAction::SetLifecycle(10, Lifecycle::Active));
    tree.apply(NavAction::SetLifecycle(20, Lifecycle::Active));
    tree.apply(NavAction::SetLifecycle(30, Lifecycle::Active));

    let result = tree.compute_layout(Rect::new(0.0, 0.0, 900.0, 600.0));
    assert_eq!(result.pane_rects.len(), 3);

    // Topology: root(H) → subtree_container(V) → [leaf(10), leaf(20), leaf(30)]
    // All three share vertical space, each gets full width.
    for (_, r) in &result.pane_rects {
        assert!(
            (r.w - 900.0).abs() < 1.0,
            "nested children should get full width, got {}",
            r.w
        );
    }
    let total_h: f32 = result.pane_rects.values().map(|r| r.h).sum();
    assert!(
        (total_h - 600.0).abs() < 1.0,
        "total height should be ~600, got {}",
        total_h
    );
}

#[test]
fn three_levels_deep_nesting() {
    // Root → child → grandchild. Direction alternates H→V→H.
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
    tree.apply(NavAction::Attach {
        member: 3,
        provenance: Provenance::Traversal {
            source: 2,
            edge_kind: None,
        },
    });
    tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
    tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));
    tree.apply(NavAction::SetLifecycle(3, Lifecycle::Active));

    let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_eq!(result.pane_rects.len(), 3);

    let r1 = result.pane_rects.get(&1).unwrap();
    let r2 = result.pane_rects.get(&2).unwrap();
    let r3 = result.pane_rects.get(&3).unwrap();

    // Level 0: root(H) → subtree_1(V direction, since toggle from H)
    //   subtree_1: [leaf(1), subtree_2(H direction, toggle from V)]
    //     subtree_2: [leaf(2), leaf(3)]
    //
    // So 1 and container_2 split V (height). Within container_2, 2 and 3 split H (width).

    // Member 1 gets full width, ~half height.
    assert!((r1.w - 800.0).abs() < 1.0, "r1 full width, got {}", r1.w);
    assert!(
        r1.h > 100.0 && r1.h < 500.0,
        "r1 partial height, got {}",
        r1.h
    );

    // Members 2 and 3 each get ~half width, same height as each other.
    assert!((r2.h - r3.h).abs() < 1.0, "r2 and r3 same height");
    assert!(
        r2.w > 100.0 && r2.w < 700.0,
        "r2 partial width, got {}",
        r2.w
    );
    assert!(
        r3.w > 100.0 && r3.w < 700.0,
        "r3 partial width, got {}",
        r3.w
    );
    assert!(
        (r2.w + r3.w - 800.0).abs() < 1.0,
        "r2+r3 widths should sum to 800, got {}",
        r2.w + r3.w
    );
}

#[test]
fn split_ratio_respected() {
    // Two roots, first has split_ratio 0.7 → gets ~70% of width.
    let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::Attach {
        member: 2,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
    tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));

    tree.apply(NavAction::SetLayoutOverride(
        1,
        LayoutOverride {
            min_width: None,
            min_height: None,
            flex_grow: None,
            flex_shrink: None,
            preferred_split: None,
            split_ratio: Some(0.7),
        },
    ));
    tree.apply(NavAction::SetLayoutOverride(
        2,
        LayoutOverride {
            min_width: None,
            min_height: None,
            flex_grow: None,
            flex_shrink: None,
            preferred_split: None,
            split_ratio: Some(0.3),
        },
    ));

    let result = tree.compute_layout(Rect::new(0.0, 0.0, 1000.0, 600.0));
    let r1 = result.pane_rects.get(&1).unwrap();
    let r2 = result.pane_rects.get(&2).unwrap();

    // r1 should be ~700, r2 ~300.
    assert!((r1.w - 700.0).abs() < 20.0, "expected ~700, got {}", r1.w);
    assert!((r2.w - 300.0).abs() < 20.0, "expected ~300, got {}", r2.w);
}

#[test]
fn direction_alternation() {
    // Root H → children V → grandchildren H.
    let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::Attach {
        member: 2,
        provenance: Provenance::Anchor,
    });
    // Make member 1 a parent with child 3
    tree.apply(NavAction::Attach {
        member: 3,
        provenance: Provenance::Traversal {
            source: 1,
            edge_kind: None,
        },
    });
    tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
    tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));
    tree.apply(NavAction::SetLifecycle(3, Lifecycle::Active));

    let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_eq!(result.pane_rects.len(), 3);

    let r1 = result.pane_rects.get(&1).unwrap();
    let r2 = result.pane_rects.get(&2).unwrap();
    let r3 = result.pane_rects.get(&3).unwrap();

    // Root container is H → member 1's subtree and member 2 split horizontally.
    // Member 2 (leaf root) gets ~half width, full height.
    assert!((r2.h - 600.0).abs() < 1.0, "r2 full height, got {}", r2.h);
    assert!(
        r2.w > 100.0 && r2.w < 700.0,
        "r2 partial width, got {}",
        r2.w
    );

    // Member 1's subtree is V (alternated from H) → leaf(1) and leaf(3) split vertically.
    assert!(
        (r1.h + r3.h - 600.0).abs() < 1.0,
        "r1+r3 heights should sum to 600, got {}",
        r1.h + r3.h
    );
    // r1 and r3 have the same width (the subtree container's width).
    assert!(
        (r1.w - r3.w).abs() < 1.0,
        "r1 and r3 should have same width, got {} vs {}",
        r1.w,
        r3.w
    );
}

#[test]
fn preferred_split_overrides_alternation() {
    // Root(H) → child normally gets V, but preferred_split=Horizontal overrides.
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

    // Override: member 1's children split H instead of the default V.
    tree.apply(NavAction::SetLayoutOverride(
        1,
        LayoutOverride {
            min_width: None,
            min_height: None,
            flex_grow: None,
            flex_shrink: None,
            preferred_split: Some(SplitDirection::Horizontal),
            split_ratio: None,
        },
    ));

    let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
    let r1 = result.pane_rects.get(&1).unwrap();
    let r2 = result.pane_rects.get(&2).unwrap();

    // With H override, both leaf(1) and leaf(2) split horizontally (row).
    // Both should have full height, each ~half width.
    assert!((r1.h - 600.0).abs() < 1.0, "r1 full height, got {}", r1.h);
    assert!((r2.h - 600.0).abs() < 1.0, "r2 full height, got {}", r2.h);
    assert!(
        (r1.w + r2.w - 800.0).abs() < 1.0,
        "widths should sum to 800, got {}",
        r1.w + r2.w
    );
}

#[test]
fn split_boundaries_between_roots() {
    // Two roots → one horizontal split boundary between them.
    let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::Attach {
        member: 2,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
    tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));

    let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_eq!(result.split_boundaries.len(), 1);

    let boundary = &result.split_boundaries[0];
    assert_eq!(boundary.direction, SplitDirection::Horizontal);
    // Boundary should be at ~400 (midpoint between the two panes).
    assert!(
        (boundary.axis_position - 400.0).abs() < 2.0,
        "expected boundary at ~400, got {}",
        boundary.axis_position
    );
    // Cross-axis spans full height.
    assert!((boundary.cross_start - 0.0).abs() < 1.0);
    assert!((boundary.cross_end - 600.0).abs() < 1.0);
    assert!((boundary.container_extent - 800.0).abs() < 1.0);
}

#[test]
fn split_boundaries_nested() {
    // Root with two children → boundaries at both levels.
    // root(H) → subtree(V) → [leaf(1), leaf(2), leaf(3)]
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
    tree.apply(NavAction::Attach {
        member: 3,
        provenance: Provenance::Traversal {
            source: 1,
            edge_kind: None,
        },
    });
    tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
    tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));
    tree.apply(NavAction::SetLifecycle(3, Lifecycle::Active));

    let result = tree.compute_layout(Rect::new(0.0, 0.0, 900.0, 600.0));
    // Subtree container(V) has 3 children [leaf(1), leaf(2), leaf(3)].
    // That gives 2 boundaries between consecutive pairs.
    assert_eq!(
        result.split_boundaries.len(),
        2,
        "expected 2 boundaries, got {}: {:?}",
        result.split_boundaries.len(),
        result.split_boundaries
    );

    for boundary in &result.split_boundaries {
        assert_eq!(
            boundary.direction,
            SplitDirection::Vertical,
            "nested container should split vertically"
        );
    }
}

#[test]
fn single_pane_no_boundaries() {
    let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
    tree.apply(NavAction::Attach {
        member: 1u64,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));

    let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
    assert!(result.split_boundaries.is_empty());
}
