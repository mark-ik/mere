/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! [`StreamingLayoutStrategy`] adapter for
//! [`graph_canvas::layout::SemanticEdgeWeight`].
//!
//! Iterative force-based projection where each pair's attraction is
//! weighted by `similarity(a, b)` (from
//! [`crate::IntelligenceSignals::affinity`]). Repulsion is uniform
//! across all pairs like FR.
//!
//! First adapter to *meaningfully* consume an intelligence signal —
//! the shared bridge already maps cartography's affinity pairs into
//! graph-canvas's `LayoutExtras::semantic_similarity` which is what
//! `SemanticEdgeWeight` reads. No model pipeline required — quality
//! is below research-grade embedding, but it works without any
//! external dep.
//!
//! Use [`crate::adapters::graph_canvas::ForceDirectedAdapter`] when
//! similarity isn't available (or just to compare visually);
//! `SemanticEdgeWeightAdapter` when similarity drives the layout
//! shape.

use std::collections::HashMap;

use euclid::default::{Point2D, Vector2D};
use crate::{
    Layout, SemanticEdgeWeight, SemanticEdgeWeightConfig, SemanticEdgeWeightState,
};
use mere_kernel::graph::NodeKey;
use serde::{Deserialize, Serialize};

use super::shared::{
    bounds_of, build_layout_extras, build_positioned_edges, build_scene_input, build_viewport,
};
use cartography::projection::{PositionedNode, Projection, ProjectionMetadata};
use cartography::request::ProjectionRequest;
use cartography::strategy::StreamingLayoutStrategy;

/// Cartography-side adapter for
/// [`graph_canvas::layout::SemanticEdgeWeight`].
#[derive(Debug, Default, Clone)]
pub struct SemanticEdgeWeightAdapter {
    pub config: SemanticEdgeWeightConfig,
}

impl SemanticEdgeWeightAdapter {
    pub const PROJECTION_ID: &'static str = "semantic.edge_weight";

    pub fn new(config: SemanticEdgeWeightConfig) -> Self {
        Self { config }
    }
}

/// Persistent state for [`SemanticEdgeWeightAdapter`].
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SemanticEdgeWeightAdapterState {
    pub positions: HashMap<NodeKey, Point2D<f32>>,
    pub inner: SemanticEdgeWeightState,
    #[serde(default)]
    pub initialized: bool,
}

impl StreamingLayoutStrategy for SemanticEdgeWeightAdapter {
    type State = SemanticEdgeWeightAdapterState;

