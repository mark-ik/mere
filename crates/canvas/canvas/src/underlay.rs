// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The canvas scene producer: graph → a painted `CanvasPaintList` underlay.
//!
//! This is the **host-agnostic scene-paint underlay** of the canvas under
//! genet-as-host (the [genet-as-host evaluation](../../../../design_docs/mere_docs/technical_architecture/2026-05-29_genet_as_host_evaluation.md)
//! §6, layer 1): edges + nodes + visual-coupling effects as one flat `PaintCmd`
//! list. The brief's load-bearing claim is that this list "renders identically
//! whether driven from a Masonry widget or a genet element", so the producer
//! lives here, host-neutral; the eventual genet canvas element (or any host)
//! calls it and contributes the result as its paint sublist.
//!
//! Node positions come from the graph's committed positions today (matching the
//! current canvas's `scene_from_graph`). When the seiche field-physics layer is
//! live, a force coupling moves a node and updates its position; this reprojects
//! from the same accessor, so the producer is unchanged. The field's *visual*
//! couplings are resolved here too, via [`crate::coupling_paint`].

use std::collections::{HashMap, HashSet};

use cartography::Projection;
use cartography::projection::{PositionedEdge, PositionedNode, ProjectionMetadata};
use forme::{Arrangement, GraphMemberId};
use kernel::geometry::{PortablePoint, PortableRect, PortableSize};
use kernel::graph::{Graph, NodeKey, RelationView};
use paint_list_api::DeviceIntSize;

use crate::coupling_paint::{paint_projection_with_visuals, visual_overlays};
use crate::scene_paint::{Camera, CanvasPaintList, ScenePaintStyle, paint_projection_filtered};

/// Build a [`Projection`] from a graph with **no positions** — every node lands
/// at the origin (positions are no longer graph truth; S2). This is the
/// structural-only projection (edges, dedup, content-bounds); a caller that wants
/// placed nodes uses [`projection_from_positions`] with a seiche / sidecar lookup.
pub fn projection_from_graph(graph: &Graph) -> Projection {
    project(graph, |_| None, |_| true, "canvas.stored", true)
}

/// Build a [`Projection`] from a caller-supplied *live* position lookup (e.g. the
/// seiche simulation's projected positions) instead of the committed positions —
/// otherwise identical to [`projection_from_graph`] (same edge dedup,
/// `content_bounds`). The canvas element feeds `position_of` from
/// `seiche::Simulation::position_of` (mapped to [`PortablePoint`]), so the underlay
/// reprojects from live motion each frame. A node with no live position
/// (`position_of` returns `None`, e.g. not yet simulated) falls back to its
/// committed position so it still draws.
///
/// Generic over the lookup so platen stays seiche-free — seiche is a sibling
/// substrate, not a platen dependency.
pub fn projection_from_positions<F>(graph: &Graph, position_of: F) -> Projection
where
    F: Fn(NodeKey) -> Option<PortablePoint>,
{
    project(graph, |k| position_of(k), |_| true, "canvas.live", false)
}

/// The canvas's read-through *Identity* arrangement for `graph`: every graph member
/// as a `MemberIntent` (spine §14.2 `FormeRef::Identity`). Derived from the current
/// node set, never stored. Projecting the canvas through this is what makes the
/// canvas and the tiled workbench two projections of one forme arrangement.
pub fn identity_arrangement(graph: &Graph) -> Arrangement {
    Arrangement::identity(graph.nodes().map(|(_, node)| node.id))
}

/// A curated arrangement of just `keys` (a subset of the graph) as `MemberIntent`s
/// keyed by their member UUIDs, in graph order — the canvas's "scope" lens. Like
/// [`identity_arrangement`] restricted to the given nodes; the same shape a
/// `FormeRef::Stored` curated arrangement would take, so a scoped canvas and a
/// curated bench are the same projection of different arrangements. (Curated canvas.)
pub fn arrangement_of_keys(graph: &Graph, keys: &[NodeKey]) -> Arrangement {
    let set: HashSet<NodeKey> = keys.iter().copied().collect();
    Arrangement::identity(
        graph
            .nodes()
            .filter(|(k, _)| set.contains(k))
            .map(|(_, node)| node.id),
    )
}

