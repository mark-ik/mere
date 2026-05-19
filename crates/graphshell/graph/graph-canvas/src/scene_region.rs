/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Scene region types: spatial zones that exert physics effects on nodes.
//!
//! Regions are authored in Arrange mode and applied during the scene runtime
//! pass. They are portable — no framework dependency. The host is responsible
//! for rendering them as overlays.

use euclid::default::{Point2D, Rect, Size2D, Vector2D};
use serde::{Deserialize, Serialize};

/// Opaque scene region identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SceneRegionId(pub u64);

/// Spatial shape for a scene region.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SceneRegionShape {
    Circle { center: Point2D<f32>, radius: f32 },
    Rect { rect: Rect<f32> },
}

impl SceneRegionShape {
    /// Center point of the shape.
    pub fn center(&self) -> Point2D<f32> {
        match self {
            Self::Circle { center, .. } => *center,
            Self::Rect { rect } => Point2D::new(
                rect.origin.x + rect.size.width * 0.5,
                rect.origin.y + rect.size.height * 0.5,
            ),
        }
    }

    /// Whether the shape contains a point.
    pub fn contains(&self, point: Point2D<f32>) -> bool {
        match self {
            Self::Circle { center, radius } => {
                let d = point - *center;
                d.x * d.x + d.y * d.y <= radius * radius
            }
            Self::Rect { rect } => rect.contains(point),
        }
    }

    /// Translate the shape by a delta.
    pub fn translate(&self, delta: Vector2D<f32>) -> Self {
        match *self {
            Self::Circle { center, radius } => Self::Circle {
                center: center + delta,
                radius,
            },
            Self::Rect { rect } => Self::Rect {
                rect: rect.translate(delta),
            },
        }
    }
}

/// Physics effect applied by a scene region to nodes within it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SceneRegionEffect {
    /// Pull nodes toward the region center.
    Attractor { strength: f32 },
    /// Push nodes away from the region center.
    Repulsor { strength: f32 },
    /// Slow down node movement toward the center (friction-like).
    Dampener { factor: f32 },
    /// Push nodes out of the region boundary (hard containment).
    Wall,
}

/// A scene region: a spatial zone with a physics effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneRegion {
    pub id: SceneRegionId,
    pub label: Option<String>,
    pub shape: SceneRegionShape,
    pub effect: SceneRegionEffect,
    pub visible: bool,
}

impl SceneRegion {
    pub fn circle(
        id: SceneRegionId,
        center: Point2D<f32>,
        radius: f32,
        effect: SceneRegionEffect,
    ) -> Self {
        Self {
            id,
            label: None,
            shape: SceneRegionShape::Circle { center, radius },
            effect,
            visible: true,
        }
    }

