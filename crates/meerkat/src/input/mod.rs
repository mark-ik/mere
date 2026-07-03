/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Mouse, keyboard, and palette input handlers for [`Shell`](super::Shell). Factored
//! from `main.rs` to keep files under the workspace 600-LOC ceiling.

use std::time::{Duration, Instant};

use crate::serval_render::hit_test_node;
use forme::GraphMemberId;
use layout_dom_api::LayoutDom;
use meerkat::{Chrome, ContextAction, ContextItem, HistoryStep, nav, submit_omnibar};
use orrery::PointerButton;
use serval_layout::ScrollOffsets;
use serval_scripted_dom::NodeId;
use serval_winit_host::key_event_from_winit;
use winit::event::{ElementState, MouseButton};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use xilem_serval::PointerClick;

use frame::PaneContent;

use super::titlebar::{self, WindowControl};
use super::{
    WindowCtx, class_bottom_in, first_tag, first_with_class, measure_class_bottom, scrying_host,
};

/// Map a winit mouse button to the scrying host's button vocabulary. (Scrying X2.)
fn scrying_btn(button: MouseButton) -> Option<scrying_host::MouseBtn> {
    match button {
        MouseButton::Left => Some(scrying_host::MouseBtn::Left),
        MouseButton::Right => Some(scrying_host::MouseBtn::Right),
        MouseButton::Middle => Some(scrying_host::MouseBtn::Middle),
        _ => None,
    }
}

/// The (active, else first) tile id under the `TileTree` node addressed by `path` (a
/// chain of split-child indices). Resolves a tab-bar drop's target stack back to a
/// member for the Workbench mutation. (Drag via pelt TileEvents.)
fn member_at_path(
    tree: &pelt_core::tile::TileTree,
    path: &[usize],
) -> Option<pelt_core::tile::TileId> {
    use pelt_core::tile::TileTree;
    match (tree, path.split_first()) {
        (TileTree::Stack(s), _) => s
            .tabs
            .get(s.active)
            .or_else(|| s.tabs.first())
            .map(|t| t.id),
        (TileTree::Split { children, .. }, Some((i, rest))) => {
            member_at_path(&children.get(*i)?.tree, rest)
        }
        (TileTree::Split { .. }, None) => None,
    }
}

/// Parse a `<i>:<count>` slider cell into the fraction `i/count` — the position a
/// click on cell `i` of a `count`-cell track maps to. `None` on a malformed key.
fn slider_cell_fraction(s: &str) -> Option<f64> {
    let (i, count) = s.split_once(':')?;
    let i: f64 = i.parse().ok()?;
    let count: f64 = count.parse().ok()?;
    (count > 0.0).then_some(i / count)
}

/// The index `i` of the hull edge (`hull[i]` → `hull[i+1]`, wrapping the last to the first)
/// nearest to point `(px, py)`, if its distance is within `tol`. Used to insert a new corner
/// where the user clicks a hull edge. (Swatch — add vertex, B3.)
fn nearest_hull_edge(hull: &[(f32, f32)], px: f32, py: f32, tol: f32) -> Option<usize> {
    let n = hull.len();
    (0..n)
        .map(|i| {
            let (ax, ay) = hull[i];
            let (bx, by) = hull[(i + 1) % n];
            (i, point_segment_dist(px, py, ax, ay, bx, by))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .filter(|&(_, d)| d <= tol)
        .map(|(i, _)| i)
}

/// Distance from point `(px, py)` to the segment `(ax, ay)`–`(bx, by)`. (Swatch — add vertex.)
fn point_segment_dist(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (ax + t * dx, ay + t * dy);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

mod chrome;
mod editing;
mod keyboard;
mod mouse_dispatch;
mod page_text;
mod panes;
mod targets;
mod text_input;
mod workbench;

#[cfg(test)]
mod tests {
    use super::{nearest_hull_edge, point_segment_dist};

    #[test]
    fn point_segment_dist_handles_interior_and_endpoints() {
        // A point above the middle of a horizontal segment: distance is the vertical gap.
        assert!((point_segment_dist(0.5, 0.3, 0.0, 0.0, 1.0, 0.0) - 0.3).abs() < 1e-5);
        // Past the segment's end: distance is to the nearer endpoint, not the infinite line.
        assert!((point_segment_dist(2.0, 0.0, 0.0, 0.0, 1.0, 0.0) - 1.0).abs() < 1e-5);
        // A degenerate (zero-length) segment is just the distance to the point.
        assert!((point_segment_dist(0.0, 1.0, 0.0, 0.0, 0.0, 0.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn nearest_hull_edge_picks_the_clicked_edge_within_tolerance() {
        // A unit square hull (CCW): edges bottom(0), right(1), top(2), left(3, wraps 3->0).
        let hull = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        // A click just outside the bottom edge picks edge 0.
        assert_eq!(nearest_hull_edge(&hull, 0.5, -0.05, 0.1), Some(0));
        // A click near the right edge picks edge 1.
        assert_eq!(nearest_hull_edge(&hull, 1.05, 0.5, 0.1), Some(1));
        // The wrapping left edge (last vertex -> first) is index 3.
        assert_eq!(nearest_hull_edge(&hull, -0.05, 0.5, 0.1), Some(3));
        // A click far from every edge (well inside) is beyond tolerance: no edge.
        assert_eq!(nearest_hull_edge(&hull, 0.5, 0.5, 0.1), None);
    }
}
