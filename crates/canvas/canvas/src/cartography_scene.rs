// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cartography projection-request derivation.
//!
//! Translates platen's reducer-owned graph state into a
//! [`cartography::ProjectionRequest`] and dispatches it through a
//! chosen [`cartography::LayoutStrategy`].
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
//! For new code, prefer [`project_with`] (analytic strategies) over
//! building a `ProjectionRequest` by hand. It sources the right
//! `ViewIntent` / `IntelligenceSignals` shape from
//! [`CartographySceneOptions`] and dispatches through the strategy in
//! one call.
//!
//! `canvas_scene::build_canvas_scene_input` still works for the
//! existing graph-canvas-only render path; the two modules will
//! eventually share infrastructure once cartography becomes the
//! single dispatch surface, but until graph-canvas's existing direct-
//! render path is fully retired they coexist.

use std::collections::{HashMap, HashSet};

use cartography::{
    AxisValue, FormFactor, IntelligenceSignals, LayoutStrategy, ProjectionDimension,
    ProjectionRequest, TargetSize, ViewIntent,
};
use kernel::geometry::PortablePoint;
use kernel::graph::{EdgeAssertion, Graph, NodeKey, SemanticSubKind};

/// Inputs the host supplies for a cartography projection.
///
/// Signals default to empty (`IntelligenceSignals::default()`), so
/// callers that don't have intelligence plumbed yet pass none and the
/// projection still works — strategies that read signals (e.g.
/// `KanbanAdapter`'s community columns, or the importance-driven size
/// encoding) just behave as if no signals were available.
#[derive(Clone, Debug)]
pub struct CartographySceneOptions {
    pub form_factor: FormFactor,
    pub dimension: ProjectionDimension,
    pub focus: Option<NodeKey>,
    pub target_size: TargetSize,
    /// Per-node visual extents `(w, h)` in px, threaded onto
    /// `ViewIntent::extents` so extent-aware strategies space placements to
    /// clear them. `None` = unmeasured (pre-P2 behavior). (Projection
    /// proofs — P2.)
    pub extents: Option<HashMap<NodeKey, (f32, f32)>>,
}

