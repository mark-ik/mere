// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! The orrery's `GraphCanvas` painter — the spatial graph view's draw logic.
//!
//! Per the composition spine: the orrery is the **Cartography projection** of
//! graph truth. This is the first real rendering of that projection — a custom
//! Masonry `Canvas` (via Xilem's `canvas` view) paints the kernel graph
//! directly: nodes as labeled discs, relations as connecting lines.
//!
//! v1 scope (small, grows on demand): a **ring layout** computed at paint time
//! (ignores stored positions so it's always visible regardless of seed
//! geometry), real edges drawn from [`Graph::relations`], parley text labels.
//! Real cartography layout (force-directed / radial via `graph-layout` /
//! `cartography`), camera pan-zoom, LOD, and hit-testing / drag are later
//! slices — this proves graph-truth → spatial paint end to end.

use std::collections::HashMap;

use kernel::graph::Graph;
use xilem::masonry::core::{MutateCtx, render_text};
use xilem::masonry::imaging::Painter;
use xilem::masonry::imaging::record::Scene;
use xilem::masonry::kurbo::{Affine, Circle, Line, Point, Size, Stroke};
use xilem::masonry::parley::{Alignment, AlignmentOptions, StyleProperty};
use xilem::masonry::peniko::Color;

const NODE_FILL: Color = Color::from_rgb8(70, 110, 200);
const NODE_RING: Color = Color::from_rgb8(220, 228, 245);
const EDGE_COLOR: Color = Color::from_rgb8(120, 130, 150);
const LABEL_COLOR: Color = Color::from_rgb8(228, 233, 242);
const LABEL_SIZE: f32 = 11.0;

/// Paint `graph` into the canvas `scene` (already cleared by the canvas view).
/// `ctx` provides the text contexts for label layout.
pub fn paint_graph(scene: &mut Scene, ctx: &mut MutateCtx<'_>, size: Size, graph: &Graph) {
    let keys: Vec<_> = graph.nodes().map(|(k, _)| k).collect();
    let n = keys.len();
    if n == 0 || size.width <= 0.0 || size.height <= 0.0 {
        return;
    }

    // Ring layout at paint time → a position per node, plus a lookup for edges.
    let mut pos_of = HashMap::new();
    for (i, &k) in keys.iter().enumerate() {
        pos_of.insert(k, ring_point(i, n, size));
    }
    let radius = node_radius(n);

    let mut painter = Painter::new(scene);

    // Edges first, so node discs sit on top of the lines.
    let edge_stroke = Stroke::new(1.5);
    for rel in graph.relations() {
        if let (Some(&a), Some(&b)) = (pos_of.get(&rel.from), pos_of.get(&rel.to)) {
            if a != b {
                painter.stroke(Line::new(a, b), &edge_stroke, EDGE_COLOR).draw();
            }
        }
    }

    // Node discs.
    let ring_stroke = Stroke::new(2.0);
    for &center in pos_of.values() {
        let disc = Circle::new(center, radius);
        painter.fill(disc, NODE_FILL).draw();
        painter.stroke(disc, &ring_stroke, NODE_RING).draw();
    }

    // Labels last, above the discs.
    for (k, node) in graph.nodes() {
        if let Some(&center) = pos_of.get(&k) {
            let text = if node.title.is_empty() {
                node.url().to_string()
            } else {
                node.title.clone()
            };
            draw_label(&mut painter, ctx, &text, center, radius);
        }
    }
}

/// Position of node `i` of `n`, evenly spaced on a ring centered in `size`,
/// starting at the top (12 o'clock).
fn ring_point(i: usize, n: usize, size: Size) -> Point {
    let cx = size.width / 2.0;
    let cy = size.height / 2.0;
    let radius = size.width.min(size.height) * 0.36;
    let theta =
        std::f64::consts::TAU * (i as f64) / (n as f64) - std::f64::consts::FRAC_PI_2;
    Point::new(cx + radius * theta.cos(), cy + radius * theta.sin())
}

/// Disc radius, shrinking a little as the node count grows so a small ring
/// doesn't overlap.
fn node_radius(n: usize) -> f64 {
    (24.0 - n as f64 * 0.7).clamp(11.0, 24.0)
}

/// Draw a centered text label just below the node disc at `center`.
fn draw_label(
    painter: &mut Painter<'_, Scene>,
    ctx: &mut MutateCtx<'_>,
    text: &str,
    center: Point,
    radius: f64,
) {
    let (fcx, lcx) = ctx.text_contexts();
    let mut builder = lcx.ranged_builder(fcx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(LABEL_SIZE));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout.align(Alignment::Start, AlignmentOptions::default());

    let x = center.x - layout.width() as f64 / 2.0;
    let y = center.y + radius + 3.0;
    render_text(
        painter,
        Affine::translate((x, y)),
        &layout,
        &[LABEL_COLOR.into()],
        true,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_points_stay_inside_the_canvas() {
        let size = Size::new(400.0, 300.0);
        for i in 0..6 {
            let p = ring_point(i, 6, size);
            assert!(p.x >= 0.0 && p.x <= size.width, "x in bounds: {}", p.x);
            assert!(p.y >= 0.0 && p.y <= size.height, "y in bounds: {}", p.y);
        }
    }

    #[test]
    fn first_node_sits_at_top_center() {
        let size = Size::new(400.0, 300.0);
        let p = ring_point(0, 6, size);
        // Starts at 12 o'clock: centered horizontally, above the middle.
        assert!((p.x - 200.0).abs() < 1e-6);
        assert!(p.y < 150.0);
    }

    #[test]
    fn node_radius_is_clamped() {
        assert!((node_radius(1) - 23.3).abs() < 1e-9);
        assert_eq!(node_radius(200), 11.0); // floor
        assert!(node_radius(6) > 11.0 && node_radius(6) < 24.0);
    }
}
