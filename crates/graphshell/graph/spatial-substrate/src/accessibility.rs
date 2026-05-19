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

use accesskit::{Node, NodeId, Rect, Role, Tree, TreeId, TreeUpdate, Uuid};
use kurbo::Point;
use mere_renderer_registry::{NodeContentKind, NodeIdentity};

use crate::host::SubstrateHost;
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
    let children: Vec<NodeId> = scene.iter().map(|n| NodeId(n.identity.as_u64())).collect();
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
        | NodeContentKind::EdgeRendering
        | NodeContentKind::Splitter => Role::Group,
    }
}

/// Build one AccessKit node for one substrate node — role from
/// content kind, bounds from placement-transformed tile rect.
fn build_node(node: &SubstrateNode) -> Node {
    let mut n = Node::new(role_for(node.content_kind));
    n.set_bounds(bounds_for(node));
    n
}

/// Collect the substrate's scene-level AccessKit tree *plus* one
/// per-producer subtree update for every `EmbeddedFrameRenderer` that
/// has pending AccessKit content (returns `Some` from
/// `take_accesskit_subtree`).
///
/// Returned vec is `[root_tree, subtree_1, subtree_2, ...]`. The host
/// hands each `TreeUpdate` to its AccessKit adapter in order; the OS
/// stitches them via `Node::tree_id` graft references on the root's
/// per-node entries.
///
/// The subtree's `tree_id` is rewritten from whatever the renderer
/// produced (typically `TreeId::ROOT`, because the renderer thinks
/// it's the only tree in the world) to the substrate's minted
/// per-producer id, computed deterministically from the node's
/// `NodeIdentity`. The host doesn't need to track tree-id mappings
/// across frames — same node identity always produces the same
/// tree id.
pub fn collect_accesskit_updates(
    host: &mut SubstrateHost,
    scene: &SubstrateScene,
) -> Vec<TreeUpdate> {
    let mut root = project_scene(scene);
    let mut subtrees = Vec::new();

    // Walk scene nodes; query each EmbeddedFrame producer for an
    // AccessKit subtree. If it returns Some, mint a TreeId,
    // annotate the root's node entry with `tree_id`, push the
    // subtree.
    let identities: Vec<NodeIdentity> = scene.iter().map(|n| n.identity).collect();
    for identity in identities {
        let Some(mut subtree) = host.take_subtree_for_node(identity) else {
            continue;
        };
        let tree_id = subtree_tree_id_for(identity);
        subtree.tree_id = tree_id;
        if let Some((_, root_node)) = root
            .nodes
            .iter_mut()
            .find(|(id, _)| *id == NodeId(identity.as_u64()))
        {
            root_node.set_tree_id(tree_id);
        }
        subtrees.push(subtree);
    }

    let mut all = Vec::with_capacity(1 + subtrees.len());
    all.push(root);
    all.extend(subtrees);
    all
}

