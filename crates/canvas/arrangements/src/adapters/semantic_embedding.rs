// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`LayoutStrategy`] adapter for
//! [`canvas_ir::layout::SemanticEmbedding`].
//!
//! Places nodes at host-precomputed embedding coordinates from
//! [`crate::IntelligenceSignals::embeddings`] — UMAP / t-SNE / PCA /
//! any projection the host has already run. The adapter just maps
//! `(embed.x, embed.y) → origin + scale × rotated(embed)` and emits
//! a [`Projection`] with absolute positions.
//!
//! Analytic for the cartography contract: given the same embeddings,
//! placement is deterministic and produced in one call. The math is
//! implemented directly here rather than delegating to
//! `canvas_ir::layout::SemanticEmbedding::step()` because the
//! underlying step() uses `StatelessPassState`, not the
//! `StaticLayoutState` that the shared `run_static_layout_one_shot`
//! helper takes — re-implementing the closed-form placement is
//! shorter than building a state-type adapter shim.
//!
//! ## Fallback for unembedded nodes
//!
//! Nodes whose key isn't present in [`NodeEmbeddings::coords`] get
//! placed per the [`EmbeddingFallback`] config:
//!
//! - `LeaveInPlace` — parks the node at `config.origin`. Positions are no
//!   longer graph truth (S2), so there is nothing in-graph to leave it at;
//!   a host that wants true leave-in-place supplies its current (view)
//!   positions to the arrangement.
//! - `CollapseToOrigin` — places the node at `config.origin`.
//! - `RingOutside` — places the node on a deterministic ring outside
//!   the main embedded cluster, at angle derived from `NodeKey.index()`.

use std::collections::HashMap;

use crate::{EmbeddingFallback, SemanticEmbeddingConfig};
use euclid::default::Point2D;
use kernel::graph::NodeKey;

use super::shared::projection_from_positions;
use cartography::projection::Projection;
use cartography::request::ProjectionRequest;
use cartography::strategy::LayoutStrategy;

/// Cartography-side adapter for
/// [`canvas_ir::layout::SemanticEmbedding`].
#[derive(Debug, Default, Clone)]
pub struct SemanticEmbeddingAdapter {
    pub config: SemanticEmbeddingConfig,
}

impl SemanticEmbeddingAdapter {
    pub const PROJECTION_ID: &'static str = "semantic.embedding";

    pub fn new(config: SemanticEmbeddingConfig) -> Self {
        Self { config }
    }
}

impl LayoutStrategy for SemanticEmbeddingAdapter {
    fn projection_id(&self) -> &'static str {
        Self::PROJECTION_ID
    }

    fn project(&self, request: &ProjectionRequest<'_>) -> Projection {
        let cos_r = self.config.rotation.cos();
        let sin_r = self.config.rotation.sin();
        let mut positions: HashMap<NodeKey, Point2D<f32>> = HashMap::new();

        for (key, node) in request.graph.nodes() {
            let embedding = request
                .signals
                .embeddings
                .as_ref()
                .and_then(|emb| emb.lookup(key));

            let pos = match embedding {
                Some((ex, ey)) => {
                    let sx = ex * self.config.scale;
                    let sy = ey * self.config.scale;
                    Point2D::new(
                        self.config.origin.x + sx * cos_r - sy * sin_r,
                        self.config.origin.y + sx * sin_r + sy * cos_r,
                    )
                }
                None => match self.config.fallback {
                    // Positions are no longer graph truth (S2), so there is no
                    // in-graph position to "leave in place"; an un-embedded node
                    // falls to the origin. A host that wants true leave-in-place
                    // supplies current positions to the arrangement (a follow-on).
                    EmbeddingFallback::LeaveInPlace => self.config.origin,
                    EmbeddingFallback::CollapseToOrigin => self.config.origin,
                    EmbeddingFallback::RingOutside => fallback_ring_position(key, &self.config),
                },
            };
            positions.insert(key, pos);
        }

        projection_from_positions(Self::PROJECTION_ID, request, positions)
    }
}

