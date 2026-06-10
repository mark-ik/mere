/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The F2.3 shellbar **session switcher**: a host-drawn strip of per-graph
//! thumbnail tiles anchored at the far (bottom) end of the shellbar. Each session
//! is a small swatch of its graph (from `build_switcher_thumbnail`); the active
//! one is highlighted; a trailing "+" tile mints a new graph and a "×" on each
//! tile trashes it. Mirrors the gloss minimap's host-draw + hit-rect pattern, so
//! clicks route through the same shellbar-region hit-test the move-menu uses.
//! (Multi-graph MG4.)

use frame::SessionId;
use netrender::{Scene, ScenePath};
use register_theme::chrome::{ChromeTheme, Color32};
use session_runtime::SwitcherThumbnail;

/// Thumbnail swatch dimensions (px) inside each tile. Kept under the 48px shellbar
/// thickness so the swatch fits a vertical (Left/Right) strip.
pub const SWITCHER_THUMB_W: u32 = 40;
pub const SWITCHER_THUMB_H: u32 = 26;

/// Per-tile full height (swatch + label gutter) and the inter-tile / edge pad.
const TILE_H: f32 = SWITCHER_THUMB_H as f32 + 8.0;
const PAD: f32 = 4.0;
/// The "+" create tile's height and the "×" close hit box (px).
const ADD_H: f32 = 18.0;
const CLOSE: f32 = 11.0;

/// The total height (px) the switcher strip needs for `session_count` sessions
/// plus the "+" tile. The host anchors a region this tall at the bottom of the
/// shellbar.
pub fn switcher_height(session_count: usize) -> f32 {
    session_count as f32 * (TILE_H + PAD) + ADD_H + 2.0 * PAD
}

/// Local hit-rects the switcher produced, all relative to the switcher region's
/// top-left (the host offsets them by the region origin).
#[derive(Default)]
pub struct SwitcherHits {
    /// Click switches to this session.
    pub rows: Vec<(SessionId, [f32; 4])>,
    /// Click trashes this session (omitted for the last remaining one).
    pub closes: Vec<(SessionId, [f32; 4])>,
    /// Click mints a new graph.
    pub add: Option<[f32; 4]>,
}

/// A chrome token at `alpha` as premultiplied `[r, g, b, a]` (matches gloss).
fn rgba(c: Color32, alpha: f32) -> [f32; 4] {
    let [r, g, b, _] = c.to_array();
    [
        r as f32 / 255.0 * alpha,
        g as f32 / 255.0 * alpha,
        b as f32 / 255.0 * alpha,
        alpha,
    ]
}

fn line(scene: &mut Scene, x0: f32, y0: f32, x1: f32, y1: f32, color: [f32; 4], width: f32) {
    let mut path = ScenePath::new();
    path.move_to(x0, y0).line_to(x1, y1);
    scene.push_shape_stroked(path, color, width);
}

/// Build the switcher scene for `entries` (id, thumbnail, active) into a `w`×`h`
/// region. Tiles stack top-to-bottom; the active tile is highlighted; a trailing
/// "+" tile mints a graph. The "×" close box is omitted when only one session
/// exists (never close the last graph). Returns the scene + local hit-rects.
pub fn switcher_scene(
    entries: &[(SessionId, &SwitcherThumbnail, bool)],
    w: u32,
    h: u32,
    theme: &ChromeTheme,
) -> (Scene, SwitcherHits) {
    let mut scene = Scene::new(w.max(1), h.max(1));
    let mut hits = SwitcherHits::default();
    let allow_close = entries.len() > 1;
    let tile_w = (w as f32 - 2.0 * PAD).max(1.0);
    let mut y = PAD;

    for (id, thumb, active) in entries {
        let tile = [PAD, y, PAD + tile_w, y + TILE_H];
        let bg = if *active {
            rgba(theme.active_bg, 1.0)
        } else {
            rgba(theme.surface_bg, 1.0)
        };
        scene.push_rect(tile[0], tile[1], tile[2], tile[3], bg);
        draw_thumbnail(&mut scene, thumb, tile, theme, *active);
        hits.rows.push((*id, tile));

        if allow_close {
            let cx0 = tile[2] - CLOSE - 2.0;
            let cy0 = tile[1] + 2.0;
            let close = [cx0, cy0, cx0 + CLOSE, cy0 + CLOSE];
            let c = rgba(theme.muted_text, 0.9);
            line(&mut scene, close[0], close[1], close[2], close[3], c, 1.2);
            line(&mut scene, close[2], close[1], close[0], close[3], c, 1.2);
            hits.closes.push((*id, close));
        }
        y += TILE_H + PAD;
    }

    // The "+" new-graph tile.
    let add = [PAD, y, PAD + tile_w, y + ADD_H];
    scene.push_rect(add[0], add[1], add[2], add[3], rgba(theme.surface_bg, 1.0));
    let plus = rgba(theme.body_text, 1.0);
    let mid_x = (add[0] + add[2]) * 0.5;
    let mid_y = (add[1] + add[3]) * 0.5;
    let arm = ADD_H * 0.3;
    line(&mut scene, mid_x - arm, mid_y, mid_x + arm, mid_y, plus, 1.6);
    line(&mut scene, mid_x, mid_y - arm, mid_x, mid_y + arm, plus, 1.6);
    hits.add = Some(add);

    (scene, hits)
}