    pub fn rect(id: SceneRegionId, rect: Rect<f32>, effect: SceneRegionEffect) -> Self {
        Self {
            id,
            label: None,
            shape: SceneRegionShape::Rect { rect },
            effect,
            visible: true,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_visibility(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }
}

/// Resize handle for scene region editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SceneRegionResizeHandle {
    CircleRadius,
    RectTopLeft,
    RectTopRight,
    RectBottomLeft,
    RectBottomRight,
}

/// Minimum circle region radius.
pub const MIN_CIRCLE_RADIUS: f32 = 24.0;
/// Minimum rect region side length.
pub const MIN_RECT_SIDE: f32 = 48.0;

/// Resize a region shape by dragging a handle to a new pointer position.
pub fn resize_shape_to_pointer(
    shape: SceneRegionShape,
    handle: SceneRegionResizeHandle,
    pointer: Point2D<f32>,
) -> SceneRegionShape {
    match (shape, handle) {
        (SceneRegionShape::Circle { center, .. }, SceneRegionResizeHandle::CircleRadius) => {
            let d = pointer - center;
            let radius = (d.x * d.x + d.y * d.y).sqrt().max(MIN_CIRCLE_RADIUS);
            SceneRegionShape::Circle { center, radius }
        }
        (SceneRegionShape::Rect { rect }, handle) => {
            let (fixed, sign_x, sign_y) = match handle {
                SceneRegionResizeHandle::RectTopLeft => (
                    Point2D::new(
                        rect.origin.x + rect.size.width,
                        rect.origin.y + rect.size.height,
                    ),
                    -1.0f32,
                    -1.0,
                ),
                SceneRegionResizeHandle::RectTopRight => (
                    Point2D::new(rect.origin.x, rect.origin.y + rect.size.height),
                    1.0,
                    -1.0,
                ),
                SceneRegionResizeHandle::RectBottomLeft => (
                    Point2D::new(rect.origin.x + rect.size.width, rect.origin.y),
                    -1.0,
                    1.0,
                ),
                SceneRegionResizeHandle::RectBottomRight => (rect.origin, 1.0, 1.0),
                SceneRegionResizeHandle::CircleRadius => return SceneRegionShape::Rect { rect },
            };
            let mut moving = pointer;
            if (moving.x - fixed.x).abs() < MIN_RECT_SIDE {
                moving.x = fixed.x + sign_x * MIN_RECT_SIDE;
            }
            if (moving.y - fixed.y).abs() < MIN_RECT_SIDE {
                moving.y = fixed.y + sign_y * MIN_RECT_SIDE;
            }
            let min_x = fixed.x.min(moving.x);
            let min_y = fixed.y.min(moving.y);
            let max_x = fixed.x.max(moving.x);
            let max_y = fixed.y.max(moving.y);
            SceneRegionShape::Rect {
                rect: Rect::new(
                    Point2D::new(min_x, min_y),
                    Size2D::new(max_x - min_x, max_y - min_y),
                ),
            }
        }
        (shape, _) => shape,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_contains_center() {
        let shape = SceneRegionShape::Circle {
            center: Point2D::new(100.0, 100.0),
            radius: 50.0,
        };
        assert!(shape.contains(Point2D::new(100.0, 100.0)));
    }

    #[test]
    fn circle_contains_inside() {
        let shape = SceneRegionShape::Circle {
            center: Point2D::new(100.0, 100.0),
            radius: 50.0,
        };
        assert!(shape.contains(Point2D::new(130.0, 100.0)));
    }

    #[test]
    fn circle_rejects_outside() {
        let shape = SceneRegionShape::Circle {
            center: Point2D::new(100.0, 100.0),
            radius: 50.0,
        };
        assert!(!shape.contains(Point2D::new(200.0, 100.0)));
    }

    #[test]
    fn rect_contains_inside() {
        let shape = SceneRegionShape::Rect {
            rect: Rect::new(Point2D::new(50.0, 50.0), Size2D::new(100.0, 100.0)),
        };
        assert!(shape.contains(Point2D::new(100.0, 100.0)));
    }

    #[test]
    fn rect_rejects_outside() {
        let shape = SceneRegionShape::Rect {
            rect: Rect::new(Point2D::new(50.0, 50.0), Size2D::new(100.0, 100.0)),
        };
        assert!(!shape.contains(Point2D::new(200.0, 200.0)));
    }

    #[test]
    fn translate_circle() {
        let shape = SceneRegionShape::Circle {
            center: Point2D::new(100.0, 100.0),
            radius: 50.0,
        };
        let moved = shape.translate(Vector2D::new(10.0, -20.0));
        match moved {
            SceneRegionShape::Circle { center, radius } => {
                assert_eq!(center.x, 110.0);
                assert_eq!(center.y, 80.0);
                assert_eq!(radius, 50.0);
            }
            _ => panic!("expected circle"),
        }
    }

    #[test]
    fn shape_center_circle() {
        let shape = SceneRegionShape::Circle {
            center: Point2D::new(42.0, 73.0),
            radius: 10.0,
        };
        assert_eq!(shape.center(), Point2D::new(42.0, 73.0));
    }

    #[test]
    fn shape_center_rect() {
        let shape = SceneRegionShape::Rect {
            rect: Rect::new(Point2D::new(100.0, 200.0), Size2D::new(50.0, 80.0)),
        };
        assert_eq!(shape.center(), Point2D::new(125.0, 240.0));
    }

    #[test]
    fn region_construction() {
        let region = SceneRegion::circle(
            SceneRegionId(1),
            Point2D::new(100.0, 100.0),
            50.0,
            SceneRegionEffect::Attractor { strength: 0.5 },
        )
        .with_label("test zone")
        .with_visibility(false);
        assert_eq!(region.label.as_deref(), Some("test zone"));
        assert!(!region.visible);
    }

    #[test]
    fn serde_roundtrip_region() {
        let region = SceneRegion::circle(
            SceneRegionId(42),
            Point2D::new(10.0, 20.0),
            30.0,
            SceneRegionEffect::Wall,
        );
        let json = serde_json::to_string(&region).unwrap();
        let back: SceneRegion = serde_json::from_str(&json).unwrap();
        assert_eq!(region, back);
    }

    #[test]
    fn resize_circle_radius() {
        let shape = SceneRegionShape::Circle {
            center: Point2D::new(100.0, 100.0),
            radius: 50.0,
        };
        let resized = resize_shape_to_pointer(
            shape,
            SceneRegionResizeHandle::CircleRadius,
            Point2D::new(200.0, 100.0),
        );
        match resized {
            SceneRegionShape::Circle { center, radius } => {
                assert_eq!(center, Point2D::new(100.0, 100.0));
                assert!((radius - 100.0).abs() < 0.1);
            }
            _ => panic!("expected circle"),
        }
    }

    #[test]
    fn resize_circle_enforces_minimum() {
        let shape = SceneRegionShape::Circle {
            center: Point2D::new(100.0, 100.0),
            radius: 50.0,
        };
        let resized = resize_shape_to_pointer(
            shape,
            SceneRegionResizeHandle::CircleRadius,
            Point2D::new(101.0, 100.0),
        );
        match resized {
            SceneRegionShape::Circle { radius, .. } => {
                assert!(radius >= MIN_CIRCLE_RADIUS);
            }
            _ => panic!("expected circle"),
        }
    }
}