impl Default for CartographySceneOptions {
    fn default() -> Self {
        Self {
            form_factor: FormFactor::Canvas,
            dimension: ProjectionDimension::TwoD,
            focus: None,
            target_size: TargetSize::Default,
            extents: None,
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
        intent.extents = self.extents.clone();
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

/// The graph-wide layout strategies the canvas's empty-canvas picker offers:
/// `(projection_id, label)`. The force-directed default (seiche physics) is the host's
/// `None`, not listed here. These analytic adapters lay out from the node set alone,
/// needing no selection focus. Focus-driven `radial.default` is *not* here — it rides
/// the selection menu (it centers on the selected node) and dispatches through
/// [`project_canvas_strategy`] all the same. Axis- and signal-driven strategies join
/// once their inputs are plumbed.
pub const CANVAS_LAYOUT_STRATEGIES: &[(&str, &str)] = &[
    // Labels use the plain arrangement register (Spiral, not Phyllotaxis)
    // per the projection-proofs naming table; ids stay technical.
    ("phyllotaxis.default", "Spiral"),
    ("grid.default", "Grid"),
    ("spectral.default", "Spectral"),
    ("penrose.default", "Penrose"),
    ("lsystem.default", "Fractal"),
    // Axis-driven, now dispatched: the host axes are graph-derived. Columns groups by URL host
    // (`by site`) or by graph-structural **community** (`by cluster`, the Louvain partition from
    // intel/signals); timeline orders by node-creation order. (Arrangements — kanban/timeline;
    // graph signals — community to columns.)
    ("kanban.default", "Columns (by site)"),
    ("kanban.community", "Columns (by cluster)"),
    ("timeline.default", "Timeline (by order)"),
];

/// The main canvas's strategy result. Existing strategies emit positions;
/// the P3 Spiral also returns the portable score it realized.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasStrategyProjection {
    pub positions: Vec<(NodeKey, PortablePoint)>,
    pub score: Option<sceno::Score>,
}

/// The URL's host/authority — the substring between `://` and the next `/` (or end), else the
/// whole string. The kanban categorical axis groups nodes by this. (Arrangements — kanban.)
fn url_host(url: &str) -> String {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme)
        .to_string()
}

/// Dispatch `id` to its cartography adapter and project against `graph` at viewport
/// `(width, height)`, returning the full [`cartography::Projection`] (positions + edges; overlays
/// are added by [`project_canvas_lens`]). [`Projection::empty`](cartography::Projection::empty) for
/// an unknown or not-yet-wired id, or radial without a focus. Only the graph-only analytic
/// strategies in [`CANVAS_LAYOUT_STRATEGIES`] are dispatched here.
fn project_canvas_dispatch(
    id: &str,
    graph: &Graph,
    focus: Option<NodeKey>,
    width: u32,
    height: u32,
    clusters: Option<&cartography::ClusterSet>,
    extents: Option<&HashMap<NodeKey, (f32, f32)>>,
    recent_first: bool,
) -> cartography::Projection {
    use cartography::adapters::{
        GridAdapter, KanbanAdapter, LSystemAdapter, PenroseAdapter, RadialAdapter, SpectralAdapter,
        TimelineAdapter,
    };
    // The real signal snapshot from intel/signals (degree-based importance for now), replacing
    // the empty `::default()` — the producer -> snapshot -> strategy spine. Strategies that read
    // `signals.importance` now see it; the rest ignore it (additive contract). The generation +
    // dirty-bit cache that gates this per-frame recompute is the next slice. (Graph signals — P1.)
    let signals = signals::produce_cheap_signals(graph);
    let mut options = CartographySceneOptions::canvas_pixels(width, height);
    options.extents = extents.cloned();
    // P3's Spiral is the product-free score path, not an arrangements adapter.
    // Its ordinal carries local recency, its score carries measured footprints,
    // and `scenomise` is the only generic solver that realizes it.
    if id == "phyllotaxis.default" {
        return cartography::project_spiral_score(graph, extents, focus, recent_first).projection;
    }
    let projection = match id {
        "grid.default" => project_with(graph, &signals, &options, &GridAdapter::default()),
        // Spectral: positions from the graph Laplacian's smallest eigenvectors, so the layout
        // reflects connectivity (clusters separate, paths unroll). The expensive analytic layout the
        // arrangement cache covers. (Graph signals — P5.)
        "spectral.default" => project_with(graph, &signals, &options, &SpectralAdapter::default()),
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
            KanbanAdapter::default().project(&ProjectionRequest {
                graph,
                signals: &signals,
                intent,
            })
        }
        // Same kanban adapter, but the column key is the graph-structural **community** (the
        // Louvain partition) instead of the URL host — so the board groups by how the graph
        // actually clusters, not by site. Community is the expensive signal, so it is computed
        // here only when this strategy is active. (Graph signals — community to columns, P3.)
        "kanban.community" => {
            // Prefer the host's generation-gated community cache; fall back to an inline compute
            // (tests, or the first frame before the cache fills) so the board is never empty.
            let computed;
            let clusters = match clusters {
                Some(cached) => cached,
                None => {
                    computed = signals::community_louvain(graph);
                    &computed
                }
            };
            let mut axis: HashMap<NodeKey, AxisValue> = HashMap::new();
            for (i, cluster) in clusters.clusters.iter().enumerate() {
                let label = cluster
                    .label
                    .clone()
                    .unwrap_or_else(|| format!("Cluster {}", i + 1));
                for &member in &cluster.members {
                    axis.insert(member, AxisValue::Categorical(label.clone()));
                }
            }
            let mut intent = options.to_view_intent();
            intent.axis_values = Some(axis);
            KanbanAdapter::default().project(&ProjectionRequest {
                graph,
                signals: &signals,
                intent,
            })
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
            TimelineAdapter::default().project(&ProjectionRequest {
                graph,
                signals: &signals,
                intent,
            })
        }
        // Focus-driven: centers on `focus` (the pane's selection), BFS rings outward.
        // Without a focus there is no layout to compute, so leave the canvas as-is.
        "radial.default" => {
            if focus.is_none() {
                return cartography::Projection::empty();
            }
            let focused = CartographySceneOptions {
                focus,
                ..options.clone()
            };
            project_with(graph, &signals, &focused, &RadialAdapter::default())
        }
        _ => return cartography::Projection::empty(),
    };
    projection
}

