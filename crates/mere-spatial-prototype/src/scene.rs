// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `SubstrateScene` — flat list of placed scene nodes.
//!
//! v0a holds nodes in insertion order; no edges, no spatial index. The IR
//! brief's full property set (placement / embeds / typed relations / LOD /
//! identity) is named here for the bits it covers; the rest is Phase-4+
//! work tracked in the adoption plan.

use kurbo::{Point, Size};
use mere_renderer_registry::{
    LodLevel, NodeContentKind, NodeIdentity, Placement, SceneNodeRef,
};

/// A single placed node in the substrate's spatial scene graph.
#[derive(Copy, Clone, Debug)]
pub struct SubstrateNode {
    pub identity: NodeIdentity,
    pub placement: Placement,
    pub size: Size,
    pub lod: LodLevel,
    pub content_kind: NodeContentKind,
}

impl SubstrateNode {
    /// Construct a node with a freshly-minted identity and `FullPane` LOD.
    pub fn new(content_kind: NodeContentKind, placement: Placement, size: Size) -> Self {
        Self {
            identity: NodeIdentity::next(),
            placement,
            size,
            lod: LodLevel::FullPane,
            content_kind,
        }
    }

    /// Borrowed view of this node for registry dispatch.
    pub fn as_ref(&self) -> SceneNodeRef {
        SceneNodeRef {
            identity: self.identity,
            placement: self.placement,
            lod: self.lod,
            size: self.size,
            content_kind: self.content_kind,
        }
    }
}

/// Flat substrate scene. Nodes paint in insertion order (back-to-front).
#[derive(Default)]
pub struct SubstrateScene {
    nodes: Vec<SubstrateNode>,
}

impl SubstrateScene {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a node; returns its identity.
    pub fn insert(&mut self, node: SubstrateNode) -> NodeIdentity {
        let id = node.identity;
        self.nodes.push(node);
        id
    }

    /// Look up a node by identity.
    pub fn get(&self, identity: NodeIdentity) -> Option<&SubstrateNode> {
        self.nodes.iter().find(|n| n.identity == identity)
    }

    /// Iterate nodes in paint order (back-to-front).
    pub fn iter(&self) -> impl Iterator<Item = &SubstrateNode> {
        self.nodes.iter()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Return the identity of the topmost node containing `point`, or
    /// `None` if no node does.
    ///
    /// Walks nodes in reverse-paint order (front-to-back) and returns
    /// the first hit — the renderer-registry contract's spatial-input
    /// router behavior. `point` is in the scene's coordinate space
    /// (same as the placement transforms in the scene), typically
    /// host-window logical pixels.
    ///
    /// Coarse hit-test only: a node is hit if `point`, mapped back into
    /// the node's local coordinates via the inverse of its placement
    /// transform, lies within `[0, size.width] × [0, size.height]`. The
    /// renderer-registry brief leaves sub-region refinement
    /// (`RendererCapabilities::hit_testable_subregions`) to the
    /// resolved renderer's own input handling — this method returns
    /// `Some(identity)` and the host hands the event to that renderer
    /// to refine if needed.
    pub fn hit_test(&self, point: Point) -> Option<NodeIdentity> {
        for node in self.nodes.iter().rev() {
            if node_contains_point(node, point) {
                return Some(node.identity);
            }
        }
        None
    }
}

/// Test whether `point` (in scene coords) falls inside `node`'s
/// transformed bounding rectangle. Degenerate (zero-determinant)
/// transforms never contain any point.
fn node_contains_point(node: &SubstrateNode, point: Point) -> bool {
    let transform = node.placement.transform;
    if transform.determinant() == 0.0 {
        return false;
    }
    let local = transform.inverse() * point;
    local.x >= 0.0
        && local.y >= 0.0
        && local.x <= node.size.width
        && local.y <= node.size.height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_node_has_full_pane_lod() {
        let node = SubstrateNode::new(
            NodeContentKind::DocumentTile,
            Placement::IDENTITY,
            Size::new(100.0, 100.0),
        );
        assert_eq!(node.lod, LodLevel::FullPane);
    }

    #[test]
    fn nodes_iter_in_insertion_order() {
        let mut scene = SubstrateScene::new();
        let a = scene.insert(SubstrateNode::new(
            NodeContentKind::DocumentTile,
            Placement::IDENTITY,
            Size::new(10.0, 10.0),
        ));
        let b = scene.insert(SubstrateNode::new(
            NodeContentKind::GraphView,
            Placement::translate(20.0, 30.0),
            Size::new(50.0, 50.0),
        ));
        let collected: Vec<_> = scene.iter().map(|n| n.identity).collect();
        assert_eq!(collected, vec![a, b]);
    }

    #[test]
    fn as_ref_round_trips_fields() {
        let node = SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(7.5, 11.0),
            Size::new(80.0, 40.0),
        );
        let r = node.as_ref();
        assert_eq!(r.identity, node.identity);
        assert_eq!(r.size, node.size);
        assert_eq!(r.content_kind, node.content_kind);
        assert_eq!(r.placement.transform, node.placement.transform);
    }

