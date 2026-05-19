/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! [`StreamingLayoutStrategy`] adapter for
//! [`graph_canvas::layout::ForceDirected`].
//!
//! Snapshots graph positions into adapter state on the first step,
//! then evolves the working positions each frame by delegating to
//! the existing `ForceDirected::step()` — graph truth is never
//! mutated.

use std::collections::HashMap;

use crate::{ForceDirected, ForceDirectedState, Layout};
use euclid::default::{Point2D, Vector2D};
use mere_kernel::graph::NodeKey;
use serde::{Deserialize, Serialize};

use super::shared::{
    bounds_of, build_layout_extras, build_positioned_edges, build_scene_input, build_viewport,
};
use cartography::projection::{PositionedNode, Projection, ProjectionMetadata};
use cartography::request::ProjectionRequest;
use cartography::strategy::StreamingLayoutStrategy;

/// Cartography-side adapter for [`graph_canvas::layout::ForceDirected`].
#[derive(Debug, Default, Clone, Copy)]
pub struct ForceDirectedAdapter;

impl ForceDirectedAdapter {
    pub const PROJECTION_ID: &'static str = "force_directed.default";
}

/// Persistent state for [`ForceDirectedAdapter`].
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ForceDirectedAdapterState {
    /// Per-node working positions. Initialized from graph truth on
    /// the first step, then evolved each frame.
    pub positions: HashMap<NodeKey, Point2D<f32>>,
    /// Inner FR state — convergence detector + tuning.
    pub fd: ForceDirectedState,
    /// `true` once positions have been snapshotted from the graph.
    #[serde(default)]
    pub initialized: bool,
}

impl StreamingLayoutStrategy for ForceDirectedAdapter {
    type State = ForceDirectedAdapterState;

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

        // Fresh FR instance per step — scratch buffer is purely a per-
        // step alloc optimization, not state. Strategy stays `&self`.
        let mut fd = ForceDirected::new();
        let deltas: HashMap<NodeKey, Vector2D<f32>> =
            fd.step(&scene, &mut state.fd, dt, &viewport, &extras);

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

        build_projection(self, request, &state.positions, &state.fd)
    }

    fn is_converged(&self, state: &Self::State) -> bool {
        state
            .fd
            .last_avg_displacement
            .is_some_and(|avg| avg < state.fd.epsilon)
    }
}

