/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The gloss pane (the Navigator), first cut: a **whole-graph minimap swatch**.
//! Mark's framing — "how do you outline a graph? with swatches" — so gloss opens
//! on the graph scope in swatch form: the orrery's node positions + edges, fit
//! into the pane, the focused node highlighted. The scope × form-factor matrix
//! (active-doc outline, graphlet swatches, MRU) is the larger Navigator the
//! design doc lays out; this is the G1/G2 seed. The gloss draws its own swatch
//! from graph geometry rather than rendering a second orrery (the Navigator is
//! one surface, never a second instance).

use forme::GraphMemberId;
use netrender::Scene;
use register_theme::chrome::{ChromeTheme, Color32};

use super::text::HostText;

/// Inset (px) of the swatch from the pane edges.
const PAD: f32 = 16.0;
/// Edge length (px) of a minimap node square.
const NODE: f32 = 7.0;
/// Fixed height (px) of the gloss "recent" section: a header plus a handful of rows
/// at [`RECENT_ROW_H`], sized like [`recent_scene`] already assumes.
const RECENT_H: f32 = 110.0;

/// Split the gloss pane's rect into its three stacked sections, top to bottom:
/// minimap (Scene), outline (DOM), recent (Scene). Recent stays fixed height; the
/// minimap and outline flex evenly over the remainder, so the DOM outline lens
/// (gloss-outline plan P1) and the two Scene sections agree on the same geometry
/// every frame. Recent shrinks (never negative) on a too-short pane.
pub fn gloss_sections(rect: [f32; 4]) -> ([f32; 4], [f32; 4], [f32; 4]) {
    let [x0, y0, x1, y1] = rect;
    let total_h = (y1 - y0).max(0.0);
    let recent_h = RECENT_H.min(total_h * 0.5);
    let remaining = total_h - recent_h;
    let minimap_h = (remaining * 0.5).round();
    let minimap_rect = [x0, y0, x1, y0 + minimap_h];
    let outline_rect = [x0, y0 + minimap_h, x1, y1 - recent_h];
    let recent_rect = [x0, y1 - recent_h, x1, y1];
    (minimap_rect, outline_rect, recent_rect)
}

/// A chrome token at `alpha` as a premultiplied `[r, g, b, a]`.
fn rgba(c: Color32, alpha: f32) -> [f32; 4] {
    let [r, g, b, _] = c.to_array();
    [
        r as f32 / 255.0 * alpha,
        g as f32 / 255.0 * alpha,
        b as f32 / 255.0 * alpha,
        alpha,
    ]
}

/// Build the minimap swatch: fit the node positions into the `w` x `h` pane
/// (uniform scale, centered), draw the edges then the nodes, and highlight the
/// selected node. Returns the scene plus each node's **pane-local** rect for the
/// host's hit-test (offset by the pane origin to focus a node on click).
pub fn minimap_scene(
    nodes: &[(GraphMemberId, (f32, f32), bool, f32)],
    edges: &[((f32, f32), (f32, f32), f32)],
    rings: &[((f32, f32), f32, [f32; 4])],
    w: u32,
    h: u32,
    theme: &ChromeTheme,
) -> (Scene, Vec<(GraphMemberId, [f32; 4])>) {
    let mut scene = Scene::new(w.max(1), h.max(1));
    let mut rects = Vec::new();
    if nodes.is_empty() {
        return (scene, rects);
    }

    // World bounding box of the node positions.
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (_, (x, y), _, _) in nodes {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }
    let bbox_w = (max_x - min_x).max(1.0);
    let bbox_h = (max_y - min_y).max(1.0);
    let avail_w = (w as f32 - 2.0 * PAD).max(1.0);
    let avail_h = (h as f32 - 2.0 * PAD).max(1.0);
    let scale = (avail_w / bbox_w).min(avail_h / bbox_h);
    // Center the scaled graph in the available area.
    let off_x = PAD + (avail_w - bbox_w * scale) * 0.5;
    let off_y = PAD + (avail_h - bbox_h * scale) * 0.5;
    let map = |(x, y): (f32, f32)| ((x - min_x) * scale + off_x, (y - min_y) * scale + off_y);

    let edge_color = rgba(theme.muted_text, 0.7);
    for (a, b, weight) in edges {
        let (ax, ay) = map(*a);
        let (bx, by) = map(*b);
        let mut path = netrender::ScenePath::new();
        path.move_to(ax, ay).line_to(bx, by);
        // Stroke width grows with the edge's multiplicity weight (relations between the pair),
        // capped so a very dense pair stays legible in the swatch. (Graph signals — edge thickness.)
        let width = (1.0 + 0.6 * (*weight - 1.0)).clamp(1.0, 3.0);
        scene.push_shape_stroked(path, edge_color, width);
    }

    // Signal rings (community halos + bridge emphasis from the lens's overlays), under the nodes so
    // each node square sits on its halo. The radius is a multiple of the node size in swatch space
    // (fixed, like the nodes — not scaled by the fit), centred on the mapped lens position. Colours
    // arrive straight-alpha, so premultiply for the scene. (Graph signals — P6b, gloss overlays.)
    const RING_SEGMENTS: usize = 20;
    for (center, factor, color) in rings {
        let (cx, cy) = map(*center);
        let r = NODE * *factor;
        let [cr, cg, cb, ca] = *color;
        let premul = [cr * ca, cg * ca, cb * ca, ca];
        let mut path = netrender::ScenePath::new();
        for i in 0..=RING_SEGMENTS {
            let t = (i as f32 / RING_SEGMENTS as f32) * std::f32::consts::TAU;
            let (x, y) = (cx + r * t.cos(), cy + r * t.sin());
            if i == 0 {
                path.move_to(x, y);
            } else {
                path.line_to(x, y);
            }
        }
        scene.push_shape_stroked(path, premul, 1.5);
    }

    let node_color = rgba(theme.body_text, 1.0);
    let selected_color = rgba(theme.strong_text, 1.0);
    for (id, pos, selected, size_factor) in nodes {
        let (cx, cy) = map(*pos);
        // The focused node draws a touch larger + brighter so it stands out; the size factor (1.0
        // unless the gloss sizes by importance) scales it further. (Graph signals — P6c.)
        let base = if *selected { NODE + 3.0 } else { NODE };
        let size = base * *size_factor;
        let half = size * 0.5;
        let color = if *selected {
            selected_color
        } else {
            node_color
        };
        // push_rect takes corners (x0, y0, x1, y1), not (x, y, w, h).
        scene.push_rect(cx - half, cy - half, cx + half, cy + half, color);
        rects.push((*id, [cx - half, cy - half, cx + half, cy + half]));
    }
    (scene, rects)
}