/// Build a [`Projection`] whose node SET comes from a forme [`Arrangement`]'s
/// membership rather than the whole graph — the seam that makes the canvas a
/// Cartography projection of an arrangement. Positions come from `position_of`
/// (committed fallback); edges still come from `graph.relations()`. For an Identity
/// arrangement (every graph member) this is byte-identical to
/// [`projection_from_positions`]; a curated arrangement projects only its members.
pub fn projection_from_arrangement<F>(
    graph: &Graph,
    arrangement: &Arrangement,
    position_of: F,
) -> Projection
where
    F: Fn(NodeKey) -> Option<PortablePoint>,
{
    let keys = arrangement_keys(graph, arrangement);
    project_keys(
        graph,
        &keys,
        |k| position_of(k),
        |_| true,
        "canvas.live",
        false,
    )
}

/// The graph node keys for an arrangement's members, in GRAPH order (so an Identity
/// arrangement projects byte-identically to the whole-graph path). The arrangement
/// keys members by UUID (`GraphMemberId`) and the projection by `NodeKey`, so this
/// is a membership filter over `graph.nodes()`; a member absent from the graph is
/// skipped.
fn arrangement_keys(graph: &Graph, arrangement: &Arrangement) -> Vec<NodeKey> {
    let members: HashSet<GraphMemberId> = arrangement.referenced_members().into_iter().collect();
    graph
        .nodes()
        .filter(|(_, node)| members.contains(&node.id))
        .map(|(key, _)| key)
        .collect()
}

/// Shared projection core: positioned nodes from `position_of`, undirected
/// de-duplicated edges (filtered by `edge_visible`), and `content_bounds`, tagged
/// with the strategy metadata. `edge_visible` is checked per *relation* before
/// the pair-collapse, so a host can hide edges by node-pair or by relation family
/// (the kind is gone once the pair collapses to one line).
fn project<F, V>(
    graph: &Graph,
    position_of: F,
    edge_visible: V,
    strategy_id: &str,
    settled: bool,
) -> Projection
where
    F: Fn(NodeKey) -> Option<PortablePoint>,
    V: Fn(&RelationView) -> bool,
{
    let keys: Vec<NodeKey> = graph.nodes().map(|(key, _node)| key).collect();
    project_keys(
        graph,
        &keys,
        position_of,
        edge_visible,
        strategy_id,
        settled,
    )
}

