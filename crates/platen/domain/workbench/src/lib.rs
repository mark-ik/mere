// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # workbench
//!
//! Workbench domain layer — projects platen's [`Workbench`](platen::Workbench)
//! tiling model into a subtree of AccessKit nodes for `uxtree`.
//!
//! See the crate README for the role / id scheme. The host pairs this subtree with
//! a sibling orrery a11y subtree for graph content (host-side `orrery_a11y_tree` now),
//! `gloss` for peripheral strips,
//! etc., merging them under a single application-root node.
//!
//! v1 projects **structure** only: a workbench root, a `Group` per slot, a `Tab`
//! node per member (the active tab marked). Richer labels (resolved titles / URLs
//! from the graph) and fuller tab semantics are a follow-on, tracked in the
//! 2026-06-04 platen taffy-retarget plan. (It previously projected the legacy
//! `WorkbenchProjection` frame model, now replaced by the forme tiling model.)

#![doc(html_root_url = "https://docs.rs/workbench/0.0.1")]

use accesskit::{Node, Role};
use platen::Workbench;
use uxtree::{UxTree, node_id_for_path};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

/// Project a [`Workbench`] into a subtree of AccessKit nodes.
///
/// Returns an [`UxTree`] whose `root` is the workbench node; the host stitches it
/// under its application root (or a window root) by adding `tree.root` to the
/// parent's `children`. Each slot becomes a `Group`; each member within a slot
/// becomes a `Tab` child (the active one described "active").
pub fn project_workbench(workbench: &Workbench) -> UxTree {
    let mut nodes = Vec::new();
    let root_path = "workbench".to_string();
    let root_id = node_id_for_path(&root_path);

    let mut root_children = Vec::new();
    for (slot_index, slot) in workbench.slot_views().enumerate() {
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
    root.set_label("Workbench");
    root.set_children(root_children);
    nodes.push((root_id, root));

    tracing::debug!(
        slot_count = workbench.slot_count(),
        tile_count = workbench.tile_count(),
        node_count = nodes.len(),
        "projected workbench into uxtree subtree"
    );

    UxTree {
        root: root_id,
        nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn m(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    /// An empty workbench projects a root node only.
    #[test]
    fn empty_workbench_emits_root_only() {
        let tree = project_workbench(&Workbench::new());
        assert_eq!(tree.nodes.len(), 1);
        let (_, root) = &tree.nodes[0];
        assert_eq!(root.role(), Role::Group);
        assert_eq!(root.label(), Some("Workbench"));
        assert_eq!(root.children().len(), 0);
    }

    /// Two single-tile slots project the root + a Group per slot + a Tab per slot.
    #[test]
    fn split_slots_project_a_group_and_tab_each() {
        let mut wb = Workbench::new();
        wb.open_tile(m(1));
        wb.open_tile(m(2));
        let tree = project_workbench(&wb);
        let groups = tree
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == Role::Group)
            .count();
        let tabs = tree
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == Role::Tab)
            .count();
        assert_eq!(groups, 3, "the workbench root + one per slot");
        assert_eq!(tabs, 2, "one tab per single-tile slot");
    }

    /// A tab-stack projects one Group whose children are the stacked tabs, with the
    /// active tab marked.
    #[test]
    fn stack_marks_the_active_tab() {
        let mut wb = Workbench::new();
        wb.open_tile(m(1));
        wb.open_tile(m(2));
        wb.open_tile(m(3));
        wb.stack_all(); // one stack of three; first active
        let tree = project_workbench(&wb);
        let tabs: Vec<_> = tree
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == Role::Tab)
            .collect();
        assert_eq!(tabs.len(), 3, "three stacked tabs");
        let active = tabs
            .iter()
            .filter(|(_, n)| n.description() == Some("active"))
            .count();
        assert_eq!(active, 1, "exactly the active tab is marked");
    }

    /// Ids are deterministic across runs (no OS randomness).
    #[test]
    fn ids_are_deterministic_across_runs() {
        let mut wb = Workbench::new();
        wb.open_tile(m(1));
        let a = project_workbench(&wb);
        let b = project_workbench(&wb);
        assert_eq!(a.root, b.root);
        assert_eq!(
            a.nodes.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            b.nodes.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        );
    }
}
