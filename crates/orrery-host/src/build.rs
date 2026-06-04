/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Construction + small paint/DOM helpers for the [`Orrery`](crate::Orrery): the
//! built-in sample graph and its force-directed [`Simulation`], the
//! pre-materialized abs-pos node-children pool, and the screen/world
//! paint-command builders the frame loop splices in. Split out of `lib.rs` to
//! keep each file under the workspace size ceiling; the sample-graph half is
//! replaced by the host's live graph session in S2 of the modular integration
//! plan.

use std::collections::{HashMap, HashSet};

use euclid::default::Point2D;
use gyre::{Boundary, EdgeSpring, NodeExclusion, Simulation};
use kernel::geometry::PortablePoint;
use kernel::graph::{EdgeAssertion, Graph, NodeKey, SemanticSubKind};
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use paint_list_api::{
    ColorF, CommonPlacement, LayoutPoint, LayoutRect, PaintCmd, PathCommand, PathData, RectItem,
    StrokeCap, StrokeItem, StrokeJoin,
};
use platen::scene_paint::ScenePaintStyle;
use serval_scripted_dom::{NodeId as DomNodeId, ScriptedDom};

/// Author CSS for the node-children document. `.stage` is the camera-transformed
/// container (also `position: relative`, so it is the containing block for the
/// abs-pos nodes); each `.gnode` is an absolutely-positioned labelled box moved to
/// its world position by an inline transform (serval propagates `.stage`'s camera
/// transform onto these abs-pos descendants — the 1A fix).
pub(crate) const NODE_SHEET: &[&str] = &[
    "div { display: block; }",
    ".stage { position: relative; }",
    ".gnode { position: absolute; left: 0; top: 0; width: 36px; height: 36px; \
        background-color: rgb(54, 92, 156); color: rgb(245, 247, 252); font-size: 15px; }",
    ".gnode-selected { position: absolute; left: 0; top: 0; width: 36px; height: 36px; \
        background-color: rgb(232, 150, 40); color: rgb(28, 22, 10); font-size: 15px; }",
    // Activation-state colors: open (green), closed (red), new/blank (blue, the
    // same fill as the default `.gnode`). The host pushes the state per node.
    ".gnode-open { position: absolute; left: 0; top: 0; width: 36px; height: 36px; \
        background-color: rgb(58, 140, 94); color: rgb(238, 250, 243); font-size: 15px; }",
    ".gnode-closed { position: absolute; left: 0; top: 0; width: 36px; height: 36px; \
        background-color: rgb(166, 72, 72); color: rgb(250, 240, 240); font-size: 15px; }",
    ".gnode-new { position: absolute; left: 0; top: 0; width: 36px; height: 36px; \
        background-color: rgb(54, 92, 156); color: rgb(245, 247, 252); font-size: 15px; }",
];

/// A small sample graph: a ring of nodes around the origin with ring edges plus a
/// few hub spokes, so the underlay has both edges and nodes to draw. (S2 replaces
/// the static ring with the host's live graph session.)
pub(crate) fn sample_graph() -> Graph {
    let mut graph = Graph::new();
    let count = 12usize;
    let radius = 220.0_f32;
    let mut keys = Vec::with_capacity(count);
    for i in 0..count {
        let theta = (i as f32) / (count as f32) * std::f32::consts::TAU;
        let pos = PortablePoint::new(radius * theta.cos(), radius * theta.sin());
        let key =
            graph.add_node_with_id(uuid::Uuid::from_u128(i as u128 + 1), format!("mere://node/{i}"), pos);
        graph.set_node_position(key, pos);
        keys.push(key);
    }
    // Ring edges around the cycle.
    for i in 0..count {
        let _ = graph.assert_relation(keys[i], keys[(i + 1) % count], hyperlink());
    }
    // A few spokes from node 0 across the ring.
    for i in (2..count).step_by(3) {
        let _ = graph.assert_relation(keys[0], keys[i], hyperlink());
    }
    graph
}

/// A plain hyperlink relation (the orrery draws one undirected line per pair).
pub(crate) fn hyperlink() -> EdgeAssertion {
    EdgeAssertion::Semantic {
        sub_kind: SemanticSubKind::Hyperlink,
        label: None,
        decay_progress: None,
    }
}

