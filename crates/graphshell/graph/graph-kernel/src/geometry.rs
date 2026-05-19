/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable 2-D geometry types used at the host ↔ runtime boundary.
//!
//! All three aliases are thin wrappers over `euclid::default::*<f32>`.
//! They appear on:
//!
//! - `FrameHostInput.pointer_hover: Option<PortablePoint>` — hover
//!   position relative to the host viewport.
//! - `FrameHostInput.viewport_size: PortableSize` — current viewport
//!   extents.
//! - `FrameViewModel.active_pane_rects: Vec<(PaneId, NodeKey, PortableRect)>`
//!   — pane layout rects.
//! - `DegradedReceiptSpec.tile_rect: PortableRect` — degraded-mode
//!   overlay placement.
//!
//! Egui hosts convert between these and `egui::{Pos2, Vec2, Rect}`
//! at the widget boundary via helpers in the shell crate's
//! `compositor_adapter` module; iced hosts consume them directly.
//! The type aliases themselves are fully portable — `euclid` has no
//! platform or framework dependencies.
//!
//! Pre-M4 slice 8 (2026-04-22) these aliases lived in
//! `shell/desktop/workbench/compositor_adapter.rs` alongside the
//! egui conversion helpers. Promoting them here makes the
//! view-model types they appear in portable without importing the
//! shell-crate compositor module.

/// Portable rectangle type. `origin` is the top-left corner; `size` is
/// the rect extent. Uses `f32` throughout.
pub type PortableRect = euclid::default::Rect<f32>;

/// Portable 2-D point, `f32`-valued. Typically represents a pointer
/// position or a layout anchor in host-viewport coordinates.
pub type PortablePoint = euclid::default::Point2D<f32>;

/// Portable 2-D size (width × height), `f32`-valued. Typically
/// represents a viewport or pane extent in host-viewport coordinates.
pub type PortableSize = euclid::default::Size2D<f32>;

/// Portable 2-D vector, `f32`-valued. Typically represents a layout or
/// physics delta in graph/canvas coordinates.
pub type PortableVector = euclid::default::Vector2D<f32>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_constructs_with_origin_and_size() {
        let r = PortableRect::new(
            PortablePoint::new(10.0, 20.0),
            PortableSize::new(300.0, 200.0),
        );
        assert_eq!(r.origin.x, 10.0);
        assert_eq!(r.origin.y, 20.0);
        assert_eq!(r.size.width, 300.0);
        assert_eq!(r.size.height, 200.0);
    }

    #[test]
    fn rect_contains_point_inside_bounds() {
        let r = PortableRect::new(PortablePoint::new(0.0, 0.0), PortableSize::new(100.0, 50.0));
        assert!(r.contains(PortablePoint::new(50.0, 25.0)));
        assert!(!r.contains(PortablePoint::new(-1.0, 25.0)));
        assert!(!r.contains(PortablePoint::new(50.0, 51.0)));
    }

    #[test]
    fn portable_types_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PortableRect>();
        assert_send_sync::<PortablePoint>();
        assert_send_sync::<PortableSize>();
    }
}
