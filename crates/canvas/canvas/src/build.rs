// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Construction + small paint/DOM helpers for the [`Canvas`](crate::Canvas): the
//! built-in sample graph and its force-directed [`Simulation`], the
//! pre-materialized abs-pos node-children pool, and the screen/world
//! paint-command builders the frame loop splices in. Split out of `lib.rs` to
//! keep each file under the workspace size ceiling; the sample-graph half is
//! replaced by the host's live graph session in S2 of the modular integration
//! plan.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use kernel::geometry::PortablePoint;
use kernel::graph::apply::{self as graph_apply};
use kernel::graph::{
    EdgeAssertion, FieldExtent, FieldId, Graph, NodeKey, RelationKind, SemanticSubKind,
};
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use paint_list_api::{
    ColorF, CommonPlacement, DashPattern, ExtendMode, GradientStop, LayoutPoint, LayoutRect,
    LayoutSize, PaintCmd, PathCommand, PathData, RadialGradientItem, RadialGradientPayload,
    RectItem, StrokeCap, StrokeItem, StrokeJoin,
};
use seiche::LayoutView;

use crate::scene_paint::ScenePaintStyle;
use genet_scripted_dom::{NodeId as DomNodeId, ScriptedDom};
use signals::{BridgeNodes, ClusterSet};

use crate::palette;

/// The geometry every `.gnode` color class repeats; only the fill/label differ.
const GNODE_BOX: &str =
    "position: absolute; left: 0; top: 0; width: 36px; height: 36px; font-size: 15px;";

/// Author CSS for the node-children document, built once. `.stage` is the
/// camera-transformed container (also `position: relative`, so it is the
/// containing block for the abs-pos nodes); each `.gnode` is an
/// absolutely-positioned labelled box moved to its world position by an inline
/// transform (genet propagates `.stage`'s camera transform onto these abs-pos
/// descendants — the 1A fix).
///
/// The node colors are **not** literals here: `.stage` carries the `--node-*`
/// custom properties from [`crate::palette`], and the classes below `var()` them,
/// so the canvas and every other representation read one palette through the
/// cascade. (Representations carry node identity.)
fn build_node_sheet() -> Vec<String> {
    vec![
        "div { display: block; }".to_string(),
        // The palette enters the cascade here; `.gnode` and `.gcaption` are
        // `.stage` descendants, so they inherit every `--node-*` var.
        format!(
            ".stage {{ position: relative; {} }}",
            palette::custom_property_declarations()
        ),
        format!(
            ".gnode {{ {GNODE_BOX} background-color: var(--node-idle-bg); color: var(--node-idle-fg); }}"
        ),
        format!(
            ".gnode-selected {{ {GNODE_BOX} background-color: var(--node-selected-bg); color: var(--node-selected-fg); }}"
        ),
        // Activation-state colors: open (green), closed (red), idle (blue, the same
        // fill as the default `.gnode`). The host pushes the state per node.
        format!(
            ".gnode-open {{ {GNODE_BOX} background-color: var(--node-open-bg); color: var(--node-open-fg); }}"
        ),
        format!(
            ".gnode-closed {{ {GNODE_BOX} background-color: var(--node-closed-bg); color: var(--node-closed-fg); }}"
        ),
        format!(
            ".gnode-idle {{ {GNODE_BOX} background-color: var(--node-idle-bg); color: var(--node-idle-fg); }}"
        ),
        // Content-type silhouettes, layered on the state/color class as a second class
        // (border-radius only, so they merge with whichever color class is set). Square
        // is the default (no class); rounded = small-web menu, circle = feed.
        ".gnode-rounded { border-radius: 9px; }".to_string(),
        ".gnode-circle { border-radius: 50%; }".to_string(),
        // Representation is a face treatment over the measured footprint.
        // Glyph is the terse icon rung. Card is deliberately the established
        // Canvas face. LivePane is framed as the focused/live-capable rung; the
        // renderer does not imply that an embedded document exists yet.
        ".gnode-representation-glyph { border-radius: 50%; }".to_string(),
        ".gnode-representation-glyph .gcaption { display: none; }".to_string(),
        ".gnode-representation-live-pane { box-shadow: 0 0 0 3px var(--node-caption-fg), 0 0 16px var(--node-caption-fg); }".to_string(),
        ".gnode-representation-live-pane .gcaption { font-weight: 700; }".to_string(),
        // The node's display label, riding beside the tile (not inside the 36px box):
        // absolutely positioned just right of the square so a long name reads in full on
        // the canvas instead of overflowing the tile. It is a child of the `.gnode`, so
        // the per-frame transform carries it along. (Node legibility.)
        ".gcaption { position: absolute; left: 42px; top: 8px; white-space: nowrap; \
            color: var(--node-caption-fg); font-size: 14px; }"
            .to_string(),
    ]
}

