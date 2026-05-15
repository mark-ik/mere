/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Quadrant inference for pane drag-drop targets.
//!
//! When a `PaneDragPayload` is dropped on a pane wrapper, the cursor's
//! position within the target's bounds picks which edge the source
//! attaches to. The pane is divided into four "edge gutter" zones
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
//! The gutters are 25% of the pane's dimension thick. Drops landing
//! in the central zone (more than 0.25 of the pane's dimension from
//! every edge) default to `Right` — the v0 fallback for ambiguous
//! gestures.

use gpui::{Bounds, Pixels, Point};
use mere_frame::InsertSide;

/// Pick which side of a pane a drop targets, given the cursor
/// position (window-local pixels) and the pane wrapper's bounds
/// (window-local). See [the module docs](self) for the gutter rule.
pub(crate) fn infer_drop_side(cursor: Point<Pixels>, bounds: Bounds<Pixels>) -> InsertSide {
    let width = bounds.size.width.as_f32().max(1.0);
    let height = bounds.size.height.as_f32().max(1.0);
    let nx = ((cursor.x - bounds.origin.x).as_f32() / width).clamp(0.0, 1.0);
    let ny = ((cursor.y - bounds.origin.y).as_f32() / height).clamp(0.0, 1.0);
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
    use gpui::{Pixels, point, px, size};

    fn bounds_at(origin_x: f32, origin_y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(origin_x), px(origin_y)), size(px(w), px(h)))
    }

    fn cursor_at(x: f32, y: f32) -> Point<Pixels> {
        point(px(x), px(y))
    }

    #[test]
    fn center_drop_falls_back_to_right() {
        // Pane at (0,0) 100x100, drop at (50,50) — squarely centered.
        let side = infer_drop_side(cursor_at(50.0, 50.0), bounds_at(0.0, 0.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Right);
    }

    #[test]
    fn left_edge_drop_picks_left() {
        // x=5 of 100 → nx = 0.05 → d_left = 0.05, smallest of the four.
        let side = infer_drop_side(cursor_at(5.0, 50.0), bounds_at(0.0, 0.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Left);
    }

    #[test]
    fn right_edge_drop_picks_right() {
        let side = infer_drop_side(cursor_at(95.0, 50.0), bounds_at(0.0, 0.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Right);
    }

    #[test]
    fn top_edge_drop_picks_above() {
        let side = infer_drop_side(cursor_at(50.0, 5.0), bounds_at(0.0, 0.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Above);
    }

    #[test]
    fn bottom_edge_drop_picks_below() {
        let side = infer_drop_side(cursor_at(50.0, 95.0), bounds_at(0.0, 0.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Below);
    }

    #[test]
    fn coordinates_are_resolved_relative_to_wrapper_origin() {
        // Pane at (200, 300) sized 100x100. Cursor at (210, 350)
        // is at local (10, 50) → left-gutter (nx = 0.1).
        let side = infer_drop_side(cursor_at(210.0, 350.0), bounds_at(200.0, 300.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Left);
    }

    #[test]
    fn cursor_outside_bounds_clamps_to_nearest_edge() {
        // Pane at (0,0) 100x100, cursor at (-50, 50) — left of pane.
        // Clamps to nx=0, picks Left.
        let side = infer_drop_side(cursor_at(-50.0, 50.0), bounds_at(0.0, 0.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Left);
    }

    #[test]
    fn corner_drop_picks_a_horizontal_edge() {
        // Top-left corner (5, 5): d_left = d_top = 0.05. Tie broken
        // by left-before-top order in the impl. Either is acceptable
        // for the user; pin the behavior so changes are deliberate.
        let side = infer_drop_side(cursor_at(5.0, 5.0), bounds_at(0.0, 0.0, 100.0, 100.0));
        assert_eq!(side, InsertSide::Left);
    }

    #[test]
    fn narrow_pane_still_resolves_to_an_edge() {
        // 1px wide pane — clamp guard prevents division by zero.
        // Cursor at x=0 → nx=0 → Left.
        let side = infer_drop_side(cursor_at(0.0, 50.0), bounds_at(0.0, 0.0, 1.0, 100.0));
        assert_eq!(side, InsertSide::Left);
    }
}
