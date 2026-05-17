// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Thumbnail capture helpers — math for fitting a scene region into a
//! target framebuffer, plus the integration shape for switcher
//! previews.
//!
//! ## Pipeline
//!
//! The actual capture is "render the scene with a fit camera into a
//! thumbnail-sized target texture" — every piece of which the
//! substrate already exposes:
//!
//! 1. Choose `source_bounds` (typically `scene.content_bounds()`).
//! 2. Choose `target_size` (e.g. `(256, 256)` for a switcher tile).
//! 3. Compute the camera via [`fit_camera`].
//! 4. Save the existing camera; call `host.set_camera(camera)`.
//! 5. Render through the usual `host.render_scene` /
//!    `vello::Renderer::render_to_texture` pair, with the target
//!    texture sized to `target_size`.
//! 6. Restore the saved camera.
//!
//! See `tests/render_to_thumbnail_png.rs` for a worked example that
//! writes a PNG.

use kurbo::{Affine, Rect};

/// How a thumbnail fits its source bounds into the target framebuffer.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ThumbnailFit {
    /// Preserve aspect ratio. The scaled source is centered in the
    /// target; any unused space (letterbox / pillarbox) is left for
    /// the caller's `RenderParams::base_color` to fill.
    ContainAspect,
    /// Stretch source non-uniformly to fill the target. May distort.
    /// Useful when the host wants every pixel of the target painted
    /// and aspect distortion is acceptable.
    StretchToFill,
}

/// Compute the camera transform that maps `source_bounds` (in scene
/// coords) into a `target_size` framebuffer (in host pixels) per the
/// requested fit mode.
///
/// Returns [`Affine::IDENTITY`] when `target_size` has any zero
/// dimension or `source_bounds` is degenerate (zero width or height).
pub fn fit_camera(source_bounds: Rect, target_size: (u32, u32), fit: ThumbnailFit) -> Affine {
    let target_w = target_size.0 as f64;
    let target_h = target_size.1 as f64;
    let source_w = source_bounds.width();
    let source_h = source_bounds.height();
    if target_w <= 0.0 || target_h <= 0.0 || source_w <= 0.0 || source_h <= 0.0 {
        return Affine::IDENTITY;
    }
    let scale_x = target_w / source_w;
    let scale_y = target_h / source_h;
    match fit {
        ThumbnailFit::ContainAspect => {
            let scale = scale_x.min(scale_y);
            // Center the scaled source within the target.
            let offset_x = (target_w - source_w * scale) / 2.0;
            let offset_y = (target_h - source_h * scale) / 2.0;
            Affine::translate((offset_x, offset_y))
                * Affine::scale(scale)
                * Affine::translate((-source_bounds.x0, -source_bounds.y0))
        }
        ThumbnailFit::StretchToFill => {
            Affine::scale_non_uniform(scale_x, scale_y)
                * Affine::translate((-source_bounds.x0, -source_bounds.y0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nearly(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn fit_camera_handles_degenerate_input() {
        let zero_target = fit_camera(Rect::new(0.0, 0.0, 10.0, 10.0), (0, 100), ThumbnailFit::ContainAspect);
        assert_eq!(zero_target, Affine::IDENTITY);
        let zero_source = fit_camera(Rect::new(5.0, 5.0, 5.0, 5.0), (100, 100), ThumbnailFit::ContainAspect);
        assert_eq!(zero_source, Affine::IDENTITY);
    }

    #[test]
    fn fit_camera_contain_aspect_centers_a_square_source() {
        // 100×100 source → 200×200 target, aspect preserved → 2× scale,
        // no letterboxing needed.
        let cam = fit_camera(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            (200, 200),
            ThumbnailFit::ContainAspect,
        );
        let mapped_origin = cam * kurbo::Point::new(0.0, 0.0);
        let mapped_corner = cam * kurbo::Point::new(100.0, 100.0);
        assert!(nearly(mapped_origin.x, 0.0));
        assert!(nearly(mapped_origin.y, 0.0));
        assert!(nearly(mapped_corner.x, 200.0));
        assert!(nearly(mapped_corner.y, 200.0));
    }

    #[test]
    fn fit_camera_contain_aspect_letterboxes_a_wide_source() {
        // 200×100 source → 200×200 target. Aspect-preserving scale is
        // min(1.0, 2.0) = 1.0 (limited by width). Result: source
        // occupies (0, 50)..(200, 150) — letterboxed vertically.
        let cam = fit_camera(
            Rect::new(0.0, 0.0, 200.0, 100.0),
            (200, 200),
            ThumbnailFit::ContainAspect,
        );
        let mapped_origin = cam * kurbo::Point::new(0.0, 0.0);
        let mapped_corner = cam * kurbo::Point::new(200.0, 100.0);
        assert!(nearly(mapped_origin.x, 0.0));
        assert!(nearly(mapped_origin.y, 50.0));
        assert!(nearly(mapped_corner.x, 200.0));
        assert!(nearly(mapped_corner.y, 150.0));
    }

    #[test]
    fn fit_camera_stretch_fills_target_non_uniformly() {
        let cam = fit_camera(
            Rect::new(0.0, 0.0, 100.0, 200.0),
            (200, 100),
            ThumbnailFit::StretchToFill,
        );
        // Each axis scaled independently: x ×2, y ×0.5.
        let mapped_origin = cam * kurbo::Point::new(0.0, 0.0);
        let mapped_corner = cam * kurbo::Point::new(100.0, 200.0);
        assert!(nearly(mapped_origin.x, 0.0));
        assert!(nearly(mapped_origin.y, 0.0));
        assert!(nearly(mapped_corner.x, 200.0));
        assert!(nearly(mapped_corner.y, 100.0));
    }

    #[test]
    fn fit_camera_offset_source_translates_to_target_origin() {
        // Source at (100, 50)–(300, 250). With aspect-preserving fit
        // into a 200×200 target, scale is 200/200 = 1.0, no letterbox.
        let cam = fit_camera(
            Rect::new(100.0, 50.0, 300.0, 250.0),
            (200, 200),
            ThumbnailFit::ContainAspect,
        );
        let mapped_origin = cam * kurbo::Point::new(100.0, 50.0);
        assert!(nearly(mapped_origin.x, 0.0));
        assert!(nearly(mapped_origin.y, 0.0));
    }
}