/// Paint one session's thumbnail (edges then nodes) centered in `tile`. The
/// thumbnail's positions are in its own `width`×`height` pixel frame.
fn draw_thumbnail(
    scene: &mut Scene,
    thumb: &SwitcherThumbnail,
    tile: [f32; 4],
    theme: &ChromeTheme,
    active: bool,
) {
    let ox = tile[0] + ((tile[2] - tile[0]) - thumb.width as f32) * 0.5;
    let oy = tile[1] + ((tile[3] - tile[1]) - thumb.height as f32) * 0.5;
    let edge_c = rgba(theme.muted_text, 0.6);
    for e in &thumb.edges {
        line(
            scene,
            ox + e.from.x,
            oy + e.from.y,
            ox + e.to.x,
            oy + e.to.y,
            edge_c,
            0.8,
        );
    }
    let node_c = if active {
        rgba(theme.strong_text, 1.0)
    } else {
        rgba(theme.body_text, 0.9)
    };
    for n in &thumb.nodes {
        let r = n.radius.max(1.5);
        scene.push_rect(
            ox + n.position.x - r,
            oy + n.position.y - r,
            ox + n.position.x + r,
            oy + n.position.y + r,
            node_c,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use session_runtime::{SwitcherThumbnailOptions, build_switcher_thumbnail};

    fn theme() -> ChromeTheme {
        register_theme::theme::ThemeRegistry::default()
            .active_theme()
            .tokens
            .chrome
    }

    fn thumb() -> SwitcherThumbnail {
        let mut g = kernel::graph::Graph::new();
        g.add_node("a".into(), kernel::geometry::PortablePoint::new(0.0, 0.0));
        g.add_node("b".into(), kernel::geometry::PortablePoint::new(10.0, 10.0));
        build_switcher_thumbnail(
            &g,
            SwitcherThumbnailOptions {
                width: SWITCHER_THUMB_W,
                height: SWITCHER_THUMB_H,
                ..Default::default()
            },
        )
    }

    #[test]
    fn one_session_has_a_row_and_add_but_no_close() {
        let t = thumb();
        let sid = SessionId::new();
        let (_scene, hits) = switcher_scene(&[(sid, &t, true)], 48, 200, &theme());
        assert_eq!(hits.rows.len(), 1);
        assert_eq!(hits.rows[0].0, sid);
        assert!(hits.add.is_some());
        assert!(hits.closes.is_empty(), "the last session has no close box");
    }

    #[test]
    fn two_sessions_each_get_a_close_box() {
        let t = thumb();
        let a = SessionId::new();
        let b = SessionId::new();
        let (_scene, hits) =
            switcher_scene(&[(a, &t, true), (b, &t, false)], 48, 240, &theme());
        assert_eq!(hits.rows.len(), 2);
        assert_eq!(hits.closes.len(), 2);
        // Rows stack downward (the second row sits below the first).
        assert!(hits.rows[1].1[1] > hits.rows[0].1[1]);
    }
}
