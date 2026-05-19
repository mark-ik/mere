/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Tests for `frame`. Split out of `lib.rs` to keep the parent
//! module under the workspace's 600-LOC ceiling.

use accesskit::{Node, Role};
use uxtree::{UxTree, node_id_for_path};

use crate::*;

fn fixture_three_pane_frame() -> FrameLayout {
    // Layout:
    //   ┌──────────┬─────────┐
    //   │          │ orrery  │
    //   │ workbench├─────────┤
    //   │          │apparatus│
    //   └──────────┴─────────┘
    let g = GraphId::from_uuid(uuid::Uuid::from_u128(0xc01));
    FrameLayout {
        id: FrameId::new("reading"),
        label: "Reading".to_string(),
        root: PaneNode::Split {
            axis: SplitAxis::Horizontal,
            ratio: 0.6,
            first: Box::new(PaneNode::Leaf {
                pane_id: PaneId(1),
                content: PaneContent::Workbench,
                graph_id: g,
            }),
            second: Box::new(PaneNode::Split {
                axis: SplitAxis::Vertical,
                ratio: 0.5,
                first: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(2),
                    content: PaneContent::Orrery,
                    graph_id: g,
                }),
                second: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(3),
                    content: PaneContent::Apparatus,
                    graph_id: g,
                }),
            }),
        },
    }
}

#[test]
fn root_carries_frame_label() {
    let layout = fixture_three_pane_frame();
    let tree = project_frame(&layout);
    let (_, root) = tree.nodes.iter().find(|(id, _)| *id == tree.root).unwrap();
    assert_eq!(root.role(), Role::Group);
    assert_eq!(root.label(), Some("Reading"));
}

#[test]
fn each_leaf_emits_a_pane_node_with_content_tag_label() {
    let layout = fixture_three_pane_frame();
    let tree = project_frame(&layout);
    let labels: Vec<_> = tree
        .nodes
        .iter()
        .filter_map(|(_, n)| n.label().map(|s| s.to_string()))
        .collect();
    assert!(labels.contains(&"workbench".to_string()));
    assert!(labels.contains(&"orrery".to_string()));
    assert!(labels.contains(&"apparatus".to_string()));
}

#[test]
fn split_nodes_carry_axis_and_ratio_in_description() {
    let layout = fixture_three_pane_frame();
    let tree = project_frame(&layout);
    let split_descriptions: Vec<_> = tree
        .nodes
        .iter()
        .filter_map(|(_, n)| n.description().map(|s| s.to_string()))
        .collect();
    assert!(
        split_descriptions
            .iter()
            .any(|d| d.contains("Horizontal") && d.contains("0.60")),
        "expected horizontal split with ratio 0.60, got {split_descriptions:?}"
    );
    assert!(
        split_descriptions
            .iter()
            .any(|d| d.contains("Vertical") && d.contains("0.50"))
    );
}

#[test]
fn ids_are_deterministic_across_runs() {
    let layout = fixture_three_pane_frame();
    let a = project_frame(&layout);
    let b = project_frame(&layout);
    assert_eq!(a.root, b.root);
    let a_ids: Vec<_> = a.nodes.iter().map(|(id, _)| *id).collect();
    let b_ids: Vec<_> = b.nodes.iter().map(|(id, _)| *id).collect();
    assert_eq!(a_ids, b_ids);
}

#[test]
fn project_frame_with_attaches_subtree_to_matching_leaf() {
    let layout = fixture_three_pane_frame();

    let tree = project_frame_with(&layout, |content, _| match content {
        PaneContent::Workbench => {
            // Build a fresh subtree on each call so the closure can be
            // FnMut without needing Clone on UxTree.
            let mut sub_root = Node::new(Role::Group);
            sub_root.set_label("workbench-content");
            Some(UxTree {
                root: node_id_for_path("workbench-content-fixture"),
                nodes: vec![(node_id_for_path("workbench-content-fixture"), sub_root)],
            })
        }
        _ => None,
    });

    assert!(
        tree.nodes
            .iter()
            .any(|(_, n)| n.label() == Some("workbench-content")),
        "expected attached workbench subtree to merge into frame"
    );
}

