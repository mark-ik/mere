// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Path B explosion — turn a cartography [`Projection`] into one
//! substrate node per graph node, plus the relation edges between
//! them.
//!
//! Path A paints a whole projection inside one `GraphView` pane
//! ([`crate::cartography_projection`] writes the snapshot, the orrery
//! renderer paints it). Path B instead inserts a
//! `NodeContentKind::GraphNode` substrate node per projected node,
//! positioned in window space, so each graph node is a first-class
//! spatial entity the substrate hit-tests and (later) drags
//! individually.
//!
//! Identity stability: each `(PaneId, NodeKey)` maps to a stable
//! `NodeIdentity` through [`GraphNodeIdentities`]. The substrate scene
//! is rebuilt every resync (so appended graph nodes vanish and get
//! re-exploded), but the identity map persists across resyncs so a
//! node keeps its identity when its position changes — the seam drag /
//! selection / streaming layout will pull on.

use std::collections::HashMap;

use cartography::Projection;
use frame::PaneId;
use kernel::graph::NodeKey;
use register_renderer::{LodLevel, NodeContentKind, NodeIdentity, Placement};
use spatial_substrate::{RelationEdge, SubstrateNode, SubstrateScene};

/// Default radius (window-space px) for a projected node whose
/// strategy emitted `radius == 0.0`. Matches the orrery renderer's
/// fallback so Path A and Path B size nodes identically.
const DEFAULT_NODE_RADIUS: f64 = 8.0;

/// Persistent `(PaneId, NodeKey) -> NodeIdentity` map. Lives on the
/// host's runtime state, not in the substrate scene (which is rebuilt
/// each resync). Keeps a graph node's substrate identity stable as its
/// projected position changes.
#[derive(Debug, Default)]
pub struct GraphNodeIdentities {
    map: HashMap<(PaneId, NodeKey), NodeIdentity>,
}

impl GraphNodeIdentities {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stable identity for `(pane, node)`, minting a fresh one on
    /// first sight.
    fn identity_for(&mut self, pane: PaneId, node: NodeKey) -> NodeIdentity {
        *self
            .map
            .entry((pane, node))
            .or_insert_with(NodeIdentity::next)
    }

    /// Identity for `(pane, node)` if one has been assigned. Read-only
    /// lookup used by edge wiring (an edge endpoint must already have a
    /// node identity from the node pass).
    fn lookup(&self, pane: PaneId, node: NodeKey) -> Option<NodeIdentity> {
        self.map.get(&(pane, node)).copied()
    }

    /// Number of tracked `(pane, node)` identities.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Per-call report from [`explode_projection_into_scene`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ExplodeReport {
    /// Graph nodes inserted as substrate `GraphNode` nodes.
    pub nodes: usize,
    /// Relation edges inserted between exploded nodes.
    pub edges: usize,
    /// Edges skipped because an endpoint had no exploded node.
    pub dangling_edges: usize,
}

/// Insert one `GraphNode` substrate node per projected node into
/// `scene`, positioned in window space (pane origin + projection-local
/// position), then wire relation edges between them. Returns counts
/// for diagnostics.
///
/// `pane_origin` is the pane's window-space top-left (the translation
/// of its `walk_leaves` placement). Projection positions are
/// pane-local — the same coordinate space Path A's orrery renderer
/// paints in — so adding `pane_origin` lifts them into the flat
/// substrate scene's window coordinates.
///
/// Nodes are inserted before their edges so edge endpoints resolve.
/// The caller is responsible for insertion order relative to panes /
/// splitters: explode after `sync_scene_from_frame_layout` so graph
/// nodes paint on top of (and hit-test before) the pane background.
pub fn explode_projection_into_scene(
    scene: &mut SubstrateScene,
    identities: &mut GraphNodeIdentities,
    pane_id: PaneId,
    pane_origin: kurbo::Point,
    projection: &Projection,
) -> ExplodeReport {
    let mut report = ExplodeReport::default();

    for positioned in &projection.nodes {
        let radius = if positioned.radius > 0.0 {
            positioned.radius as f64
        } else {
            DEFAULT_NODE_RADIUS
        };
        let center_x = pane_origin.x + positioned.position.x as f64;
        let center_y = pane_origin.y + positioned.position.y as f64;
        let identity = identities.identity_for(pane_id, positioned.node);
        scene.insert(SubstrateNode {
            identity,
            placement: Placement::translate(center_x - radius, center_y - radius),
            size: kurbo::Size::new(radius * 2.0, radius * 2.0),
            lod: LodLevel::FullPane,
            content_kind: NodeContentKind::GraphNode,
            renderer_pin: None,
        });
        report.nodes += 1;
    }

    for edge in &projection.edges {
        let (Some(from), Some(to)) = (
            identities.lookup(pane_id, edge.from),
            identities.lookup(pane_id, edge.to),
        ) else {
            report.dangling_edges += 1;
            continue;
        };
        scene.insert_edge(RelationEdge::new(from, to));
        report.edges += 1;
    }

    report
}

