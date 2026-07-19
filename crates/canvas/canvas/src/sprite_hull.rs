// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sprite collider-hull tracing: RGBA pixels → a face-normalized convex hull.
//!
//! Promoted from meerkat's `sprite_import.rs` at the meerkat harvest
//! (2026-07-18) so every canvas host shares one tracer beside
//! [`set_node_sprite_hull`](crate::Canvas::set_node_sprite_hull) — a dropped
//! sprite collides at its picture, not a bounding box, in any host that
//! decodes an image and calls [`trace_sprite_hull`]. (Node rep P2 — hull.)

/// Trace the sprite's opaque region to a face-normalized convex hull for the
/// collider. The image is treated cover-fit, so only the centered square (the
/// visible part of a non-square image) is sampled, on a grid bounded to ~64
/// cells per side so the hull stays small. Returns points in `[-0.5, 0.5]`; a
/// fully transparent / tiny image yields fewer than 3 points, which the canvas
/// treats as "no hull" (the node keeps its silhouette collider).
pub fn trace_sprite_hull(rgba: &[u8], w: u32, h: u32) -> Vec<(f32, f32)> {
    let (w, h) = (w as i32, h as i32);
    if w < 1 || h < 1 {
        return Vec::new();
    }
    let side = w.min(h);
    let (ox, oy) = ((w - side) / 2, (h - side) / 2);
    let step = (side / 64).max(1);
    const ALPHA_THRESHOLD: u8 = 24;
    let mut pts: Vec<(f32, f32)> = Vec::new();
    let mut sy = 0;
    while sy < side {
        let mut sx = 0;
        while sx < side {
            let idx = (((oy + sy) * w + (ox + sx)) * 4 + 3) as usize;
            if rgba.get(idx).copied().unwrap_or(0) > ALPHA_THRESHOLD {
                pts.push((sx as f32 / side as f32 - 0.5, sy as f32 / side as f32 - 0.5));
            }
            sx += step;
        }
        sy += step;
    }
    // Simplify the convex hull so a *curved* sprite (a wifi fan, a rounded
    // gamepad) yields a handful of draggable vertices, not the dozens its arc
    // would otherwise produce.
    simplify_hull(convex_hull(&pts), HULL_SIMPLIFY_TOL)
}

/// Max deviation (face-normalized units, `[-0.5, 0.5]` space) a hull vertex may
/// sit from the line between its neighbors before it is dropped — ~2% of the face.
const HULL_SIMPLIFY_TOL: f32 = 0.02;

/// Decimate a closed convex polygon: repeatedly drop the vertex closest to the
/// line between its neighbors, until every remaining vertex deviates by more
/// than `tol` (or only 4 remain). Turns the many near-collinear vertices a
/// curve's hull produces into the few that actually shape it, so drag handles
/// stay manageable and the collider stays cheap.
fn simplify_hull(mut hull: Vec<(f32, f32)>, tol: f32) -> Vec<(f32, f32)> {
    while hull.len() > 4 {
        let n = hull.len();
        let mut min_dev = f32::INFINITY;
        let mut min_i = 0;
        for i in 0..n {
            let dev = perp_distance(hull[i], hull[(i + n - 1) % n], hull[(i + 1) % n]);
            if dev < min_dev {
                min_dev = dev;
                min_i = i;
            }
        }
        if min_dev > tol {
            break;
        }
        hull.remove(min_i);
    }
    hull
}

/// Perpendicular distance from point `p` to the line through `a`–`b`
/// (degenerate `a == b` falls back to the distance to `a`).
fn perp_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt();
    }
    ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / len
}

/// Andrew's monotone-chain convex hull. Returns the hull vertices
/// counter-clockwise, collinear runs collapsed to their endpoints (so a full
/// square yields 4 points). Fewer than 3 input points returns them as-is (the
/// caller treats < 3 as "no hull").
fn convex_hull(points: &[(f32, f32)]) -> Vec<(f32, f32)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: (f32, f32), a: (f32, f32), b: (f32, f32)| {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    };
    let mut hull: Vec<(f32, f32)> = Vec::with_capacity(pts.len() + 1);
    // Lower hull (left to right).
    for &p in &pts {
        while hull.len() >= 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
            hull.pop();
        }
        hull.push(p);
    }
    // Upper hull (right to left), not re-adding the rightmost point.
    let lower_len = hull.len() + 1;
    for &p in pts.iter().rev().skip(1) {
        while hull.len() >= lower_len && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0
        {
            hull.pop();
        }
        hull.push(p);
    }
    // The last point closes back to the first — drop the duplicate.
    hull.pop();
    hull
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fully opaque square traces to (about) the 4 face corners.
    #[test]
    fn opaque_square_traces_to_four_corners() {
        let (w, h) = (64u32, 64u32);
        let rgba = vec![255u8; (w * h * 4) as usize];
        let hull = trace_sprite_hull(&rgba, w, h);
        assert!(hull.len() >= 3 && hull.len() <= 6, "a square is a few points: {hull:?}");
        // Extremes reach (nearly) the face bounds.
        let max_x = hull.iter().map(|p| p.0).fold(f32::MIN, f32::max);
        let min_x = hull.iter().map(|p| p.0).fold(f32::MAX, f32::min);
        assert!(min_x <= -0.4 && max_x >= 0.4, "spans the face: {hull:?}");
    }

    /// A transparent image yields no hull (the caller keeps the silhouette).
    #[test]
    fn transparent_image_yields_no_hull() {
        let rgba = vec![0u8; 64 * 64 * 4];
        assert!(trace_sprite_hull(&rgba, 64, 64).len() < 3);
    }

    /// Simplification keeps a square a square but decimates a near-circle.
    #[test]
    fn simplify_decimates_curves_but_keeps_corners() {
        let square = vec![(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];
        assert_eq!(simplify_hull(square.clone(), 0.02), square);
        let circle: Vec<(f32, f32)> = (0..64)
            .map(|i| {
                let a = i as f32 / 64.0 * std::f32::consts::TAU;
                (0.5 * a.cos(), 0.5 * a.sin())
            })
            .collect();
        let simplified = simplify_hull(convex_hull(&circle), 0.02);
        assert!(simplified.len() < 16, "a circle decimates: {}", simplified.len());
        assert!(simplified.len() >= 4);
    }
}
