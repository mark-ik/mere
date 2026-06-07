/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Cartography projection-request derivation.
//!
//! Translates platen's reducer-owned graph state into a
//! [`cartography::ProjectionRequest`] and dispatches it through a
//! chosen [`cartography::LayoutStrategy`] or
//! [`cartography::StreamingLayoutStrategy`].
//!
//! Sibling of [`crate::canvas_scene`]: where `canvas_scene` builds a
//! `CanvasSceneInput` for graph-canvas to render directly,
//! `cartography_scene` builds the higher-level `ProjectionRequest`
//! that goes through cartography's strategy contract first — the
//! eventual home for graph-view layout selection (force-directed,
//! radial, phyllotaxis, cluster-collapsed, etc.) once the host wants
//! per-pane strategy choice.
//!
//! ## Two paths, same input shape
//!
//! For new code, prefer [`project_with`] (analytic strategies) and
//! [`step_with`] (streaming strategies) over building a
//! `ProjectionRequest` by hand. They source the right `ViewIntent` /
//! `IntelligenceSignals` shape from [`CartographySceneOptions`] and
//! dispatch through the strategy in one call.
//!
//! `canvas_scene::build_canvas_scene_input` still works for the
//! existing graph-canvas-only render path; the two modules will
//! eventually share infrastructure once cartography becomes the
//! single dispatch surface, but until graph-canvas's existing direct-
//! render path is fully retired they coexist.

use cartography::{
    FormFactor, IntelligenceSignals, LayoutStrategy, ProjectionDimension, ProjectionRequest,
    StreamingLayoutStrategy, TargetSize, ViewIntent,
};
use kernel::graph::{Graph, NodeKey};

/// Inputs the host supplies for a cartography projection.
///
/// Signals default to empty (`IntelligenceSignals::default()`), so
/// callers that don't have intelligence plumbed yet pass none and the
/// projection still works — strategies that need signals (e.g.
/// `SemanticEdgeWeightAdapter`) just behave as if no signals were
/// available.
#[derive(Clone, Debug)]
pub struct CartographySceneOptions {
    pub form_factor: FormFactor,
    pub dimension: ProjectionDimension,
    pub focus: Option<NodeKey>,
    pub target_size: TargetSize,
}

impl Default for CartographySceneOptions {
    fn default() -> Self {
        Self {
            form_factor: FormFactor::Canvas,
            dimension: ProjectionDimension::TwoD,
            focus: None,
            target_size: TargetSize::Default,
        }
    }
}

impl CartographySceneOptions {
    pub fn canvas_pixels(width: u32, height: u32) -> Self {
        Self {
            target_size: TargetSize::Pixels { width, height },
            ..Self::default()
        }
    }

    pub fn minimap(width: f32, height: f32) -> Self {
        Self {
            form_factor: FormFactor::Minimap,
            target_size: TargetSize::Logical { width, height },
            ..Self::default()
        }
    }

    pub fn with_focus(mut self, focus: NodeKey) -> Self {
        self.focus = Some(focus);
        self
    }

    fn to_view_intent(&self) -> ViewIntent {
        let mut intent = ViewIntent::default();
        intent.form_factor = self.form_factor;
        intent.dimension = self.dimension;
        intent.focus = self.focus;
        intent.target_size = self.target_size;
        intent
    }
}

/// Build a [`ProjectionRequest`] borrowing the supplied graph and
/// signals.
///
/// The returned request has its `ViewIntent` constructed from
/// `options`. Callers can mutate the request before dispatching if
/// they need to set `axis_values`, `filter`, or other intent fields
/// not exposed on [`CartographySceneOptions`].
pub fn build_projection_request<'a>(
    graph: &'a Graph,
    signals: &'a IntelligenceSignals,
    options: &CartographySceneOptions,
) -> ProjectionRequest<'a> {
    ProjectionRequest {
        graph,
        signals,
        intent: options.to_view_intent(),
    }
}

/// Dispatch an analytic strategy. Convenience wrapper around
/// [`build_projection_request`] + `strategy.project(&request)`.
pub fn project_with<S: LayoutStrategy>(
    graph: &Graph,
    signals: &IntelligenceSignals,
    options: &CartographySceneOptions,
    strategy: &S,
) -> cartography::Projection {
    let request = build_projection_request(graph, signals, options);
    strategy.project(&request)
}

/// Dispatch a streaming strategy. Threads the host-owned state through
/// the call so iteration progresses across frames.
pub fn step_with<S: StreamingLayoutStrategy>(
    graph: &Graph,
    signals: &IntelligenceSignals,
    options: &CartographySceneOptions,
    strategy: &S,
    state: &mut S::State,
    dt: f32,
) -> cartography::Projection {
    let request = build_projection_request(graph, signals, options);
    strategy.step(&request, state, dt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrangements::adapters::{
        PhyllotaxisAdapter, SemanticEdgeWeightAdapter, SemanticEdgeWeightAdapterState,
    };
    use kernel::geometry::PortablePoint;
    use uuid::Uuid;

    fn triangle_graph() -> (Graph, [NodeKey; 3]) {
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
            PortablePoint::new(50.0, 86.6),
        );
        (graph, [a, b, c])
    }

    #[test]
    fn project_with_phyllotaxis_returns_three_positioned_nodes() {
        let (graph, _) = triangle_graph();
        let signals = IntelligenceSignals::default();
        let options = CartographySceneOptions::canvas_pixels(800, 600);
        let projection = project_with(&graph, &signals, &options, &PhyllotaxisAdapter::default());
        assert_eq!(projection.nodes.len(), 3);
        assert_eq!(
            projection.metadata.strategy_id.as_deref(),
            Some("phyllotaxis.default")
        );
        assert!(projection.metadata.settled);
    }

    #[test]
    fn step_with_streaming_strategy_advances_state_across_calls() {
        let (graph, _) = triangle_graph();
        let signals = IntelligenceSignals::default();
        let options = CartographySceneOptions::canvas_pixels(800, 600);
        let adapter = SemanticEdgeWeightAdapter::default();
        let mut state = SemanticEdgeWeightAdapterState::default();

        let p1 = step_with(&graph, &signals, &options, &adapter, &mut state, 1.0 / 60.0);
        assert_eq!(p1.nodes.len(), 3);
        // The streaming adapter seeds its persistent position store from
        // graph truth on the first step.
        assert!(state.initialized);
        assert_eq!(state.positions.len(), 3);

        // Second call keeps iterating the same persistent state.
        let p2 = step_with(&graph, &signals, &options, &adapter, &mut state, 1.0 / 60.0);
        assert_eq!(p2.nodes.len(), 3);
    }

    #[test]
    fn build_projection_request_threads_intent_from_options() {
        let (graph, [a, _, _]) = triangle_graph();
        let signals = IntelligenceSignals::default();
        let options = CartographySceneOptions::minimap(120.0, 90.0).with_focus(a);
        let request = build_projection_request(&graph, &signals, &options);
        assert_eq!(request.intent.form_factor, FormFactor::Minimap);
        assert_eq!(request.intent.focus, Some(a));
        assert_eq!(
            request.intent.target_size,
            TargetSize::Logical {
                width: 120.0,
                height: 90.0
            }
        );
    }

    #[test]
    fn options_default_is_canvas_form_factor() {
        let opts = CartographySceneOptions::default();
        assert_eq!(opts.form_factor, FormFactor::Canvas);
        assert_eq!(opts.dimension, ProjectionDimension::TwoD);
        assert_eq!(opts.target_size, TargetSize::Default);
    }
}
