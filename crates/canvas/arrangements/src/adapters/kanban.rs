// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`LayoutStrategy`] adapter for [`canvas_ir::layout::Kanban`].
//!
//! Categorical-bucket layout: nodes placed by their
//! [`crate::AxisValue::Categorical`] tag into named columns.
//! Configurable column order; nodes whose tag isn't in the configured
//! order are appended to an "other" column when `include_other_column`
//! is set.
//!
//! Analytic: same input → same output in one call.

use crate::{Kanban, KanbanConfig};

use super::shared::{projection_from_positions, run_static_layout_one_shot};
use cartography::projection::Projection;
use cartography::request::ProjectionRequest;
use cartography::strategy::LayoutStrategy;

#[derive(Debug, Default, Clone)]
pub struct KanbanAdapter {
    pub config: KanbanConfig,
}

impl KanbanAdapter {
    pub const PROJECTION_ID: &'static str = "kanban.default";

    pub fn new(config: KanbanConfig) -> Self {
        Self { config }
    }
}

impl LayoutStrategy for KanbanAdapter {
    fn projection_id(&self) -> &'static str {
        Self::PROJECTION_ID
    }

    fn project(&self, request: &ProjectionRequest<'_>) -> Projection {
        let mut kanban = Kanban::new(self.config.clone());
        let positions = run_static_layout_one_shot(request, &mut kanban);
        projection_from_positions(Self::PROJECTION_ID, request, positions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cartography::request::{AxisValue, ViewIntent};
    use cartography::signals::IntelligenceSignals;
    use kernel::geometry::PortablePoint;
    use kernel::graph::{Graph, NodeKey};
    use std::collections::HashMap;
    use kernel::graph::fixtures::GraphFixtures;
    use uuid::Uuid;

    fn four_nodes() -> (Graph, [NodeKey; 4]) {
        let mut graph = Graph::new();
        let keys = [
            graph.add_node_with_id(
                Uuid::from_u128(1),
                "test://a".into(),
                PortablePoint::new(0.0, 0.0),
            ),
            graph.add_node_with_id(
                Uuid::from_u128(2),
                "test://b".into(),
                PortablePoint::new(0.0, 0.0),
            ),
            graph.add_node_with_id(
                Uuid::from_u128(3),
                "test://c".into(),
                PortablePoint::new(0.0, 0.0),
            ),
            graph.add_node_with_id(
                Uuid::from_u128(4),
                "test://d".into(),
                PortablePoint::new(0.0, 0.0),
            ),
        ];
        (graph, keys)
    }

    #[test]
    fn adapter_projection_id_is_stable() {
        let adapter = KanbanAdapter::default();
        assert_eq!(adapter.projection_id(), "kanban.default");
    }

    #[test]
    fn nodes_with_same_tag_share_a_column() {
        let (graph, [a, b, c, d]) = four_nodes();
        let mut axis_values = HashMap::new();
        // a and c in "todo"; b in "doing"; d in "done".
        axis_values.insert(a, AxisValue::Categorical("todo".into()));
        axis_values.insert(b, AxisValue::Categorical("doing".into()));
        axis_values.insert(c, AxisValue::Categorical("todo".into()));
        axis_values.insert(d, AxisValue::Categorical("done".into()));

        let signals = IntelligenceSignals::default();
        let intent = ViewIntent::default().with_axis_values(axis_values);
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent,
        };
        let adapter = KanbanAdapter::new(KanbanConfig {
            column_order: vec!["todo".into(), "doing".into(), "done".into()],
            ..KanbanConfig::default()
        });
        let projection = adapter.project(&request);

        // a and c are in the same column → same x.
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
        assert!(
            (pos_a.x - pos_c.x).abs() < 0.001,
            "a.x={} and c.x={} should match (same 'todo' column)",
            pos_a.x,
            pos_c.x
        );

        // todo (column 0) at x=0; doing (column 1) at x=column_gap=240;
        // done (column 2) at x=480.
        let pos_b = projection
            .nodes
            .iter()
            .find(|n| n.node == b)
            .unwrap()
            .position;
        let pos_d = projection
            .nodes
            .iter()
            .find(|n| n.node == d)
            .unwrap()
            .position;
        assert!(pos_a.x.abs() < 0.001);
        assert!((pos_b.x - 240.0).abs() < 0.001);
        assert!((pos_d.x - 480.0).abs() < 0.001);
    }

    #[test]
    fn unassigned_nodes_go_to_other_column_when_enabled() {
        let (graph, [a, b, _, _]) = four_nodes();
        // Only a has an axis value; the other 3 are unassigned.
        let mut axis_values = HashMap::new();
        axis_values.insert(a, AxisValue::Categorical("todo".into()));

        let signals = IntelligenceSignals::default();
        let intent = ViewIntent::default().with_axis_values(axis_values);
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent,
        };
        let adapter = KanbanAdapter::default();
        let projection = adapter.project(&request);

        // a in column 0 (the only configured column, "todo"); b in
        // column 1 (the "_other_" bucket).
        let pos_a = projection
            .nodes
            .iter()
            .find(|n| n.node == a)
            .unwrap()
            .position;
        let pos_b = projection
            .nodes
            .iter()
            .find(|n| n.node == b)
            .unwrap()
            .position;
        assert!(pos_a.x.abs() < 0.001);
        assert!(
            (pos_b.x - 240.0).abs() < 0.001,
            "expected b in _other_ column at x=240, got {}",
            pos_b.x
        );
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
        let adapter = KanbanAdapter::default();
        let projection = adapter.project(&request);
        assert!(projection.nodes.is_empty());
        assert!(projection.metadata.settled);
    }
}
