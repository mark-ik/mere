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
            Footprint::Rect { size } => Some(Rect::new(
                Vec2::new(-size.w / 2.0, -size.h / 2.0),
                *size,
            )),
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
    fn degenerate_polygon_has_no_bounds() {
        let two = Footprint::Polygon {
            points: vec![Vec2::ZERO, Vec2::new(1.0, 1.0)],
        };
        assert_eq!(two.bounds(), None);
    }
}
