/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! [`LayoutStrategy`] adapter for [`graph_canvas::layout::Grid`].
//!
//! Analytic: places nodes in a `columns × rows` arrangement at fixed
//! pixel intervals. Selectable column-count strategy (Auto / Explicit
//! / AspectRatio) and traversal order (RowMajor / ColumnMajor /
//! Snaking / Spiral). Use for structured note-taking views, list
//! mode, or tile-style overviews.

use crate::{Grid, GridConfig};

use super::shared::{projection_from_positions, run_static_layout_one_shot};
use cartography::projection::Projection;
use cartography::request::ProjectionRequest;
use cartography::strategy::LayoutStrategy;

/// Cartography-side adapter for [`graph_canvas::layout::Grid`].
#[derive(Debug, Default, Clone, Copy)]
pub struct GridAdapter {
    pub config: GridConfig,
}

impl GridAdapter {
    pub const PROJECTION_ID: &'static str = "grid.default";

    pub fn new(config: GridConfig) -> Self {
        Self { config }
    }
}

impl LayoutStrategy for GridAdapter {
    fn projection_id(&self) -> &'static str {
        Self::PROJECTION_ID
    }

    fn project(&self, request: &ProjectionRequest<'_>) -> Projection {
        let mut grid = Grid::new(self.config);
        let positions = run_static_layout_one_shot(request, &mut grid);
        projection_from_positions(Self::PROJECTION_ID, request, positions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cartography::request::ViewIntent;
    use cartography::signals::IntelligenceSignals;
    use euclid::default::Point2D;
    use crate::{GridColumns, GridTraversal};
    use mere_kernel::geometry::PortablePoint;
    use mere_kernel::graph::{Graph, NodeKey};
    use uuid::Uuid;

    fn nine_node_graph() -> (Graph, Vec<NodeKey>) {
        let mut graph = Graph::new();
        let keys: Vec<NodeKey> = (0u128..9)
            .map(|i| {
                graph.add_node_with_id(
                    Uuid::from_u128(i + 1),
                    format!("test://n{i}"),
                    PortablePoint::new(0.0, 0.0),
                )
            })
            .collect();
        (graph, keys)
    }

    #[test]
    fn adapter_projection_id_is_stable() {
        let adapter = GridAdapter::default();
        assert_eq!(adapter.projection_id(), "grid.default");
    }

    #[test]
    fn project_places_every_node() {
        let (graph, keys) = nine_node_graph();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = GridAdapter::default();
        let projection = adapter.project(&request);
        assert_eq!(projection.nodes.len(), keys.len());
        assert!(projection.metadata.settled);
        assert_eq!(
            projection.metadata.strategy_id.as_deref(),
            Some("grid.default")
        );
    }

    #[test]
    fn auto_columns_yields_square_grid() {
        // 9 nodes with Auto columns → 3 columns × 3 rows. With default
        // gap=120.0 and origin=(0,0), cell (col, row) sits at
        // (col*120, row*120). Cell 0 at origin; cell 8 at (240, 240).
        let (graph, keys) = nine_node_graph();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = GridAdapter::default();
        let projection = adapter.project(&request);

        let first = projection
            .nodes
            .iter()
            .find(|n| n.node == keys[0])
            .unwrap()
            .position;
        let last = projection
            .nodes
            .iter()
            .find(|n| n.node == keys[8])
            .unwrap()
            .position;
        assert!(first.x.abs() < 0.001 && first.y.abs() < 0.001);
        assert!((last.x - 240.0).abs() < 0.001 && (last.y - 240.0).abs() < 0.001);
    }

    #[test]
    fn explicit_columns_overrides_auto() {
        // 9 nodes with 2 columns → 2 columns × ceil(9/2)=5 rows. Last
        // cell index 8 at (col=0, row=4) → (0, 480).
        let (graph, keys) = nine_node_graph();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = GridAdapter::new(GridConfig {
            columns: GridColumns::Explicit(2),
            ..GridConfig::default()
        });
        let projection = adapter.project(&request);
        let last = projection
            .nodes
            .iter()
            .find(|n| n.node == keys[8])
            .unwrap()
            .position;
        assert!(last.x.abs() < 0.001);
        assert!((last.y - 480.0).abs() < 0.001, "expected y=480, got {}", last.y);
    }

    #[test]
    fn custom_gap_and_origin_propagate() {
        let (graph, keys) = nine_node_graph();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = GridAdapter::new(GridConfig {
            gap: 50.0,
            origin: Point2D::new(100.0, 200.0),
            columns: GridColumns::Explicit(3),
            traversal: GridTraversal::RowMajor,
        });
        let projection = adapter.project(&request);
        let first = projection
            .nodes
            .iter()
            .find(|n| n.node == keys[0])
            .unwrap()
            .position;
        // Cell (0, 0) at origin (100, 200).
        assert!((first.x - 100.0).abs() < 0.001);
        assert!((first.y - 200.0).abs() < 0.001);

        // Cell (2, 2) — last of a 3x3 — at (100 + 2*50, 200 + 2*50).
        let last = projection
            .nodes
            .iter()
            .find(|n| n.node == keys[8])
            .unwrap()
            .position;
        assert!((last.x - 200.0).abs() < 0.001);
        assert!((last.y - 300.0).abs() < 0.001);
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
        let adapter = GridAdapter::default();
        let projection = adapter.project(&request);
        assert!(projection.nodes.is_empty());
        assert!(projection.metadata.settled);
    }

    #[test]
    fn graph_truth_positions_are_never_mutated() {
        let (graph, keys) = nine_node_graph();
        let original = graph.get_node(keys[0]).unwrap().projected_position();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = GridAdapter::default();
        for _ in 0..5 {
            adapter.project(&request);
        }
        let after = graph.get_node(keys[0]).unwrap().projected_position();
        assert_eq!(original, after);
    }
}