static NODE_SHEET_STRINGS: LazyLock<Vec<String>> = LazyLock::new(build_node_sheet);

/// The node-children sheet as the `&[&str]` the layout seam takes. Built once:
/// `apply` re-passes this every frame, so it must not allocate.
pub(crate) static NODE_SHEET: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| NODE_SHEET_STRINGS.iter().map(String::as_str).collect());

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
        let key = graph_apply::add_node(
            &mut graph,
            Some(uuid::Uuid::from_u128(i as u128 + 1)),
            format!("mere://node/{i}"),
            pos,
        );
        keys.push(key);
    }
    // Ring edges around the cycle.
    for i in 0..count {
        let _ =
            graph_apply::assert_relation(&mut graph, keys[i], keys[(i + 1) % count], hyperlink());
    }
    // A few spokes from node 0 across the ring.
    for i in (2..count).step_by(3) {
        let _ = graph_apply::assert_relation(&mut graph, keys[0], keys[i], hyperlink());
    }
    graph
}

/// A plain hyperlink relation (the canvas draws one undirected line per pair).
pub(crate) fn hyperlink() -> EdgeAssertion {
    EdgeAssertion::Semantic {
        sub_kind: SemanticSubKind::Hyperlink,
        label: None,
        decay_progress: None,
    }
}

/// The undirected, de-duplicated relation pairs that feed the layout springs.
/// seiche stays relation-taxonomy agnostic, so the canvas picks the topology: one
/// edge per unordered node pair (a reciprocal A↔B counts once). Reused by
/// [`build_simulation`] at startup and [`Canvas::visit`](crate::Canvas::visit)
/// when the graph grows.
pub(crate) fn dedup_edges(graph: &Graph) -> Vec<(NodeKey, NodeKey)> {
    let mut seen = HashSet::new();
    graph
        .relations()
        .filter_map(|r| {
            let pair = if r.from <= r.to {
                (r.from, r.to)
            } else {
                (r.to, r.from)
            };
            seen.insert(pair).then_some((r.from, r.to))
        })
        .collect()
}

// `visible_relation_edges` moved to `seiche_bridge` (the graph ↔ seiche
// adapters) with the rest of the simulation construction.

/// Like [`dedup_edges`], but each collapsed pair carries its **statement multiplicity**: the number
/// of *statement* relations connecting it (more statements between two nodes => a heavier edge — the
/// multigraph truth made legible). **Traversal events are excluded**: a navigation re-visit is not a
/// statement, and the kernel deliberately does not bump the revision on a re-visit append (it must
/// not churn the structural caches), so counting traversals would let the revision-gated
/// weighted-edge memo disagree with a direct recompute. A pair connected only by traversals still
/// draws (weight floor 1). First-seen order, so it stays deterministic. (Graph signals — edge weight.)
pub(crate) fn dedup_edges_weighted(graph: &Graph) -> Vec<(NodeKey, NodeKey, u32)> {
    let mut index: HashMap<(NodeKey, NodeKey), usize> = HashMap::new();
    let mut edges: Vec<(NodeKey, NodeKey, u32)> = Vec::new();
    for r in graph.relations() {
        let pair = if r.from <= r.to {
            (r.from, r.to)
        } else {
            (r.to, r.from)
        };
        let is_statement = !matches!(r.kind, RelationKind::Traversal);
        match index.get(&pair) {
            Some(&i) => {
                if is_statement {
                    edges[i].2 += 1;
                }
            }
            None => {
                index.insert(pair, edges.len());
                edges.push((r.from, r.to, u32::from(is_statement)));
            }
        }
    }
    // A pair connected only by traversal events still draws an edge (weight floor 1).
    for e in &mut edges {
        if e.2 == 0 {
            e.2 = 1;
        }
    }
    edges
}

// `build_simulation` / `sync_sim_with_graph` / `coupling_force_from_graph` /
// `seed_cluster` moved to `seiche_bridge` — the graph ↔ seiche adapter half of
// this file, split out at the size ceiling (and the home of the deferred
// coupling graph-resolution tests).

