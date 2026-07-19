// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Workbench layout tests.

use uuid::Uuid;

use super::*;

fn m(n: u128) -> GraphMemberId {
    Uuid::from_u128(n)
}

#[test]
fn new_workbench_is_cartography_and_empty() {
    let wb = Workbench::new();
    assert_eq!(wb.mode(), ProjectionKind::Cartography);
    assert!(!wb.is_tiled());
    assert_eq!(wb.tile_count(), 0);
    assert_eq!(wb.slot_count(), 0);
}

#[test]
fn open_tile_appends_single_slots_and_dedups() {
    let mut wb = Workbench::new();
    assert!(wb.open_tile(m(1)), "first open is new");
    assert!(wb.open_tile(m(2)), "a distinct member is new");
    assert!(!wb.open_tile(m(1)), "re-opening an open member is a no-op");
    assert_eq!(wb.open_members(), vec![m(1), m(2)]);
    assert_eq!(wb.slot_count(), 2, "two single-tile columns");
    assert_eq!(wb.tile_count(), 2);
}

#[test]
fn stack_all_then_split_all_separates() {
    let mut wb = Workbench::new();
    wb.open_tile(m(1));
    wb.open_tile(m(2));
    wb.open_tile(m(3));
    wb.stack_all();
    assert_eq!(wb.slot_count(), 1, "all three collapse into one stack");
    assert_eq!(wb.tile_count(), 3, "all three tabs are still open");
    wb.split_all();
    assert_eq!(wb.slot_count(), 3, "each tab back to its own column");
}

#[test]
fn activate_switches_the_visible_tab_in_a_stack() {
    let mut wb = Workbench::new();
    wb.open_tile(m(1));
    wb.open_tile(m(2));
    wb.open_tile(m(3));
    wb.stack_all();
    {
        let view = wb.slot_views().next().unwrap();
        assert_eq!(view.members, &[m(1), m(2), m(3)], "three stacked tabs");
        assert_eq!(view.active, 0, "first tab active by default");
    }
    assert!(
        wb.activate(m(3)),
        "activating a member in the stack succeeds"
    );
    assert_eq!(
        wb.slot_views().next().unwrap().active,
        2,
        "the active tab switched"
    );
}

#[test]
fn open_split_opens_each_as_its_own_slot_and_dedups() {
    let mut wb = Workbench::new();
    assert_eq!(wb.open_split(&[m(1), m(2), m(3)]), 3, "all three are new");
    assert_eq!(wb.slot_count(), 3, "one column per member");
    assert_eq!(
        wb.open_split(&[m(2), m(4)]),
        1,
        "only the unseen member opens"
    );
    assert_eq!(wb.slot_count(), 4);
}

#[test]
fn open_stack_gathers_into_one_slot_and_rehomes_open_members() {
    let mut wb = Workbench::new();
    wb.open_split(&[m(1), m(2)]); // two single columns
    wb.open_stack(&[m(2), m(3)]); // m(2) is pulled out of its column into the stack
    assert_eq!(wb.slot_count(), 2, "m(1)'s column + the new [2,3] stack");
    assert_eq!(wb.tile_count(), 3, "no member is lost or duplicated");
    assert!(wb.activate(m(3)));
    assert!(wb.has_tile(m(2)) && wb.has_tile(m(3)));
    let mut wb2 = Workbench::new();
    wb2.open_stack(&[m(7), m(7), m(8)]);
    assert_eq!(wb2.slot_count(), 1);
    assert_eq!(wb2.tile_count(), 2, "the repeat collapses");
}

#[test]
fn ensure_tiled_switches_once() {
    let mut wb = Workbench::new();
    assert!(wb.ensure_tiled(), "first call flips Cartography → Tree");
    assert!(wb.is_tiled());
    assert!(!wb.ensure_tiled(), "already tiled → no change");
}

#[test]
fn close_tab_drops_it_and_removes_an_empty_slot() {
    let mut wb = Workbench::new();
    wb.open_tile(m(1));
    wb.open_tile(m(2));
    wb.stack_all();
    assert!(wb.close_tile(m(1)), "closing a tab in the stack");
    assert_eq!(wb.tile_count(), 1, "the other tab remains");
    assert_eq!(wb.slot_count(), 1, "the cell survives with one tab");
    assert!(wb.close_tile(m(2)), "closing the last tab");
    assert_eq!(wb.slot_count(), 0, "the emptied cell is removed");
}

