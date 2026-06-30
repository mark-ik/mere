/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shellbar geometry helpers (F2.1). The shellbar is a chrome strip docked to
//! one window edge, outside the frame tree. These pure functions project a
//! [`ShellbarEdge`] preference onto pixel rects so render and input agree on
//! where the strip lives without carrying the edge through multiple call sites.

use session_runtime::ShellbarEdge;

/// Base width (when Left/Right) or height (when Top/Bottom) of the shellbar strip at
/// `ui_scale == 1.0`. The callers multiply by the chrome's `ui_scale` so the strip
/// widens with its (CSS-scaled) buttons — otherwise the buttons overflow the fixed
/// strip at higher zoom / DPI. (Auto-DPI — shellbar scales with the chrome.)
pub const SHELLBAR_THICKNESS: f32 = 44.0;

/// The shellbar strip rect `[x0, y0, x1, y1]` in window coordinates, the strip
/// `SHELLBAR_THICKNESS × scale` thick (`scale` = the chrome `ui_scale`, so the strip
/// tracks its scaled buttons).
///
/// The strip starts immediately below the toolbar (`toolbar_h`) — it shares the
/// content band's vertical extent, so the toolbar chrome is never occluded.
pub fn shellbar_rect(edge: ShellbarEdge, w: f32, h: f32, toolbar_h: f32, scale: f32) -> [f32; 4] {
    let t = SHELLBAR_THICKNESS * scale;
    match edge {
        ShellbarEdge::Left => [0.0, toolbar_h, t, h],
        ShellbarEdge::Right => [w - t, toolbar_h, w, h],
        ShellbarEdge::Top => [0.0, toolbar_h, w, toolbar_h + t],
        ShellbarEdge::Bottom => [0.0, h - t, w, h],
    }
}

/// The content band rect `[x0, y0, x1, y1]` after subtracting the shellbar strip
/// (`SHELLBAR_THICKNESS × scale` thick). This rect is passed to
/// [`frame_view::leaf_rects`] so the frame tree fills the remaining space.
pub fn band_after_shellbar(
    edge: ShellbarEdge,
    w: f32,
    h: f32,
    toolbar_h: f32,
    scale: f32,
) -> [f32; 4] {
    let t = SHELLBAR_THICKNESS * scale;
    match edge {
        ShellbarEdge::Left => [t, toolbar_h, w, h],
        ShellbarEdge::Right => [0.0, toolbar_h, w - t, h],
        ShellbarEdge::Top => [0.0, toolbar_h + t, w, h],
        ShellbarEdge::Bottom => [0.0, toolbar_h, w, h - t],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_strip_starts_at_zero_x_below_toolbar() {
        let r = shellbar_rect(ShellbarEdge::Left, 1000.0, 700.0, 42.0, 1.0);
        assert_eq!(r, [0.0, 42.0, SHELLBAR_THICKNESS, 700.0]);
        let b = band_after_shellbar(ShellbarEdge::Left, 1000.0, 700.0, 42.0, 1.0);
        assert_eq!(b, [SHELLBAR_THICKNESS, 42.0, 1000.0, 700.0]);
    }

    #[test]
    fn right_strip_abuts_right_edge() {
        let r = shellbar_rect(ShellbarEdge::Right, 1000.0, 700.0, 42.0, 1.0);
        assert_eq!(r, [1000.0 - SHELLBAR_THICKNESS, 42.0, 1000.0, 700.0]);
        let b = band_after_shellbar(ShellbarEdge::Right, 1000.0, 700.0, 42.0, 1.0);
        assert_eq!(b, [0.0, 42.0, 1000.0 - SHELLBAR_THICKNESS, 700.0]);
    }

    #[test]
    fn top_strip_sits_just_below_toolbar() {
        let r = shellbar_rect(ShellbarEdge::Top, 1000.0, 700.0, 42.0, 1.0);
        assert_eq!(r, [0.0, 42.0, 1000.0, 42.0 + SHELLBAR_THICKNESS]);
        let b = band_after_shellbar(ShellbarEdge::Top, 1000.0, 700.0, 42.0, 1.0);
        assert_eq!(b, [0.0, 42.0 + SHELLBAR_THICKNESS, 1000.0, 700.0]);
    }

    #[test]
    fn bottom_strip_abuts_bottom_edge() {
        let r = shellbar_rect(ShellbarEdge::Bottom, 1000.0, 700.0, 42.0, 1.0);
        assert_eq!(r, [0.0, 700.0 - SHELLBAR_THICKNESS, 1000.0, 700.0]);
        let b = band_after_shellbar(ShellbarEdge::Bottom, 1000.0, 700.0, 42.0, 1.0);
        assert_eq!(b, [0.0, 42.0, 1000.0, 700.0 - SHELLBAR_THICKNESS]);
    }
}