/// The signal-driven **overlays** for a lens, in the cartography [`Overlay`](cartography::Overlay)
/// vocabulary: a `ClusterHalo` per community (in cluster order, so a consumer's colour index matches
/// the main view) and a `BridgeEmphasis` per structural broker. **Position-independent** — overlays
/// carry only node references, so a consumer paints them at whatever positions it renders (the gloss
/// at its lens positions, the main view at its live positions). The overlays are the same regardless
/// of the layout strategy, which is why this is a pure function of the signals, not the projection.
/// (Graph signals — P6b, the overlay pipe.)
pub fn signal_overlays(
    clusters: Option<&cartography::ClusterSet>,
    bridges: Option<&cartography::BridgeNodes>,
) -> Vec<cartography::Overlay> {
    use cartography::Overlay;
    let mut overlays = Vec::new();
    if let Some(set) = clusters {
        for cluster in &set.clusters {
            overlays.push(Overlay::ClusterHalo {
                cluster_id: cluster.id.clone(),
                members: cluster.members.clone(),
                label: cluster.label.clone(),
                confidence: cluster.confidence,
            });
        }
    }
    if let Some(b) = bridges {
        for &node in &b.bridges {
            overlays.push(Overlay::BridgeEmphasis { node, weight: 1.0 });
        }
    }
    overlays
}

/// Compute an canvas layout strategy's full [`cartography::Projection`] — positions **plus**
/// signal-driven [`overlays`](cartography::Projection::overlays) (community halos + bridge emphasis,
/// from [`signal_overlays`]). The gloss is the first consumer of the overlay channel: it paints the
/// overlays at its own lens positions, so a second lens can show the clusters/brokers under a
/// different layout than the main view. Positions-only callers use [`project_canvas_strategy`].
/// (Graph signals — P6b, the overlay pipe.)
pub fn project_canvas_lens(
    id: &str,
    graph: &Graph,
    focus: Option<NodeKey>,
    width: u32,
    height: u32,
    clusters: Option<&cartography::ClusterSet>,
    bridges: Option<&cartography::BridgeNodes>,
) -> cartography::Projection {
    let mut projection =
        project_canvas_dispatch(id, graph, focus, width, height, clusters, None, false);
    projection.overlays = signal_overlays(clusters, bridges);
    projection
}

