// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Camera + scene geometry for the orrery's `GraphCanvas` — the pure,
//! window-free half (so it's unit-testable without a running widget).
//!
//! Two coordinate spaces:
//! - **world** — graph node positions (a stable coordinate space; the kernel
//!   graph owns them). Independent of canvas size or zoom.
//! - **screen** — widget-local pixels.
//!
//! [`Camera`] maps between them. The orrery widget owns one camera; pan moves
//! [`Camera::center`], zoom scales around the cursor ([`Camera::zoom_around`]).

use xilem::masonry::kurbo::{Point, Size, Vec2};

/// A node positioned in world space, with its display label.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneNode {
    /// Stable kernel node id — the identity carried back in `NodeMoved`.
    pub id: uuid::Uuid,
    /// World-space position.
    pub world: Point,
    pub label: String,
}

/// The orrery's render input: nodes in world space + edges as index pairs into
/// [`Self::nodes`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphScene {
    pub nodes: Vec<SceneNode>,
    pub edges: Vec<(usize, usize)>,
}

/// Maps world ↔ screen. `center` is the world point shown at the viewport
/// middle; `zoom` is screen-pixels-per-world-unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub center: Point,
    pub zoom: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            center: Point::ZERO,
            zoom: 1.0,
        }
    }
}

/// Zoom is clamped to this range so the view can't invert or run away.
pub const MIN_ZOOM: f64 = 0.15;
pub const MAX_ZOOM: f64 = 8.0;

impl Camera {
    fn viewport_center(size: Size) -> Vec2 {
        Vec2::new(size.width / 2.0, size.height / 2.0)
    }

    /// World → widget-local pixels.
    pub fn world_to_screen(&self, world: Point, size: Size) -> Point {
        let c = Self::viewport_center(size);
        Point::new(
            c.x + (world.x - self.center.x) * self.zoom,
            c.y + (world.y - self.center.y) * self.zoom,
        )
    }

    /// Widget-local pixels → world.
    pub fn screen_to_world(&self, screen: Point, size: Size) -> Point {
        let c = Self::viewport_center(size);
        Point::new(
            self.center.x + (screen.x - c.x) / self.zoom,
            self.center.y + (screen.y - c.y) / self.zoom,
        )
    }

    /// Pan by a screen-space delta (drag the empty canvas). Dragging right
    /// moves the view's center left, so content follows the cursor.
    pub fn pan_by_screen(&mut self, delta_screen: Vec2) {
        self.center -= delta_screen / self.zoom;
    }

    /// Multiply zoom by `factor`, keeping the world point under `screen` fixed
    /// (zoom toward the cursor). Clamped to [`MIN_ZOOM`, `MAX_ZOOM`].
    pub fn zoom_around(&mut self, screen: Point, size: Size, factor: f64) {
        let before = self.screen_to_world(screen, size);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let after = self.screen_to_world(screen, size);
        self.center += before - after;
    }
}

/// The node nearest `screen` within `screen_radius`, if any (topmost-feel: ties
/// resolve to the later node, which paints last / on top).
pub fn hit_test(
    scene: &GraphScene,
    camera: &Camera,
    size: Size,
    screen: Point,
    screen_radius: f64,
) -> Option<usize> {
    let mut best: Option<usize> = None;
    let mut best_d = screen_radius * screen_radius;
    for (i, node) in scene.nodes.iter().enumerate() {
        let s = camera.world_to_screen(node.world, size);
        let d = (s.x - screen.x).powi(2) + (s.y - screen.y).powi(2);
        if d <= best_d {
            best_d = d;
            best = Some(i);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(points: &[(f64, f64)]) -> GraphScene {
        GraphScene {
            nodes: points
                .iter()
                .enumerate()
                .map(|(i, &(x, y))| SceneNode {
                    id: uuid::Uuid::from_u128(i as u128 + 1),
                    world: Point::new(x, y),
                    label: format!("n{i}"),
                })
                .collect(),
            edges: Vec::new(),
        }
    }

    #[test]
    fn world_screen_round_trips() {
        let size = Size::new(400.0, 300.0);
        let cam = Camera {
            center: Point::new(10.0, -20.0),
            zoom: 1.7,
        };
        let w = Point::new(33.0, 44.0);
        let back = cam.screen_to_world(cam.world_to_screen(w, size), size);
        assert!((back.x - w.x).abs() < 1e-9 && (back.y - w.y).abs() < 1e-9);
    }

    #[test]
    fn center_maps_to_viewport_middle() {
        let size = Size::new(400.0, 300.0);
        let cam = Camera {
            center: Point::new(5.0, 5.0),
            zoom: 2.0,
        };
        let s = cam.world_to_screen(cam.center, size);
        assert!((s.x - 200.0).abs() < 1e-9 && (s.y - 150.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_around_keeps_cursor_world_point_fixed() {
        let size = Size::new(400.0, 300.0);
        let mut cam = Camera::default();
        let cursor = Point::new(120.0, 80.0);
        let before = cam.screen_to_world(cursor, size);
        cam.zoom_around(cursor, size, 1.5);
        let after = cam.screen_to_world(cursor, size);
        assert!((before.x - after.x).abs() < 1e-9 && (before.y - after.y).abs() < 1e-9);
        assert!((cam.zoom - 1.5).abs() < 1e-9);
    }

    #[test]
    fn zoom_is_clamped() {
        let size = Size::new(400.0, 300.0);
        let mut cam = Camera::default();
        for _ in 0..40 {
            cam.zoom_around(Point::new(200.0, 150.0), size, 2.0);
        }
        assert!(cam.zoom <= MAX_ZOOM + 1e-9);
        for _ in 0..80 {
            cam.zoom_around(Point::new(200.0, 150.0), size, 0.5);
        }
        assert!(cam.zoom >= MIN_ZOOM - 1e-9);
    }

    #[test]
    fn pan_shifts_center_in_world_units() {
        let mut cam = Camera {
            center: Point::ZERO,
            zoom: 2.0,
        };
        cam.pan_by_screen(Vec2::new(20.0, 0.0));
        // 20 screen px at zoom 2 = 10 world units, center moves left.
        assert!((cam.center.x + 10.0).abs() < 1e-9);
    }

    #[test]
    fn hit_test_picks_node_under_cursor_and_misses_empty_space() {
        let size = Size::new(400.0, 300.0);
        let cam = Camera::default();
        let sc = scene(&[(0.0, 0.0), (50.0, 0.0)]);
        // Node 0 sits at world origin → viewport middle (200,150).
        assert_eq!(hit_test(&sc, &cam, size, Point::new(200.0, 150.0), 15.0), Some(0));
        // Node 1 at world (50,0) → screen (250,150).
        assert_eq!(hit_test(&sc, &cam, size, Point::new(250.0, 150.0), 15.0), Some(1));
        // Far corner hits nothing.
        assert_eq!(hit_test(&sc, &cam, size, Point::new(10.0, 10.0), 15.0), None);
    }
}
