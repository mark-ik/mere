/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shared helpers used across graph-canvas adapters.
//!
//! - [`build_layout_extras`] — bridge from cartography's
//!   [`crate::IntelligenceSignals`] to graph-canvas's `LayoutExtras<N>`.
//!   Used by streaming adapters that delegate to existing
//!   `Layout::step()` calls.
//! - [`build_scene_input`] — assemble a `CanvasSceneInput<NodeKey>`
//!   from a graph + a working position map.
//! - [`build_viewport`] — assemble a `CanvasViewport` from a request's
//!   `target_size`.
//! - [`bounds_of`] — axis-aligned bounding box around a set of
//!   positions, for `Projection.content_bounds`.
//! - [`build_positioned_edges`] — translate graph edges to
//!   [`crate::PositionedEdge`]s given a position map.
//! - [`run_one_step`] — one-shot convenience over the streaming
//!   contract.

use std::collections::HashMap;

use euclid::default::{Point2D, Rect, Size2D};
use graph_canvas::camera::CanvasViewport;
use crate::LayoutExtras;
use graph_canvas::scene::{CanvasEdge, CanvasNode, CanvasSceneInput, ViewId};
use mere_kernel::graph::{NodeKey, RelationKind};

use cartography::projection::{PositionedEdge, Projection};
use cartography::request::ProjectionRequest;
use cartography::strategy::StreamingLayoutStrategy;

/// Build a [`CanvasSceneInput`] from the request's graph and the
/// caller's working positions. Positions come from the supplied
/// `positions` map (not from `graph.node.position`) so simulation
/// diverges from graph truth as it iterates — the adapter contract
/// is that graph truth stays canonical.
pub fn build_scene_input(
    request: &ProjectionRequest<'_>,
    positions: &HashMap<NodeKey, Point2D<f32>>,
) -> CanvasSceneInput<NodeKey> {
    let nodes: Vec<CanvasNode<NodeKey>> = request
        .graph
        .nodes()
        .map(|(key, _node)| {
            let position = positions.get(&key).copied().unwrap_or_default();
            CanvasNode {
                id: key,
                position,
                radius: 0.0,
                label: None,
            }
        })
        .collect();

    // The kernel's `relations()` iterator emits one `RelationView`
    // per relation on the same underlying edge (Hyperlink + Traversal
    // + Provenance on the same pair, etc.). For force-attraction
    // purposes that's fine — each relation is a spring.
    let edges: Vec<CanvasEdge<NodeKey>> = request
        .graph
        .relations()
        .filter(|view| view.from != view.to)
        .map(|view| CanvasEdge {
            weight: edge_weight(view.kind),
            ..CanvasEdge::untagged(view.from, view.to)
        })
        .collect();

    CanvasSceneInput {
        view_id: ViewId(0),
        nodes,
        edges,
        scene_objects: Vec::new(),
        overlays: Vec::new(),
        scene_mode: graph_canvas::scene::SceneMode::default(),
        projection: graph_canvas::projection::ProjectionMode::default(),
    }
}

/// Edge weight per [`RelationKind`]. v0 returns 1.0 for every
/// relation — a future strategy that wants family-weighted springs
/// can swap this for a richer mapping.
pub fn edge_weight(_kind: RelationKind) -> f32 {
    1.0
}

/// Build a [`CanvasViewport`] from the request's `target_size`.
pub fn build_viewport(request: &ProjectionRequest<'_>) -> CanvasViewport {
    let logical = request.intent.target_size.logical_size();
    CanvasViewport {
        rect: Rect::new(
            Point2D::origin(),
            Size2D::new(logical.width.max(1.0), logical.height.max(1.0)),
        ),
        scale_factor: 1.0,
    }
}