#[test]
fn set_split_ratio_clamps_and_finds_path() {
    let mut layout = fixture_three_pane_frame();
    // Root split is horizontal, ratio 0.6
    assert_eq!(layout.split_at(&[]), Some((SplitAxis::Horizontal, 0.6)));
    assert!(layout.set_split_ratio(&[], 0.3));
    assert_eq!(layout.split_at(&[]), Some((SplitAxis::Horizontal, 0.3)));

    // Inner vertical split is at root.second
    assert_eq!(
        layout.split_at(&[SplitChoice::Second]),
        Some((SplitAxis::Vertical, 0.5))
    );
    assert!(layout.set_split_ratio(&[SplitChoice::Second], 0.75));
    assert_eq!(
        layout.split_at(&[SplitChoice::Second]),
        Some((SplitAxis::Vertical, 0.75))
    );

    // Out-of-range ratios clamp.
    assert!(layout.set_split_ratio(&[], 5.0));
    assert_eq!(layout.split_at(&[]), Some((SplitAxis::Horizontal, 0.95)));
    assert!(layout.set_split_ratio(&[], -1.0));
    assert_eq!(layout.split_at(&[]), Some((SplitAxis::Horizontal, 0.05)));

    // Path into a leaf returns None / no-op.
    assert!(!layout.set_split_ratio(&[SplitChoice::First], 0.5));
    assert_eq!(layout.split_at(&[SplitChoice::First]), None);
}

#[test]
fn frame_layout_round_trips_through_serde() {
    let layout = fixture_three_pane_frame();
    let json = serde_json::to_string(&layout).expect("serialize");
    let restored: FrameLayout = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(layout, restored);
}

#[test]
fn reparent_leaf_moves_leaf_without_losing_pane_id() {
    let mut layout = fixture_three_pane_frame();
    // Fixture has 3 leaves: workbench (pane 1), orrery (pane 2),
    // apparatus (pane 3). Move pane 3 to be on the left of pane 1.
    // Paths: workbench at [First]; apparatus at [Second, Second].
    let apparatus_path = vec![SplitChoice::Second, SplitChoice::Second];
    let workbench_path = vec![SplitChoice::First];
    assert!(layout.reparent_leaf(&apparatus_path, &workbench_path, InsertSide::Left));

    // After: apparatus should be present somewhere in the tree.
    let apparatus_after =
        crate::layout::path_for_pane_id(&layout.root, PaneId(3)).expect("apparatus still present");
    let workbench_after =
        crate::layout::path_for_pane_id(&layout.root, PaneId(1)).expect("workbench still present");
    let orrery_after =
        crate::layout::path_for_pane_id(&layout.root, PaneId(2)).expect("orrery still present");
    // All three leaves present, no duplication.
    let leaves: Vec<_> = layout.iter_leaves().collect();
    assert_eq!(leaves.len(), 3);
    // Apparatus + workbench now share a parent (the new split
    // wraps them), distinct from the orrery's parent.
    assert_ne!(apparatus_after, workbench_after);
    // Both should have one shared prefix step less than their
    // depths (siblings under a common split).
    assert_eq!(apparatus_after.len(), workbench_after.len());
    assert!(orrery_after != apparatus_after && orrery_after != workbench_after);
}

#[test]
fn reparent_leaf_refuses_self_move() {
    let mut layout = fixture_three_pane_frame();
    let workbench_path = vec![SplitChoice::First];
    assert!(!layout.reparent_leaf(&workbench_path, &workbench_path, InsertSide::Right,));
}

#[test]
fn reparent_leaf_refuses_when_target_is_not_a_leaf() {
    let mut layout = fixture_three_pane_frame();
    // Empty path = root, which is a Split, not a Leaf.
    let workbench_path = vec![SplitChoice::First];
    assert!(!layout.reparent_leaf(&workbench_path, &[], InsertSide::Right));
}
