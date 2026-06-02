/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The orrery scene producer: graph → a painted `CanvasPaintList` underlay.
//!
//! This is the **host-agnostic scene-paint underlay** of the orrery under
//! serval-as-host (the [serval-as-host evaluation](../../../../design_docs/mere_docs/technical_architecture/2026-05-29_serval_as_host_evaluation.md)
//! §6, layer 1): edges + nodes + visual-coupling effects as one flat `PaintCmd`
//! list. The brief's load-bearing claim is that this list "renders identically
//! whether driven from a Masonry widget or a serval element", so the producer
//! lives here, host-neutral; the eventual serval orrery element (or any host)
//! calls it and contributes the result as its paint sublist.
//!
//! Node positions come from the graph's committed positions today (matching the
//! current orrery's `scene_from_graph`). When the gyre field-physics layer is
//! live, a force coupling moves a node and updates its position; this reprojects
//! from the same accessor, so the producer is unchanged. The field's *visual*
//! couplings are resolved here too, via [`crate::coupling_paint`].

use std::collections::HashSet;

use cartography::Projection;
use cartography::projection::{PositionedEdge, PositionedNode, ProjectionMetadata};
use kernel::geometry::{PortablePoint, PortableRect, PortableSize};
use kernel::graph::{Graph, NodeKey};
use paint_list_api::DeviceIntSize;

use crate::coupling_paint::{paint_projection_with_visuals, visual_overlays};
use crate::scene_paint::{paint_projection_filtered, Camera, CanvasPaintList, ScenePaintStyle};

/// Build a [`Projection`] from the graph's committed node positions and its
/// relations (collapsed to undirected, de-duplicated pairs — the orrery draws one
/// line per connected pair, not one per typed sidecar). Node radii are left `0.0`
/// (the paint layer substitutes the style default); `content_bounds` is the
/// axis-aligned box of the node positions, for a host that fits the view.
pub fn projection_from_graph(graph: &Graph) -> Projection {
    project(graph, |k| graph.node_committed_position(k), "orrery.stored", true)
}

/// Build a [`Projection`] from a caller-supplied *live* position lookup (e.g. the
/// gyre simulation's projected positions) instead of the committed positions —
/// otherwise identical to [`projection_from_graph`] (same edge dedup,
/// `content_bounds`). The orrery element feeds `position_of` from
/// `gyre::Simulation::position_of` (mapped to [`PortablePoint`]), so the underlay
/// reprojects from live motion each frame. A node with no live position
/// (`position_of` returns `None`, e.g. not yet simulated) falls back to its
/// committed position so it still draws.
///
/// Generic over the lookup so platen stays gyre-free — gyre is a sibling
/// substrate, not a platen dependency.
pub fn projection_from_positions<F>(graph: &Graph, position_of: F) -> Projection
where
    F: Fn(NodeKey) -> Option<PortablePoint>,
{
    project(
        graph,
        |k| position_of(k).or_else(|| graph.node_committed_position(k)),
        "orrery.live",
        false,
    )
}

/// Shared projection core: positioned nodes from `position_of`, undirected
/// de-duplicated edges, and `content_bounds`, tagged with the strategy metadata.
fn project<F>(graph: &Graph, position_of: F, strategy_id: &str, settled: bool) -> Projection
where
    F: Fn(NodeKey) -> Option<PortablePoint>,
{
    let nodes: Vec<PositionedNode> = graph
        .nodes()
        .map(|(key, _node)| PositionedNode {
            node: key,
            position: position_of(key).unwrap_or_default(),
            radius: 0.0,
        })
        .collect();
    let edges = projected_undirected_edges(graph);
    let content_bounds = bounds_of_nodes(&nodes);
    Projection {
        nodes,
        edges,
        content_bounds,
        metadata: ProjectionMetadata {
            strategy_id: Some(strategy_id.to_string()),
            settled,
        },
        ..Projection::empty()
    }
}

/// The graph's relations collapsed to undirected, de-duplicated `(from, to)`
/// pairs — one [`PositionedEdge`] per connected pair, no self-loops. `relations()`
/// projects without a stable `EdgeKey`, so `edge` is `None`.
fn projected_undirected_edges(graph: &Graph) -> Vec<PositionedEdge> {
    let mut seen = HashSet::new();
    let mut edges = Vec::new();
    for rel in graph.relations() {
        let (a, b) = (rel.from, rel.to);
        if a == b {
            continue; // no self-loops in the scene
        }
        let pair = if a <= b { (a, b) } else { (b, a) };
        if seen.insert(pair) {
            edges.push(PositionedEdge { edge: None, from: a, to: b, path: Vec::new() });
        }
    }
    edges
}