/// Bridge from cartography's [`crate::IntelligenceSignals`] to
/// graph-canvas's [`LayoutExtras<NodeKey>`].
///
/// First-pass mapping: clusters → `domain_by_node`, affinity →
/// `semantic_similarity`. Other extras (`pinned`, `frame_regions`,
/// `embedding_by_node`, `axis_value_by_node`, `dragging`) stay empty —
/// cartography's signal surface grows to cover them as concrete
/// consumers materialize.
pub fn build_layout_extras(request: &ProjectionRequest<'_>) -> LayoutExtras<NodeKey> {
    let mut extras = LayoutExtras::<NodeKey>::default();

    if let Some(clusters) = &request.signals.clusters {
        for cluster in &clusters.clusters {
            for member in &cluster.members {
                extras
                    .domain_by_node
                    .insert(*member, cluster.id.clone());
            }
        }
    }

    if let Some(affinity) = &request.signals.affinity {
        for ((a, b), score) in &affinity.pairs {
            extras.semantic_similarity.insert((*a, *b), *score);
        }
    }

    if let Some(embeddings) = &request.signals.embeddings {
        for (key, (x, y)) in &embeddings.coords {
            extras
                .embedding_by_node
                .insert(*key, Point2D::new(*x, *y));
        }
    }

    if let Some(values) = &request.intent.axis_values {
        for (key, value) in values {
            extras
                .axis_value_by_node
                .insert(*key, axis_value_to_graph_canvas(value));
        }
    }

    extras
}

/// Convert cartography's [`crate::AxisValue`] to graph-canvas's
/// [`crate::AxisValue`]. Structurally identical;
/// cartography keeps its own enum so the contract stays decoupled
/// from graph-canvas's internals.
fn axis_value_to_graph_canvas(
    value: &cartography::request::AxisValue,
) -> crate::AxisValue {
    match value {
        cartography::request::AxisValue::Numeric(n) => crate::AxisValue::Numeric(*n),
        cartography::request::AxisValue::Categorical(tag) => {
            crate::AxisValue::Categorical(tag.clone())
        }
    }
}

/// Axis-aligned bounding box around a set of positions. Returns a
/// zero-size rect when positions is empty.
pub fn bounds_of(positions: &HashMap<NodeKey, Point2D<f32>>) -> Rect<f32> {
    let mut iter = positions.values();
    let Some(first) = iter.next() else {
        return Rect::zero();
    };
    let (mut min_x, mut min_y) = (first.x, first.y);
    let (mut max_x, mut max_y) = (first.x, first.y);
    for pos in iter {
        if pos.x < min_x {
            min_x = pos.x;
        }
        if pos.x > max_x {
            max_x = pos.x;
        }
        if pos.y < min_y {
            min_y = pos.y;
        }
        if pos.y > max_y {
            max_y = pos.y;
        }
    }
    Rect::new(
        Point2D::new(min_x, min_y),
        Size2D::new((max_x - min_x).max(0.0), (max_y - min_y).max(0.0)),
    )
}

/// Translate graph edges to [`PositionedEdge`]s given a position map.
/// Self-loops are filtered out (FR and most layouts don't render
/// them).
pub fn build_positioned_edges(
    request: &ProjectionRequest<'_>,
    positions: &HashMap<NodeKey, Point2D<f32>>,
) -> Vec<PositionedEdge> {
    request
        .graph
        .relations()
        .filter(|view| view.from != view.to)
        .map(|view| {
            let from_pos = positions.get(&view.from).copied().unwrap_or_default();
            let to_pos = positions.get(&view.to).copied().unwrap_or_default();
            PositionedEdge {
                edge: None,
                from: view.from,
                to: view.to,
                path: vec![from_pos, to_pos],
            }
        })
        .collect()
}

