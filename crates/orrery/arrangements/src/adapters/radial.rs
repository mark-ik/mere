/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! [`LayoutStrategy`] adapter for [`canvas_ir::layout::Radial`].
//!
//! Radial places nodes on concentric rings via BFS from a focal node:
//! the focal node sits at `center`, its direct neighbors at radius
//! `ring_spacing`, the next layer at `2 × ring_spacing`, and so on.
//! Unreachable nodes get an outer ring, get collapsed to center, or
//! stay in place per [`RadialUnreachablePolicy`].
//!
//! Cartography contract: this is *analytic* — for a given focal node
//! and ring spacing, the formula produces the same target positions
//! in one call. No iteration, no state.
//!
//! The focal node comes from [`ViewIntent::focus`] — strategies are
//! configuration-shaped (`&self`), so the per-call focus lives on
//! the request, not in the adapter struct. An adapter with no
//! focus-via-intent and no default focus emits an empty projection.

use std::collections::HashMap;

use crate::{
    Layout, Radial, RadialAngularPolicy, RadialConfig, RadialUnreachablePolicy, StaticLayoutState,
};
use euclid::default::Point2D;
use crate::scene::{CanvasEdge, CanvasNode, CanvasSceneInput};
use kernel::graph::NodeKey;

use super::shared::{bounds_of, build_positioned_edges};
use cartography::projection::{PositionedNode, Projection, ProjectionMetadata};
use cartography::request::ProjectionRequest;
use cartography::strategy::LayoutStrategy;

/// Cartography-side adapter for [`canvas_ir::layout::Radial`].
///
/// The focal node comes from [`crate::ViewIntent::focus`]; if no focus
/// is set on the request, the adapter returns an empty projection
/// marked settled.
#[derive(Debug, Clone, Copy)]
pub struct RadialAdapter {
    pub center: Point2D<f32>,
    pub ring_spacing: f32,
    pub angular_policy: RadialAngularPolicy,
    pub rotation_offset: f32,
    pub unreachable_policy: RadialUnreachablePolicy,
}

impl RadialAdapter {
    pub const PROJECTION_ID: &'static str = "radial.default";
}

impl Default for RadialAdapter {
    fn default() -> Self {
        Self {
            center: Point2D::new(0.0, 0.0),
            ring_spacing: 120.0,
            angular_policy: RadialAngularPolicy::default(),
            rotation_offset: 0.0,
            unreachable_policy: RadialUnreachablePolicy::default(),
        }
    }
}

