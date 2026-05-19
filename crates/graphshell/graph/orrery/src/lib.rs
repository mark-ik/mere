/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # mere-orrery
//!
//! Orrery domain layer — projects [`kernel::graph::Graph`] into a
//! subtree of AccessKit nodes for `uxtree`. Each graph node becomes a
//! `Role::Link` accessibility / automation entry; edges are deferred.
//!
//! See the crate README for the role / id scheme.

#![doc(html_root_url = "https://docs.rs/mere-orrery/0.0.1")]

use accesskit::{Node, Role};
use kernel::graph::Graph;
use uxtree::{UxTree, node_id_for_path};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

/// Project a [`Graph`] into a subtree of AccessKit nodes rooted at an
/// orrery group node.
pub fn project_graph(graph: &Graph) -> UxTree {
    let mut nodes = Vec::new();
    let root_path = "orrery".to_string();
    let root_id = node_id_for_path(&root_path);

    let mut child_ids = Vec::new();
    for (_key, node) in graph.nodes() {
        let node_path = format!("{root_path}/node/{}", node.id);
        let acc_id = node_id_for_path(&node_path);

        let mut acc_node = Node::new(Role::Link);
        let label = if node.title.is_empty() {
            node.primary_address().as_url_str().to_string()
        } else {
            node.title.clone()
        };
        acc_node.set_label(label);
        acc_node.set_value(node.primary_address().as_url_str().to_string());

        nodes.push((acc_id, acc_node));
        child_ids.push(acc_id);
    }

    let mut root = Node::new(Role::Group);
    root.set_label("Graph");
    root.set_children(child_ids);
    nodes.push((root_id, root));

    tracing::debug!(
        node_count = nodes.len() - 1,
        "projected Graph into uxtree subtree"
    );

    UxTree {
        root: root_id,
        nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::default::Point2D;
    use kernel::graph::Graph;

    fn fixture_graph() -> Graph {
        let mut g = Graph::new();
        g.add_node_with_id(
            uuid::Uuid::from_u128(0x1111),
            "https://a.test/".to_string(),
            Point2D::new(0.0, 0.0),
        );
        g.add_node_with_id(
            uuid::Uuid::from_u128(0x2222),
            "https://b.test/".to_string(),
            Point2D::new(10.0, 10.0),
        );
        g
    }

    #[test]
    fn root_is_orrery_group_with_label_graph() {
        let g = fixture_graph();
        let tree = project_graph(&g);
        let (_, root) = tree.nodes.iter().find(|(id, _)| *id == tree.root).unwrap();
        assert_eq!(root.role(), Role::Group);
        assert_eq!(root.label(), Some("Graph"));
    }

    #[test]
    fn each_graph_node_projects_as_role_link() {
        let g = fixture_graph();
        let tree = project_graph(&g);
        let link_count = tree
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == Role::Link)
            .count();
        assert_eq!(link_count, 2);
    }

    #[test]
    fn link_value_is_address_url() {
        let g = fixture_graph();
        let tree = project_graph(&g);
        let link_values: Vec<_> = tree
            .nodes
            .iter()
            .filter_map(|(_, n)| {
                if n.role() == Role::Link {
                    n.value().map(|v| v.to_string())
                } else {
                    None
                }
            })
            .collect();
        assert!(link_values.contains(&"https://a.test/".to_string()));
        assert!(link_values.contains(&"https://b.test/".to_string()));
    }

    #[test]
    fn ids_are_deterministic_across_runs() {
        let g = fixture_graph();
        let a = project_graph(&g);
        let b = project_graph(&g);
        assert_eq!(a.root, b.root);
        let a_ids: Vec<_> = a.nodes.iter().map(|(id, _)| *id).collect();
        let b_ids: Vec<_> = b.nodes.iter().map(|(id, _)| *id).collect();
        assert_eq!(a_ids, b_ids);
    }

    #[test]
    fn empty_graph_emits_root_only() {
        let g = Graph::new();
        let tree = project_graph(&g);
        assert_eq!(tree.nodes.len(), 1);
    }
}