fn build_projection(
    adapter: &ForceDirectedAdapter,
    request: &ProjectionRequest<'_>,
    positions: &HashMap<NodeKey, Point2D<f32>>,
    fd_state: &ForceDirectedState,
) -> Projection {
    let nodes: Vec<PositionedNode> = positions
        .iter()
        .map(|(key, pos)| PositionedNode {
            node: *key,
            position: *pos,
            radius: 0.0,
        })
        .collect();

    let edges = build_positioned_edges(request, positions);

    let settled = fd_state
        .last_avg_displacement
        .is_some_and(|avg| avg < fd_state.epsilon);

    Projection {
        nodes,
        edges,
        overlays: Vec::new(),
        minimap: None,
        content_bounds: bounds_of(positions),
        metadata: ProjectionMetadata {
            strategy_id: Some(adapter.projection_id().to_string()),
            settled,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cartography::request::ViewIntent;
    use cartography::signals::{AffinityScores, Cluster, ClusterSet, IntelligenceSignals};
    use mere_kernel::geometry::PortablePoint;
    use mere_kernel::graph::Graph;
    use uuid::Uuid;

    fn triangle_graph() -> (Graph, [NodeKey; 3]) {
        let mut graph = Graph::new();
        let a = graph.add_node_with_id(
            Uuid::from_u128(1),
            "test://a".to_string(),
            PortablePoint::new(0.0, 0.0),
        );
        let b = graph.add_node_with_id(
            Uuid::from_u128(2),
            "test://b".to_string(),
            PortablePoint::new(100.0, 0.0),
        );
        let c = graph.add_node_with_id(
            Uuid::from_u128(3),
            "test://c".to_string(),
            PortablePoint::new(50.0, 86.6),
        );
        (graph, [a, b, c])
    }

    #[test]
    fn adapter_projection_id_is_stable() {
        let adapter = ForceDirectedAdapter;
        assert_eq!(adapter.projection_id(), "force_directed.default");
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

        let adapter = ForceDirectedAdapter;
        let mut state = ForceDirectedAdapterState::default();
        assert!(!state.initialized);

        adapter.step(&request, &mut state, 1.0 / 60.0);

        assert!(state.initialized);
        assert_eq!(state.positions.len(), 3);
        assert!(state.positions.contains_key(&a));
        assert!(state.positions.contains_key(&b));
        assert!(state.positions.contains_key(&c));
    }

    #[test]
    fn step_emits_projection_with_all_nodes() {
        let (graph, _) = triangle_graph();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::canvas_pixels(800, 600),
        };

        let adapter = ForceDirectedAdapter;
        let mut state = ForceDirectedAdapterState::default();
        let projection = adapter.step(&request, &mut state, 1.0 / 60.0);

        assert_eq!(projection.nodes.len(), 3);
        assert_eq!(
            projection.metadata.strategy_id.as_deref(),
            Some("force_directed.default")
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
        let adapter = ForceDirectedAdapter;
        let mut state = ForceDirectedAdapterState::default();
        let projection = adapter.step(&request, &mut state, 1.0 / 60.0);
        assert!(projection.nodes.is_empty());
        assert!(projection.edges.is_empty());
    }

    #[test]
    fn graph_truth_positions_are_never_mutated() {
        let (graph, [a, _, _]) = triangle_graph();
        let original_a_pos = graph.get_node(a).unwrap().projected_position();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::canvas_pixels(800, 600),
        };
        let adapter = ForceDirectedAdapter;
        let mut state = ForceDirectedAdapterState::default();

        for _ in 0..10 {
            adapter.step(&request, &mut state, 1.0 / 60.0);
        }

        let after_a_pos = graph.get_node(a).unwrap().projected_position();
        assert_eq!(original_a_pos, after_a_pos);
    }

    #[test]
    fn affinity_signals_become_semantic_similarity_extras() {
        let (graph, [a, b, _]) = triangle_graph();
        let signals = IntelligenceSignals {
            affinity: Some(AffinityScores {
                pairs: vec![((a, b), 0.9)],
            }),
            ..Default::default()
        };
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::canvas_pixels(800, 600),
        };
        let extras = build_layout_extras(&request);
        assert_eq!(extras.semantic_similarity.get(&(a, b)), Some(&0.9));
    }

    #[test]
    fn cluster_signals_become_domain_by_node_extras() {
        let (graph, [a, b, c]) = triangle_graph();
        let signals = IntelligenceSignals {
            clusters: Some(ClusterSet {
                clusters: vec![Cluster {
                    id: "cluster_alpha".into(),
                    label: Some("Alpha".into()),
                    members: vec![a, b],
                    confidence: 0.85,
                }],
            }),
            ..Default::default()
        };
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::canvas_pixels(800, 600),
        };
        let extras = build_layout_extras(&request);
        assert_eq!(
            extras.domain_by_node.get(&a),
            Some(&"cluster_alpha".to_string())
        );
        assert_eq!(
            extras.domain_by_node.get(&b),
            Some(&"cluster_alpha".to_string())
        );
        assert!(!extras.domain_by_node.contains_key(&c));
    }

    #[test]
    fn run_one_step_returns_one_frame_projection_without_state_management() {
        let (graph, _) = triangle_graph();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::canvas_pixels(800, 600),
        };
        let adapter = ForceDirectedAdapter;
        let projection = super::super::shared::run_one_step(&adapter, &request);
        assert_eq!(projection.nodes.len(), 3);
    }
}