impl LayoutStrategy for RadialAdapter {
    fn projection_id(&self) -> &'static str {
        Self::PROJECTION_ID
    }

    fn project(&self, request: &ProjectionRequest<'_>) -> Projection {
        let Some(focus) = request.intent.focus else {
            return Projection {
                metadata: ProjectionMetadata {
                    strategy_id: Some(self.projection_id().to_string()),
                    settled: true,
                },
                ..Projection::empty()
            };
        };

        // Build a scene with all node positions at origin. Radial's
        // existing `Layout::step()` returns deltas from `node.position`
        // to its target; with positions at origin, the delta IS the
        // absolute target. The shared `emit()` applies damping — we
        // set `damping = 1.0` on the state so the unmodified target
        // comes through.
        let nodes: Vec<CanvasNode<NodeKey>> = request
            .graph
            .nodes()
            .map(|(key, _node)| CanvasNode {
                id: key,
                position: Point2D::origin(),
                radius: 0.0,
                label: None,
            })
            .collect();
        if nodes.is_empty() {
            return Projection {
                metadata: ProjectionMetadata {
                    strategy_id: Some(self.projection_id().to_string()),
                    settled: true,
                },
                ..Projection::empty()
            };
        }
        let edges: Vec<CanvasEdge<NodeKey>> = request
            .graph
            .relations()
            .filter(|view| view.from != view.to)
            .map(|view| CanvasEdge::untagged(view.from, view.to))
            .collect();

        let scene = CanvasSceneInput { nodes, edges };

        let mut radial = Radial::new(RadialConfig {
            focus: Some(focus),
            center: self.center,
            ring_spacing: self.ring_spacing,
            angular_policy: self.angular_policy,
            rotation_offset: self.rotation_offset,
            unreachable_policy: self.unreachable_policy,
        });
        let mut state = StaticLayoutState {
            damping: 1.0,
            step_count: 0,
        };
        let viewport = crate::camera::CanvasViewport::default();
        let extras = crate::LayoutExtras::<NodeKey>::default();
        let deltas = radial.step(&scene, &mut state, 0.0, &viewport, &extras);

        // `deltas[id] = target - origin = target`. Build the
        // position map.
        let mut positions: HashMap<NodeKey, Point2D<f32>> = HashMap::with_capacity(deltas.len());
        for (key, delta) in deltas {
            positions.insert(key, Point2D::new(delta.x, delta.y));
        }
        // The focal node itself never appears in deltas (its target
        // equals its origin under our shifted scene). Insert it
        // explicitly at `center` so the projection includes it.
        positions.insert(focus, self.center);

        let projection_nodes: Vec<PositionedNode> = positions
            .iter()
            .map(|(key, pos)| PositionedNode {
                node: *key,
                position: *pos,
                radius: 0.0,
            })
            .collect();
        let projection_edges = build_positioned_edges(request, &positions);

        Projection {
            nodes: projection_nodes,
            edges: projection_edges,
            overlays: Vec::new(),
            minimap: None,
            content_bounds: bounds_of(&positions),
            metadata: ProjectionMetadata {
                strategy_id: Some(self.projection_id().to_string()),
                settled: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cartography::request::ViewIntent;
    use cartography::signals::IntelligenceSignals;
    use kernel::geometry::PortablePoint;
    use kernel::graph::{EdgeAssertion, Graph, SemanticSubKind};
    use uuid::Uuid;

    fn star_graph() -> (Graph, [NodeKey; 5]) {
        // a is hub; b,c,d,e all connect to a. Single-hop radial.
        let mut graph = Graph::new();
        let a = graph.add_node_with_id(
            Uuid::from_u128(1),
            "test://a".into(),
            PortablePoint::new(0.0, 0.0),
        );
        let b = graph.add_node_with_id(
            Uuid::from_u128(2),
            "test://b".into(),
            PortablePoint::new(0.0, 0.0),
        );
        let c = graph.add_node_with_id(
            Uuid::from_u128(3),
            "test://c".into(),
            PortablePoint::new(0.0, 0.0),
        );
        let d = graph.add_node_with_id(
            Uuid::from_u128(4),
            "test://d".into(),
            PortablePoint::new(0.0, 0.0),
        );
        let e = graph.add_node_with_id(
            Uuid::from_u128(5),
            "test://e".into(),
            PortablePoint::new(0.0, 0.0),
        );
        let hyperlink = || EdgeAssertion::Semantic {
            sub_kind: SemanticSubKind::Hyperlink,
            label: None,
            decay_progress: None,
        };
        graph.assert_relation(a, b, hyperlink());
        graph.assert_relation(a, c, hyperlink());
        graph.assert_relation(a, d, hyperlink());
        graph.assert_relation(a, e, hyperlink());
        (graph, [a, b, c, d, e])
    }

    #[test]
    fn adapter_projection_id_is_stable() {
        let adapter = RadialAdapter::default();
        assert_eq!(adapter.projection_id(), "radial.default");
    }

    #[test]
    fn missing_focus_in_intent_returns_empty_projection_marked_settled() {
        let (graph, _) = star_graph();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = RadialAdapter::default();
        let projection = adapter.project(&request);
        assert!(projection.nodes.is_empty());
        assert!(projection.metadata.settled);
    }

    #[test]
    fn project_places_focus_at_center_and_neighbors_on_first_ring() {
        let (graph, [a, b, c, d, e]) = star_graph();
        let signals = IntelligenceSignals::default();
        let intent = ViewIntent::default().with_focus(a);
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent,
        };
        let adapter = RadialAdapter::default();
        let projection = adapter.project(&request);

        assert_eq!(projection.nodes.len(), 5);

        let focus_pos = projection
            .nodes
            .iter()
            .find(|n| n.node == a)
            .unwrap()
            .position;
        assert!(focus_pos.x.abs() < 0.001);
        assert!(focus_pos.y.abs() < 0.001);

        // b/c/d/e should all be on the first ring (radius =
        // ring_spacing = 120.0). Allow small floating-point slop.
        for key in [b, c, d, e] {
            let pos = projection
                .nodes
                .iter()
                .find(|n| n.node == key)
                .unwrap()
                .position;
            let r = (pos.x * pos.x + pos.y * pos.y).sqrt();
            assert!(
                (r - 120.0).abs() < 0.01,
                "node {key:?} at radius {r}, expected 120.0"
            );
        }
    }

    #[test]
    fn custom_center_and_ring_spacing_propagate() {
        let (graph, [a, b, _, _, _]) = star_graph();
        let signals = IntelligenceSignals::default();
        let intent = ViewIntent::default().with_focus(a);
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent,
        };
        let adapter = RadialAdapter {
            center: Point2D::new(50.0, 50.0),
            ring_spacing: 200.0,
            ..RadialAdapter::default()
        };
        let projection = adapter.project(&request);

        let focus_pos = projection
            .nodes
            .iter()
            .find(|n| n.node == a)
            .unwrap()
            .position;
        assert_eq!(focus_pos, Point2D::new(50.0, 50.0));

        let b_pos = projection
            .nodes
            .iter()
            .find(|n| n.node == b)
            .unwrap()
            .position;
        let r = ((b_pos.x - 50.0).powi(2) + (b_pos.y - 50.0).powi(2)).sqrt();
        assert!(
            (r - 200.0).abs() < 0.01,
            "first-ring node at radius {r} from center, expected 200.0"
        );
    }

    #[test]
    fn graph_truth_positions_are_never_mutated() {
        let (graph, [a, _, _, _, _]) = star_graph();
        let original = graph.get_node(a).unwrap().projected_position();
        let signals = IntelligenceSignals::default();
        let intent = ViewIntent::default().with_focus(a);
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent,
        };
        let adapter = RadialAdapter::default();
        for _ in 0..5 {
            adapter.project(&request);
        }
        let after = graph.get_node(a).unwrap().projected_position();
        assert_eq!(original, after);
    }
}
