// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Footprints — the extent a projected item occupies, in its local space.
//!
//! The lesson of proof 1 (the overlapping spiral): a placement contract that
//! carries only positions cannot avoid collisions, because the solver cannot
//! see what it must clear. A footprint is the item-local answer. All variants
//! are centered on / anchored at the local origin; the item's transform
//! carries them into its space.

use serde::{Deserialize, Serialize};

use crate::geometry::{Rect, Size2, Vec2};

/// The extent an item occupies in its local space (origin = the item's
/// anchor point). `Point` is the degenerate footprint (position-only, the
/// pre-P2 world); solvers treat it as zero-extent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Footprint {
    /// No extent — a bare position.
    Point,
    /// A disc of `radius` centered on the origin (the cartography
    /// `PositionedNode.radius` migrates here).
    Circle { radius: f32 },
    /// An axis-aligned rectangle centered on the origin (cards, panes,
    /// tiles).
    Rect { size: Size2 },
    /// A closed polygon in local coordinates (hulls, map regions, sprite
    /// silhouettes). Winding is not significant; must have >= 3 points to
    /// be meaningful.
    Polygon { points: Vec<Vec2> },
    /// An open polyline of `width` (routes, paths, fingerings).
    Path { points: Vec<Vec2>, width: f32 },
}

impl Footprint {
    /// Local-space bounding rect. `None` for `Point` (zero extent) and for
    /// degenerate polygons/paths (fewer than the meaningful minimum).
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            Footprint::Point => None,
            Footprint::Circle { radius } => Some(Rect::new(
                Vec2::new(-radius, -radius),
                Size2::new(radius * 2.0, radius * 2.0),
            )),
            Footprint::Rect { size } => {
                Some(Rect::new(Vec2::new(-size.w / 2.0, -size.h / 2.0), *size))
            }
            Footprint::Polygon { points } if points.len() >= 3 => points_bounds(points),
            Footprint::Path { points, width } if points.len() >= 2 => {
                points_bounds(points).map(|r| {
                    let half = width / 2.0;
                    Rect::new(
                        Vec2::new(r.origin.x - half, r.origin.y - half),
                        Size2::new(r.size.w + width, r.size.h + width),
                    )
                })
            }
            _ => None,
        }
    }

    /// Whether `local` falls inside this footprint, in the same item-local
    /// coordinates the footprint is expressed in.
    ///
    /// [`Footprint::Point`] contains nothing: a zero-extent item has no area
    /// to strike. That is deliberate rather than an oversight. An item that
    /// should be clickable while drawing as a bare position supplies a
    /// separate hit shape (`ProjectedItem::hit`), which is exactly the case
    /// that field exists for. Degenerate polygons and paths behave the same
    /// way, matching [`Footprint::bounds`].
    pub fn contains(&self, local: Vec2) -> bool {
        match self {
            Footprint::Point => false,
            Footprint::Circle { radius } => {
                local.x * local.x + local.y * local.y <= radius * radius
            }
            Footprint::Rect { size } => {
                local.x.abs() <= size.w / 2.0 && local.y.abs() <= size.h / 2.0
            }
            Footprint::Polygon { points } if points.len() >= 3 => polygon_contains(points, local),
            Footprint::Path { points, width } if points.len() >= 2 => {
                let half = width / 2.0;
                points
                    .windows(2)
                    .any(|seg| distance_to_segment(local, seg[0], seg[1]) <= half)
            }
            _ => false,
        }
    }
}

/// Even-odd ray cast along +x. Points exactly on an edge are not guaranteed
/// either way, which is the usual and acceptable ambiguity for picking.
fn polygon_contains(points: &[Vec2], p: Vec2) -> bool {
    let mut inside = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let (a, b) = (points[i], points[j]);
        if (a.y > p.y) != (b.y > p.y) {
            let t = (p.y - a.y) / (b.y - a.y);
            if p.x < a.x + t * (b.x - a.x) {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len_sq = dx * dx + dy * dy;
    let t = if len_sq == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a.x + t * dx, a.y + t * dy);
    ((p.x - cx).powi(2) + (p.y - cy).powi(2)).sqrt()
}

fn points_bounds(points: &[Vec2]) -> Option<Rect> {
    let first = points.first()?;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (first.x, first.y, first.x, first.y);
    for p in &points[1..] {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    Some(Rect::new(
        Vec2::new(min_x, min_y),
        Size2::new(max_x - min_x, max_y - min_y),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_has_no_bounds() {
        assert_eq!(Footprint::Point.bounds(), None);
    }

    #[test]
    fn rect_bounds_center_on_origin() {
        let b = Footprint::Rect {
            size: Size2::new(4.0, 2.0),
        }
        .bounds()
        .unwrap();
        assert_eq!(b.origin, Vec2::new(-2.0, -1.0));
        assert_eq!(b.size, Size2::new(4.0, 2.0));
    }

    #[test]
    fn path_bounds_include_width() {
        let b = Footprint::Path {
            points: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)],
            width: 2.0,
        }
        .bounds()
        .unwrap();
        assert_eq!(b.origin, Vec2::new(-1.0, -1.0));
        assert_eq!(b.size, Size2::new(12.0, 2.0));
    }

    #[test]
    fn a_point_footprint_contains_nothing() {
        assert!(!Footprint::Point.contains(Vec2::ZERO));
    }

    #[test]
    fn rect_contains_inside_and_rejects_outside() {
        let r = Footprint::Rect {
            size: Size2::new(4.0, 2.0),
        };
        assert!(r.contains(Vec2::new(1.9, 0.9)));
        assert!(r.contains(Vec2::ZERO));
        assert!(!r.contains(Vec2::new(2.1, 0.0)));
        assert!(!r.contains(Vec2::new(0.0, 1.1)));
    }

    #[test]
    fn circle_contains_by_radius_not_bounding_box() {
        let c = Footprint::Circle { radius: 1.0 };
        // 0.7^2 + 0.7^2 = 0.98, inside; the box corner at 0.99 is not.
        assert!(c.contains(Vec2::new(0.7, 0.7)));
        assert!(!c.contains(Vec2::new(0.99, 0.99)));
    }

    #[test]
    fn polygon_contains_uses_the_shape_not_its_bounds() {
        // An L: the bounding-box corner at (9, 9) is outside the shape.
        let l = Footprint::Polygon {
            points: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(10.0, 0.0),
                Vec2::new(10.0, 2.0),
                Vec2::new(2.0, 2.0),
                Vec2::new(2.0, 10.0),
                Vec2::new(0.0, 10.0),
            ],
        };
        assert!(l.contains(Vec2::new(1.0, 5.0)));
        assert!(l.contains(Vec2::new(5.0, 1.0)));
        assert!(!l.contains(Vec2::new(9.0, 9.0)));
    }

    #[test]
    fn path_contains_within_half_width() {
        let p = Footprint::Path {
            points: vec![Vec2::ZERO, Vec2::new(10.0, 0.0)],
            width: 2.0,
        };
        assert!(p.contains(Vec2::new(5.0, 0.9)));
        assert!(!p.contains(Vec2::new(5.0, 1.1)));
        assert!(!p.contains(Vec2::new(11.5, 0.0)));
    }

    #[test]
    fn degenerate_polygon_has_no_bounds() {
        let two = Footprint::Polygon {
            points: vec![Vec2::ZERO, Vec2::new(1.0, 1.0)],
        };
        assert_eq!(two.bounds(), None);
    }
}