/// Deterministic `TreeId` for a producer's subtree, derived from the
/// substrate node's identity. Same NodeIdentity → same TreeId,
/// frame after frame, with no host-side state to maintain.
///
/// The high 64 bits use a constant tag (`0x4D45_5245_5354_4553` —
/// ASCII-ish "MERESTES") so the result never collides with
/// `TreeId::ROOT` (the nil UUID).
pub fn subtree_tree_id_for(identity: NodeIdentity) -> TreeId {
    const NAMESPACE_HI: u64 = 0x4D45_5245_5354_4553;
    TreeId(Uuid::from_u64_pair(NAMESPACE_HI, identity.as_u64()))
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
    fn subtree_tree_id_is_deterministic_and_non_root() {
        let id_a = NodeIdentity::next();
        let id_b = NodeIdentity::next();
        assert_eq!(subtree_tree_id_for(id_a), subtree_tree_id_for(id_a));
        assert_ne!(subtree_tree_id_for(id_a), subtree_tree_id_for(id_b));
        assert_ne!(subtree_tree_id_for(id_a), TreeId::ROOT);
    }

    #[test]
    fn collect_returns_only_root_when_no_embedded_producers() {
        let mut host = crate::host::SubstrateHost::with_default_registry();
        let mut scene = SubstrateScene::new();
        scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::IDENTITY,
            Size::new(100.0, 100.0),
        ));
        let updates = collect_accesskit_updates(&mut host, &scene);
        assert_eq!(updates.len(), 1, "only root tree when no producers");
        assert_eq!(updates[0].tree_id, TreeId::ROOT);
    }

    #[test]
    fn collect_grafts_subtree_with_minted_tree_id() {
        use mere_renderer_registry::{
            CompositionMode, EmbeddedFrameRenderer, InputDisposition, InputEvent,
            NodeContentKindSet, NodeRenderer, ProducerHandle, ProfileBindingExpectation,
            RendererCapabilities, RendererId, RendererRegistry, SceneNodeRef,
        };

        struct SubtreeStub {
            id: RendererId,
            // Pretend the subtree has a single accessibility node.
            subtree_node_id: NodeId,
        }

        impl NodeRenderer for SubtreeStub {
            fn renderer_id(&self) -> RendererId {
                self.id.clone()
            }
            fn handles(&self) -> NodeContentKindSet {
                NodeContentKindSet::from_one(NodeContentKind::Panel)
            }
            fn composition_mode(&self) -> CompositionMode {
                CompositionMode::EmbeddedFrame
            }
            fn capabilities(&self) -> RendererCapabilities {
                RendererCapabilities {
                    accepts_input: false,
                    handles_ime: false,
                    handles_a11y: true,
                    scrollable: false,
                    hit_testable_subregions: false,
                    profile_binding: ProfileBindingExpectation::None,
                    supports_capture: false,
                }
            }
            fn as_embedded_frame(&mut self) -> Option<&mut dyn EmbeddedFrameRenderer> {
                Some(self)
            }
        }

        impl EmbeddedFrameRenderer for SubtreeStub {
            fn ensure_producer(&mut self, _node: &SceneNodeRef) -> ProducerHandle {
                ProducerHandle::next()
            }
            fn next_frame(&mut self, _handle: ProducerHandle) -> Option<wgpu::Texture> {
                None
            }
            fn deliver_input(&mut self, _h: ProducerHandle, _e: &InputEvent) -> InputDisposition {
                InputDisposition::Passthrough
            }
            fn release(&mut self, _h: ProducerHandle) {}

            fn take_accesskit_subtree(&mut self, _h: ProducerHandle) -> Option<TreeUpdate> {
                let mut node = Node::new(Role::Pane);
                node.set_bounds(Rect {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 100.0,
                    y1: 100.0,
                });
                Some(TreeUpdate {
                    nodes: vec![(self.subtree_node_id, node)],
                    tree: Some(Tree::new(self.subtree_node_id)),
                    tree_id: TreeId::ROOT, // host rewrites
                    focus: self.subtree_node_id,
                })
            }
        }

        let mut registry = RendererRegistry::with_default_selector();
        registry
            .register(Box::new(SubtreeStub {
                id: RendererId::from_static("subtree-stub"),
                subtree_node_id: NodeId(99),
            }))
            .unwrap();
        let mut host = crate::host::SubstrateHost::new(registry);

        let mut scene = SubstrateScene::new();
        let panel_id = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(40.0, 60.0),
            Size::new(120.0, 80.0),
        ));

        // Pre-warm the producer so collect_accesskit_updates can query it.
        let created = host.ensure_embedded_producers(&scene);
        assert_eq!(created, 1);

        let updates = collect_accesskit_updates(&mut host, &scene);
        assert_eq!(updates.len(), 2, "root + one subtree");

        // Root tree: substrate node should be a graft node with the
        // minted tree_id.
        let expected_tree_id = subtree_tree_id_for(panel_id);
        let root = &updates[0];
        assert_eq!(root.tree_id, TreeId::ROOT);
        let (_, root_panel_node) = root
            .nodes
            .iter()
            .find(|(id, _)| *id == NodeId(panel_id.as_u64()))
            .expect("substrate panel node present");
        assert_eq!(
            root_panel_node.tree_id(),
            Some(expected_tree_id),
            "root's panel node is a graft node referencing the subtree"
        );

        // Subtree update: tree_id rewritten by collect_accesskit_updates.
        let subtree = &updates[1];
        assert_eq!(
            subtree.tree_id, expected_tree_id,
            "subtree's tree_id rewritten from ROOT to the minted id"
        );
        assert_eq!(subtree.nodes.len(), 1);
        assert_eq!(subtree.nodes[0].0, NodeId(99));
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
