/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Camera state and viewport for the graph canvas.

use euclid::default::{Point2D, Rect, Size2D, Vector2D};
use serde::{Deserialize, Serialize};

/// Viewport rectangle and display scale factor for a canvas pane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasViewport {
    /// The pane rectangle in logical (host-framework) coordinates.
    pub rect: Rect<f32>,
    /// Display scale factor (e.g. 2.0 for Retina / HiDPI).
    pub scale_factor: f32,
}

impl CanvasViewport {
    pub fn new(origin: Point2D<f32>, size: Size2D<f32>, scale_factor: f32) -> Self {
        Self {
            rect: Rect::new(origin, size),
            scale_factor,
        }
    }

    pub fn size(&self) -> Size2D<f32> {
        self.rect.size
    }

    pub fn center(&self) -> Point2D<f32> {
        self.rect.center()
    }
}

impl Default for CanvasViewport {
    fn default() -> Self {
        Self {
            rect: Rect::new(Point2D::origin(), Size2D::new(800.0, 600.0)),
            scale_factor: 1.0,
        }
    }
}

/// Camera state: pan offset and zoom level.
///
/// The camera transforms canvas (world) coordinates to screen coordinates.
/// `pan` is the world-space offset of the viewport center, and `zoom` is a
/// multiplicative scale factor (1.0 = no zoom).
///
/// `pan_velocity` is a sidecar slot for inertia decay on drag release —
/// hosts that want "coast after release" feel sample it each frame and
/// apply it via [`CanvasCamera::tick_inertia`]. Hosts that want no
/// inertia simply leave it zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasCamera {
    /// Pan offset in world coordinates.
    pub pan: Vector2D<f32>,
    /// Zoom level (1.0 = identity, >1.0 = zoomed in).
    pub zoom: f32,
    /// Pan velocity in world units per second, used for coast-after-release
    /// inertia. Zero when the camera is at rest. See
    /// [`tick_inertia`](Self::tick_inertia).
    #[serde(default)]
    pub pan_velocity: Vector2D<f32>,
}

/// Default inertia damping per second: `pan_velocity *= damping^dt`.
/// 0.003 → roughly 500ms to fade to 10% of the release velocity at 60fps.
/// Chosen to feel "physical" without overshooting readable bounds.
pub const DEFAULT_PAN_DAMPING_PER_SECOND: f32 = 0.003;

/// Minimum pan-velocity magnitude (world units / sec) below which inertia
/// snaps to zero. Avoids floating-point creep and sub-pixel jitter.
pub const PAN_VELOCITY_EPSILON: f32 = 0.5;

impl CanvasCamera {
    pub fn new(pan: Vector2D<f32>, zoom: f32) -> Self {
        Self {
            pan,
            zoom,
            pan_velocity: Vector2D::zero(),
        }
    }

    /// Advance pan by the current velocity and decay the velocity by
    /// `damping^dt`. Call this once per frame on the host side to get
    /// coast-after-release feel.
    ///
    /// Returns the pan delta applied this frame (useful for telemetry /
    /// redraw-request logic). Returns `Vector2D::zero()` once velocity
    /// falls below [`PAN_VELOCITY_EPSILON`] — hosts can stop requesting
    /// redraws at that point.
    pub fn tick_inertia(&mut self, dt: f32, damping_per_second: f32) -> Vector2D<f32> {
        if self.pan_velocity.length() < PAN_VELOCITY_EPSILON {
            self.pan_velocity = Vector2D::zero();
            return Vector2D::zero();
        }
        let delta = self.pan_velocity * dt;
        self.pan += delta;
        let decay = damping_per_second.powf(dt);
        self.pan_velocity *= decay;
        delta
    }

    /// Fit the camera so that a world-space bounds rectangle is visible
    /// in the viewport, with `padding_ratio` headroom around the edges
    /// (e.g. `1.08` for ~8 % margin). Zoom is clamped to the supplied
    /// `zoom_min..=zoom_max` range; when the bounds collapse to a point
    /// (zero area), `fallback_zoom` is used instead — the camera still
    /// centers on the point.
    ///
    /// Pan is set so that `bounds.center()` lands at the viewport center
    /// after projection. Pan velocity is cleared — fitting is an
    /// authoritative jump, not a drag, so residual inertia should not
    /// keep coasting past the target.
    ///
    /// No-op if `viewport.rect.size` has zero area (headless / pre-layout
    /// frames); returns `false` so the caller can leave the pending
    /// request in place for a later frame.
    pub fn fit_to_bounds(
        &mut self,
        bounds: Rect<f32>,
        viewport: &CanvasViewport,
        padding_ratio: f32,
        zoom_min: f32,
        zoom_max: f32,
        fallback_zoom: f32,
    ) -> bool {
        let viewport_size = viewport.rect.size;
        if viewport_size.width <= 0.0 || viewport_size.height <= 0.0 {
            return false;
        }

        let padded = padding_ratio.max(1.0);
        let bounds_size = bounds.size;
        let fit_zoom = if bounds_size.width > 0.0 && bounds_size.height > 0.0 {
            let zoom_x = viewport_size.width / bounds_size.width;
            let zoom_y = viewport_size.height / bounds_size.height;
            (zoom_x.min(zoom_y) / padded).clamp(zoom_min, zoom_max)
        } else {
            fallback_zoom.clamp(zoom_min, zoom_max)
        };

        // Position the camera so `bounds.center()` projects to the
        // viewport center. From `world_to_screen`:
        //   screen = (world + pan) * zoom + viewport_center
        // For the projection to land at viewport_center we need:
        //   (world + pan) * zoom = 0 → pan = -world
        let bounds_center = bounds.center();
        self.zoom = fit_zoom;
        self.pan = Vector2D::new(-bounds_center.x, -bounds_center.y);
        self.pan_velocity = Vector2D::zero();
        true
    }