#[test]
fn move_to_slot_of_moves_across_and_reorders_within() {
    let mut wb = Workbench::new();
    wb.open_split(&[m(1), m(2), m(3)]);
    assert!(wb.move_to_slot_of(m(1), m(3)));
    assert_eq!(wb.slot_count(), 2, "m(1)'s column emptied + dropped");
    assert_eq!(wb.tile_count(), 3, "no tab lost");
    let stack = wb.slot_views().find(|s| s.members.contains(&m(1))).unwrap();
    assert_eq!(stack.members, &[m(3), m(1)], "m(1) appended to m(3)'s cell");
    assert_eq!(
        stack.members[stack.active],
        m(1),
        "the dragged tab is active"
    );
    assert!(wb.move_to_slot_of(m(3), m(1)));
    let stack = wb.slot_views().find(|s| s.members.contains(&m(3))).unwrap();
    assert_eq!(stack.members, &[m(1), m(3)], "m(3) reordered to after m(1)");
    assert!(!wb.move_to_slot_of(m(1), m(1)));
    assert!(!wb.move_to_slot_of(m(99), m(1)));
}

#[test]
fn open_in_slot_of_stacks_a_new_tab_and_activates_it() {
    let mut wb = Workbench::new();
    wb.open_split(&[m(1), m(2)]);
    assert!(wb.open_in_slot_of(m(3), m(1)));
    assert_eq!(wb.slot_count(), 2, "no new column — it joined m(1)'s");
    let stack = wb.slot_views().find(|s| s.members.contains(&m(3))).unwrap();
    assert_eq!(stack.members, &[m(1), m(3)], "appended to m(1)'s cell");
    assert_eq!(stack.members[stack.active], m(3), "the new tab is active");
    assert!(wb.open_in_slot_of(m(2), m(1)));
    assert_eq!(wb.tile_count(), 3, "m(2) moved, not copied");
    assert_eq!(wb.slot_count(), 1, "m(2)'s old column emptied + dropped");
    assert!(!wb.open_in_slot_of(m(9), m(404)));
}

#[test]
fn split_beside_pulls_a_tab_into_its_own_slot() {
    let mut wb = Workbench::new();
    wb.open_tile(m(1));
    wb.open_tile(m(2));
    wb.stack_all(); // one stack [1, 2]
    assert_eq!(wb.slot_count(), 1);
    assert!(wb.split_beside(m(2), m(1), true));
    assert_eq!(wb.slot_count(), 2, "m(2) is its own column now");
    assert_eq!(wb.tile_count(), 2);
    let members: Vec<_> = wb.slot_views().map(|s| s.members.to_vec()).collect();
    assert_eq!(members, vec![vec![m(1)], vec![m(2)]]);
    assert!(wb.split_beside(m(1), m(2), false));
    let members: Vec<_> = wb.slot_views().map(|s| s.members.to_vec()).collect();
    assert_eq!(
        members,
        vec![vec![m(1)], vec![m(2)]],
        "m(1) inserted before m(2)"
    );
    assert!(!wb.split_beside(m(1), m(1), true), "self is a no-op");
}

#[test]
fn split_beside_axis_makes_a_vertical_split() {
    let mut wb = Workbench::new();
    wb.open_split(&[m(1), m(2)]); // [1 | 2] (top-level Row)
    // Split m(2) below m(1) along Column: m(1)'s column nests a Column [1 / 2].
    assert!(wb.split_beside_axis(m(2), m(1), SplitAxis::Column, true));
    assert_eq!(wb.slot_count(), 2, "two leaves: 1 and 2");
    assert_eq!(wb.tile_count(), 2);
    // The top level is still a Row of one child (collapsed), nesting the Column.
    let tree = wb.to_tile_tree(actor_tile_for).expect("tree");
    // A single top-level child collapses, so the root is the Column split itself.
    match tree {
        pelt_core::tile::TileTree::Split { axis, children } => {
            assert_eq!(axis, SplitAxis::Column, "a vertical split");
            assert_eq!(children.len(), 2);
        }
        other => panic!("expected a column split, got {other:?}"),
    }
}

#[test]
fn split_out_pulls_a_tab_out_of_its_own_stack() {
    let mut wb = Workbench::new();
    wb.open_tile(m(1));
    wb.open_tile(m(2));
    wb.stack_all(); // one stack [1, 2]
    assert_eq!(wb.slot_count(), 1);
    assert!(
        wb.split_out(m(1), SplitAxis::Row, true),
        "m(1) splits out beside the stack"
    );
    assert_eq!(wb.slot_count(), 2);
    assert_eq!(wb.tile_count(), 2, "no tab lost");
    // A tab alone in its cell has no sibling to anchor on — a no-op.
    let mut wb2 = Workbench::new();
    wb2.open_tile(m(5));
    assert!(
        !wb2.split_out(m(5), SplitAxis::Column, false),
        "alone → no-op"
    );
}

