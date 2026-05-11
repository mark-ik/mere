/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Dual strategy contract — analytic + streaming.
//!
//! Cartography exposes **two** strategy traits because two kinds of
//! layout exist:
//!
//! - **Analytic** ([`LayoutStrategy`]) — one-shot, stateless from
//!   cartography's perspective. Phyllotaxis, Penrose, Radial, Grid,
//!   Timeline, Kanban, L-system, ClusterCollapsed (astroid).
//!   `project()` produces a final projection in one call.
//!
//! - **Streaming** ([`StreamingLayoutStrategy`]) — iterative,
//!   state-carrying. ForceDirected, BarnesHut, SemanticEmbedding,
//!   any algorithm that converges over multiple frames. The canvas
//!   calls `step()` each frame, threads mutable state, and stops when
//!   `is_converged()` reports true.
//!
//! Strategies pick which trait fits their algorithm. Canvases that
//! support iteration call `step()` on streaming strategies and
//! `project()` on analytic ones. Both emit the same [`Projection`]
//! output type so canvases consume one shape uniformly.

use serde::{Deserialize, Serialize};

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

/// Iterative, state-carrying projection.
///
/// `step()` advances the strategy's state by `dt` seconds and returns
/// a [`Projection`] reflecting the strategy's best output so far.
/// Canvases call `step()` each render frame until `is_converged()`
/// reports true.
///
/// **State threading**: state is an associated type, not held by the
/// strategy itself. The host carries state across frames (persists it
/// across sessions, snapshots it for undo, etc.). Strategy instances
/// remain configuration-shaped (`&self`), not stateful actors.
///
/// **Mirror of `graph-canvas::layout::Layout<N>`**: this trait is the
/// cartography-side contract that graph-canvas's existing iterative
/// `Layout<N>` impls (ForceDirected, BarnesHut, SemanticEmbedding,
/// SemanticEdgeWeight) conform to once the `graph-layout` extraction
/// lands. The shape is intentionally analogous: associated `State`,
/// `step()`, optional `is_converged()`. The differences:
///
/// - Cartography's `step()` returns a full [`Projection`], not just
///   position deltas — so streaming and analytic strategies emit the
///   same output type and canvases consume one shape uniformly.
/// - Cartography's `step()` reads a [`ProjectionRequest`] (graph +
///   signals + intent), not a `CanvasSceneInput` + viewport +
///   `LayoutExtras`. The narrower bundle lives in cartography so
///   strategies don't depend on canvas types.
pub trait StreamingLayoutStrategy {
    /// Serializable persistent state for this strategy.
    type State: Default + Clone + Serialize + for<'de> Deserialize<'de>;

    /// Stable string identifier — same convention as
    /// [`LayoutStrategy::projection_id`].
    fn projection_id(&self) -> &'static str;

    /// Advance one frame. `state` is the host-owned state from the
    /// previous frame (or `Self::State::default()` on the first
    /// call). Mutate it in place; return the strategy's best
    /// projection given that state.
    fn step(
        &self,
        request: &ProjectionRequest<'_>,
        state: &mut Self::State,
        dt: f32,
    ) -> Projection;

    /// True when the strategy considers `state` settled — no further
    /// useful change expected from additional `step()` calls. Default:
    /// never converged (caller drives explicit stop). Strategies that
    /// converge (force-directed, semantic-embedding annealing) should
    /// override.
    fn is_converged(&self, _state: &Self::State) -> bool {
        false
    }

    /// Convenience: construct default state and run one step. Useful
    /// for first-frame initialization and for one-shot callers that
    /// don't care about iteration.
    fn project_initial(&self, request: &ProjectionRequest<'_>) -> (Projection, Self::State) {
        let mut state = Self::State::default();
        let projection = self.step(request, &mut state, 0.0);
        (projection, state)
    }
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

    /// Streaming strategy that converges after a fixed number of frames.
    struct CountdownStrategy {
        target_frames: u32,
    }

    #[derive(Clone, Default, Serialize, Deserialize)]
    struct CountdownState {
        frame: u32,
    }

    impl StreamingLayoutStrategy for CountdownStrategy {
        type State = CountdownState;

        fn projection_id(&self) -> &'static str {
            "countdown.test"
        }

        fn step(
            &self,
            _request: &ProjectionRequest<'_>,
            state: &mut Self::State,
            _dt: f32,
        ) -> Projection {
            state.frame += 1;
            let settled = state.frame >= self.target_frames;
            Projection {
                metadata: ProjectionMetadata {
                    strategy_id: Some(self.projection_id().to_string()),
                    settled,
                },
                ..Projection::empty()
            }
        }

        fn is_converged(&self, state: &Self::State) -> bool {
            state.frame >= self.target_frames
        }
    }

    #[test]
    fn noop_strategy_returns_strategy_id_in_metadata() {
        let graph = mere_kernel::graph::Graph::new();
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

    #[test]
    fn streaming_strategy_advances_state_and_converges_after_target_frames() {
        let graph = mere_kernel::graph::Graph::new();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let strategy = CountdownStrategy { target_frames: 3 };
        let mut state = CountdownState::default();

        let p1 = strategy.step(&request, &mut state, 1.0 / 60.0);
        assert_eq!(state.frame, 1);
        assert!(!strategy.is_converged(&state));
        assert!(!p1.metadata.settled);

        while !strategy.is_converged(&state) {
            let _ = strategy.step(&request, &mut state, 1.0 / 60.0);
        }
        assert_eq!(state.frame, 3);
        assert!(strategy.is_converged(&state));
    }

    #[test]
    fn streaming_strategy_project_initial_builds_default_state_and_steps_once() {
        let graph = mere_kernel::graph::Graph::new();
        let signals = IntelligenceSignals::default();
        let request = ProjectionRequest {
            graph: &graph,
            signals: &signals,
            intent: ViewIntent::default(),
        };
        let strategy = CountdownStrategy { target_frames: 5 };
        let (projection, state) = strategy.project_initial(&request);

        assert_eq!(state.frame, 1, "project_initial should run exactly one step");
        assert_eq!(
            projection.metadata.strategy_id.as_deref(),
            Some("countdown.test")
        );
    }

    #[test]
    fn streaming_strategy_default_is_converged_returns_false() {
        struct DefaultStrategy;
        impl StreamingLayoutStrategy for DefaultStrategy {
            type State = CountdownState;
            fn projection_id(&self) -> &'static str {
                "default.test"
            }
            fn step(
                &self,
                _request: &ProjectionRequest<'_>,
                _state: &mut Self::State,
                _dt: f32,
            ) -> Projection {
                Projection::empty()
            }
        }
        let state = CountdownState { frame: 9999 };
        assert!(!DefaultStrategy.is_converged(&state));
    }
}