    /// Convert a point from world space to screen space given a viewport.
    pub fn world_to_screen(
        &self,
        world_pos: Point2D<f32>,
        viewport: &CanvasViewport,
    ) -> Point2D<f32> {
        let center = viewport.center();
        Point2D::new(
            (world_pos.x + self.pan.x) * self.zoom + center.x,
            (world_pos.y + self.pan.y) * self.zoom + center.y,
        )
    }

    /// Convert a point from screen space to world space given a viewport.
    pub fn screen_to_world(
        &self,
        screen_pos: Point2D<f32>,
        viewport: &CanvasViewport,
    ) -> Point2D<f32> {
        let center = viewport.center();
        Point2D::new(
            (screen_pos.x - center.x) / self.zoom - self.pan.x,
            (screen_pos.y - center.y) / self.zoom - self.pan.y,
        )
    }
}

impl Default for CanvasCamera {
    fn default() -> Self {
        Self {
            pan: Vector2D::zero(),
            zoom: 1.0,
            pan_velocity: Vector2D::zero(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_world_screen_roundtrip() {
        let viewport = CanvasViewport::default();
        let camera = CanvasCamera::new(Vector2D::new(10.0, -20.0), 2.0);
        let world = Point2D::new(50.0, 75.0);
        let screen = camera.world_to_screen(world, &viewport);
        let back = camera.screen_to_world(screen, &viewport);
        assert!((back.x - world.x).abs() < 1e-4);
        assert!((back.y - world.y).abs() < 1e-4);
    }

    #[test]
    fn identity_camera_preserves_center() {
        let viewport = CanvasViewport::default();
        let camera = CanvasCamera::default();
        let origin = Point2D::origin();
        let screen = camera.world_to_screen(origin, &viewport);
        // World origin maps to viewport center with identity camera.
        assert!((screen.x - viewport.center().x).abs() < 1e-4);
        assert!((screen.y - viewport.center().y).abs() < 1e-4);
    }

    #[test]
    fn serde_roundtrip_viewport() {
        let vp = CanvasViewport::new(Point2D::new(10.0, 20.0), Size2D::new(1920.0, 1080.0), 2.0);
        let json = serde_json::to_string(&vp).unwrap();
        let back: CanvasViewport = serde_json::from_str(&json).unwrap();
        assert_eq!(vp, back);
    }

    #[test]
    fn serde_roundtrip_camera() {
        let cam = CanvasCamera::new(Vector2D::new(-5.0, 3.0), 1.5);
        let json = serde_json::to_string(&cam).unwrap();
        let back: CanvasCamera = serde_json::from_str(&json).unwrap();
        assert_eq!(cam, back);
    }

    #[test]
    fn serde_camera_without_pan_velocity_accepts_default() {
        // Older persisted cameras lack the `pan_velocity` field; the
        // `#[serde(default)]` attribute must let them round-trip cleanly.
        let json = r#"{"pan":[0.0,0.0],"zoom":1.0}"#;
        let cam: CanvasCamera = serde_json::from_str(json).unwrap();
        assert_eq!(cam.pan_velocity, Vector2D::zero());
    }

    #[test]
    fn tick_inertia_applies_and_decays() {
        let mut cam = CanvasCamera::default();
        cam.pan_velocity = Vector2D::new(100.0, 0.0);
        let dt = 1.0 / 60.0;
        let delta = cam.tick_inertia(dt, DEFAULT_PAN_DAMPING_PER_SECOND);
        assert!(delta.x > 0.0);
        assert!(cam.pan.x > 0.0);
        // Velocity decayed.
        assert!(cam.pan_velocity.x < 100.0);
    }

    #[test]
    fn tick_inertia_snaps_to_zero_below_epsilon() {
        let mut cam = CanvasCamera::default();
        cam.pan_velocity = Vector2D::new(PAN_VELOCITY_EPSILON * 0.5, 0.0);
        let delta = cam.tick_inertia(1.0 / 60.0, DEFAULT_PAN_DAMPING_PER_SECOND);
        assert_eq!(delta, Vector2D::zero());
        assert_eq!(cam.pan_velocity, Vector2D::zero());
    }

    #[test]
    fn fit_to_bounds_centers_bounds_in_viewport() {
        let viewport = CanvasViewport::new(Point2D::origin(), Size2D::new(1000.0, 800.0), 1.0);
        let mut cam = CanvasCamera::default();
        let bounds = Rect::new(Point2D::new(100.0, 200.0), Size2D::new(400.0, 200.0));
        let applied = cam.fit_to_bounds(bounds, &viewport, 1.08, 0.1, 10.0, 1.0);
        assert!(applied);
        let bounds_center = bounds.center();
        let projected = cam.world_to_screen(bounds_center, &viewport);
        let viewport_center = viewport.center();
        assert!((projected.x - viewport_center.x).abs() < 1e-3);
        assert!((projected.y - viewport_center.y).abs() < 1e-3);
    }

    #[test]
    fn fit_to_bounds_respects_padding_ratio() {
        let viewport = CanvasViewport::new(Point2D::origin(), Size2D::new(1000.0, 1000.0), 1.0);
        let bounds = Rect::new(Point2D::new(-100.0, -100.0), Size2D::new(200.0, 200.0));
        // zoom without padding would be 1000/200 = 5.0
        let mut tight = CanvasCamera::default();
        tight.fit_to_bounds(bounds, &viewport, 1.0, 0.1, 10.0, 1.0);
        let mut padded = CanvasCamera::default();
        padded.fit_to_bounds(bounds, &viewport, 1.25, 0.1, 10.0, 1.0);
        assert!(
            padded.zoom < tight.zoom,
            "padding should reduce zoom: tight={} padded={}",
            tight.zoom,
            padded.zoom
        );
    }

    #[test]
    fn fit_to_bounds_clamps_to_zoom_limits() {
        let viewport = CanvasViewport::new(Point2D::origin(), Size2D::new(1000.0, 1000.0), 1.0);
        // Extremely tiny bounds would push zoom far above max — clamp.
        let bounds = Rect::new(Point2D::new(0.0, 0.0), Size2D::new(0.01, 0.01));
        let mut cam = CanvasCamera::default();
        cam.fit_to_bounds(bounds, &viewport, 1.0, 0.1, 4.0, 1.0);
        assert!(cam.zoom <= 4.0 + 1e-4);
    }

    #[test]
    fn fit_to_bounds_zero_area_uses_fallback_zoom_and_centers() {
        let viewport = CanvasViewport::new(Point2D::origin(), Size2D::new(800.0, 600.0), 1.0);
        // Single-point bounds (zero area).
        let point = Point2D::new(50.0, 25.0);
        let bounds = Rect::new(point, Size2D::zero());
        let mut cam = CanvasCamera::default();
        let applied = cam.fit_to_bounds(bounds, &viewport, 1.0, 0.1, 10.0, 2.0);
        assert!(applied);
        assert!((cam.zoom - 2.0).abs() < 1e-4);
        let projected = cam.world_to_screen(point, &viewport);
        let vc = viewport.center();
        assert!((projected.x - vc.x).abs() < 1e-3);
        assert!((projected.y - vc.y).abs() < 1e-3);
    }

    #[test]
    fn fit_to_bounds_returns_false_for_zero_area_viewport() {
        let viewport = CanvasViewport::new(Point2D::origin(), Size2D::zero(), 1.0);
        let bounds = Rect::new(Point2D::new(0.0, 0.0), Size2D::new(100.0, 100.0));
        let mut cam = CanvasCamera::default();
        let before = cam.clone();
        let applied = cam.fit_to_bounds(bounds, &viewport, 1.0, 0.1, 10.0, 1.0);
        assert!(!applied);
        assert_eq!(cam, before, "no-op when viewport has zero area");
    }

    #[test]
    fn fit_to_bounds_clears_pan_velocity() {
        let viewport = CanvasViewport::new(Point2D::origin(), Size2D::new(1000.0, 1000.0), 1.0);
        let mut cam = CanvasCamera::default();
        cam.pan_velocity = Vector2D::new(250.0, -100.0);
        let bounds = Rect::new(Point2D::new(-50.0, -50.0), Size2D::new(100.0, 100.0));
        cam.fit_to_bounds(bounds, &viewport, 1.0, 0.1, 10.0, 1.0);
        assert_eq!(cam.pan_velocity, Vector2D::zero());
    }
}