#[cfg(test)]
mod tests {
    use cartography::projection::{PositionedEdge, PositionedNode};
    use kernel::geometry::PortablePoint;
    use kernel::graph::Graph;

    use super::*;

    /// Build a projection with `n` nodes at known positions and an
    /// optional chain of edges between consecutive nodes. Node keys
    /// come from a real graph so `NodeKey` values are valid.
    fn projection_with(n: usize, with_edges: bool) -> (Projection, Vec<NodeKey>) {
        let mut graph = Graph::new();
        let keys: Vec<NodeKey> = (0..n)
            .map(|i| graph.add_node(format!("test://n/{i}"), PortablePoint::new(0.0, 0.0)))
            .collect();
        let nodes = keys
            .iter()
            .enumerate()
            .map(|(i, &node)| PositionedNode {
                node,
                position: PortablePoint::new(i as f32 * 10.0, i as f32 * 5.0),
                radius: 6.0,
            })
            .collect();
        let edges = if with_edges {
            keys.windows(2)
                .map(|w| PositionedEdge {
                    edge: None,
                    from: w[0],
                    to: w[1],
                    path: Vec::new(),
                })
                .collect()
        } else {
            Vec::new()
        };
        (
            Projection {
                nodes,
                edges,
                ..Projection::empty()
            },
            keys,
        )
    }

    #[test]
    fn explodes_one_substrate_node_per_projected_node() {
        let (projection, _) = projection_with(4, false);
        let mut scene = SubstrateScene::new();
        let mut identities = GraphNodeIdentities::new();
        let report = explode_projection_into_scene(
            &mut scene,
            &mut identities,
            PaneId(2),
            kurbo::Point::new(100.0, 50.0),
            &projection,
        );
        assert_eq!(report.nodes, 4);
        assert_eq!(scene.len(), 4);
        assert_eq!(identities.len(), 4);
    }

    #[test]
    fn positions_nodes_in_window_space() {
        let (projection, _) = projection_with(2, false);
        let mut scene = SubstrateScene::new();
        let mut identities = GraphNodeIdentities::new();
        explode_projection_into_scene(
            &mut scene,
            &mut identities,
            PaneId(2),
            kurbo::Point::new(100.0, 50.0),
            &projection,
        );
        // Node 1 is at projection-local (10, 5), radius 6. Window
        // center = (110, 55); top-left = (110-6, 55-6) = (104, 49).
        let placements: Vec<_> = scene
            .iter()
            .map(|n| n.placement.transform.translation())
            .collect();
        assert!(placements.iter().any(|t| (t.x - 104.0).abs() < 1e-6
            && (t.y - 49.0).abs() < 1e-6));
    }

    #[test]
    fn wires_edges_between_exploded_nodes() {
        let (projection, _) = projection_with(3, true);
        let mut scene = SubstrateScene::new();
        let mut identities = GraphNodeIdentities::new();
        let report = explode_projection_into_scene(
            &mut scene,
            &mut identities,
            PaneId(2),
            kurbo::Point::ZERO,
            &projection,
        );
        // 3 nodes, 2 chain edges.
        assert_eq!(report.nodes, 3);
        assert_eq!(report.edges, 2);
        assert_eq!(report.dangling_edges, 0);
        assert_eq!(scene.edge_count(), 2);
    }

    #[test]
    fn identity_is_stable_across_explosions() {
        let (projection, keys) = projection_with(3, false);
        let mut identities = GraphNodeIdentities::new();

        let mut scene_a = SubstrateScene::new();
        explode_projection_into_scene(
            &mut scene_a,
            &mut identities,
            PaneId(2),
            kurbo::Point::ZERO,
            &projection,
        );
        let first: Vec<NodeIdentity> = scene_a.iter().map(|n| n.identity).collect();

        // Re-explode into a fresh scene (mirrors a resync rebuild).
        let mut scene_b = SubstrateScene::new();
        explode_projection_into_scene(
            &mut scene_b,
            &mut identities,
            PaneId(2),
            kurbo::Point::ZERO,
            &projection,
        );
        let second: Vec<NodeIdentity> = scene_b.iter().map(|n| n.identity).collect();

        assert_eq!(first, second, "identities must survive a rebuild");
        // No new identities minted on the second pass.
        assert_eq!(identities.len(), keys.len());
    }

    #[test]
    fn same_node_key_in_different_panes_gets_distinct_identity() {
        let (projection, _) = projection_with(2, false);
        let mut identities = GraphNodeIdentities::new();
        let mut scene = SubstrateScene::new();
        explode_projection_into_scene(
            &mut scene,
            &mut identities,
            PaneId(2),
            kurbo::Point::ZERO,
            &projection,
        );
        explode_projection_into_scene(
            &mut scene,
            &mut identities,
            PaneId(3),
            kurbo::Point::ZERO,
            &projection,
        );
        // Two panes × two nodes = four distinct identities.
        assert_eq!(identities.len(), 4);
    }
}
