// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `SubstrateScene` — flat list of placed scene nodes.
//!
//! v0a holds nodes in insertion order; no edges, no spatial index. The IR
//! brief's full property set (placement / embeds / typed relations / LOD /
//! identity) is named here for the bits it covers; the rest is Phase-4+
//! work tracked in the adoption plan.

use kurbo::Size;
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
}