/// Deterministic ring placement for unembedded nodes — derives an
/// angle from the node's stable petgraph index so repeated projections
/// place the same fallback node at the same ring slot.
fn fallback_ring_position(key: NodeKey, config: &SemanticEmbeddingConfig) -> Point2D<f32> {
    // `NodeIndex::index()` is petgraph's stable u32 slot for the node.
    // We hash it into [0, 2π) so unembedded nodes spread across the
    // ring deterministically.
    let raw = key.index() as u64;
    // Mix bits so adjacent indices don't end up adjacent on the ring.
    // Splitmix64 step — cheap, deterministic, decent distribution.
    let mut h = raw.wrapping_add(0x9E3779B97F4A7C15);
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D049BB133111EB);
    h ^= h >> 31;
    let angle = (h as f32 / u64::MAX as f32) * std::f32::consts::TAU;
    let radius = config.scale * 1.5;
    Point2D::new(
        config.origin.x + radius * angle.cos(),
        config.origin.y + radius * angle.sin(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use cartography::request::ViewIntent;
    use cartography::signals::{IntelligenceSignals, NodeEmbeddings};
    use kernel::geometry::PortablePoint;
    use kernel::graph::Graph;
    use kernel::graph::fixtures::GraphFixtures;
    use uuid::Uuid;

    fn small_graph() -> (Graph, [NodeKey; 3]) {
        let mut graph = Graph::new();
        let a = graph.add_node_with_id(
            Uuid::from_u128(1),
            "test://a".into(),
            PortablePoint::new(10.0, 20.0),
        );
        let b = graph.add_node_with_id(
            Uuid::from_u128(2),
            "test://b".into(),
            PortablePoint::new(30.0, 40.0),
        );
        let c = graph.add_node_with_id(
            Uuid::from_u128(3),
            "test://c".into(),
            PortablePoint::new(50.0, 60.0),
        );
        (graph, [a, b, c])
    }

    #[test]
    fn adapter_projection_id_is_stable() {
        let adapter = SemanticEmbeddingAdapter::default();
        assert_eq!(adapter.projection_id(), "semantic.embedding");
    }

    #[test]
    fn embedded_nodes_get_scaled_origin_offset_positions() {
        let (graph, [a, b, _]) = small_graph();
        // Embeddings in [-1, 1] range, default scale = 400.
        let signals = IntelligenceSignals {
            embeddings: Some(NodeEmbeddings {
                coords: vec![(a, (0.25, 0.0)), (b, (0.0, 0.5))],
            }),
            ..Default::default()
        };
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = SemanticEmbeddingAdapter::default();
        let projection = adapter.project(&request);

        // a at (0.25, 0) × 400 + origin(0,0) = (100, 0).
        let pos_a = projection
            .nodes
            .iter()
            .find(|n| n.node == a)
            .unwrap()
            .position;
        assert!((pos_a.x - 100.0).abs() < 0.001);
        assert!(pos_a.y.abs() < 0.001);

        // b at (0, 0.5) × 400 + origin = (0, 200).
        let pos_b = projection
            .nodes
            .iter()
            .find(|n| n.node == b)
            .unwrap()
            .position;
        assert!(pos_b.x.abs() < 0.001);
        assert!((pos_b.y - 200.0).abs() < 0.001);
    }

    #[test]
    fn leave_in_place_fallback_parks_unembedded_at_the_origin() {
        let (graph, [a, b, c]) = small_graph();
        let signals = IntelligenceSignals {
            embeddings: Some(NodeEmbeddings {
                coords: vec![(a, (0.0, 0.0))], // only a embedded
            }),
            ..Default::default()
        };
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = SemanticEmbeddingAdapter::default();
        let projection = adapter.project(&request);

        // The graph carries no positions any more (S2), so LeaveInPlace has no
        // committed position to inherit: unembedded nodes park at the configured
        // origin, and the canvas's live view keeps them where they were on screen
        // (the projection consumer skips origin-parked fallback nodes).
        let nodes_by_pos: HashMap<_, _> = projection
            .nodes
            .iter()
            .map(|n| (n.node, n.position))
            .collect();
        let origin = SemanticEmbeddingConfig::default().origin;
        assert_eq!(nodes_by_pos[&b], origin, "b parks at the fallback origin");
        assert_eq!(nodes_by_pos[&c], origin, "c parks at the fallback origin");
    }

    #[test]
    fn collapse_to_origin_fallback_places_unembedded_at_origin() {
        let (graph, [a, b, c]) = small_graph();
        let signals = IntelligenceSignals {
            embeddings: Some(NodeEmbeddings {
                coords: vec![(a, (0.5, 0.5))],
            }),
            ..Default::default()
        };
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = SemanticEmbeddingAdapter::new(SemanticEmbeddingConfig {
            fallback: EmbeddingFallback::CollapseToOrigin,
            ..SemanticEmbeddingConfig::default()
        });
        let projection = adapter.project(&request);
        let pos_b = projection
            .nodes
            .iter()
            .find(|n| n.node == b)
            .unwrap()
            .position;
        let pos_c = projection
            .nodes
            .iter()
            .find(|n| n.node == c)
            .unwrap()
            .position;
        assert_eq!(pos_b, Point2D::new(0.0, 0.0));
        assert_eq!(pos_c, Point2D::new(0.0, 0.0));
    }

    #[test]
    fn ring_outside_fallback_deterministically_places_unembedded() {
        let (graph, [a, b, c]) = small_graph();
        let signals = IntelligenceSignals {
            embeddings: Some(NodeEmbeddings {
                coords: vec![(a, (0.0, 0.0))],
            }),
            ..Default::default()
        };
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = SemanticEmbeddingAdapter::new(SemanticEmbeddingConfig {
            fallback: EmbeddingFallback::RingOutside,
            ..SemanticEmbeddingConfig::default()
        });
        let p1 = adapter.project(&request);
        let p2 = adapter.project(&request);

        // Deterministic — same projection should produce same fallback
        // positions across runs.
        let map1: HashMap<_, _> = p1.nodes.iter().map(|n| (n.node, n.position)).collect();
        let map2: HashMap<_, _> = p2.nodes.iter().map(|n| (n.node, n.position)).collect();
        assert_eq!(map1, map2);

        // b and c on the ring at radius = scale × 1.5 = 600.
        let pos_b = map1[&b];
        let pos_c = map1[&c];
        let r_b = (pos_b.x * pos_b.x + pos_b.y * pos_b.y).sqrt();
        let r_c = (pos_c.x * pos_c.x + pos_c.y * pos_c.y).sqrt();
        assert!((r_b - 600.0).abs() < 0.01, "b radius={r_b}, expected 600");
        assert!((r_c - 600.0).abs() < 0.01, "c radius={r_c}, expected 600");
    }

    #[test]
    fn missing_embeddings_signal_uses_default_leave_in_place_fallback() {
        let (graph, [a, _, _]) = small_graph();
        let signals = IntelligenceSignals::default(); // no embeddings
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let adapter = SemanticEmbeddingAdapter::default();
        let projection = adapter.project(&request);
        // All nodes get the LeaveInPlace fallback (default), which parks at the
        // configured origin now that the graph carries no positions (S2).
        assert_eq!(projection.nodes.len(), 3);
        let pos_a = projection
            .nodes
            .iter()
            .find(|n| n.node == a)
            .unwrap()
            .position;
        assert_eq!(pos_a, SemanticEmbeddingConfig::default().origin);
    }
}