/// Run a [`StreamingLayoutStrategy`] for a single step and return the
/// resulting [`Projection`]. Convenience for canvases / hosts that
/// don't want to manage iteration state themselves and accept a less-
/// converged result.
pub fn run_one_step<S>(strategy: &S, request: &ProjectionRequest<'_>) -> Projection
where
    S: StreamingLayoutStrategy,
{
    let (projection, _) = strategy.project_initial(request);
    projection
}

/// Extract absolute target positions from a static (analytic) layout
/// using the **origin trick**.
///
/// Static layouts in `graph_canvas::layout` are wrapped in the
/// iterative `Layout::step()` trait which returns *deltas* from
/// scene positions to target positions, damped by `state.damping`.
/// For cartography's analytic [`crate::LayoutStrategy`] contract we
/// want *absolute* targets in one call.
///
/// The trick: build the scene with every node positioned at origin
/// (0, 0) and set `damping = 1.0`. The returned delta then equals
/// `target - origin = target` — i.e. the absolute target itself.
///
/// Nodes whose target equals origin are filtered by `emit()`'s
/// epsilon check; we treat their missing entries as
/// `Point2D::origin()` (which is correct under the trick).
///
/// Works for any `Layout<NodeKey>` whose `State = StaticLayoutState`
/// (Grid, Phyllotaxis, LSystem, Penrose, Radial, Timeline, Kanban).
/// Streaming layouts (`ForceDirected`, `BarnesHut`) use the regular
/// state-threading path instead.
pub fn run_static_layout_one_shot<L>(
    request: &ProjectionRequest<'_>,
    layout: &mut L,
) -> HashMap<NodeKey, Point2D<f32>>
where
    L: crate::Layout<NodeKey, State = crate::StaticLayoutState>,
{
    let nodes: Vec<CanvasNode<NodeKey>> = request
        .graph
        .nodes()
        .map(|(key, _)| CanvasNode {
            id: key,
            position: Point2D::origin(),
            radius: 0.0,
            label: None,
        })
        .collect();
    if nodes.is_empty() {
        return HashMap::new();
    }
    let edges: Vec<CanvasEdge<NodeKey>> = request
        .graph
        .relations()
        .filter(|view| view.from != view.to)
        .map(|view| CanvasEdge::untagged(view.from, view.to))
        .collect();

    let scene = CanvasSceneInput {
        view_id: ViewId(0),
        nodes,
        edges,
        scene_objects: Vec::new(),
        overlays: Vec::new(),
        scene_mode: graph_canvas::scene::SceneMode::default(),
        projection: graph_canvas::projection::ProjectionMode::default(),
    };
    let mut state = crate::StaticLayoutState {
        damping: 1.0,
        step_count: 0,
    };
    let viewport = graph_canvas::camera::CanvasViewport::default();
    // Build the full LayoutExtras bridge so axial strategies (Timeline,
    // Kanban) can read `axis_value_by_node` and embedding-aware ones
    // see their inputs. Static strategies that don't consume extras
    // pay no cost.
    let extras = build_layout_extras(request);
    let deltas = layout.step(&scene, &mut state, 0.0, &viewport, &extras);

    let mut positions: HashMap<NodeKey, Point2D<f32>> = HashMap::with_capacity(deltas.len());
    for (key, _) in request.graph.nodes() {
        // `emit()` drops zero-magnitude deltas, which under the origin
        // trick means the target equals origin. Treat missing entries
        // as `Point2D::origin()` rather than leaving the node out of
        // the projection.
        let pos = deltas
            .get(&key)
            .map(|d| Point2D::new(d.x, d.y))
            .unwrap_or(Point2D::origin());
        positions.insert(key, pos);
    }
    positions
}

/// Build a [`crate::Projection`] from absolute node positions, with
/// edges sourced from the request's graph and content_bounds from the
/// position map. Shared by analytic adapters.
pub fn projection_from_positions(
    strategy_id: &str,
    request: &ProjectionRequest<'_>,
    positions: HashMap<NodeKey, Point2D<f32>>,
) -> Projection {
    let nodes: Vec<cartography::projection::PositionedNode> = positions
        .iter()
        .map(|(key, pos)| cartography::projection::PositionedNode {
            node: *key,
            position: *pos,
            radius: 0.0,
        })
        .collect();
    let edges = build_positioned_edges(request, &positions);
    Projection {
        nodes,
        edges,
        overlays: Vec::new(),
        minimap: None,
        content_bounds: bounds_of(&positions),
        metadata: cartography::projection::ProjectionMetadata {
            strategy_id: Some(strategy_id.to_string()),
            settled: true,
        },
    }
}
