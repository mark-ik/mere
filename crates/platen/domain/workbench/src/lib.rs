/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # workbench
//!
//! Workbench domain layer — projects [`platen::WorkbenchProjection`]
//! into a subtree of AccessKit nodes for `uxtree`.
//!
//! See the crate README for the role / id scheme. The host pairs this
//! subtree with a sibling `mere-orrery` subtree for graph content,
//! `gloss` for peripheral strips, etc., merging them under a single
//! application-root node.

#![doc(html_root_url = "https://docs.rs/workbench/0.0.1")]

use accesskit::{Node, Role};
use platen::{ProjectedPane, WorkbenchProjection};
use uxtree::{UxTree, node_id_for_path};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

/// Project a [`WorkbenchProjection`] into a subtree of AccessKit nodes.
///
/// Returns an [`UxTree`] whose `root` is the workbench node; the host
/// stitches this under its application root (or a window root) by
/// adding `tree.root` to the parent's `children`.
pub fn project_workbench(projection: &WorkbenchProjection) -> UxTree {
    let mut nodes = Vec::new();
    let root_path = "workbench".to_string();
    let root_id = node_id_for_path(&root_path);

    let mut root_children = Vec::new();

    if let Some(frame_id) = &projection.active_frame {
        let frame_path = format!("{root_path}/frame/{frame_id:?}");
        let frame_id_node = node_id_for_path(&frame_path);

        let mut pane_ids = Vec::with_capacity(projection.panes.len());
        for pane in &projection.panes {
            let pane_path = format!("{frame_path}/pane/{:?}", pane.pane_id);
            let pane_node_id = node_id_for_path(&pane_path);
            nodes.push((pane_node_id, build_pane_node(pane)));
            pane_ids.push(pane_node_id);
        }

        let mut frame_node = Node::new(Role::Group);
        let frame_label = projection
            .frame_label
            .as_deref()
            .unwrap_or("Frame")
            .to_string();
        frame_node.set_label(frame_label);
        frame_node.set_children(pane_ids);
        nodes.push((frame_id_node, frame_node));
        root_children.push(frame_id_node);
    }

    let mut root = Node::new(Role::Group);
    let root_label = projection
        .frame_label
        .as_deref()
        .unwrap_or("Workbench")
        .to_string();
    root.set_label(root_label);
    root.set_children(root_children);
    nodes.push((root_id, root));

    tracing::debug!(
        active_frame = ?projection.active_frame,
        pane_count = projection.panes.len(),
        node_count = nodes.len(),
        "projected WorkbenchProjection into uxtree subtree"
    );

    UxTree {
        root: root_id,
        nodes,
    }
}

fn build_pane_node(pane: &ProjectedPane) -> Node {
    let mut node = Node::new(Role::Group);
    node.set_label(format!("Pane {:?}", pane.pane_id));
    if pane.is_primary {
        node.set_description("primary");
    }
    if let Some(host) = &pane.surface_host {
        // Host is a portable opaque ID; expose as the node value so
        // automation / inspector tooling can match panes to their
        // surface backing without poking at internals.
        node.set_value(format!("{host:?}"));
    }
    node
}

#[cfg(test)]
mod tests {
    use super::*;
    use platen::{FrameId, PaneBinding, ProjectedPane, WorkbenchProjection};
    // Fixtures need lower-level types: PaneId / NodeKey are kernel
    // types that flow through platen; SurfaceHostId is from verso-core.
    use kernel::graph::NodeKey;
    use kernel::pane::PaneId;
    use verso_core::surface::SurfaceHostId;

    fn fixture_pane(seed: u128, primary: bool) -> ProjectedPane {
        // Deterministic UUIDs so test ids are stable across runs without
        // pulling in OS randomness in tests.
        let uuid = uuid::Uuid::from_u128(seed);
        ProjectedPane {
            pane_id: PaneId::from_uuid(uuid),
            node: NodeKey::default(),
            surface_host: None,
            is_primary: primary,
        }
    }

    fn fixture_projection() -> WorkbenchProjection {
        WorkbenchProjection {
            active_frame: Some(FrameId::new("frame-a")),
            frame_label: Some("Reading".to_string()),
            root_view: None,
            panes: vec![fixture_pane(0x1111, true), fixture_pane(0x2222, false)],
        }
    }

    #[test]
    fn root_uses_frame_label_when_present() {
        let projection = fixture_projection();
        let tree = project_workbench(&projection);
        let (_, root) = tree.nodes.iter().find(|(id, _)| *id == tree.root).unwrap();
        assert_eq!(root.role(), Role::Group);
        assert_eq!(root.label(), Some("Reading"));
    }

    #[test]
    fn root_falls_back_to_workbench_label_when_no_frame_label() {
        let mut projection = fixture_projection();
        projection.frame_label = None;
        let tree = project_workbench(&projection);
        let (_, root) = tree.nodes.iter().find(|(id, _)| *id == tree.root).unwrap();
        assert_eq!(root.label(), Some("Workbench"));
    }

    #[test]
    fn frame_node_carries_each_pane_as_child() {
        let projection = fixture_projection();
        let tree = project_workbench(&projection);
        let frame_node = tree
            .nodes
            .iter()
            .map(|(_, n)| n)
            .find(|n| n.label() == Some("Reading") && n.children().len() == 2)
            .expect("frame node with two pane children");
        assert_eq!(frame_node.children().len(), 2);
    }

    #[test]
    fn primary_pane_gets_description() {
        let projection = fixture_projection();
        let tree = project_workbench(&projection);
        let primary_count = tree
            .nodes
            .iter()
            .map(|(_, n)| n)
            .filter(|n| n.description() == Some("primary"))
            .count();
        assert_eq!(primary_count, 1);
    }

    #[test]
    fn ids_are_deterministic_across_runs() {
        let projection = fixture_projection();
        let a = project_workbench(&projection);
        let b = project_workbench(&projection);
        assert_eq!(a.root, b.root);
        assert_eq!(
            a.nodes.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            b.nodes.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn no_active_frame_emits_root_only() {
        let projection = WorkbenchProjection::default();
        let tree = project_workbench(&projection);
        assert_eq!(tree.nodes.len(), 1);
        let (_, root) = &tree.nodes[0];
        assert_eq!(root.children().len(), 0);
    }

    #[test]
    fn surface_host_emitted_as_node_value_when_present() {
        let host = SurfaceHostId::new("gpui:surface:1");
        let mut projection = fixture_projection();
        projection.panes[0].surface_host = Some(host);

        let tree = project_workbench(&projection);
        let pane_with_value = tree
            .nodes
            .iter()
            .map(|(_, n)| n)
            .find(|n| n.value().is_some())
            .expect("expected one pane node to carry a surface_host value");
        assert!(pane_with_value.value().unwrap().contains("gpui:surface:1"));
    }

    // Suppress an unused import; ensures the binding still compiles even
    // if PaneBinding becomes useful in a future fixture.
    #[allow(dead_code)]
    fn _unused_pane_binding(_b: PaneBinding) {}
}