/// Row height (px), row font, and header font for the recent-visited list.
const RECENT_ROW_H: f32 = 16.0;
const RECENT_FONT: f32 = 11.0;
const RECENT_HEADER_FONT: f32 = 10.0;

/// Build the gloss "recent" list: a header plus one row per recently-visited node
/// (its label, truncated to fit), drawn into the `w` x `h` recent sub-pane. `text`
/// shapes the labels. Returns the scene plus each row's **pane-local** rect (node
/// id), so the host hit-tests a click to focus that node, exactly like the minimap
/// squares. (gloss recent-nodes; the `SharedNavigationMemory` projection.)
pub fn recent_scene(
    recent: &[(GraphMemberId, String)],
    w: u32,
    h: u32,
    theme: &ChromeTheme,
    text: &mut HostText,
) -> (Scene, Vec<(GraphMemberId, [f32; 4])>) {
    let mut scene = Scene::new(w.max(1), h.max(1));
    let mut rects = Vec::new();
    let wf = w as f32;
    // A thin divider at the top separates the list from the minimap above.
    let mut path = netrender::ScenePath::new();
    path.move_to(PAD, 0.5).line_to(wf - PAD, 0.5);
    scene.push_shape_stroked(path, rgba(theme.muted_text, 0.4), 1.0);

    let mut y = 5.0;
    text.push_line(
        &mut scene,
        "Recent",
        RECENT_HEADER_FONT,
        rgba(theme.muted_text, 0.9),
        [PAD, y],
    );
    y += RECENT_HEADER_FONT + 5.0;

    if recent.is_empty() {
        text.push_line(
            &mut scene,
            "nothing visited yet",
            RECENT_FONT,
            rgba(theme.muted_text, 0.55),
            [PAD, y],
        );
        return (scene, rects);
    }

    let row_color = rgba(theme.body_text, 0.95);
    for (id, label) in recent {
        if y + RECENT_ROW_H > h as f32 {
            break; // out of vertical room
        }
        let fitted = fit_label(text, label, wf - 2.0 * PAD, RECENT_FONT);
        if !fitted.is_empty() {
            text.push_line(&mut scene, &fitted, RECENT_FONT, row_color, [PAD, y]);
        }
        rects.push((*id, [PAD, y, wf - PAD, y + RECENT_ROW_H]));
        y += RECENT_ROW_H;
    }
    (scene, rects)
}

/// Trim `label` from the end until it fits `max_w` at `font`. Returns at least the
/// first char of a non-empty label.
fn fit_label(text: &mut HostText, label: &str, max_w: f32, font: f32) -> String {
    let label = label.trim();
    if label.is_empty() || max_w <= 0.0 {
        return String::new();
    }
    if text.measure(label, font).0 <= max_w {
        return label.to_string();
    }
    let mut chars: Vec<char> = label.chars().collect();
    while chars.len() > 1 {
        chars.pop();
        let candidate: String = chars.iter().collect();
        if text.measure(&candidate, font).0 <= max_w {
            return candidate;
        }
    }
    chars.into_iter().collect()
}
