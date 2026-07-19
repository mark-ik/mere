// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The analytic strategy contract.
//!
//! Cartography exposes the [`LayoutStrategy`] trait: one-shot, stateless
//! from cartography's perspective. Phyllotaxis, Penrose, Radial, Grid,
//! Timeline, Kanban, L-system, Spectral, SemanticEmbedding —
//! `project()` produces a final projection in one call. Live force
//! physics (force-directed, the affinity force) is seiche's domain, not an
//! analytic strategy; the old streaming-strategy contract was retired
//! with the `SemanticEdgeWeight` projection once the seiche affinity force
//! reached parity.

use crate::projection::Projection;
use crate::request::ProjectionRequest;

/// One-shot, analytic projection from `(Graph, IntelligenceSignals,
/// ViewIntent)` to a [`Projection`].
///
/// Implementations live in sibling crates:
///
/// - `graph-layout` — Phyllotaxis, Penrose, Radial, Grid, Timeline,
///   Kanban, L-system, ClusterCollapsed (astroid). Any algorithm
///   whose output is determined entirely by its input.
/// - `document-layout` — outline-based document minimap projections
///   (lands when document-canvas minimaps materialize).
///
/// Strategies should be **deterministic** for a given input — the
/// same `(Graph, IntelligenceSignals, ViewIntent)` triple should
/// produce the same projection. Non-determinism (seeded annealing,
/// stochastic placement) is allowed only if the strategy threads its
/// own seed through `ViewIntent` or `Projection::metadata`.
pub trait LayoutStrategy {
    /// Stable string identifier for this strategy. Used for
    /// serialization and for picking strategies by name (e.g.
    /// user-pinned strategy on a per-view basis).
    ///
    /// Convention: `"<family>.<variant>"` — e.g.
    /// `"force_directed.default"`, `"radial.volvelle"`,
    /// `"cluster_collapsed.astroid"`.
    fn projection_id(&self) -> &'static str;

    /// Compute a projection for `request`. May allocate. Should not
    /// panic on empty input — return [`Projection::empty`].
    fn project(&self, request: &ProjectionRequest<'_>) -> Projection;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::ProjectionMetadata;
    use crate::request::ViewIntent;
    use crate::signals::IntelligenceSignals;

    struct NoopStrategy;

    impl LayoutStrategy for NoopStrategy {
        fn projection_id(&self) -> &'static str {
            "noop.test"
        }

        fn project(&self, _request: &ProjectionRequest<'_>) -> Projection {
            Projection {
                metadata: ProjectionMetadata {
                    strategy_id: Some(self.projection_id().to_string()),
                    settled: true,
                },
                ..Projection::empty()
            }
        }
    }

    #[test]
    fn noop_strategy_returns_strategy_id_in_metadata() {
        let graph = kernel::graph::Graph::new();
        let signals = IntelligenceSignals::default();
        let intent = ViewIntent::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent,
        };
        let strategy = NoopStrategy;
        let projection = strategy.project(&request);
        assert_eq!(
            projection.metadata.strategy_id.as_deref(),
            Some("noop.test")
        );
        assert!(projection.metadata.settled);
    }
}
