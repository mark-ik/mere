// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`LayoutStrategy`] adapter for [`canvas_ir::layout::LSystem`].
//!
//! Analytic: nodes are placed along the path of an L-system fractal
//! expansion. Built-in grammars include Hilbert (cache-coherent
//! space-filling — adjacent indices stay spatially close, good for
//! very large graphs), Koch, and Dragon. Pick by aesthetic + locality
//! goals.

use crate::{LSystem, LSystemConfig};

use super::shared::{projection_from_positions, run_static_layout_one_shot};
use cartography::projection::Projection;
use cartography::request::ProjectionRequest;
use cartography::strategy::LayoutStrategy;

/// Cartography-side adapter for [`canvas_ir::layout::LSystem`].
#[derive(Debug, Default, Clone)]
pub struct LSystemAdapter {
    pub config: LSystemConfig,
}

impl LSystemAdapter {
    pub const PROJECTION_ID: &'static str = "lsystem.default";

    pub fn new(config: LSystemConfig) -> Self {
        Self { config }
    }
}

impl LayoutStrategy for LSystemAdapter {
    fn projection_id(&self) -> &'static str {
        Self::PROJECTION_ID
    }

    fn project(&self, request: &ProjectionRequest<'_>) -> Projection {
        let mut lsystem = LSystem::new(self.config.clone());
        let positions = run_static_layout_one_shot(request, &mut lsystem);
        projection_from_positions(Self::PROJECTION_ID, request, positions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IterationDepth, LSystemGrammar};
    use cartography::request::ViewIntent;
    use cartography::signals::IntelligenceSignals;
    use kernel::geometry::PortablePoint;
    use kernel::graph::{Graph, NodeKey};
    use kernel::graph::fixtures::GraphFixtures;
    use uuid::Uuid;

    fn small_graph(n: usize) -> (Graph, Vec<NodeKey>) {
        let mut graph = Graph::new();
        let keys: Vec<NodeKey> = (0..n)
            .map(|i| {
                graph.add_node_with_id(
                    Uuid::from_u128((i + 1) as u128),
                    format!("test://n{i}"),
                    PortablePoint::new(0.0, 0.0),
                )
            })
            .collect();
        (graph, keys)
    }

    #[test]
    fn adapter_projection_id_is_stable() {
        let adapter = LSystemAdapter::default();
        assert_eq!(adapter.projection_id(), "lsystem.default");
    }

    #[test]
    fn project_places_every_node_with_distinct_positions() {
        let (graph, keys) = small_graph(8);
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = LSystemAdapter::default();
        let projection = adapter.project(&request);

        assert_eq!(projection.nodes.len(), keys.len());
        assert!(projection.metadata.settled);

        // Hilbert-curve placement: 8 nodes ought to produce 8 distinct
        // points along the curve.
        let mut sorted: Vec<_> = projection
            .nodes
            .iter()
            .map(|n| (n.position.x.to_bits(), n.position.y.to_bits()))
            .collect();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len(), "positions should be distinct");
    }

    #[test]
    fn project_is_deterministic_for_same_graph() {
        let (graph, _) = small_graph(6);
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = LSystemAdapter::default();
        let p1 = adapter.project(&request);
        let p2 = adapter.project(&request);
        let map1: std::collections::HashMap<NodeKey, _> =
            p1.nodes.iter().map(|n| (n.node, n.position)).collect();
        let map2: std::collections::HashMap<NodeKey, _> =
            p2.nodes.iter().map(|n| (n.node, n.position)).collect();
        assert_eq!(map1, map2);
    }

    #[test]
    fn explicit_depth_propagates_through_config() {
        let adapter = LSystemAdapter::new(LSystemConfig {
            grammar: LSystemGrammar::Hilbert,
            iteration_depth: IterationDepth::Explicit(3),
            ..LSystemConfig::default()
        });
        assert!(matches!(
            adapter.config.iteration_depth,
            IterationDepth::Explicit(3)
        ));
    }

    #[test]
    fn empty_graph_produces_empty_projection_marked_settled() {
        let graph = Graph::new();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = LSystemAdapter::default();
        let projection = adapter.project(&request);
        assert!(projection.nodes.is_empty());
        assert!(projection.metadata.settled);
    }
}
