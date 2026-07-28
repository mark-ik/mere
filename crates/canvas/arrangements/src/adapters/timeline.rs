// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`LayoutStrategy`] adapter for [`canvas_ir::layout::Timeline`].
//!
//! Numeric-axis layout: nodes placed by their
//! [`crate::AxisValue::Numeric`] entry in
//! [`crate::ViewIntent::axis_values`] along the horizontal axis. Nodes
//! sharing the same x stack vertically by `row_gap`.
//!
//! Analytic: same input → same output in one call.

use crate::{Timeline, TimelineConfig};

use super::shared::{projection_from_positions, run_static_layout_one_shot};
use cartography::projection::Projection;
use cartography::request::ProjectionRequest;
use cartography::strategy::LayoutStrategy;

#[derive(Debug, Default, Clone)]
pub struct TimelineAdapter {
    pub config: TimelineConfig,
}

impl TimelineAdapter {
    pub const PROJECTION_ID: &'static str = "timeline.default";

    pub fn new(config: TimelineConfig) -> Self {
        Self { config }
    }
}

impl LayoutStrategy for TimelineAdapter {
    fn projection_id(&self) -> &'static str {
        Self::PROJECTION_ID
    }

    fn project(&self, request: &ProjectionRequest<'_>) -> Projection {
        let mut timeline = Timeline::new(self.config.clone());
        let positions = run_static_layout_one_shot(request, &mut timeline);
        projection_from_positions(Self::PROJECTION_ID, request, positions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cartography::request::{AxisValue, ViewIntent};
    use cartography::signals::IntelligenceSignals;
    use kernel::geometry::PortablePoint;
    use kernel::graph::fixtures::GraphFixtures;
    use kernel::graph::{Graph, NodeKey};
    use std::collections::HashMap;
    use uuid::Uuid;

    fn three_nodes() -> (Graph, [NodeKey; 3]) {
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
        (graph, [a, b, c])
    }

    #[test]
    fn adapter_projection_id_is_stable() {
        let adapter = TimelineAdapter::default();
        assert_eq!(adapter.projection_id(), "timeline.default");
    }

    #[test]
    fn nodes_with_axis_values_get_placed_along_time_axis() {
        let (graph, [a, b, c]) = three_nodes();
        let mut axis_values = HashMap::new();
        axis_values.insert(a, AxisValue::Numeric(0.0));
        axis_values.insert(b, AxisValue::Numeric(0.5));
        axis_values.insert(c, AxisValue::Numeric(1.0));

        let signals = IntelligenceSignals::default();
        let intent = ViewIntent::default().with_axis_values(axis_values);
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent,
        };
        let adapter = TimelineAdapter::default();
        let projection = adapter.project(&request);

        assert_eq!(projection.nodes.len(), 3);

        // a (numeric=0) should be at axis-origin x; c (numeric=1) at
        // axis-origin + axis_length x. With default config origin=(0,0)
        // and axis_length=800, a.x ≈ 0 and c.x ≈ 800.
        let pos_a = projection
            .nodes
            .iter()
            .find(|n| n.node == a)
            .unwrap()
            .position;
        let pos_c = projection
            .nodes
            .iter()
            .find(|n| n.node == c)
            .unwrap()
            .position;
        assert!(pos_a.x.abs() < 0.1, "a.x should be near 0, got {}", pos_a.x);
        assert!(
            (pos_c.x - 800.0).abs() < 0.1,
            "c.x should be near 800, got {}",
            pos_c.x
        );
        // b (numeric=0.5) should be in between.
        let pos_b = projection
            .nodes
            .iter()
            .find(|n| n.node == b)
            .unwrap()
            .position;
        assert!(pos_b.x > pos_a.x && pos_b.x < pos_c.x);
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
        let adapter = TimelineAdapter::default();
        let projection = adapter.project(&request);
        assert!(projection.nodes.is_empty());
        assert!(projection.metadata.settled);
    }
}