/// Project a layout strategy over the **induced subgraph** of `scope` (the scoped nodes plus the
/// relations whose endpoints are both in scope), returning positions keyed by the **original**
/// graph's `NodeKey`s. Lets the gloss lens lay a selection out by *its own* structure (e.g. a
/// spectral of just the selection) rather than cropping the whole-graph layout. Nodes are re-added
/// by their stable id, so each result position remaps back to the live graph; the induced edges are
/// added once per pair (topology — multiplicity is a later fidelity step). (Graph signals — P6c, the
/// gloss subgraph re-layout.)
pub fn project_canvas_subgraph(
    graph: &Graph,
    scope: &[NodeKey],
    id: &str,
    focus: Option<NodeKey>,
    width: u32,
    height: u32,
) -> Vec<(NodeKey, PortablePoint)> {
    let scope_set: HashSet<NodeKey> = scope.iter().copied().collect();
    let mut sub = Graph::new();
    // Re-add each scoped node by its stable id (so positions remap back). Positions are
    // no longer graph truth (S2), so the induced subgraph carries none — the layout
    // strategy re-derives them from structure. A missing node is simply skipped.
    for &key in scope {
        if let Some(node) = graph.get_node(key) {
            let _ = kernel::graph::apply::add_node(
                &mut sub,
                Some(node.id),
                node.url().to_string(),
                PortablePoint::zero(),
            );
        }
    }
    // Induced edges: one per scoped pair (no self-loops), as a plain hyperlink so `relations()` (the
    // spring topology the layouts read) sees it.
    let mut seen: HashSet<(NodeKey, NodeKey)> = HashSet::new();
    for r in graph.relations() {
        if r.from == r.to || !scope_set.contains(&r.from) || !scope_set.contains(&r.to) {
            continue;
        }
        let pair = if r.from <= r.to {
            (r.from, r.to)
        } else {
            (r.to, r.from)
        };
        if !seen.insert(pair) {
            continue;
        }
        let (Some(fa), Some(fb)) = (graph.get_node(r.from), graph.get_node(r.to)) else {
            continue;
        };
        let (Some(sa), Some(sb)) = (sub.get_node_key_by_id(fa.id), sub.get_node_key_by_id(fb.id))
        else {
            continue;
        };
        let _ = kernel::graph::apply::assert_relation(
            &mut sub,
            sa,
            sb,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Hyperlink,
                label: None,
                decay_progress: None,
            },
        );
    }
    // Map the focus into the subgraph (only if it is in scope).
    let sub_focus = focus
        .and_then(|f| graph.get_node(f))
        .and_then(|n| sub.get_node_key_by_id(n.id));
    // Project the subgraph, then remap each position back to the original graph via the node id.
    // Extents are keyed by the original graph's NodeKeys, which do not survive the subgraph
    // re-add, so the subgraph path stays unmeasured for now; recency ordering likewise off.
    project_canvas_strategy(id, &sub, sub_focus, width, height, None, None, false)
        .into_iter()
        .filter_map(|(sub_key, pos)| {
            let nid = sub.get_node(sub_key)?.id;
            let main_key = graph.get_node_key_by_id(nid)?;
            Some((main_key, pos))
        })
        .collect()
}

/// Compute an canvas layout strategy's node positions: the `(NodeKey, position)` pairs the canvas
/// applies through [`Canvas::apply_strategy_positions`](../../canvas). Empty for an unknown or
/// not-yet-wired id (the host then leaves the layout unchanged). The positions-only path the main
/// view uses; the gloss uses [`project_canvas_lens`] when it also needs the overlay channel.
pub fn project_canvas_strategy(
    id: &str,
    graph: &Graph,
    focus: Option<NodeKey>,
    width: u32,
    height: u32,
    clusters: Option<&cartography::ClusterSet>,
    extents: Option<&HashMap<NodeKey, (f32, f32)>>,
    recent_first: bool,
) -> Vec<(NodeKey, PortablePoint)> {
    project_canvas_dispatch(
        id,
        graph,
        focus,
        width,
        height,
        clusters,
        extents,
        recent_first,
    )
    .nodes
    .iter()
    .map(|n| (n.node, n.position))
    .collect()
}

/// Like [`project_canvas_strategy`], with the product-free score when the
/// selected strategy has one. Mere builds the score; `scenomise` alone solves
/// its placement.
pub fn project_canvas_strategy_with_score(
    id: &str,
    graph: &Graph,
    focus: Option<NodeKey>,
    width: u32,
    height: u32,
    clusters: Option<&cartography::ClusterSet>,
    extents: Option<&HashMap<NodeKey, (f32, f32)>>,
    recent_first: bool,
) -> CanvasStrategyProjection {
    project_canvas_strategy_with_score_for_view(
        id,
        graph,
        focus,
        width,
        height,
        clusters,
        extents,
        recent_first,
        1.0,
        None,
    )
}

