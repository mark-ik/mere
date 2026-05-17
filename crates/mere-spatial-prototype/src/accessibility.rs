// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Substrate scene → `accesskit::TreeUpdate` projection.
//!
//! Per the spatial chrome IR brief §5, the substrate runs an
//! `accessibility-tree-emit` system that turns the spatial scene into
//! an AccessKit tree the host's OS adapter consumes (UIA on Windows,
//! AT-SPI on Linux, NSAccessibility on macOS).
//!
//! ## Scope (v0a)
//!
//! Builds a tree where the root is a Window node whose children are
//! all substrate nodes (Panel / Document / WebPage / etc.). Each node
//! contributes:
//!
//! - `NodeId` derived from `NodeIdentity::as_u64()` (substrate
//!   identities are `NonZeroU64`, so they never collide with the
//!   root's `NodeId(0)`).
//! - `Role` mapped from `NodeContentKind` (WebPage → WebView,
//!   DocumentTile → Document, Panel → Pane, Knot → Group,
//!   GraphView → Group, CustomCanvas → Canvas, Composite → Group,
//!   EdgeRendering → Group).
//! - `bounds`: axis-aligned bounding box of the placement-transformed
//!   tile rect, in scene-space pixels.
//!
//! ## What's not in v0a
//!
//! - Renderer-contributed sub-trees: `mere-masonry::MasonryTile` already
//!   produces its own `accesskit::TreeUpdate` via
//!   `take_accesskit_update`, but the registry trait doesn't expose
//!   that yet. Hosts wanting full content-level accessibility merge
//!   the renderer's update by hand for now; substrate-level merge is a
//!   follow-up that needs `EmbeddedFrameRenderer` to surface its tree.
//! - Edges: the IR brief leaves their AccessKit treatment open (Role::Link
//!   between two endpoints? a separate relation node?). v0a omits edges
//!   from the tree; they're still drawn by the paint pass + hit-testable
//!   via `SubstrateScene::hit_test_edge`.
//! - Focus tracking: the projection's `focus` field is set to the root
//!   for now; substrate-level focus management is its own piece.

use accesskit::{Node, NodeId, Rect, Role, Tree, TreeId, TreeUpdate};
use kurbo::Point;
use mere_renderer_registry::NodeContentKind;

use crate::scene::{SubstrateNode, SubstrateScene};

/// `NodeId` reserved for the root container. Substrate node identities
/// are `NonZeroU64`, so `NodeId(0)` never collides.
pub const ROOT_NODE_ID: NodeId = NodeId(0);

/// Build an `accesskit::TreeUpdate` projecting `scene`.
///
/// Returns a complete tree (suitable for the first update on a fresh
/// AccessKit adapter); subsequent calls produce equivalent complete
/// trees rather than diffs. Diffing against the previous frame is a
/// follow-up optimization.
pub fn project_scene(scene: &SubstrateScene) -> TreeUpdate {
    let mut nodes = Vec::with_capacity(scene.len() + 1);

    // Root window node, children = all substrate nodes.
    let mut root = Node::new(Role::Window);
    let children: Vec<NodeId> = scene
        .iter()
        .map(|n| NodeId(n.identity.as_u64()))
        .collect();
    for id in &children {
        root.push_child(*id);
    }
    nodes.push((ROOT_NODE_ID, root));

    for node in scene.iter() {
        nodes.push((NodeId(node.identity.as_u64()), build_node(node)));
    }

    TreeUpdate {
        nodes,
        tree: Some(Tree::new(ROOT_NODE_ID)),
        tree_id: TreeId::ROOT,
        focus: ROOT_NODE_ID,
    }
}

/// Map `NodeContentKind` to the closest semantic AccessKit `Role`.
fn role_for(kind: NodeContentKind) -> Role {
    match kind {
        NodeContentKind::WebPage => Role::WebView,
        NodeContentKind::DocumentTile => Role::Document,
        NodeContentKind::Panel => Role::Pane,
        NodeContentKind::CustomCanvas => Role::Canvas,
        NodeContentKind::GraphView
        | NodeContentKind::Knot
        | NodeContentKind::Composite
        | NodeContentKind::EdgeRendering => Role::Group,
    }
}

/// Build one AccessKit node for one substrate node — role from
/// content kind, bounds from placement-transformed tile rect.
fn build_node(node: &SubstrateNode) -> Node {
    let mut n = Node::new(role_for(node.content_kind));
    n.set_bounds(bounds_for(node));
    n
}

