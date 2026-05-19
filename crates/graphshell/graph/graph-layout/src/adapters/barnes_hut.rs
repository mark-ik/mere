/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! [`StreamingLayoutStrategy`] adapter for
//! [`graph_canvas::layout::BarnesHut`].
//!
//! Same shape as [`super::ForceDirectedAdapter`] — BarnesHut shares
//! [`ForceDirectedState`] with `ForceDirected` (the two are drop-in
//! swappable; pick by graph size). The adapter just routes step()
//! calls through `BarnesHut::with_config()` instead.
//!
//! Use this strategy for graphs with > 500 nodes where the O(n²)
//! repulsion of plain FR becomes a frame-budget concern. The
//! `BarnesHut` approximation drops the cost to O(n log n) via a
//! quadtree, controlled by [`BarnesHutConfig::theta`] (approximation
//! threshold) and `min_cell_size` (subdivision stop).

use std::collections::HashMap;

use crate::{BarnesHut, BarnesHutConfig, ForceDirectedState, Layout};
use euclid::default::{Point2D, Vector2D};
use mere_kernel::graph::NodeKey;
use serde::{Deserialize, Serialize};

use super::shared::{
    bounds_of, build_layout_extras, build_positioned_edges, build_scene_input, build_viewport,
};
use cartography::projection::{PositionedNode, Projection, ProjectionMetadata};
use cartography::request::ProjectionRequest;
use cartography::strategy::StreamingLayoutStrategy;

/// Cartography-side adapter for [`graph_canvas::layout::BarnesHut`].
#[derive(Debug, Default, Clone, Copy)]
pub struct BarnesHutAdapter {
    pub config: BarnesHutConfig,
}

impl BarnesHutAdapter {
    pub const PROJECTION_ID: &'static str = "force_directed.barnes_hut";

    pub fn new(config: BarnesHutConfig) -> Self {
        Self { config }
    }
}

/// Persistent state for [`BarnesHutAdapter`].
///
/// Shares the inner `ForceDirectedState` with the plain FR adapter
/// — the two strategies are drop-in swappable. State carries working
/// positions separate from graph truth (cartography contract: never
/// mutate the graph) plus FR convergence + tuning state.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BarnesHutAdapterState {
    pub positions: HashMap<NodeKey, Point2D<f32>>,
    pub fd: ForceDirectedState,
    #[serde(default)]
    pub initialized: bool,
}

impl StreamingLayoutStrategy for BarnesHutAdapter {
    type State = BarnesHutAdapterState;

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

        // Fresh BarnesHut instance per step — scratch buffer is a per-
        // step alloc, not state. Strategy stays `&self`.
        let mut bh = BarnesHut::with_config(self.config);
        let deltas: HashMap<NodeKey, Vector2D<f32>> =
            bh.step(&scene, &mut state.fd, dt, &viewport, &extras);

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
            .fd
            .last_avg_displacement
            .is_some_and(|avg| avg < state.fd.epsilon);

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
            .fd
            .last_avg_displacement
            .is_some_and(|avg| avg < state.fd.epsilon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cartography::request::ViewIntent;
    use cartography::signals::IntelligenceSignals;
    use mere_kernel::geometry::PortablePoint;
    use mere_kernel::graph::Graph;
    use uuid::Uuid;

    fn small_graph() -> (Graph, [NodeKey; 4]) {
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
            PortablePoint::new(0.0, 100.0),
        );
        let d = graph.add_node_with_id(
            Uuid::from_u128(4),
            "test://d".into(),
            PortablePoint::new(100.0, 100.0),
        );
        (graph, [a, b, c, d])
    }

    #[test]
    fn adapter_projection_id_is_stable() {
        let adapter = BarnesHutAdapter::default();
        assert_eq!(adapter.projection_id(), "force_directed.barnes_hut");
    }

    #[test]
    fn first_step_initializes_positions_from_graph_truth() {
        let (graph, [a, b, c, d]) = small_graph();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::canvas_pixels(800, 600),
        };
        let adapter = BarnesHutAdapter::default();
        let mut state = BarnesHutAdapterState::default();
        assert!(!state.initialized);

        adapter.step(&request, &mut state, 1.0 / 60.0);

        assert!(state.initialized);
        assert_eq!(state.positions.len(), 4);
        for key in [a, b, c, d] {
            assert!(state.positions.contains_key(&key));
        }
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
        let adapter = BarnesHutAdapter::default();
        let mut state = BarnesHutAdapterState::default();
        let projection = adapter.step(&request, &mut state, 1.0 / 60.0);
        assert!(projection.nodes.is_empty());
        assert!(projection.edges.is_empty());
    }

    #[test]
    fn graph_truth_positions_are_never_mutated() {
        let (graph, [a, _, _, _]) = small_graph();
        let original = graph.get_node(a).unwrap().projected_position();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::canvas_pixels(800, 600),
        };
        let adapter = BarnesHutAdapter::default();
        let mut state = BarnesHutAdapterState::default();
        for _ in 0..10 {
            adapter.step(&request, &mut state, 1.0 / 60.0);
        }
        let after = graph.get_node(a).unwrap().projected_position();
        assert_eq!(original, after);
    }

    #[test]
    fn custom_config_is_passed_through() {
        let adapter = BarnesHutAdapter::new(BarnesHutConfig {
            theta: 1.2,
            min_cell_size: 0.5,
        });
        assert_eq!(adapter.config.theta, 1.2);
        assert_eq!(adapter.config.min_cell_size, 0.5);
    }

    #[test]
    fn step_emits_projection_with_all_nodes_and_strategy_id() {
        let (graph, _) = small_graph();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::canvas_pixels(800, 600),
        };
        let adapter = BarnesHutAdapter::default();
        let mut state = BarnesHutAdapterState::default();
        let projection = adapter.step(&request, &mut state, 1.0 / 60.0);
        assert_eq!(projection.nodes.len(), 4);
        assert_eq!(
            projection.metadata.strategy_id.as_deref(),
            Some("force_directed.barnes_hut")
        );
    }
}