    fn projection_id(&self) -> &'static str {
        Self::PROJECTION_ID
    }

    fn step(
        &self,
        request: &ProjectionRequest<'_>,
        state: &mut Self::State,
        dt: f32,
    ) -> Projection {
        if !state.initialized {
            for (key, node) in request.graph.nodes() {
                state.positions.insert(key, node.projected_position());
            }
            state.initialized = true;
        }

        let scene = build_scene_input(request, &state.positions);
        let viewport = build_viewport(request);
        let extras = build_layout_extras(request);

        // Fresh instance per step — scratch buffer is a per-step alloc.
        let mut sew = SemanticEdgeWeight::new(self.config.clone());
        let deltas: HashMap<NodeKey, Vector2D<f32>> =
            sew.step(&scene, &mut state.inner, dt, &viewport, &extras);

        for (key, delta) in deltas {
            state
                .positions
                .entry(key)
                .and_modify(|pos| {
                    pos.x += delta.x;
                    pos.y += delta.y;
                })
                .or_insert_with(|| Point2D::new(delta.x, delta.y));
        }

        let nodes: Vec<PositionedNode> = state
            .positions
            .iter()
            .map(|(key, pos)| PositionedNode {
                node: *key,
                position: *pos,
                radius: 0.0,
            })
            .collect();
        let edges = build_positioned_edges(request, &state.positions);

        let settled = state
            .inner
            .last_avg_displacement
            .is_some_and(|avg| avg < self.config.epsilon);

        Projection {
            nodes,
            edges,
            overlays: Vec::new(),
            minimap: None,
            content_bounds: bounds_of(&state.positions),
            metadata: ProjectionMetadata {
                strategy_id: Some(self.projection_id().to_string()),
                settled,
            },
        }
    }

    fn is_converged(&self, state: &Self::State) -> bool {
        state
            .inner
            .last_avg_displacement
            .is_some_and(|avg| avg < self.config.epsilon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cartography::request::ViewIntent;
    use cartography::signals::{AffinityScores, IntelligenceSignals};
    use mere_kernel::geometry::PortablePoint;
    use mere_kernel::graph::Graph;
    use uuid::Uuid;

    fn triangle_graph() -> (Graph, [NodeKey; 3]) {
        let mut graph = Graph::new();
        let a = graph.add_node_with_id(
            Uuid::from_u128(1),
            "test://a".into(),
            PortablePoint::new(0.0, 0.0),
        );
        let b = graph.add_node_with_id(
            Uuid::from_u128(2),
            "test://b".into(),
            PortablePoint::new(100.0, 0.0),
        );
        let c = graph.add_node_with_id(
            Uuid::from_u128(3),
            "test://c".into(),
            PortablePoint::new(50.0, 86.6),
        );
        (graph, [a, b, c])
    }

    #[test]
    fn adapter_projection_id_is_stable() {
        let adapter = SemanticEdgeWeightAdapter::default();
        assert_eq!(adapter.projection_id(), "semantic.edge_weight");
    }

    #[test]
    fn first_step_initializes_positions_from_graph_truth() {
        let (graph, [a, b, c]) = triangle_graph();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::canvas_pixels(800, 600),
        };
        let adapter = SemanticEdgeWeightAdapter::default();
        let mut state = SemanticEdgeWeightAdapterState::default();
        assert!(!state.initialized);

        adapter.step(&request, &mut state, 1.0 / 60.0);

        assert!(state.initialized);
        assert_eq!(state.positions.len(), 3);
        assert!(state.positions.contains_key(&a));
        assert!(state.positions.contains_key(&b));
        assert!(state.positions.contains_key(&c));
    }

    #[test]
    fn step_emits_projection_with_all_nodes_and_strategy_id() {
        let (graph, _) = triangle_graph();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::canvas_pixels(800, 600),
        };
        let adapter = SemanticEdgeWeightAdapter::default();
        let mut state = SemanticEdgeWeightAdapterState::default();
        let projection = adapter.step(&request, &mut state, 1.0 / 60.0);
        assert_eq!(projection.nodes.len(), 3);
        assert_eq!(
            projection.metadata.strategy_id.as_deref(),
            Some("semantic.edge_weight")
        );
    }

    #[test]
    fn empty_graph_produces_empty_projection() {
        let graph = Graph::new();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::canvas_pixels(800, 600),
        };
        let adapter = SemanticEdgeWeightAdapter::default();
        let mut state = SemanticEdgeWeightAdapterState::default();
        let projection = adapter.step(&request, &mut state, 1.0 / 60.0);
        assert!(projection.nodes.is_empty());
        assert!(projection.edges.is_empty());
    }

    #[test]
    fn similar_pairs_attract_more_than_dissimilar_pairs() {
        // a and b are highly similar (0.95); a and c are not (0.05).
        // After several steps, a-b distance should shrink relative to
        // a-c distance.
        let (graph, [a, b, c]) = triangle_graph();
        let signals = IntelligenceSignals {
            affinity: Some(AffinityScores {
                pairs: vec![((a, b), 0.95), ((a, c), 0.05), ((b, c), 0.05)],
            }),
            ..Default::default()
        };
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::canvas_pixels(800, 600),
        };

        let initial_ab = ((100.0_f32 - 0.0).powi(2) + (0.0 - 0.0_f32).powi(2)).sqrt();
        let initial_ac = ((50.0_f32 - 0.0).powi(2) + (86.6 - 0.0_f32).powi(2)).sqrt();

        let adapter = SemanticEdgeWeightAdapter::default();
        let mut state = SemanticEdgeWeightAdapterState::default();
        // Run many steps to let similarity-weighted forces dominate.
        for _ in 0..100 {
            adapter.step(&request, &mut state, 1.0 / 60.0);
        }

        let pos_a = state.positions[&a];
        let pos_b = state.positions[&b];
        let pos_c = state.positions[&c];
        let final_ab = ((pos_b.x - pos_a.x).powi(2) + (pos_b.y - pos_a.y).powi(2)).sqrt();
        let final_ac = ((pos_c.x - pos_a.x).powi(2) + (pos_c.y - pos_a.y).powi(2)).sqrt();

        // The ratio ab/ac should have shrunk: a-b are more similar so
        // they pull together more than a-c do.
        let initial_ratio = initial_ab / initial_ac;
        let final_ratio = final_ab / final_ac;
        assert!(
            final_ratio < initial_ratio,
            "similar pair should be closer relative to dissimilar pair after sim: \
             initial ratio={initial_ratio:.3}, final ratio={final_ratio:.3}"
        );
    }

    #[test]
    fn graph_truth_positions_are_never_mutated() {
        let (graph, [a, _, _]) = triangle_graph();
        let original = graph.get_node(a).unwrap().projected_position();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::canvas_pixels(800, 600),
        };
        let adapter = SemanticEdgeWeightAdapter::default();
        let mut state = SemanticEdgeWeightAdapterState::default();
        for _ in 0..10 {
            adapter.step(&request, &mut state, 1.0 / 60.0);
        }
        let after = graph.get_node(a).unwrap().projected_position();
        assert_eq!(original, after);
    }
}
