/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Drag-and-drop sprite import (node-rep P2 — sprite): a dropped image file becomes a node's
//! custom sprite face. The `image` crate decodes the file (png / jpeg / gif / bmp), it is
//! downscaled to a face-sized thumbnail, encoded to a PNG data-URI (the favicon encoder), and
//! stored on the node under the cursor (else the focused node) via [`Orrery::set_node_sprite`].
//! Wired by the `WindowEvent::DroppedFile` arm in `app_handler`. See the
//! `2026-06-18_node_representation_arrangement_plan` (P2 — sprite).

use std::path::Path;

use crate::WindowCtx;
use crate::render::favicon_data_uri;

/// The largest face-texture dimension a dropped sprite is downscaled to, so the per-node
/// data-URI stays small: the face draws at ~24-120px (the size tiers), so a full-res photo
/// would bloat the snapshot for no visible gain. `thumbnail` keeps aspect within the box.
const SPRITE_MAX: u32 = 256;

impl WindowCtx<'_> {
    /// Import a dropped image file as the sprite face of the node under the cursor (where the
    /// file landed), falling back to the focused node when the drop is not over a node. A
    /// decode failure (unsupported format / unreadable file) or no target node is a no-op with
    /// a log line. (Node representation P2 — sprite drop.)
    pub(super) fn import_sprite_from_file(&mut self, path: &Path) {
        let Some((data_uri, hull)) = decode_sprite(path) else {
            tracing::warn!(path = %path.display(), "sprite import: could not decode the dropped file");
            return;
        };
        // The drop target: the node under the cursor, else the focused node. On platforms that
        // don't track the cursor mid file-drag, the last cursor position is usually the
        // just-clicked node, which is also the focused one, so the two agree in practice.
        let (cx, cy) = self.view.cursor;
        let (ox, oy) = self.orrery_point(cx, cy);
        let target = self
            .orrery()
            .node_at_screen(ox, oy)
            .or_else(|| self.orrery().focused_member());
        let Some(target) = target else {
            tracing::info!("sprite import: no node under the drop and none focused; ignoring");
            return;
        };
        // The face image, then the collider hull traced from the same pixels (so the node
        // collides at its picture, not a bounding box). `set_node_sprite_hull` pushes the new
        // physics geometry. (Node-rep P2 — sprite + sprite hull.)
        self.orrery_mut().set_node_sprite(target, data_uri);
        self.orrery_mut().set_node_sprite_hull(target, hull);
        self.view.request_redraw();
    }
}

/// Decode an image file once into its face data-URI **and** its collider hull: downscale to a
/// face-sized thumbnail, encode a PNG data-URI for the face, and trace the opaque region to a
/// convex hull for the collider. `None` if the file can't be decoded. (Node rep P2 — sprite.)
fn decode_sprite(path: &Path) -> Option<(String, Vec<(f32, f32)>)> {
    let rgba = image::open(path)
        .ok()?
        .thumbnail(SPRITE_MAX, SPRITE_MAX)
        .to_rgba8();
    let (w, h) = rgba.dimensions();
    let data_uri = favicon_data_uri(rgba.as_raw(), w, h)?;
    let hull = sprite_hull(rgba.as_raw(), w, h);
    Some((data_uri, hull))
}

/// Trace the sprite's opaque region to a face-normalized convex hull for the collider. The
/// image is treated cover-fit, so only the centered square (the visible part of a non-square
/// image) is sampled, on a grid bounded to ~64 cells per side so the hull stays small. Returns
/// points in [-0.5, 0.5]; a fully transparent / tiny image yields fewer than 3 points, which
/// the orrery treats as "no hull" (the node keeps its silhouette collider). (Node rep P2 — hull.)
fn sprite_hull(rgba: &[u8], w: u32, h: u32) -> Vec<(f32, f32)> {
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
    // Simplify the convex hull so a *curved* sprite (a wifi fan, a rounded gamepad) yields a
    // handful of draggable vertices, not the dozens its arc would otherwise produce.
    simplify_hull(convex_hull(&pts), HULL_SIMPLIFY_TOL)
}

/// Max deviation (face-normalized units, `[-0.5, 0.5]` space) a hull vertex may sit from the
/// line between its neighbors before it is dropped — ~2% of the face. (Node rep P2 — hull.)
const HULL_SIMPLIFY_TOL: f32 = 0.02;

/// Decimate a closed convex polygon: repeatedly drop the vertex closest to the line between its
/// neighbors, until every remaining vertex deviates by more than `tol` (or only 4 remain). Turns
/// the many near-collinear vertices a curve's hull produces into the few that actually shape it,
/// so the swatch's drag handles stay manageable and the parry collider stays cheap.
/// (Node rep P2 — sprite hull simplification.)
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

/// Perpendicular distance from point `p` to the line through `a`–`b` (degenerate `a==b` falls
/// back to the distance to `a`).
fn perp_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt();
    }
    ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / len
}

/// Andrew's monotone-chain convex hull. Returns the hull vertices counter-clockwise, collinear
/// runs collapsed to their endpoints (so a full square yields 4 points). Fewer than 3 input
/// points returns them as-is (the caller treats < 3 as "no hull"). (Node rep P2 — sprite hull.)
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
    use super::{convex_hull, simplify_hull};

    #[test]
    fn simplify_hull_drops_near_collinear_vertices() {
        // A square with a near-collinear midpoint on each edge (deviating ~0.01) — the midpoints
        // collapse, leaving the four real corners (a curved sprite's dozens behave the same way).
        let hull = vec![
            (-0.5, -0.5),
            (0.0, -0.49),
            (0.5, -0.5),
            (0.49, 0.0),
            (0.5, 0.5),
            (0.0, 0.49),
            (-0.5, 0.5),
            (-0.49, 0.0),
        ];
        assert_eq!(
            simplify_hull(hull, 0.05).len(),
            4,
            "near-collinear edge midpoints collapse"
        );
    }

    #[test]
    fn simplify_hull_keeps_small_hulls() {
        // A triangle and a quad are already minimal — nothing is dropped (the floor is 4).
        let tri = vec![(0.0, 0.0), (1.0, 0.0), (0.5, 1.0)];
        assert_eq!(simplify_hull(tri, 0.1).len(), 3);
    }

    #[test]
    fn convex_hull_of_a_filled_square_is_four_corners() {
        // A 5x5 grid of points fills a square; the hull collapses its collinear edges to the
        // four corners — what a fully-opaque (rectangular) sprite should produce.
        let mut pts = Vec::new();
        for i in 0..=4 {
            for j in 0..=4 {
                pts.push((i as f32, j as f32));
            }
        }
        assert_eq!(
            convex_hull(&pts).len(),
            4,
            "a filled square hulls to 4 corners"
        );
    }

    #[test]
    fn convex_hull_drops_interior_points() {
        // The interior point (2,2) is not a hull vertex.
        let pts = vec![(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0), (2.0, 2.0)];
        assert_eq!(convex_hull(&pts).len(), 4, "the interior point is dropped");
    }

    #[test]
    fn convex_hull_passes_through_too_few_points() {
        // Fewer than 3 points can't form a polygon; returned as-is for the caller to reject.
        assert_eq!(convex_hull(&[(0.0, 0.0), (1.0, 1.0)]).len(), 2);
        assert_eq!(convex_hull(&[]).len(), 0);
    }
}
