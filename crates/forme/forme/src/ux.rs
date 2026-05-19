// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! UxTree emission for accessibility and automation.
//!
//! Follows `ux_tree_and_probe_spec.md`: every visible pane produces at
//! least one `UxNodeDescriptor` when ux-semantics is active. The host
//! bridges these to AccessKit (egui), platform a11y (mobile), or DOM
//! aria attributes (extension/PWA).

use crate::MemberId;
use crate::layout::LayoutMode;
use crate::member::Lifecycle;
use crate::tree::GraphTree;

/// A node in the UxTree — structural description for accessibility/automation.
#[derive(Clone, Debug)]
pub struct UxNodeDescriptor<N: MemberId> {
    pub ux_node_id: String,
    pub role: UxRole,
    pub label: String,
    pub state: UxState,
    pub member: Option<N>,
    pub depth: usize,
    pub children: Vec<UxNodeDescriptor<N>>,
}

/// Semantic role for accessibility.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UxRole {
    /// The sidebar tree itself.
    TreeView,
    /// A member row in the tree.
    TreeItem,
    /// Flat tab bar container.
    TabList,
    /// Individual tab.
    Tab,
    /// Split pane container.
    SplitContainer,
    /// Content pane.
    Pane,
}

/// Accessibility state flags.
#[derive(Clone, Debug)]
pub struct UxState {
    pub focused: bool,
    pub selected: bool,
    /// None if not expandable (leaf node).
    pub expanded: Option<bool>,
    pub lifecycle: Lifecycle,
}

/// Build the UxNode tree from a GraphTree. Pure function.
///
/// The structure depends on layout mode:
/// - TreeStyleTabs → TreeView with TreeItem children
/// - FlatTabs → TabList with Tab children
/// - SplitPanes → SplitContainer with Pane children
///
/// In all modes, the tree sidebar is emitted as a TreeView.
pub fn emit_ux_tree<N: MemberId>(tree: &GraphTree<N>) -> UxNodeDescriptor<N> {
    let active = tree.active().cloned();

    match tree.layout_mode() {
        LayoutMode::TreeStyleTabs => emit_tree_view(tree, &active),
        LayoutMode::FlatTabs => emit_tab_list(tree, &active),
        LayoutMode::SplitPanes => emit_split_container(tree, &active),
    }
}

fn emit_tree_view<N: MemberId>(tree: &GraphTree<N>, active: &Option<N>) -> UxNodeDescriptor<N> {
    let rows = tree.visible_rows();
    let children: Vec<UxNodeDescriptor<N>> = rows
        .iter()
        .map(|row| {
            let member = row.member.clone();
            let is_active = active.as_ref() == Some(&member);
            let lifecycle = tree
                .get(&member)
                .map(|e| e.lifecycle)
                .unwrap_or(Lifecycle::Cold);

            UxNodeDescriptor {
                ux_node_id: format!("tree-item-{:?}", member),
                role: UxRole::TreeItem,
                label: format!("{:?}", member),
                state: UxState {
                    focused: is_active,
                    selected: is_active,
                    expanded: if row.has_children {
                        Some(row.is_expanded)
                    } else {
                        None
                    },
                    lifecycle,
                },
                member: Some(member),
                depth: row.depth,
                children: Vec::new(),
            }
        })
        .collect();

    UxNodeDescriptor {
        ux_node_id: "tree-view-root".to_string(),
        role: UxRole::TreeView,
        label: "Graph Tree".to_string(),
        state: UxState {
            focused: false,
            selected: false,
            expanded: Some(true),
            lifecycle: Lifecycle::Active,
        },
        member: None,
        depth: 0,
        children,
    }
}

fn emit_tab_list<N: MemberId>(tree: &GraphTree<N>, active: &Option<N>) -> UxNodeDescriptor<N> {
    // Use insertion order for stable, deterministic tab ordering
    let children: Vec<UxNodeDescriptor<N>> = tree
        .topology()
        .insertion_order()
        .iter()
        .filter_map(|id| {
            let entry = tree.get(id)?;
            if !entry.is_visible_in_pane() {
                return None;
            }
            let is_active = active.as_ref() == Some(id);
            Some(UxNodeDescriptor {
                ux_node_id: format!("tab-{:?}", id),
                role: UxRole::Tab,
                label: format!("{:?}", id),
                state: UxState {
                    focused: is_active,
                    selected: is_active,
                    expanded: None,
                    lifecycle: entry.lifecycle,
                },
                member: Some(id.clone()),
                depth: 0,
                children: Vec::new(),
            })
        })
        .collect();

    UxNodeDescriptor {
        ux_node_id: "tab-list-root".to_string(),
        role: UxRole::TabList,
        label: "Tabs".to_string(),
        state: UxState {
            focused: false,
            selected: false,
            expanded: Some(true),
            lifecycle: Lifecycle::Active,
        },
        member: None,
        depth: 0,
        children,
    }
}

