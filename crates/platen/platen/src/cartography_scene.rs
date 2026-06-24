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

use std::collections::HashMap;

use cartography::{
    AxisValue, FormFactor, IntelligenceSignals, LayoutStrategy, ProjectionDimension,
    ProjectionRequest, StreamingLayoutStrategy, TargetSize, ViewIntent,
};
use kernel::geometry::PortablePoint;
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

/// The graph-wide layout strategies the orrery's empty-canvas picker offers:
/// `(projection_id, label)`. The force-directed default (gyre physics) is the host's
/// `None`, not listed here. These analytic adapters lay out from the node set alone,
/// needing no selection focus. Focus-driven `radial.default` is *not* here — it rides
/// the selection menu (it centers on the selected node) and dispatches through
/// [`project_orrery_strategy`] all the same. Axis- and signal-driven strategies join
/// once their inputs are plumbed.
pub const ORRERY_LAYOUT_STRATEGIES: &[(&str, &str)] = &[
    ("phyllotaxis.default", "Phyllotaxis"),
    ("grid.default", "Grid"),
    ("penrose.default", "Penrose"),
    ("lsystem.default", "L-system"),
    // Axis-driven, now dispatched: the host axes are graph-derived first-pass — kanban groups
    // by URL host, timeline orders by node-creation order. The signals layer will enrich these
    // (content-type / community columns, real timestamps). (Arrangements — kanban/timeline.)
    ("kanban.default", "Kanban (by site)"),
    ("timeline.default", "Timeline (by order)"),
];

/// The URL's host/authority — the substring between `://` and the next `/` (or end), else the
/// whole string. The kanban categorical axis groups nodes by this. (Arrangements — kanban.)
fn url_host(url: &str) -> String {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    after_scheme.split(['/', '?', '#']).next().unwrap_or(after_scheme).to_string()
}

/// Compute an orrery layout strategy's node positions: dispatch `id` to its
/// cartography adapter and project against `graph` at viewport `(width, height)`,
/// returning the `(NodeKey, position)` pairs the orrery applies through
/// [`Orrery::apply_strategy_positions`](../../orrery). Empty for an unknown or
/// not-yet-wired id (the host then leaves the layout unchanged). Only the graph-only
/// analytic strategies in [`ORRERY_LAYOUT_STRATEGIES`] are dispatched here; focus /
/// axis / signal-driven strategies join once their inputs are plumbed.
pub fn project_orrery_strategy(
    id: &str,
    graph: &Graph,
    focus: Option<NodeKey>,
    width: u32,
    height: u32,
) -> Vec<(NodeKey, PortablePoint)> {
    use arrangements::adapters::{
        GridAdapter, KanbanAdapter, LSystemAdapter, PenroseAdapter, PhyllotaxisAdapter,
        RadialAdapter, TimelineAdapter,
    };
    // The real signal snapshot from intel/signals (degree-based importance for now), replacing
    // the empty `::default()` — the producer -> snapshot -> strategy spine. Strategies that read
    // `signals.importance` now see it; the rest ignore it (additive contract). The generation +
    // dirty-bit cache that gates this per-frame recompute is the next slice. (Graph signals — P1.)
    let signals = signals::produce_cheap_signals(graph);
    let options = CartographySceneOptions::canvas_pixels(width, height);
    let projection = match id {
        "phyllotaxis.default" => {
            project_with(graph, &signals, &options, &PhyllotaxisAdapter::default())
        }
        "grid.default" => project_with(graph, &signals, &options, &GridAdapter::default()),
        "penrose.default" => project_with(graph, &signals, &options, &PenroseAdapter::default()),
        "lsystem.default" => project_with(graph, &signals, &options, &LSystemAdapter::default()),
        // Axis-driven: the host derives the per-node axis (graph-only first pass) and threads it on
        // the intent, since `axis_values` lives on `ViewIntent`, not `CartographySceneOptions`.
        // Kanban groups by URL host (a categorical column per site). (Arrangements — kanban.)
        "kanban.default" => {
            let axis = graph
                .nodes()
                .map(|(key, node)| (key, AxisValue::Categorical(url_host(node.url()))))
                .collect::<HashMap<_, _>>();
            let mut intent = options.to_view_intent();
            intent.axis_values = Some(axis);
            KanbanAdapter::default()
                .project(&ProjectionRequest { graph, signals: &signals, intent })
        }
        // Timeline orders nodes along the horizontal axis by creation order (their enumeration
        // index) — a stand-in until a real per-node timestamp is plumbed. (Arrangements — timeline.)
        "timeline.default" => {
            let axis = graph
                .nodes()
                .enumerate()
                .map(|(i, (key, _node))| (key, AxisValue::Numeric(i as f64)))
                .collect::<HashMap<_, _>>();
            let mut intent = options.to_view_intent();
            intent.axis_values = Some(axis);
            TimelineAdapter::default()
                .project(&ProjectionRequest { graph, signals: &signals, intent })
        }
        // Focus-driven: centers on `focus` (the pane's selection), BFS rings outward.
        // Without a focus there is no layout to compute, so leave the orrery as-is.
        "radial.default" => {
            if focus.is_none() {
                return Vec::new();
            }
            let focused = CartographySceneOptions { focus, ..options.clone() };
            project_with(graph, &signals, &focused, &RadialAdapter::default())
        }
        _ => return Vec::new(),
    };
    projection.nodes.iter().map(|n| (n.node, n.position)).collect()
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
    fn project_orrery_strategy_radial_centers_on_focus_and_no_ops_without_one() {
        let (graph, [a, _, _]) = triangle_graph();
        // With a focus, radial lays out the whole graph (focus at center).
        let with_focus = project_orrery_strategy("radial.default", &graph, Some(a), 800, 600);
        assert_eq!(with_focus.len(), 3, "radial projects every node around the focus");
        let focus_pos = with_focus.iter().find(|(k, _)| *k == a).unwrap().1;
        assert!(
            focus_pos.x.abs() < 0.001 && focus_pos.y.abs() < 0.001,
            "the focus sits at the radial center"
        );
        // Without a focus there is nothing to center on, so it leaves the layout alone.
        let no_focus = project_orrery_strategy("radial.default", &graph, None, 800, 600);
        assert!(no_focus.is_empty(), "radial without a selection no-ops");
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