/// Projection core over an explicit node-key set — the membership source. Both the
/// whole graph ([`project`]) and a forme [`Arrangement`]'s members
/// ([`arrangement_keys`]) feed this; the output is identical when `keys` is every
/// graph node in graph order. Edges still come from `graph.relations()` (an
/// arrangement is geometry-free and holds no spatial edges).
fn project_keys<F, V>(
    graph: &Graph,
    keys: &[NodeKey],
    position_of: F,
    edge_visible: V,
    strategy_id: &str,
    settled: bool,
) -> Projection
where
    F: Fn(NodeKey) -> Option<PortablePoint>,
    V: Fn(&RelationView) -> bool,
{
    let nodes: Vec<PositionedNode> = keys
        .iter()
        .map(|&key| PositionedNode {
            node: key,
            position: position_of(key).unwrap_or_default(),
            radius: 0.0,
        })
        .collect();
    let edges = projected_undirected_edges(graph, &edge_visible);
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
fn projected_undirected_edges(
    graph: &Graph,
    edge_visible: &impl Fn(&RelationView) -> bool,
) -> Vec<PositionedEdge> {
    // One edge per pair, but count how many visible relations land on it: the multigraph
    // multiplicity (more statements between a pair => a heavier, thicker edge). First time a
    // pair is seen records its endpoints + index; repeats just bump the weight. (Graph signals.)
    let mut index_of: HashMap<(NodeKey, NodeKey), usize> = HashMap::new();
    let mut edges: Vec<PositionedEdge> = Vec::new();
    for rel in graph.relations() {
        let (a, b) = (rel.from, rel.to);
        if a == b {
            continue; // no self-loops in the scene
        }
        if !edge_visible(&rel) {
            continue; // the host hid this relation (by node-pair or by family)
        }
        let pair = if a <= b { (a, b) } else { (b, a) };
        match index_of.get(&pair) {
            Some(&i) => edges[i].weight += 1.0,
            None => {
                index_of.insert(pair, edges.len());
                edges.push(PositionedEdge {
                    edge: None,
                    from: a,
                    to: b,
                    path: Vec::new(),
                    weight: 1.0,
                });
            }
        }
    }
    edges
}

/// The canvas scene as one paint list: the graph projected from its committed
/// positions, painted with edges, nodes, and the recognized visual-coupling
/// overlays. The §6 layer-1 underlay in a single call, host-neutral.
pub fn canvas_paint_list(
    graph: &Graph,
    viewport: DeviceIntSize,
    camera: Camera,
    style: &ScenePaintStyle,
    generation: u64,
) -> CanvasPaintList {
    let projection = projection_from_graph(graph);
    paint_projection_with_visuals(graph, &projection, viewport, camera, style, generation)
}

/// The canvas underlay from *live* positions (the seiche-driven variant of
/// [`canvas_paint_list`]): same edges + nodes + visual-coupling overlays, but the
/// node positions come from `position_of` rather than the committed graph. The
/// genet canvas element calls this each frame with `seiche::Simulation`'s
/// positions, so the underlay tracks the simulation (the plan's "positions
/// transition from committed to seiche-live"). Generic over the lookup to keep
/// platen seiche-free.
pub fn canvas_paint_list_from_positions<F>(
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

/// The canvas underlay with off-screen nodes **demoted**: same edges + visual
/// overlays as [`canvas_paint_list_from_positions`], but a node rect is drawn
/// only for nodes where `is_demoted` returns `true` (the off-screen set). The
/// host draws the on-screen nodes as richer DOM children and excludes them here,
/// so the underlay and the DOM layer do not double-draw a node.
///
/// Edges still use every node's position, so an edge from an on-screen node to a
/// demoted one renders. Generic over both lookups to keep platen seiche-free.
///
/// Visual-coupling overlays currently ride the underlay for every coupled node
/// (the sample canvas has none); demoting overlays to the off-screen set as well
/// is a follow-on, like the richer demoted-node glyphs.
pub fn canvas_paint_list_demoted<F, G, V>(
    graph: &Graph,
    position_of: F,
    is_demoted: G,
    edge_visible: V,
    viewport: DeviceIntSize,
    camera: Camera,
    style: &ScenePaintStyle,
    generation: u64,
) -> CanvasPaintList
where
    F: Fn(NodeKey) -> Option<PortablePoint>,
    G: Fn(NodeKey) -> bool,
    V: Fn(&RelationView) -> bool,
{
    // Build the live projection directly (mirroring `projection_from_positions`'s
    // committed-position fallback) so the edge-visibility predicate threads in.
    let projection = project(
        graph,
        |k| position_of(k),
        edge_visible,
        "canvas.live",
        false,
    );
    let mut list =
        paint_projection_filtered(&projection, viewport, camera, style, generation, is_demoted);
    list.splice_world_overlays(visual_overlays(graph, &projection, style));
    list
}

/// [`canvas_paint_list_demoted`] with the node SET sourced from a forme
/// [`Arrangement`]'s membership instead of the whole graph — the canvas rendered as
/// a Cartography projection of an arrangement (the spine's "two projections of one
/// arrangement"). For an Identity arrangement the output is byte-identical to
/// `canvas_paint_list_demoted`; a curated arrangement would show only its members.
/// Edges, positions, demotion, and visual overlays are otherwise unchanged.
pub fn canvas_paint_list_demoted_from_arrangement<F, G, V, R>(
    graph: &Graph,
    arrangement: &Arrangement,
    position_of: F,
    is_demoted: G,
    edge_visible: V,
    radius_of: R,
    viewport: DeviceIntSize,
    camera: Camera,
    style: &ScenePaintStyle,
    generation: u64,
) -> CanvasPaintList
where
    F: Fn(NodeKey) -> Option<PortablePoint>,
    G: Fn(NodeKey) -> bool,
    V: Fn(&RelationView) -> bool,
    R: Fn(NodeKey) -> Option<f32>,
{
    let keys = arrangement_keys(graph, arrangement);
    let mut projection = project_keys(
        graph,
        &keys,
        |k| position_of(k),
        edge_visible,
        "canvas.live",
        false,
    );
    // Carry each node's face radius into the projection so straight edges trim to the
    // face (not the center) and a demoted node's underlay rect draws at its true size.
    // A node the host gives no radius keeps the style default. (Node-rep Decision 5.)
    for node in &mut projection.nodes {
        if let Some(r) = radius_of(node.node) {
            node.radius = r;
        }
    }
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
mod tests;