/// Axis-aligned bounding box (in scene-space pixels) of `node`'s
/// placement-transformed tile rectangle.
fn bounds_for(node: &SubstrateNode) -> Rect {
    let t = node.placement.transform;
    let w = node.size.width;
    let h = node.size.height;
    let corners = [
        t * Point::new(0.0, 0.0),
        t * Point::new(w, 0.0),
        t * Point::new(0.0, h),
        t * Point::new(w, h),
    ];
    let mut x0 = corners[0].x;
    let mut x1 = corners[0].x;
    let mut y0 = corners[0].y;
    let mut y1 = corners[0].y;
    for c in corners.iter().skip(1) {
        if c.x < x0 {
            x0 = c.x;
        }
        if c.x > x1 {
            x1 = c.x;
        }
        if c.y < y0 {
            y0 = c.y;
        }
        if c.y > y1 {
            y1 = c.y;
        }
    }
    Rect { x0, y0, x1, y1 }
}

#[cfg(test)]
mod tests {
    use kurbo::Size;
    use mere_renderer_registry::{NodeContentKind, NodeIdentity, Placement};

    use super::*;
    use crate::scene::{SubstrateNode, SubstrateScene};

    #[test]
    fn empty_scene_projects_root_only() {
        let scene = SubstrateScene::new();
        let update = project_scene(&scene);
        assert_eq!(update.nodes.len(), 1);
        let (id, root) = &update.nodes[0];
        assert_eq!(*id, ROOT_NODE_ID);
        assert_eq!(root.role(), Role::Window);
        assert_eq!(update.focus, ROOT_NODE_ID);
    }

    #[test]
    fn each_substrate_node_appears_in_tree() {
        let mut scene = SubstrateScene::new();
        let panel = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(10.0, 20.0),
            Size::new(100.0, 80.0),
        ));
        let doctile = scene.insert(SubstrateNode::new(
            NodeContentKind::DocumentTile,
            Placement::translate(200.0, 30.0),
            Size::new(150.0, 200.0),
        ));

        let update = project_scene(&scene);
        // Root + 2 nodes
        assert_eq!(update.nodes.len(), 3);

        // Both substrate nodes appear under root's children, in scene
        // order.
        let (_, root) = update
            .nodes
            .iter()
            .find(|(id, _)| *id == ROOT_NODE_ID)
            .unwrap();
        let expected = vec![NodeId(panel.as_u64()), NodeId(doctile.as_u64())];
        assert_eq!(root.children().to_vec(), expected);
    }

    #[test]
    fn role_maps_per_content_kind() {
        let cases = [
            (NodeContentKind::WebPage, Role::WebView),
            (NodeContentKind::DocumentTile, Role::Document),
            (NodeContentKind::Panel, Role::Pane),
            (NodeContentKind::CustomCanvas, Role::Canvas),
            (NodeContentKind::GraphView, Role::Group),
            (NodeContentKind::Knot, Role::Group),
            (NodeContentKind::Composite, Role::Group),
            (NodeContentKind::EdgeRendering, Role::Group),
        ];
        for (kind, expected_role) in cases {
            let mut scene = SubstrateScene::new();
            scene.insert(SubstrateNode::new(
                kind,
                Placement::IDENTITY,
                Size::new(10.0, 10.0),
            ));
            let update = project_scene(&scene);
            let node = update
                .nodes
                .iter()
                .find_map(|(id, n)| if *id != ROOT_NODE_ID { Some(n) } else { None })
                .unwrap();
            assert_eq!(node.role(), expected_role, "kind {:?}", kind);
        }
    }

    #[test]
    fn bounds_reflect_placement_translation() {
        let mut scene = SubstrateScene::new();
        scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(40.0, 60.0),
            Size::new(120.0, 80.0),
        ));
        let update = project_scene(&scene);
        let node = update
            .nodes
            .iter()
            .find_map(|(id, n)| if *id != ROOT_NODE_ID { Some(n) } else { None })
            .unwrap();
        let bounds = node.bounds().expect("bounds set");
        assert_eq!(bounds.x0, 40.0);
        assert_eq!(bounds.y0, 60.0);
        assert_eq!(bounds.x1, 160.0);
        assert_eq!(bounds.y1, 140.0);
    }

    #[test]
    fn bounds_aabb_a_scaled_node() {
        // Node at (50, 50) with 2× scale, source 80×40 → in scene
        // coords occupies (50, 50)..(210, 130).
        let mut scene = SubstrateScene::new();
        scene.insert(SubstrateNode {
            identity: NodeIdentity::next(),
            placement: Placement::new(
                kurbo::Affine::translate((50.0, 50.0)) * kurbo::Affine::scale(2.0),
            ),
            size: Size::new(80.0, 40.0),
            lod: mere_renderer_registry::LodLevel::FullPane,
            content_kind: NodeContentKind::Panel,
            renderer_pin: None,
        });
        let update = project_scene(&scene);
        let node = update
            .nodes
            .iter()
            .find_map(|(id, n)| if *id != ROOT_NODE_ID { Some(n) } else { None })
            .unwrap();
        let bounds = node.bounds().expect("bounds set");
        assert_eq!(bounds.x0, 50.0);
        assert_eq!(bounds.y0, 50.0);
        assert_eq!(bounds.x1, 210.0);
        assert_eq!(bounds.y1, 130.0);
    }
}
