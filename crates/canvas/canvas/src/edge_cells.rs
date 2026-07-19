// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Relation-cell geometry for canvas edge display and picking.

use std::collections::{HashMap, HashSet};

use euclid::default::{Box2D, Point2D};
use seiche::LayoutView;
use kernel::graph::{EdgeFamily, Graph, NodeKey, RelationKind, RelationSelector};
use paint_list_api::{
    ColorF, CommonPlacement, LayoutPoint, LayoutRect, PaintCmd, PathCommand, PathData, StrokeCap,
    StrokeItem, StrokeJoin,
};

use crate::EdgeCell;

const CELL_FAN_GAP: f32 = 6.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct EdgeCellSegment {
    pub cell: EdgeCell,
    pub from: Point2D<f32>,
    pub to: Point2D<f32>,
    pub kind: RelationKind,
}

pub(crate) fn edge_cell_hit_test(
    graph: &Graph,
    view: &LayoutView,
    hidden_edges: &HashSet<EdgeCell>,
    point: Point2D<f32>,
    tolerance: f32,
) -> Option<EdgeCell> {
    let mut best: Option<(EdgeCell, f32)> = None;
    for segment in visible_edge_cell_segments(graph, view, hidden_edges) {
        let d = point_segment_distance(point, segment.from, segment.to);
        if d <= tolerance && best.is_none_or(|(_, best_d)| d < best_d) {
            best = Some((segment.cell, d));
        }
    }
    best.map(|(cell, _)| cell)
}

pub(crate) fn edge_cells_in_rect(
    graph: &Graph,
    view: &LayoutView,
    hidden_edges: &HashSet<EdgeCell>,
    region: Box2D<f32>,
) -> Vec<EdgeCell> {
    visible_edge_cell_segments(graph, view, hidden_edges)
        .into_iter()
        .filter(|segment| segment_intersects_box(segment.from, segment.to, region))
        .map(|segment| segment.cell)
        .collect()
}

pub(crate) fn relation_cell_overlay(
    graph: &Graph,
    view: &LayoutView,
    hidden_edges: &HashSet<EdgeCell>,
) -> Vec<PaintCmd> {
    visible_edge_cell_segments(graph, view, hidden_edges)
        .into_iter()
        .map(|segment| stroke(segment.from, segment.to, relation_color(segment.kind), 1.8))
        .collect()
}

/// Highlight selected relation cells in world space, spliced inside the underlay's camera
/// transform so it uses the same pan/zoom/orbit as the base edge pass.
pub(crate) fn selected_edge_overlay(
    graph: &Graph,
    view: &LayoutView,
    hidden_edges: &HashSet<EdgeCell>,
    selected_edges: &HashSet<EdgeCell>,
) -> Vec<PaintCmd> {
    visible_edge_cell_segments(graph, view, hidden_edges)
        .into_iter()
        .filter(|segment| selected_edges.contains(&segment.cell))
        .map(|segment| {
            stroke(
                segment.from,
                segment.to,
                ColorF::new(0.91, 0.59, 0.16, 1.0),
                4.0,
            )
        })
        .collect()
}

pub(crate) fn visible_edge_cell_segments(
    graph: &Graph,
    view: &LayoutView,
    hidden_edges: &HashSet<EdgeCell>,
) -> Vec<EdgeCellSegment> {
    let mut groups: HashMap<(NodeKey, NodeKey), Vec<(EdgeCell, RelationKind)>> = HashMap::new();
    for relation in graph.relations() {
        let cell = edge_cell_for_relation(relation.from, relation.to, relation.kind);
        if hidden_edges.contains(&cell) {
            continue;
        }
        let pair = endpoint_pair(relation.from, relation.to);
        groups.entry(pair).or_default().push((cell, relation.kind));
    }

    let mut segments = Vec::new();
    for (a, b, pa, pb) in view.edge_segments() {
        let pair = endpoint_pair(a, b);
        let Some(cells) = groups.get(&pair) else {
            continue;
        };
        for (index, &(cell, kind)) in cells.iter().enumerate() {
            let (from, to) = fan_segment(pa, pb, index, cells.len());
            segments.push(EdgeCellSegment {
                cell,
                from,
                to,
                kind,
            });
        }
    }
    segments
}