/// Build the pre-materialized node-children pool **once**: a `.stage` container
/// (the camera transform's element) holding one `.gnode` per graph node, each
/// labelled with its index and given an initial `transform` (so the per-frame
/// updates are value→value changes — paint-tier — not the first none→value one
/// that can relayout). Returns the DOM, the node→gnode map, and the stage id; the
/// frame loop mutates these elements' attributes and never rebuilds them.
pub(crate) fn build_pool_dom(
    graph: &Graph,
) -> (ScriptedDom, HashMap<NodeKey, DomNodeId>, DomNodeId) {
    let mut dom = ScriptedDom::new();
    let root = dom.document();

    let stage = dom.create_element(qual("div"));
    dom.set_attribute(stage, qual("class"), "stage");
    dom.set_attribute(
        stage,
        qual("style"),
        "transform: translate(0px, 0px) scale(1);",
    );
    dom.append_child(root, stage);

    let mut gnode_of = HashMap::new();
    for (key, _node) in graph.nodes() {
        let gnode = dom.create_element(qual("div"));
        dom.set_attribute(gnode, qual("class"), "gnode");
        dom.set_attribute(gnode, qual("style"), "transform: translate(0px, 0px);");
        // A human-meaningful label (an ingested entity reads as "publisher" /
        // "Organization", a page as its host, not a raw index or `urn:`), riding in a
        // caption beside the tile so a long name reads in full rather than overflowing
        // the 36px square. (Node legibility.)
        let caption = dom.create_element(qual("div"));
        dom.set_attribute(caption, qual("class"), "gcaption");
        let label = dom.create_text(&graph.node_display_label(key));
        dom.append_child(caption, label);
        dom.append_child(gnode, caption);
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
pub(crate) fn qual(local: &str) -> QualName {
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
// The canvas paints its own opaque backdrop as the bottom composite layer, so
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
/// camera transform) — the canvas's bottom composite layer.
pub(crate) fn background_cmds(w: u32, h: u32, color: ColorF) -> Vec<PaintCmd> {
    let rect = LayoutRect::new(LayoutPoint::zero(), LayoutPoint::new(w as f32, h as f32));
    vec![PaintCmd::DrawRect(RectItem {
        placement: CommonPlacement::new(rect),
        color,
    })]
}

/// A categorical palette for community rings: distinct hues legible on the dark backdrop, cycled
/// by cluster index (clusters past the palette wrap — adjacent ones rarely sit together). (Graph
/// signals — community to a ring.)
pub(crate) fn cluster_color(index: usize) -> ColorF {
    const PALETTE: [(f32, f32, f32); 8] = [
        (0.36, 0.72, 0.96), // sky
        (0.96, 0.55, 0.36), // coral
        (0.56, 0.86, 0.46), // green
        (0.86, 0.50, 0.90), // violet
        (0.96, 0.80, 0.34), // amber
        (0.45, 0.84, 0.80), // teal
        (0.92, 0.46, 0.60), // rose
        (0.66, 0.66, 0.96), // periwinkle
    ];
    let (r, g, b) = PALETTE[index % PALETTE.len()];
    ColorF::new(r, g, b, 0.85)
}

/// A stroked-circle path of `segments` chords approximating a ring at `center`. (Community rings.)
fn circle_path(center: LayoutPoint, radius: f32, segments: usize) -> PathData {
    let mut commands = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let theta = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let pt = LayoutPoint::new(
            center.x + radius * theta.cos(),
            center.y + radius * theta.sin(),
        );
        commands.push(if i == 0 {
            PathCommand::MoveTo(pt)
        } else {
            PathCommand::LineTo(pt)
        });
    }
    PathData { commands }
}

/// World-space **community ring** overlay: a halo around each node in the colour of its Louvain
/// community, so the partition reads as spatial clusters under any layout. Spliced inside the
/// underlay's camera transform (world space), like the selected-edge overlay. (Graph signals —
/// community to a ring.)
pub(crate) fn community_ring_overlay(
    view: &LayoutView,
    community: &ClusterSet,
    radius_of: impl Fn(NodeKey) -> f32,
) -> Vec<PaintCmd> {
    let mut cmds = Vec::new();
    for (i, cluster) in community.clusters.iter().enumerate() {
        let color = cluster_color(i);
        for &key in &cluster.members {
            let Some(p) = view.position_of(key) else {
                continue;
            };
            let r = radius_of(key) + 6.0;
            let center = LayoutPoint::new(p.x, p.y);
            let bounds = LayoutRect::new(
                LayoutPoint::new(center.x - r, center.y - r),
                LayoutPoint::new(center.x + r, center.y + r),
            );
            cmds.push(PaintCmd::DrawStroke(StrokeItem {
                placement: CommonPlacement::new(bounds),
                path: circle_path(center, r, 24),
                color,
                width: 2.5,
                cap: StrokeCap::Round,
                join: StrokeJoin::Round,
                dash: None,
            }));
        }
    }
    cmds
}

/// World-space **bridge ring** overlay: a bold ring around each structural broker (a high-betweenness
/// node), in one distinct colour and a larger radius than the community rings, so the connectors read
/// clearly even when both overlays are on at once. (Graph signals — bridges.)
pub(crate) fn bridge_ring_overlay(
    view: &LayoutView,
    bridges: &BridgeNodes,
    radius_of: impl Fn(NodeKey) -> f32,
) -> Vec<PaintCmd> {
    let mut cmds = Vec::new();
    // A bold near-white ring — distinct from the cluster palette and the orange selection.
    let color = ColorF::new(1.0, 1.0, 1.0, 0.92);
    for &key in &bridges.bridges {
        let Some(p) = view.position_of(key) else {
            continue;
        };
        let r = radius_of(key) + 9.0;
        let center = LayoutPoint::new(p.x, p.y);
        let bounds = LayoutRect::new(
            LayoutPoint::new(center.x - r, center.y - r),
            LayoutPoint::new(center.x + r, center.y + r),
        );
        cmds.push(PaintCmd::DrawStroke(StrokeItem {
            placement: CommonPlacement::new(bounds),
            path: circle_path(center, r, 24),
            color,
            width: 3.5,
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            dash: None,
        }));
    }
    cmds
}

/// Background paint for placed field regions, in **world space** (no transform) —
/// spliced *under* the canvas underlay so the graph appears to sit within each
/// field. A field with a `Region` extent draws as a soft radial-gradient "well"
/// (its `Disk` falloff made visible); its faint dashed extent square draws **only
/// while it is the `active` (hovered) field** — box-on-interaction, so the canvas
/// stays uncluttered at rest. `hidden` fields are skipped entirely; retired fields
/// too. (Field regions — disk-in-box visual + box-on-interaction.)
pub(crate) fn field_overlay(
    graph: &Graph,
    active: Option<FieldId>,
    hidden: &HashSet<FieldId>,
) -> Vec<PaintCmd> {
    let mut cmds = Vec::new();
    for field in graph.fields() {
        if !field.is_active() || hidden.contains(&field.id) {
            continue;
        }
        let FieldExtent::Region {
            min_x,
            min_y,
            max_x,
            max_y,
        } = field.extent
        else {
            continue;
        };
        let (w, h) = (max_x - min_x, max_y - min_y);
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        let rect = LayoutRect::new(
            LayoutPoint::new(min_x, min_y),
            LayoutPoint::new(max_x, max_y),
        );
        let (cx, cy) = ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
        let radius = w.min(h) / 2.0;
        // The soft falloff well: a translucent tint at the center fading to clear at
        // the disk radius (the field's `Disk` definition made visible).
        cmds.push(PaintCmd::DrawRadialGradient(RadialGradientItem {
            placement: CommonPlacement::new(rect),
            gradient: RadialGradientPayload {
                center: LayoutPoint::new(cx, cy),
                radius: LayoutSize::new(radius, radius),
                extend_mode: ExtendMode::Clamp,
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: ColorF::new(0.42, 0.62, 0.98, 0.20),
                    },
                    GradientStop {
                        offset: 1.0,
                        color: ColorF::new(0.42, 0.62, 0.98, 0.0),
                    },
                ],
            },
            tile_size: LayoutSize::new(w, h),
            tile_spacing: LayoutSize::zero(),
        }));
        // The faint dashed square (the field's extent) draws only while this is the
        // hovered field — box-on-interaction. Dashed to distinguish it from edges.
        if active == Some(field.id) {
            let corner = |x: f32, y: f32| LayoutPoint::new(x, y);
            cmds.push(PaintCmd::DrawStroke(StrokeItem {
                placement: CommonPlacement::new(rect),
                path: PathData {
                    commands: vec![
                        PathCommand::MoveTo(corner(min_x, min_y)),
                        PathCommand::LineTo(corner(max_x, min_y)),
                        PathCommand::LineTo(corner(max_x, max_y)),
                        PathCommand::LineTo(corner(min_x, max_y)),
                        PathCommand::LineTo(corner(min_x, min_y)),
                    ],
                },
                color: ColorF::new(0.62, 0.74, 0.98, 0.65),
                width: 1.5,
                cap: StrokeCap::Round,
                join: StrokeJoin::Round,
                dash: Some(DashPattern {
                    intervals: vec![7.0, 5.0],
                    offset: 0.0,
                }),
            }));
        }
    }
    cmds
}
