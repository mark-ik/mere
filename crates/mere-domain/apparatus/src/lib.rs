/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # mere-apparatus
//!
//! Apparatus domain layer — peripheral system-inspector strip. v0
//! emits a placeholder skeleton subtree; each section grows real
//! content as the corresponding host bridge lands.

#![doc(html_root_url = "https://docs.rs/mere-apparatus/0.0.1")]

use accesskit::{Node, Role};
use uxtree::{UxTree, node_id_for_path};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

/// Inspector lanes that the apparatus panel will eventually populate.
/// v0 emits each as an empty group; real content lands per lane.
const SECTIONS: &[&str] = &[
    "tracing events",
    "register-diagnostics channels",
    "uxtree",
    "accesskit",
];

/// Emit the v0 apparatus skeleton: a root labeled "Apparatus" with one
/// empty group per inspector lane.
pub fn project_skeleton() -> UxTree {
    let mut nodes = Vec::new();
    let root_path = "apparatus".to_string();
    let root_id = node_id_for_path(&root_path);

    let mut child_ids = Vec::with_capacity(SECTIONS.len());
    for &section in SECTIONS {
        let path = format!("{root_path}/section/{section}");
        let id = node_id_for_path(&path);
        let mut n = Node::new(Role::Group);
        n.set_label(section.to_string());
        nodes.push((id, n));
        child_ids.push(id);
    }

    let mut root = Node::new(Role::Group);
    root.set_label("Apparatus");
    root.set_children(child_ids);
    nodes.push((root_id, root));

    tracing::debug!(
        section_count = SECTIONS.len(),
        "projected apparatus skeleton"
    );

    UxTree {
        root: root_id,
        nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_apparatus_group() {
        let tree = project_skeleton();
        let (_, root) = tree.nodes.iter().find(|(id, _)| *id == tree.root).unwrap();
        assert_eq!(root.role(), Role::Group);
        assert_eq!(root.label(), Some("Apparatus"));
    }

    #[test]
    fn skeleton_includes_each_section() {
        let tree = project_skeleton();
        let labels: Vec<_> = tree
            .nodes
            .iter()
            .filter_map(|(_, n)| n.label().map(|s| s.to_string()))
            .collect();
        for section in SECTIONS {
            assert!(
                labels.contains(&section.to_string()),
                "missing apparatus section {section}"
            );
        }
    }

    #[test]
    fn ids_are_deterministic_across_runs() {
        let a = project_skeleton();
        let b = project_skeleton();
        assert_eq!(a.root, b.root);
        let a_ids: Vec<_> = a.nodes.iter().map(|(id, _)| *id).collect();
        let b_ids: Vec<_> = b.nodes.iter().map(|(id, _)| *id).collect();
        assert_eq!(a_ids, b_ids);
    }
}