/// The orrery scene as one paint list: the graph projected from its committed
/// positions, painted with edges, nodes, and the recognized visual-coupling
/// overlays. The §6 layer-1 underlay in a single call, host-neutral.
pub fn orrery_paint_list(
    graph: &Graph,
    viewport: DeviceIntSize,
    camera: Camera,
    style: &ScenePaintStyle,
    generation: u64,
) -> CanvasPaintList {
    let projection = projection_from_graph(graph);
    paint_projection_with_visuals(graph, &projection, viewport, camera, style, generation)
}

/// The orrery underlay from *live* positions (the gyre-driven variant of
/// [`orrery_paint_list`]): same edges + nodes + visual-coupling overlays, but the
/// node positions come from `position_of` rather than the committed graph. The
/// serval orrery element calls this each frame with `gyre::Simulation`'s
/// positions, so the underlay tracks the simulation (the plan's "positions
/// transition from committed to gyre-live"). Generic over the lookup to keep
/// platen gyre-free.
pub fn orrery_paint_list_from_positions<F>(
    graph: &Graph,
    position_of: F,
    viewport: DeviceIntSize,
    camera: Camera,
    style: &ScenePaintStyle,
    generation: u64,
) -> CanvasPaintList
where
    F: Fn(NodeKey) -> Option<PortablePoint>,
{
    let projection = projection_from_positions(graph, position_of);
    paint_projection_with_visuals(graph, &projection, viewport, camera, style, generation)
}

/// The orrery underlay with off-screen nodes **demoted**: same edges + visual
/// overlays as [`orrery_paint_list_from_positions`], but a node rect is drawn
/// only for nodes where `is_demoted` returns `true` (the off-screen set). The
/// host draws the on-screen nodes as richer DOM children and excludes them here,
/// so the underlay and the DOM layer do not double-draw a node.
///
/// Edges still use every node's position, so an edge from an on-screen node to a
/// demoted one renders. Generic over both lookups to keep platen gyre-free.
///
/// Visual-coupling overlays currently ride the underlay for every coupled node
/// (the sample orrery has none); demoting overlays to the off-screen set as well
/// is a follow-on, like the richer demoted-node glyphs.
pub fn orrery_paint_list_demoted<F, G>(
    graph: &Graph,
    position_of: F,
    is_demoted: G,
    viewport: DeviceIntSize,
    camera: Camera,
    style: &ScenePaintStyle,
    generation: u64,
) -> CanvasPaintList
where
    F: Fn(NodeKey) -> Option<PortablePoint>,
    G: Fn(NodeKey) -> bool,
{
    let projection = projection_from_positions(graph, position_of);
    let mut list =
        paint_projection_filtered(&projection, viewport, camera, style, generation, is_demoted);
    list.splice_world_overlays(visual_overlays(graph, &projection, style));
    list
}