    #[test]
    fn hit_test_empty_scene_returns_none() {
        let scene = SubstrateScene::new();
        assert_eq!(scene.hit_test(Point::new(10.0, 10.0)), None);
    }

    #[test]
    fn hit_test_inside_translated_rect_returns_identity() {
        let mut scene = SubstrateScene::new();
        let id = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(40.0, 60.0),
            Size::new(100.0, 80.0),
        ));
        // Inside the rect (45, 70) is at (5, 10) tile-local.
        assert_eq!(scene.hit_test(Point::new(45.0, 70.0)), Some(id));
        // Corner (139.99, 139.99) is inside.
        assert_eq!(scene.hit_test(Point::new(139.99, 139.99)), Some(id));
    }

    #[test]
    fn hit_test_outside_translated_rect_returns_none() {
        let mut scene = SubstrateScene::new();
        scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(40.0, 60.0),
            Size::new(100.0, 80.0),
        ));
        // Left of rect.
        assert_eq!(scene.hit_test(Point::new(20.0, 70.0)), None);
        // Above rect.
        assert_eq!(scene.hit_test(Point::new(50.0, 30.0)), None);
        // Below rect.
        assert_eq!(scene.hit_test(Point::new(50.0, 200.0)), None);
        // Right of rect.
        assert_eq!(scene.hit_test(Point::new(200.0, 70.0)), None);
    }

    #[test]
    fn hit_test_returns_topmost_when_overlapping() {
        let mut scene = SubstrateScene::new();
        let _under = scene.insert(SubstrateNode::new(
            NodeContentKind::CustomCanvas,
            Placement::IDENTITY,
            Size::new(200.0, 200.0),
        ));
        let over = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(50.0, 50.0),
            Size::new(100.0, 100.0),
        ));
        // (100, 100) is inside both — should return the topmost (last inserted).
        assert_eq!(scene.hit_test(Point::new(100.0, 100.0)), Some(over));
    }

    #[test]
    fn hit_test_through_scale_transform() {
        // Node placed at (50, 50) with 2× scale. The 80×40 source rect
        // covers (50..210, 50..130) in scene coords.
        let mut scene = SubstrateScene::new();
        let id = scene.insert(SubstrateNode {
            identity: NodeIdentity::next(),
            placement: Placement::new(
                kurbo::Affine::translate((50.0, 50.0)) * kurbo::Affine::scale(2.0),
            ),
            size: kurbo::Size::new(80.0, 40.0),
            lod: LodLevel::FullPane,
            content_kind: NodeContentKind::Panel,
        });
        // (100, 70) → tile-local (25, 10) under inverse → inside.
        assert_eq!(scene.hit_test(Point::new(100.0, 70.0)), Some(id));
        // (250, 70) → tile-local (100, 10) → outside (width = 80).
        assert_eq!(scene.hit_test(Point::new(250.0, 70.0)), None);
    }
}
