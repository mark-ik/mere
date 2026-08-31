// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Accessible projections of Platen-owned layout structure.

use accesskit::{Node, Role};
use uxtree::{UxTree, node_id_for_path};

use crate::TileLayout;

/// Project a [`TileLayout`] into a subtree of AccessKit nodes.
///
/// The root is a generic tiled-layout group. A host stitches it under its application or window
/// root. Each slot becomes a `Group`; each member within a slot becomes a `Tab` child, with the
/// active tab described as active. This is a structural projection only: graph-resolved titles,
/// URLs, bounds, and actions remain host concerns.
pub fn project_tile_layout(layout: &TileLayout) -> UxTree {
    let mut nodes = Vec::new();
    let root_path = "tile-layout".to_string();
    let root_id = node_id_for_path(&root_path);

    let mut root_children = Vec::new();
    for (slot_index, slot) in layout.slot_views().enumerate() {
        let slot_path = format!("{root_path}/slot/{slot_index}");
        let slot_node_id = node_id_for_path(&slot_path);

        let mut tab_ids = Vec::with_capacity(slot.members.len());
        for (tab_index, member) in slot.members.iter().enumerate() {
            let tab_path = format!("{slot_path}/tab/{member}");
            let tab_node_id = node_id_for_path(&tab_path);
            let mut tab_node = Node::new(Role::Tab);
            tab_node.set_label(format!("Tile {member}"));
            if tab_index == slot.active {
                tab_node.set_description("active");
            }
            nodes.push((tab_node_id, tab_node));
            tab_ids.push(tab_node_id);
        }

        let mut slot_node = Node::new(Role::Group);
        slot_node.set_label(format!("Slot {slot_index}"));
        slot_node.set_children(tab_ids);
        nodes.push((slot_node_id, slot_node));
        root_children.push(slot_node_id);
    }

    let mut root = Node::new(Role::Group);
    root.set_label("Tile layout");
    root.set_children(root_children);
    nodes.push((root_id, root));

    tracing::debug!(
        slot_count = layout.slot_count(),
        tile_count = layout.tile_count(),
        node_count = nodes.len(),
        "projected tile layout into uxtree subtree"
    );

    UxTree {
        root: root_id,
        nodes,
    }
}

#[cfg(test)]
mod tests {
    use accesskit::Role;
    use uuid::Uuid;

    use super::project_tile_layout;
    use crate::TileLayout;

    fn m(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn empty_layout_emits_root_only() {
        let tree = project_tile_layout(&TileLayout::new());
        assert_eq!(tree.nodes.len(), 1);
        let (_, root) = &tree.nodes[0];
        assert_eq!(root.role(), Role::Group);
        assert_eq!(root.label(), Some("Tile layout"));
        assert!(root.children().is_empty());
    }

    #[test]
    fn split_slots_project_a_group_and_tab_each() {
        let mut layout = TileLayout::new();
        layout.open_tile(m(1));
        layout.open_tile(m(2));
        let tree = project_tile_layout(&layout);
        let groups = tree
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == Role::Group)
            .count();
        let tabs = tree
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == Role::Tab)
            .count();
        assert_eq!(groups, 3, "the root plus one group per slot");
        assert_eq!(tabs, 2, "one tab per single-tile slot");
    }

    #[test]
    fn stacked_layout_marks_exactly_one_active_tab() {
        let mut layout = TileLayout::new();
        layout.open_tile(m(1));
        layout.open_tile(m(2));
        layout.open_tile(m(3));
        layout.stack_all();
        let tree = project_tile_layout(&layout);
        let active = tree
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == Role::Tab)
            .filter(|(_, node)| node.description() == Some("active"))
            .count();
        assert_eq!(active, 1);
    }

    #[test]
    fn ids_are_deterministic() {
        let mut layout = TileLayout::new();
        layout.open_tile(m(1));
        let a = project_tile_layout(&layout);
        let b = project_tile_layout(&layout);
        assert_eq!(a.root, b.root);
        assert_eq!(
            a.nodes.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            b.nodes.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        );
    }
}