/// The undirected, de-duplicated relation pairs that feed the layout springs.
/// gyre stays relation-taxonomy agnostic, so the orrery picks the topology: one
/// edge per unordered node pair (a reciprocal A↔B counts once). Reused by
/// [`build_simulation`] at startup and [`Orrery::visit`](crate::Orrery::visit)
/// when the graph grows.
pub(crate) fn dedup_edges(graph: &Graph) -> Vec<(NodeKey, NodeKey)> {
    let mut seen = HashSet::new();
    graph
        .relations()
        .filter_map(|r| {
            let pair = if r.from <= r.to { (r.from, r.to) } else { (r.to, r.from) };
            seen.insert(pair).then_some((r.from, r.to))
        })
        .collect()
}

/// Build the force-directed simulation from `graph`: a body per node, the
/// undirected de-duplicated relation pairs as the spring topology, the standard
/// force trio (exclusion + edge-springs + a centering boundary), seeded into a
/// tight central spiral so the first settle is visible.
pub(crate) fn build_simulation(graph: &Graph) -> Simulation {
    let mut sim = Simulation::new();
    sim.sync_with_graph(graph);
    sim.sync_edges(dedup_edges(graph));

    sim.add_force(NodeExclusion::default());
    sim.add_force(EdgeSpring::default());
    sim.add_force(Boundary::default());

    seed_cluster(&mut sim, graph);
    sim
}

/// Seed every node into a tight central spiral (golden-angle), so a ticked settle
/// visibly expands it into a readable layout.
pub(crate) fn seed_cluster(sim: &mut Simulation, graph: &Graph) {
    let seeds: Vec<(NodeKey, Point2D<f32>)> = graph
        .nodes()
        .enumerate()
        .map(|(i, (key, _node))| {
            let r = 6.0 + i as f32 * 3.0;
            let theta = i as f32 * 2.399_963; // golden angle in radians
            (key, Point2D::new(r * theta.cos(), r * theta.sin()))
        })
        .collect();
    sim.seed_positions(seeds);
}

/// Build the pre-materialized node-children pool **once**: a `.stage` container
/// (the camera transform's element) holding one `.gnode` per graph node, each
/// labelled with its index and given an initial `transform` (so the per-frame
/// updates are value→value changes — paint-tier — not the first none→value one
/// that can relayout). Returns the DOM, the node→gnode map, and the stage id; the
/// frame loop mutates these elements' attributes and never rebuilds them.
pub(crate) fn build_pool_dom(graph: &Graph) -> (ScriptedDom, HashMap<NodeKey, DomNodeId>, DomNodeId) {
    let mut dom = ScriptedDom::new();
    let root = dom.document();

    let stage = dom.create_element(qual("div"));
    dom.set_attribute(stage, qual("class"), "stage");
    dom.set_attribute(stage, qual("style"), "transform: translate(0px, 0px) scale(1);");
    dom.append_child(root, stage);

    let mut gnode_of = HashMap::new();
    for (i, (key, _node)) in graph.nodes().enumerate() {
        let gnode = dom.create_element(qual("div"));
        dom.set_attribute(gnode, qual("class"), "gnode");
        dom.set_attribute(gnode, qual("style"), "transform: translate(0px, 0px);");
        let label = dom.create_text(&i.to_string());
        dom.append_child(gnode, label);
        dom.append_child(stage, gnode);
        gnode_of.insert(key, gnode);
    }
    (dom, gnode_of, stage)
}

/// Set an element's inline `style` (records an attribute mutation for `apply`).
pub(crate) fn set_style(dom: &mut ScriptedDom, node: DomNodeId, style: &str) {
    dom.set_attribute(node, qual("style"), style);
}

/// Set an element's `class` (records an attribute mutation for `apply`).
pub(crate) fn set_class(dom: &mut ScriptedDom, node: DomNodeId, class: &str) {
    dom.set_attribute(node, qual("class"), class);
}

/// A `QualName` in the null namespace (the shape `ScriptedDom` builders take).
fn qual(local: &str) -> QualName {
    QualName::new(None, Namespace::from(""), LocalName::from(local))
}

/// The marquee rubber-band as a single translucent screen-space fill from `a` to
/// `b` (px). No transform, so it composites at screen coords over the camera-space
/// layers.
pub(crate) fn marquee_rect_cmds(a: (f32, f32), b: (f32, f32)) -> Vec<PaintCmd> {
    let rect = LayoutRect::new(
        LayoutPoint::new(a.0.min(b.0), a.1.min(b.1)),
        LayoutPoint::new(a.0.max(b.0), a.1.max(b.1)),
    );
    vec![PaintCmd::DrawRect(RectItem {
        placement: CommonPlacement::new(rect),
        color: ColorF::new(0.30, 0.50, 0.92, 0.18),
    })]
}

