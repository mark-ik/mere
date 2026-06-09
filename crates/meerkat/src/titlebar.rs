/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Custom titlebar for the borderless window (theming pass, part B). The OS
//! decorations are off (`with_decorations(false)` kills the platform title bar +
//! its accent border), so the chrome's toolbar band doubles as the titlebar: the
//! host draws min / maximize / close controls at its right edge, drags the window
//! from the bar's empty space, and resizes from the window edges. This module is
//! the geometry + the controls scene; the host (`input` / `render`) wires it.

use netrender::Scene;
use register_theme::chrome::{ChromeTheme, Color32};
use winit::window::{CursorIcon, ResizeDirection};

/// Width (px) of one window-control button cell.
pub const CTL_W: f32 = 46.0;
/// Total reserved width (px) for the three-button control strip — the toolbar
/// reserves this much right padding so the omnibar / chip don't slide under it.
pub const CONTROLS_W: f32 = CTL_W * 3.0;
/// How close (px) to a window edge a press must be to start a resize drag. The
/// borderless window draws no OS resize frame, so this is the whole grab target;
/// kept generous since the cursor only hints at it via [`resize_cursor`].
const RESIZE_BORDER: f32 = 8.0;

/// Which window control a press fell on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowControl {
    Minimize,
    Maximize,
    Close,
}

const CONTROLS: [WindowControl; 3] = [
    WindowControl::Minimize,
    WindowControl::Maximize,
    WindowControl::Close,
];

/// A chrome [`Color32`] token as a premultiplied `[f32; 4]` for the scene
/// primitives (the glyphs are opaque, so this is a straight normalize).
fn rgba(c: Color32) -> [f32; 4] {
    let [r, g, b, a] = c.to_array();
    let af = a as f32 / 255.0;
    [
        r as f32 / 255.0 * af,
        g as f32 / 255.0 * af,
        b as f32 / 255.0 * af,
        af,
    ]
}

/// The window-space rect `[x0, y0, x1, y1]` of control `idx` (0 = minimize …
/// 2 = close): the strip is right-aligned in the toolbar band.
pub fn control_rect(idx: usize, win_w: u32, band_h: u32) -> [f32; 4] {
    let x0 = win_w as f32 - CONTROLS_W + CTL_W * idx as f32;
    [x0, 0.0, x0 + CTL_W, band_h as f32]
}

/// Which control, if any, window point `(x, y)` falls on (the toolbar band's
/// top-right strip).
pub fn control_at(x: f32, y: f32, win_w: u32, band_h: u32) -> Option<WindowControl> {
    CONTROLS.iter().copied().enumerate().find_map(|(idx, ctl)| {
        let r = control_rect(idx, win_w, band_h);
        (x >= r[0] && x < r[2] && y >= r[1] && y < r[3]).then_some(ctl)
    })
}

/// The resize direction for a press near a window edge / corner, or `None` when
/// the press is in the interior. Corners (within the border of two edges) take
/// precedence over edges.
pub fn resize_dir_at(x: f32, y: f32, win_w: u32, win_h: u32) -> Option<ResizeDirection> {
    let left = x <= RESIZE_BORDER;
    let right = x >= win_w as f32 - RESIZE_BORDER;
    let top = y <= RESIZE_BORDER;
    let bottom = y >= win_h as f32 - RESIZE_BORDER;
    Some(match (top, bottom, left, right) {
        (true, _, true, _) => ResizeDirection::NorthWest,
        (true, _, _, true) => ResizeDirection::NorthEast,
        (_, true, true, _) => ResizeDirection::SouthWest,
        (_, true, _, true) => ResizeDirection::SouthEast,
        (true, ..) => ResizeDirection::North,
        (_, true, ..) => ResizeDirection::South,
        (.., true, _) => ResizeDirection::West,
        (.., _, true) => ResizeDirection::East,
        _ => return None,
    })
}

/// The cursor icon for a hover over resize edge `dir` (the resize arrows the
/// borderless window has no OS frame to show on its own).
pub fn resize_cursor(dir: ResizeDirection) -> CursorIcon {
    match dir {
        ResizeDirection::East | ResizeDirection::West => CursorIcon::EwResize,
        ResizeDirection::North | ResizeDirection::South => CursorIcon::NsResize,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::NwseResize,
        ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::NeswResize,
    }
}

/// The `CONTROLS_W` x `band_h` control strip (minimize line, maximize square,
/// close ×), glyphs stroked over a transparent ground so the toolbar shows
/// through. Minimize / maximize take the chrome's body text; close tints red so
/// it reads as the destructive control. The host rasterizes this once per size
/// and composites it over the chrome at the toolbar's top-right.
pub fn controls_scene(band_h: u32, theme: &ChromeTheme) -> Scene {
    let w = CONTROLS_W.round().max(1.0) as u32;
    let mut scene = Scene::new(w, band_h);
    let cy = band_h as f32 / 2.0;
    let glyph = rgba(theme.body_text);
    let close = [0.86, 0.34, 0.34, 1.0];
    let stroke = 1.5;
    let r = 6.5; // glyph half-extent

    // Minimize: a centered horizontal line.
    let cx0 = CTL_W * 0.5;
    let mut min_path = netrender::ScenePath::new();
    min_path.move_to(cx0 - r, cy).line_to(cx0 + r, cy);
    scene.push_shape_stroked(min_path, glyph, stroke);

    // Maximize: a square outline.
    let cx1 = CTL_W * 1.5;
    let mut max_path = netrender::ScenePath::new();
    max_path
        .move_to(cx1 - r, cy - r)
        .line_to(cx1 + r, cy - r)
        .line_to(cx1 + r, cy + r)
        .line_to(cx1 - r, cy + r)
        .line_to(cx1 - r, cy - r);
    scene.push_shape_stroked(max_path, glyph, stroke);

    // Close: an ×.
    let cx2 = CTL_W * 2.5;
    let mut close_path = netrender::ScenePath::new();
    close_path.move_to(cx2 - r, cy - r).line_to(cx2 + r, cy + r);
    close_path.move_to(cx2 + r, cy - r).line_to(cx2 - r, cy + r);
    scene.push_shape_stroked(close_path, close, stroke);

    scene
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_strip_is_right_aligned_and_hit_tests() {
        let (w, band) = (1024u32, 64u32);
        // Close is the rightmost cell, flush to the window's right edge.
        let close = control_rect(2, w, band);
        assert_eq!(close[2], w as f32);
        // A point in the close cell hits close; one left of the strip misses.
        assert_eq!(
            control_at(w as f32 - 10.0, 20.0, w, band),
            Some(WindowControl::Close)
        );
        assert_eq!(control_at(w as f32 - CONTROLS_W - 5.0, 20.0, w, band), None);
        // Below the band there are no controls.
        assert_eq!(
            control_at(w as f32 - 10.0, band as f32 + 5.0, w, band),
            None
        );
    }

    #[test]
    fn resize_edges_and_corners() {
        let (w, h) = (800u32, 600u32);
        assert_eq!(
            resize_dir_at(0.0, 0.0, w, h),
            Some(ResizeDirection::NorthWest)
        );
        assert_eq!(
            resize_dir_at(w as f32, h as f32, w, h),
            Some(ResizeDirection::SouthEast)
        );
        assert_eq!(resize_dir_at(0.0, 300.0, w, h), Some(ResizeDirection::West));
        assert_eq!(
            resize_dir_at(400.0, h as f32, w, h),
            Some(ResizeDirection::South)
        );
        assert_eq!(resize_dir_at(400.0, 300.0, w, h), None);
    }
}