/// Axis-aligned bounds of the positioned nodes (a zero rect when there are none).
fn bounds_of_nodes(nodes: &[PositionedNode]) -> PortableRect {
    let mut iter = nodes.iter();
    let Some(first) = iter.next() else {
        return PortableRect::zero();
    };
    let (mut min_x, mut min_y) = (first.position.x, first.position.y);
    let (mut max_x, mut max_y) = (min_x, min_y);
    for n in iter {
        min_x = min_x.min(n.position.x);
        min_y = min_y.min(n.position.y);
        max_x = max_x.max(n.position.x);
        max_y = max_y.max(n.position.y);
    }
    PortableRect::new(
        PortablePoint::new(min_x, min_y),
        PortableSize::new(max_x - min_x, max_y - min_y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::graph::{
        COUPLING_VOCAB, Coupling, CouplingId, CouplingResponse, EdgeAssertion, Field,
        FieldDefinition, FieldId, NodeSelector, ScalarField, SemanticSubKind,
    };
    use paint_list_api::{PaintCmd, PaintList};
    use uuid::Uuid;

    fn node(g: &mut Graph, id: u128, x: f32, y: f32) -> kernel::graph::NodeKey {
        let key = g.add_node_with_id(Uuid::from_u128(id), format!("mere://{id}"), PortablePoint::new(x, y));
        g.set_node_position(key, PortablePoint::new(x, y));
        key
    }

    fn draw_rect_count(list: &CanvasPaintList) -> usize {
        list.commands()
            .iter()
            .filter(|c| matches!(c, PaintCmd::DrawRect(_)))
            .count()
    }

    #[test]
    fn projection_from_graph_carries_positions_and_dedupes_edges() {
        let mut g = Graph::new();
        let a = node(&mut g, 1, 10.0, 20.0);
        let b = node(&mut g, 2, 30.0, 40.0);
        // Two relations on the same pair (both directions) collapse to one edge.
        let _ = g.assert_relation(a, b, hyperlink());
        let _ = g.assert_relation(b, a, hyperlink());

        let p = projection_from_graph(&g);
        assert_eq!(p.nodes.len(), 2);
        let pa = p.nodes.iter().find(|n| n.node == a).unwrap();
        assert_eq!(pa.position, PortablePoint::new(10.0, 20.0));
        assert_eq!(p.edges.len(), 1, "the bidirectional pair de-dupes to one edge");
        // content_bounds spans the two nodes.
        assert_eq!(p.content_bounds.min_x(), 10.0);
        assert_eq!(p.content_bounds.max_y(), 40.0);
    }

    #[test]
    fn empty_graph_projects_empty() {
        let p = projection_from_graph(&Graph::new());
        assert!(p.nodes.is_empty() && p.edges.is_empty());
    }

    #[test]
    fn demoted_underlay_draws_only_the_off_screen_nodes() {
        // Two connected nodes; demote only `a`. The underlay must draw a's rect
        // but not b's, while still drawing the edge (which needs both positions).
        let mut g = Graph::new();
        let a = node(&mut g, 1, 0.0, 0.0);
        let b = node(&mut g, 2, 100.0, 0.0);
        let _ = g.assert_relation(a, b, hyperlink());

        let full = orrery_paint_list_demoted(
            &g,
            |k| Some(if k == a { PortablePoint::new(0.0, 0.0) } else { PortablePoint::new(100.0, 0.0) }),
            |_| true,
            DeviceIntSize::new(200, 200),
            Camera::default(),
            &ScenePaintStyle::default(),
            0,
        );
        assert_eq!(draw_rect_count(&full), 2, "both nodes drawn when all demoted");

        let one = orrery_paint_list_demoted(
            &g,
            |k| Some(if k == a { PortablePoint::new(0.0, 0.0) } else { PortablePoint::new(100.0, 0.0) }),
            |k| k == a,
            DeviceIntSize::new(200, 200),
            Camera::default(),
            &ScenePaintStyle::default(),
            0,
        );
        assert_eq!(draw_rect_count(&one), 1, "only the demoted node draws a rect");
        // The edge still renders (one DrawStroke) even though b's rect is culled.
        let strokes = one
            .commands()
            .iter()
            .filter(|c| matches!(c, PaintCmd::DrawStroke(_)))
            .count();
        assert_eq!(strokes, 1, "the edge to the on-screen node still draws");
    }

    #[test]
    fn projection_from_positions_overrides_committed_and_falls_back() {
        let mut g = Graph::new();
        let a = node(&mut g, 1, 10.0, 20.0); // committed (10, 20)
        let b = node(&mut g, 2, 30.0, 40.0); // committed (30, 40)
        // A live position for `a` only; `b` has none → falls back to committed.
        let live =
            move |k: NodeKey| (k == a).then_some(PortablePoint::new(99.0, 99.0));
        let p = projection_from_positions(&g, live);

        let pa = p.nodes.iter().find(|n| n.node == a).unwrap();
        let pb = p.nodes.iter().find(|n| n.node == b).unwrap();
        assert_eq!(pa.position, PortablePoint::new(99.0, 99.0), "a uses the live position");
        assert_eq!(pb.position, PortablePoint::new(30.0, 40.0), "b falls back to committed");
        assert_eq!(p.metadata.strategy_id.as_deref(), Some("orrery.live"));
    }

    #[test]
    fn orrery_paint_list_composes_nodes_and_a_visual_overlay() {
        // One node at the origin, a Gaussian field peaking there, and a visual/halo
        // coupling over it. The paint list should carry the node rect *and* the
        // resolved halo overlay — the underlay producer wiring the field's light in.
        let mut g = Graph::new();
        node(&mut g, 1, 0.0, 0.0);

        // Baseline: no coupling → just the node rect.
        let baseline = orrery_paint_list(
            &g,
            DeviceIntSize::new(200, 200),
            Camera::default(),
            &ScenePaintStyle::default(),
            0,
        );
        assert_eq!(draw_rect_count(&baseline), 1, "node rect only");

        // Add the field + a visual/halo coupling over all nodes.
        let fid = FieldId::from_uuid(Uuid::from_u128(0xF1));
        g.add_field(Field::new(
            fid,
            FieldDefinition::Scalar(ScalarField::gaussian_at(0.0, 0.0, 50.0)),
        ));
        g.add_coupling(Coupling::new(
            CouplingId::from_uuid(Uuid::from_u128(1)),
            fid,
            NodeSelector::All,
            CouplingResponse::open(format!("{COUPLING_VOCAB}visual/halo")),
            1.0,
        ));

        let with_halo = orrery_paint_list(
            &g,
            DeviceIntSize::new(200, 200),
            Camera::default(),
            &ScenePaintStyle::default(),
            0,
        );
        assert_eq!(
            draw_rect_count(&with_halo),
            2,
            "node rect + the resolved visual/halo overlay"
        );
        assert!(matches!(
            with_halo.commands().last(),
            Some(PaintCmd::PopTransform)
        ));
    }

    fn hyperlink() -> EdgeAssertion {
        EdgeAssertion::Semantic {
            sub_kind: SemanticSubKind::Hyperlink,
            label: None,
            decay_progress: None,
        }
    }
}