// ----- Dark-mode palette + backdrop -----------------------------------------
// The orrery paints its own opaque backdrop as the bottom composite layer, so
// the content surface is dark regardless of the host's clear color (and stops
// silently depending on a WHITE clear). A light variant + a runtime toggle are
// the follow-up; these three are the single seam to flip when that lands.

/// The content-surface backdrop (opaque, dark slate).
pub(crate) fn surface_bg() -> ColorF {
    ColorF::new(0.067, 0.078, 0.100, 1.0)
}

/// Scene-paint style tuned for the dark backdrop: a luminous node fill and
/// lifted, lightly-translucent edge strokes that read on near-black (the
/// platen default is tuned for a white canvas). `default_node_radius` matches
/// the lib's `NODE_HALF`, so demoted-underlay rects align with the DOM gnodes.
pub(crate) fn dark_scene_style() -> ScenePaintStyle {
    ScenePaintStyle {
        node_color: ColorF::new(0.50, 0.68, 0.92, 1.0),
        default_node_radius: 18.0,
        edge_color: ColorF::new(0.56, 0.61, 0.72, 0.65),
        edge_width: 1.5,
    }
}

/// The backdrop as a single screen-space fill over the whole viewport (no
/// camera transform) — the orrery's bottom composite layer.
pub(crate) fn background_cmds(w: u32, h: u32, color: ColorF) -> Vec<PaintCmd> {
    let rect = LayoutRect::new(LayoutPoint::zero(), LayoutPoint::new(w as f32, h as f32));
    vec![PaintCmd::DrawRect(RectItem { placement: CommonPlacement::new(rect), color })]
}

/// Highlight strokes for the selected edges, in **world space** (no transform) —
/// spliced inside the underlay's camera transform, so they reuse the producer's
/// camera rather than replicating it. Each selected edge redraws as a thicker
/// orange line over the underlay's thin grey one.
pub(crate) fn selected_edge_overlay(
    sim: &Simulation,
    selected_edges: &HashSet<(NodeKey, NodeKey)>,
) -> Vec<PaintCmd> {
    let mut cmds = Vec::new();
    for (a, b, pa, pb) in sim.edge_segments() {
        if !selected_edges.contains(&(a, b)) {
            continue;
        }
        let p0 = LayoutPoint::new(pa.x, pa.y);
        let p1 = LayoutPoint::new(pb.x, pb.y);
        let bounds = LayoutRect::new(
            LayoutPoint::new(p0.x.min(p1.x), p0.y.min(p1.y)),
            LayoutPoint::new(p0.x.max(p1.x), p0.y.max(p1.y)),
        );
        cmds.push(PaintCmd::DrawStroke(StrokeItem {
            placement: CommonPlacement::new(bounds),
            path: PathData { commands: vec![PathCommand::MoveTo(p0), PathCommand::LineTo(p1)] },
            color: ColorF::new(0.91, 0.59, 0.16, 1.0),
            width: 3.5,
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            dash: None,
        }));
    }
    cmds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_graph_has_nodes_and_edges() {
        let g = sample_graph();
        assert_eq!(g.nodes().count(), 12, "the ring has twelve nodes");
        assert!(g.relations().count() >= 12, "at least the ring edges");
    }

    #[test]
    fn pool_has_a_gnode_per_node() {
        let g = sample_graph();
        let (_dom, gnode_of, _stage) = build_pool_dom(&g);
        assert_eq!(
            gnode_of.len(),
            g.nodes().count(),
            "the pre-materialized pool has one gnode per graph node",
        );
    }

    #[test]
    fn simulation_has_a_body_per_node_and_the_edge_topology() {
        let g = sample_graph();
        let sim = build_simulation(&g);
        assert_eq!(sim.body_count(), 12, "one physics body per node");
        assert!(sim.edge_count() >= 12, "the spring topology carries the edges");
    }

    #[test]
    fn ticking_moves_nodes_from_the_seed() {
        let g = sample_graph();
        let mut sim = build_simulation(&g);
        let before: Vec<(NodeKey, Point2D<f32>)> = sim.positions().collect();
        for _ in 0..60 {
            sim.tick(crate::TICK_DT);
        }
        let after: HashMap<NodeKey, Point2D<f32>> = sim.positions().collect();
        let moved = before
            .iter()
            .any(|(k, p0)| after.get(k).is_some_and(|p1| (p1.x - p0.x).hypot(p1.y - p0.y) > 1.0));
        assert!(moved, "the force-directed settle moves nodes off the seed");
    }
}