/// Like [`project_canvas_strategy_with_score`], evaluated for the host's
/// current zoom and prior score. These two view facts affect only the selected
/// representation rung and its hysteresis; placement remains score-driven.
pub fn project_canvas_strategy_with_score_for_view(
    id: &str,
    graph: &Graph,
    focus: Option<NodeKey>,
    width: u32,
    height: u32,
    clusters: Option<&cartography::ClusterSet>,
    extents: Option<&HashMap<NodeKey, (f32, f32)>>,
    recent_first: bool,
    zoom_level: f32,
    previous_score: Option<&sceno::Score>,
) -> CanvasStrategyProjection {
    if id == "phyllotaxis.default" {
        let result = cartography::project_spiral_score_for_view(
            graph,
            extents,
            focus,
            recent_first,
            zoom_level,
            previous_score,
        );
        return CanvasStrategyProjection {
            positions: result
                .projection
                .nodes
                .iter()
                .map(|node| (node.node, node.position))
                .collect(),
            score: Some(result.score),
        };
    }
    CanvasStrategyProjection {
        positions: project_canvas_strategy(
            id,
            graph,
            focus,
            width,
            height,
            clusters,
            extents,
            recent_first,
        ),
        score: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::geometry::PortablePoint;
    use kernel::graph::fixtures::GraphFixtures;
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
    fn project_spiral_uses_the_portable_score_solver() {
        let (graph, _) = triangle_graph();
        let projection = project_canvas_dispatch(
            "phyllotaxis.default",
            &graph,
            None,
            800,
            600,
            None,
            None,
            false,
        );
        assert_eq!(projection.nodes.len(), 3);
        assert_eq!(
            projection.metadata.strategy_id.as_deref(),
            Some("phyllotaxis.default")
        );
        assert!(projection.metadata.settled);
    }

    #[test]
    fn kanban_community_groups_each_cluster_into_its_own_column() {
        // Two triangles {0,1,2} and {3,4,5} joined by one bridge edge => two Louvain communities,
        // so the cluster-kanban board lays them out as two columns.
        let mut graph = Graph::new();
        let n: Vec<NodeKey> = (0..6)
            .map(|i| graph.add_node(format!("test://{i}"), PortablePoint::new(i as f32, 0.0)))
            .collect();
        for &(a, b) in &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)] {
            graph.assert_semantic_predicate(n[a], n[b], "links".to_string());
        }
        let positions = project_canvas_strategy(
            "kanban.community",
            &graph,
            None,
            800,
            600,
            None,
            None,
            false,
        );
        assert_eq!(positions.len(), 6, "every node is placed");
        let x_of = |key: NodeKey| positions.iter().find(|(k, _)| *k == key).unwrap().1.x;
        assert!(
            (x_of(n[0]) - x_of(n[1])).abs() < 0.001,
            "triangle A shares one column"
        );
        assert!((x_of(n[0]) - x_of(n[2])).abs() < 0.001);
        assert!(
            (x_of(n[3]) - x_of(n[4])).abs() < 0.001,
            "triangle B shares one column"
        );
        assert!(
            (x_of(n[0]) - x_of(n[3])).abs() > 0.001,
            "the two communities are distinct columns"
        );
    }

    #[test]
    fn project_canvas_strategy_radial_centers_on_focus_and_no_ops_without_one() {
        let (graph, [a, _, _]) = triangle_graph();
        // With a focus, radial lays out the whole graph (focus at center).
        let with_focus = project_canvas_strategy(
            "radial.default",
            &graph,
            Some(a),
            800,
            600,
            None,
            None,
            false,
        );
        assert_eq!(
            with_focus.len(),
            3,
            "radial projects every node around the focus"
        );
        let focus_pos = with_focus.iter().find(|(k, _)| *k == a).unwrap().1;
        assert!(
            focus_pos.x.abs() < 0.001 && focus_pos.y.abs() < 0.001,
            "the focus sits at the radial center"
        );
        // Without a focus there is nothing to center on, so it leaves the layout alone.
        let no_focus =
            project_canvas_strategy("radial.default", &graph, None, 800, 600, None, None, false);
        assert!(no_focus.is_empty(), "radial without a selection no-ops");
    }

    #[test]
    fn signal_overlays_builds_halos_in_cluster_order_then_bridge_emphasis() {
        use cartography::{BridgeNodes, Cluster, ClusterSet, Overlay};
        let (a, b, c) = (NodeKey::new(0), NodeKey::new(1), NodeKey::new(2));
        let clusters = ClusterSet {
            clusters: vec![
                Cluster {
                    id: "c0".into(),
                    label: Some("A".into()),
                    members: vec![a, b],
                    confidence: 1.0,
                },
                Cluster {
                    id: "c1".into(),
                    label: None,
                    members: vec![c],
                    confidence: 1.0,
                },
            ],
        };
        let bridges = BridgeNodes { bridges: vec![b] };
        let overlays = signal_overlays(Some(&clusters), Some(&bridges));
        assert_eq!(overlays.len(), 3, "two halos + one bridge emphasis");
        assert!(
            matches!(&overlays[0], Overlay::ClusterHalo { members, .. } if *members == vec![a, b]),
            "the first halo is the first cluster (order preserved for colour matching)"
        );
        assert!(
            matches!(&overlays[1], Overlay::ClusterHalo { cluster_id, .. } if cluster_id == "c1")
        );
        assert!(matches!(&overlays[2], Overlay::BridgeEmphasis { node, .. } if *node == b));
    }

    #[test]
    fn signal_overlays_empty_without_signals() {
        assert!(
            signal_overlays(None, None).is_empty(),
            "no signals => no overlays"
        );
    }

    #[test]
    fn project_canvas_lens_carries_the_community_halos() {
        // Two triangles + a bridge: the lens projection carries the two community halos on the
        // overlay channel, so the gloss can paint the clusters at its own (spectral) positions.
        let mut graph = Graph::new();
        let n: Vec<NodeKey> = (0..6)
            .map(|i| graph.add_node(format!("test://{i}"), PortablePoint::new(i as f32, 0.0)))
            .collect();
        for &(a, b) in &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)] {
            graph.assert_semantic_predicate(n[a], n[b], "links".to_string());
        }
        let clusters = signals::community_louvain(&graph);
        let lens = project_canvas_lens(
            "spectral.default",
            &graph,
            None,
            800,
            600,
            Some(&clusters),
            None,
        );
        assert_eq!(lens.nodes.len(), 6, "spectral lays out every node");
        let halos = lens
            .overlays
            .iter()
            .filter(|o| matches!(o, cartography::Overlay::ClusterHalo { .. }))
            .count();
        assert_eq!(halos, 2, "two communities => two halos ride the projection");
        // The positions-only path is unchanged (it drops the overlays).
        let positions = project_canvas_strategy(
            "spectral.default",
            &graph,
            None,
            800,
            600,
            Some(&clusters),
            None,
            false,
        );
        assert_eq!(
            positions.len(),
            6,
            "the positions wrapper still returns every node"
        );
    }

    #[test]
    fn project_canvas_subgraph_lays_out_only_the_scope() {
        // Path a-b-c-d; scope to {a, b}. The subgraph projection returns positions only for the
        // scoped nodes, keyed by the ORIGINAL graph's keys (remapped via stable id). (P6c.)
        let mut graph = Graph::new();
        let n: Vec<NodeKey> = (0..4)
            .map(|i| graph.add_node(format!("test://{i}"), PortablePoint::new(i as f32, 0.0)))
            .collect();
        for &(a, b) in &[(0, 1), (1, 2), (2, 3)] {
            graph.assert_relation(
                n[a],
                n[b],
                EdgeAssertion::Semantic {
                    sub_kind: SemanticSubKind::Hyperlink,
                    label: None,
                    decay_progress: None,
                },
            );
        }
        let scope = vec![n[0], n[1]];
        let positions = project_canvas_subgraph(&graph, &scope, "spectral.default", None, 800, 600);
        assert_eq!(positions.len(), 2, "only the scoped nodes are laid out");
        let keys: HashSet<NodeKey> = positions.iter().map(|(k, _)| *k).collect();
        assert!(
            keys.contains(&n[0]) && keys.contains(&n[1]),
            "positions are keyed by the original graph keys"
        );
        assert!(
            !keys.contains(&n[2]) && !keys.contains(&n[3]),
            "out-of-scope nodes are absent from the lens"
        );
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