pub(crate) fn selector_for_relation_kind(kind: RelationKind) -> RelationSelector {
    match kind {
        RelationKind::Semantic(sub) => RelationSelector::Semantic(sub),
        RelationKind::Traversal => RelationSelector::Family(EdgeFamily::Traversal),
        RelationKind::Containment(sub) => RelationSelector::Containment(sub),
        RelationKind::Arrangement(sub) => RelationSelector::Arrangement(sub),
        RelationKind::Imported(sub) => RelationSelector::Imported(sub),
        RelationKind::Provenance(sub) => RelationSelector::Provenance(sub),
    }
}

pub(crate) fn edge_cell_for_relation(from: NodeKey, to: NodeKey, kind: RelationKind) -> EdgeCell {
    EdgeCell {
        from,
        to,
        selector: selector_for_relation_kind(kind),
    }
}

fn endpoint_pair(a: NodeKey, b: NodeKey) -> (NodeKey, NodeKey) {
    if a <= b { (a, b) } else { (b, a) }
}

fn fan_segment(
    from: Point2D<f32>,
    to: Point2D<f32>,
    index: usize,
    count: usize,
) -> (Point2D<f32>, Point2D<f32>) {
    if count <= 1 {
        return (from, to);
    }
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        return (from, to);
    }
    let normal = (-dy / len, dx / len);
    let centered_index = index as f32 - (count as f32 - 1.0) * 0.5;
    let offset = centered_index * CELL_FAN_GAP;
    (
        Point2D::new(from.x + normal.0 * offset, from.y + normal.1 * offset),
        Point2D::new(to.x + normal.0 * offset, to.y + normal.1 * offset),
    )
}

fn stroke(from: Point2D<f32>, to: Point2D<f32>, color: ColorF, width: f32) -> PaintCmd {
    let p0 = LayoutPoint::new(from.x, from.y);
    let p1 = LayoutPoint::new(to.x, to.y);
    let bounds = LayoutRect::new(
        LayoutPoint::new(p0.x.min(p1.x), p0.y.min(p1.y)),
        LayoutPoint::new(p0.x.max(p1.x), p0.y.max(p1.y)),
    );
    PaintCmd::DrawStroke(StrokeItem {
        placement: CommonPlacement::new(bounds),
        path: PathData {
            commands: vec![PathCommand::MoveTo(p0), PathCommand::LineTo(p1)],
        },
        color,
        width,
        cap: StrokeCap::Round,
        join: StrokeJoin::Round,
        dash: None,
    })
}

fn relation_color(kind: RelationKind) -> ColorF {
    match kind.family() {
        EdgeFamily::Semantic => ColorF::new(0.93, 0.70, 0.34, 0.76),
        EdgeFamily::Traversal => ColorF::new(0.55, 0.72, 0.92, 0.62),
        EdgeFamily::Containment => ColorF::new(0.47, 0.80, 0.58, 0.70),
        EdgeFamily::Arrangement => ColorF::new(0.75, 0.63, 0.92, 0.70),
        EdgeFamily::Imported => ColorF::new(0.50, 0.78, 0.82, 0.68),
        EdgeFamily::Provenance => ColorF::new(0.90, 0.58, 0.68, 0.72),
    }
}

fn point_segment_distance(p: Point2D<f32>, a: Point2D<f32>, b: Point2D<f32>) -> f32 {
    let ab = b - a;
    let len2 = ab.square_length();
    if len2 <= f32::EPSILON {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    let proj = a + ab * t;
    (p - proj).length()
}

fn segment_intersects_box(a: Point2D<f32>, b: Point2D<f32>, r: Box2D<f32>) -> bool {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let p = [-dx, dx, -dy, dy];
    let q = [a.x - r.min.x, r.max.x - a.x, a.y - r.min.y, r.max.y - a.y];
    let mut u1 = 0.0_f32;
    let mut u2 = 1.0_f32;
    for i in 0..4 {
        if p[i].abs() < f32::EPSILON {
            if q[i] < 0.0 {
                return false;
            }
        } else {
            let t = q[i] / p[i];
            if p[i] < 0.0 {
                if t > u2 {
                    return false;
                }
                if t > u1 {
                    u1 = t;
                }
            } else {
                if t < u1 {
                    return false;
                }
                if t < u2 {
                    u2 = t;
                }
            }
        }
    }
    u1 <= u2
}