#[test]
fn weights_are_fractions_default_equal_and_set_renormalizes() {
    let mut wb = Workbench::new();
    wb.open_split(&[m(1), m(2)]);
    assert_eq!(wb.weights(), vec![0.5, 0.5], "equal fractions by default");
    wb.set_weights(&[3.0, 1.0]);
    let w = wb.weights();
    assert!(
        (w[0] - 0.75).abs() < 1e-5 && (w[1] - 0.25).abs() < 1e-5,
        "renormalized to sum 1"
    );
    wb.set_weights(&[-1.0, 0.0]);
    assert_eq!(
        wb.weights(),
        vec![0.5, 0.5],
        "clamped + renormalized so neither collapses"
    );
}

#[test]
fn nested_split_fractions_addressed_by_path() {
    let mut wb = Workbench::new();
    wb.open_split(&[m(1), m(2), m(3)]); // [1 | 2 | 3]
    wb.split_beside_axis(m(3), m(2), SplitAxis::Column, true); // [1 | (2 / 3)]
    // The top-level row has two children; child 1 is the nested column.
    assert_eq!(wb.split_fractions(&[]).unwrap().len(), 2);
    assert_eq!(
        wb.split_fractions(&[1]).unwrap().len(),
        2,
        "the nested column"
    );
    assert!(wb.set_split_fractions(&[1], &[0.7, 0.3]));
    let nested = wb.split_fractions(&[1]).unwrap();
    assert!((nested[0] - 0.7).abs() < 1e-5 && (nested[1] - 0.3).abs() < 1e-5);
}

// ── Workbench -> pelt TileTree projection (the V6 surface builder) ──

use pelt_core::tile::{ContentSource, SplitAxis as Axis, TextureKey, Tile, TileId, TileTree};

/// A host resolver standing in for meerkat's: a member maps to its actor-texture
/// lane, keyed (id + texture) by the member's low bits.
fn actor_tile_for(member: GraphMemberId) -> Tile {
    let key = member.as_u128() as u64;
    Tile {
        id: TileId(key),
        title: String::new(),
        content: ContentSource::ExternalTexture(TextureKey(key)),
        accent: None,
    }
}

#[test]
fn to_tile_tree_empty_is_none() {
    assert!(Workbench::new().to_tile_tree(actor_tile_for).is_none());
}

#[test]
fn to_tile_tree_single_slot_is_a_stack() {
    let mut wb = Workbench::new();
    wb.open_tile(m(1));
    let tree = wb.to_tile_tree(actor_tile_for).expect("one slot");
    assert!(
        matches!(tree, TileTree::Stack(_)),
        "a lone slot is the stack itself"
    );
    assert_eq!(tree.tiles().len(), 1);
}

#[test]
fn to_tile_tree_stacked_slot_keeps_active() {
    let mut wb = Workbench::new();
    wb.open_tile(m(1));
    wb.open_tile(m(2));
    wb.open_tile(m(3));
    wb.stack_all();
    wb.activate(m(3));
    let tree = wb.to_tile_tree(actor_tile_for).expect("tree");
    match tree {
        TileTree::Stack(s) => {
            assert_eq!(s.tabs.len(), 3, "all tabs carried");
            assert_eq!(s.active, 2, "the active tab is preserved");
        }
        other => panic!("expected a stack, got {other:?}"),
    }
}

#[test]
fn to_tile_tree_slots_become_a_weighted_row_split() {
    let mut wb = Workbench::new();
    wb.open_split(&[m(1), m(2)]);
    wb.set_weights(&[3.0, 1.0]);
    let tree = wb.to_tile_tree(actor_tile_for).expect("tree");
    match tree {
        TileTree::Split { axis, children } => {
            assert_eq!(axis, Axis::Row, "slots lay side by side");
            assert_eq!(children.len(), 2);
            assert!(
                (children[0].fraction - 0.75).abs() < 1e-6,
                "got {}",
                children[0].fraction
            );
            assert!(
                (children[1].fraction - 0.25).abs() < 1e-6,
                "got {}",
                children[1].fraction
            );
        }
        other => panic!("expected a row split, got {other:?}"),
    }
}