fn emit_split_container<N: MemberId>(
    tree: &GraphTree<N>,
    active: &Option<N>,
) -> UxNodeDescriptor<N> {
    // Use insertion order for stable, deterministic pane ordering
    let children: Vec<UxNodeDescriptor<N>> = tree
        .topology()
        .insertion_order()
        .iter()
        .filter_map(|id| {
            let entry = tree.get(id)?;
            if !entry.is_visible_in_pane() {
                return None;
            }
            let is_active = active.as_ref() == Some(id);
            Some(UxNodeDescriptor {
                ux_node_id: format!("pane-{:?}", id),
                role: UxRole::Pane,
                label: format!("{:?}", id),
                state: UxState {
                    focused: is_active,
                    selected: is_active,
                    expanded: None,
                    lifecycle: entry.lifecycle,
                },
                member: Some(id.clone()),
                depth: 0,
                children: Vec::new(),
            })
        })
        .collect();

    UxNodeDescriptor {
        ux_node_id: "split-container-root".to_string(),
        role: UxRole::SplitContainer,
        label: "Panes".to_string(),
        state: UxState {
            focused: false,
            selected: false,
            expanded: Some(true),
            lifecycle: Lifecycle::Active,
        },
        member: None,
        depth: 0,
        children,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::LayoutMode;
    use crate::lens::ProjectionLens;
    use crate::member::Provenance;
    use crate::nav::NavAction;
    use std::collections::HashSet;

    fn make_ux_tree(mode: LayoutMode) -> GraphTree<u64> {
        let mut tree = GraphTree::new(mode, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 1,
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
        tree.apply(NavAction::SetLifecycle(2, Lifecycle::Warm));
        // 3 remains Cold
        tree.apply(NavAction::Activate(1));
        tree.apply(NavAction::ToggleExpand(1));
        tree
    }

    /// Collect all UxNode IDs recursively.
    fn collect_ids<N: MemberId>(node: &UxNodeDescriptor<N>, ids: &mut Vec<String>) {
        ids.push(node.ux_node_id.clone());
        for child in &node.children {
            collect_ids(child, ids);
        }
    }

    /// Collect all member-bearing UxNodes recursively.
    fn collect_members<N: MemberId + Clone>(node: &UxNodeDescriptor<N>, members: &mut Vec<N>) {
        if let Some(ref m) = node.member {
            members.push(m.clone());
        }
        for child in &node.children {
            collect_members(child, members);
        }
    }

    #[test]
    fn ux_tree_emits_tree_view() {
        let tree = make_ux_tree(LayoutMode::TreeStyleTabs);
        let ux = emit_ux_tree(&tree);

        assert_eq!(ux.role, UxRole::TreeView);
        // Visible rows: root(1) expanded → children 2, 3 visible
        assert_eq!(ux.children.len(), 3);
        assert!(ux.children.iter().all(|c| c.role == UxRole::TreeItem));
    }

    #[test]
    fn ux_tree_emits_tab_list() {
        let tree = make_ux_tree(LayoutMode::FlatTabs);
        let ux = emit_ux_tree(&tree);

        assert_eq!(ux.role, UxRole::TabList);
        // Only visible-in-pane members: 1 (Active) + 2 (Warm). 3 is Cold.
        assert_eq!(ux.children.len(), 2);
        assert!(ux.children.iter().all(|c| c.role == UxRole::Tab));
    }

    #[test]
    fn ux_tree_emits_split_container() {
        let tree = make_ux_tree(LayoutMode::SplitPanes);
        let ux = emit_ux_tree(&tree);

        assert_eq!(ux.role, UxRole::SplitContainer);
        // Only visible-in-pane: 1 (Active) + 2 (Warm)
        assert_eq!(ux.children.len(), 2);
        assert!(ux.children.iter().all(|c| c.role == UxRole::Pane));
    }

    #[test]
    fn cold_members_excluded_from_tab_list() {
        let tree = make_ux_tree(LayoutMode::FlatTabs);
        let ux = emit_ux_tree(&tree);

        let mut members = Vec::new();
        collect_members(&ux, &mut members);
        // Member 3 is Cold — should not appear
        assert!(!members.contains(&3));
    }

    #[test]
    fn cold_members_excluded_from_split_panes() {
        let tree = make_ux_tree(LayoutMode::SplitPanes);
        let ux = emit_ux_tree(&tree);

        let mut members = Vec::new();
        collect_members(&ux, &mut members);
        assert!(!members.contains(&3));
    }

    #[test]
    fn cold_members_visible_in_tree_view() {
        // TreeView shows ALL visible rows, including cold members
        // (they appear in the sidebar tree even if not in a pane)
        let tree = make_ux_tree(LayoutMode::TreeStyleTabs);
        let ux = emit_ux_tree(&tree);

        let mut members = Vec::new();
        collect_members(&ux, &mut members);
        // All 3 members visible in tree sidebar (root expanded)
        assert!(members.contains(&1));
        assert!(members.contains(&2));
        assert!(members.contains(&3));
    }

    #[test]
    fn completeness_every_visible_pane_has_ux_node() {
        // Completeness contract: every visible-in-pane member produces a UxNode
        for mode in [LayoutMode::FlatTabs, LayoutMode::SplitPanes] {
            let tree = make_ux_tree(mode);
            let ux = emit_ux_tree(&tree);

            let mut ux_members = Vec::new();
            collect_members(&ux, &mut ux_members);

            for (id, entry) in tree.members() {
                if entry.is_visible_in_pane() {
                    assert!(
                        ux_members.contains(id),
                        "visible member {:?} missing from UxTree in {:?} mode",
                        id,
                        mode
                    );
                }
            }
        }
    }

    #[test]
    fn ux_node_ids_are_unique() {
        for mode in [
            LayoutMode::TreeStyleTabs,
            LayoutMode::FlatTabs,
            LayoutMode::SplitPanes,
        ] {
            let tree = make_ux_tree(mode);
            let ux = emit_ux_tree(&tree);

            let mut ids = Vec::new();
            collect_ids(&ux, &mut ids);
            let unique: HashSet<&String> = ids.iter().collect();
            assert_eq!(
                unique.len(),
                ids.len(),
                "duplicate UxNode IDs in {:?} mode: {:?}",
                mode,
                ids
            );
        }
    }

    #[test]
    fn no_empty_labels_on_interactive_nodes() {
        for mode in [
            LayoutMode::TreeStyleTabs,
            LayoutMode::FlatTabs,
            LayoutMode::SplitPanes,
        ] {
            let tree = make_ux_tree(mode);
            let ux = emit_ux_tree(&tree);

            fn check_labels<N: MemberId>(node: &UxNodeDescriptor<N>) {
                if node.member.is_some() {
                    assert!(
                        !node.label.is_empty(),
                        "interactive UxNode {:?} has empty label",
                        node.ux_node_id
                    );
                }
                for child in &node.children {
                    check_labels(child);
                }
            }

            check_labels(&ux);
        }
    }

    #[test]
    fn focused_state_matches_active_member() {
        let tree = make_ux_tree(LayoutMode::TreeStyleTabs);
        let ux = emit_ux_tree(&tree);

        // Member 1 is active — should be focused
        let m1 = ux.children.iter().find(|c| c.member == Some(1)).unwrap();
        assert!(m1.state.focused);
        assert!(m1.state.selected);

        // Member 2 is warm — should not be focused
        let m2 = ux.children.iter().find(|c| c.member == Some(2)).unwrap();
        assert!(!m2.state.focused);
    }

    #[test]
    fn expanded_state_on_parent_nodes() {
        let tree = make_ux_tree(LayoutMode::TreeStyleTabs);
        let ux = emit_ux_tree(&tree);

        // Member 1 has children and is expanded
        let m1 = ux.children.iter().find(|c| c.member == Some(1)).unwrap();
        assert_eq!(m1.state.expanded, Some(true));

        // Members 2 and 3 are leaves — expanded should be None
        let m2 = ux.children.iter().find(|c| c.member == Some(2)).unwrap();
        assert_eq!(m2.state.expanded, None);
    }

    #[test]
    fn empty_tree_produces_valid_ux() {
        for mode in [
            LayoutMode::TreeStyleTabs,
            LayoutMode::FlatTabs,
            LayoutMode::SplitPanes,
        ] {
            let tree = GraphTree::<u64>::new(mode, ProjectionLens::Traversal);
            let ux = emit_ux_tree(&tree);
            // Root container exists, no children
            assert!(ux.children.is_empty());
            assert!(!ux.ux_node_id.is_empty());
        }
    }
}
