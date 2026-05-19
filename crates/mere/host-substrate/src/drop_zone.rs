// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Quadrant inference for drop targets — port of the gpui-host
//! `mere_host::drop_zone` against `kurbo` types so the spatial-chrome
//! / xilem-side path can reuse the geometry without dragging in
//! gpui. Lifted shape; no behavior change.
//!
//! When a drag drop lands on a target region, the cursor's position
//! within the target's bounds picks which edge the source attaches
//! to. The region is divided into four "edge gutter" zones
//! (top / bottom / left / right) plus a central zone:
//!
//! ```text
//!   ┌─────────────────────────┐
//!   │\        top            /│
//!   │  ─────────────────────  │
//!   │ │                     │ │
//!   │ │       center        │ │
//!   │l│      → Right        │r│
//!   │e│                     │i│
//!   │f│                     │g│
//!   │t│                     │h│
//!   │ │                     │t│
//!   │ │                     │ │
//!   │  ─────────────────────  │
//!   │/       bottom         \ │
//!   └─────────────────────────┘
//! ```
//!
//! The gutters are 25% of the target's dimension thick. Drops landing
//! in the central zone (more than 0.25 of the target's dimension from
//! every edge) default to `Right` — the v0 fallback for ambiguous
//! gestures, matching the legacy gpui host.

use kurbo::{Point, Rect};
use mere_frame::InsertSide;

/// Pick which side of a target a drop lands on, given the cursor
/// position and the target's bounds. See the module docs for the
/// gutter rule.
///
/// Both `cursor` and `bounds` are in the same coordinate space —
/// host pixels or scene units, whichever the caller uses. The
/// function is dimensionless: it normalises cursor position into
/// the target's local 0..1 frame, picks the closest edge, and
/// falls back to `Right` when the cursor is in the central zone.
pub fn infer_drop_side(cursor: Point, bounds: Rect) -> InsertSide {
    let width = bounds.width().max(1.0);
    let height = bounds.height().max(1.0);
    let nx = ((cursor.x - bounds.x0) / width).clamp(0.0, 1.0) as f32;
    let ny = ((cursor.y - bounds.y0) / height).clamp(0.0, 1.0) as f32;
    let d_left = nx;
    let d_right = 1.0 - nx;
    let d_top = ny;
    let d_bottom = 1.0 - ny;
    let min_d = d_left.min(d_right).min(d_top).min(d_bottom);
    if min_d > 0.25 {
        return InsertSide::Right;
    }
    if d_left == min_d {
        InsertSide::Left
    } else if d_right == min_d {
        InsertSide::Right
    } else if d_top == min_d {
        InsertSide::Above
    } else {
        InsertSide::Below
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect::new(x, y, x + w, y + h)
    }

    #[test]
    fn center_drop_falls_back_to_right() {
        let side = infer_drop_side(Point::new(50.0, 50.0), rect(0.0, 0.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Right);
    }

    #[test]
    fn left_edge_drop_picks_left() {
        let side = infer_drop_side(Point::new(5.0, 50.0), rect(0.0, 0.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Left);
    }

    #[test]
    fn right_edge_drop_picks_right() {
        let side = infer_drop_side(Point::new(95.0, 50.0), rect(0.0, 0.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Right);
    }

    #[test]
    fn top_edge_drop_picks_above() {
        let side = infer_drop_side(Point::new(50.0, 5.0), rect(0.0, 0.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Above);
    }

    #[test]
    fn bottom_edge_drop_picks_below() {
        let side = infer_drop_side(Point::new(50.0, 95.0), rect(0.0, 0.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Below);
    }

    #[test]
    fn coordinates_are_resolved_relative_to_wrapper_origin() {
        // Target at (200, 300) sized 100x100. Cursor at (210, 350)
        // is at local (10, 50) → left-gutter (nx = 0.1).
        let side = infer_drop_side(Point::new(210.0, 350.0), rect(200.0, 300.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Left);
    }

    #[test]
    fn cursor_outside_bounds_clamps_to_nearest_edge() {
        // Cursor at (-50, 50) — left of target. Clamps to nx=0,
        // picks Left.
        let side = infer_drop_side(Point::new(-50.0, 50.0), rect(0.0, 0.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Left);
    }

    #[test]
    fn corner_drop_picks_a_horizontal_edge() {
        // Top-left corner (5, 5): d_left = d_top = 0.05. Tie broken
        // by left-before-top order in the impl — pinned so changes
        // are deliberate.
        let side = infer_drop_side(Point::new(5.0, 5.0), rect(0.0, 0.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Left);
    }

    #[test]
    fn narrow_target_still_resolves_to_an_edge() {
        // 1px wide target — clamp guard prevents division by zero.
        let side = infer_drop_side(Point::new(0.0, 50.0), rect(0.0, 0.0, 1.0, 100.0));
        assert_eq!(side, InsertSide::Left);
    }
}
