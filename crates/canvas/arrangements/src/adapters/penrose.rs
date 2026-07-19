// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`LayoutStrategy`] adapter for [`canvas_ir::layout::Penrose`].
//!
//! Analytic: nodes placed at vertices of a Penrose aperiodic tiling
//! (P2 kite-dart or P3 rhombus). The tiling is non-periodic — it
//! never exactly repeats — but locally structured. Useful for
//! "more than grid, less than organic" UIs where users need spatial
//! memory of where things are: each region looks distinctive at the
//! local scale despite never repeating globally.

use crate::{Penrose, PenroseConfig};

use super::shared::{projection_from_positions, run_static_layout_one_shot};
use cartography::projection::Projection;
use cartography::request::ProjectionRequest;
use cartography::strategy::LayoutStrategy;

/// Cartography-side adapter for [`canvas_ir::layout::Penrose`].
#[derive(Debug, Default, Clone)]
pub struct PenroseAdapter {
    pub config: PenroseConfig,
}

impl PenroseAdapter {
    pub const PROJECTION_ID: &'static str = "penrose.default";

    pub fn new(config: PenroseConfig) -> Self {
        Self { config }
    }
}

impl LayoutStrategy for PenroseAdapter {
    fn projection_id(&self) -> &'static str {
        Self::PROJECTION_ID
    }

    fn project(&self, request: &ProjectionRequest<'_>) -> Projection {
        let mut penrose = Penrose::new(self.config.clone());
        let positions = run_static_layout_one_shot(request, &mut penrose);
        projection_from_positions(Self::PROJECTION_ID, request, positions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeAssignmentStrategy, PenroseVariant, SubdivisionCount, UnusedVertexPolicy};
    use cartography::request::ViewIntent;
    use cartography::signals::IntelligenceSignals;
    use euclid::default::Point2D;
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
        let adapter = PenroseAdapter::default();
        assert_eq!(adapter.projection_id(), "penrose.default");
    }

    #[test]
    fn project_places_every_node_with_distinct_positions() {
        let (graph, keys) = small_graph(10);
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = PenroseAdapter::default();
        let projection = adapter.project(&request);
        assert_eq!(projection.nodes.len(), keys.len());
        assert!(projection.metadata.settled);

        // Penrose vertices are aperiodic — no two share a position.
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
        let (graph, _) = small_graph(7);
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = PenroseAdapter::default();
        let p1 = adapter.project(&request);
        let p2 = adapter.project(&request);
        let map1: std::collections::HashMap<NodeKey, _> =
            p1.nodes.iter().map(|n| (n.node, n.position)).collect();
        let map2: std::collections::HashMap<NodeKey, _> =
            p2.nodes.iter().map(|n| (n.node, n.position)).collect();
        assert_eq!(map1, map2);
    }

    #[test]
    fn custom_center_propagates() {
        let adapter = PenroseAdapter::new(PenroseConfig {
            variant: PenroseVariant::default(),
            subdivision_count: SubdivisionCount::Explicit(3),
            assignment: NodeAssignmentStrategy::default(),
            unused_vertices: UnusedVertexPolicy::default(),
            center: Point2D::new(500.0, 500.0),
            tile_scale: 300.0,
        });
        assert_eq!(adapter.config.center.x, 500.0);
        assert_eq!(adapter.config.tile_scale, 300.0);
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
        let adapter = PenroseAdapter::default();
        let projection = adapter.project(&request);
        assert!(projection.nodes.is_empty());
        assert!(projection.metadata.settled);
    }
}
