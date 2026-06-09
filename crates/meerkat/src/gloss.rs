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

/// Inset (px) of the swatch from the pane edges.
const PAD: f32 = 16.0;
/// Edge length (px) of a minimap node square.
const NODE: f32 = 7.0;

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
    nodes: &[(GraphMemberId, (f32, f32), bool)],
    edges: &[((f32, f32), (f32, f32))],
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
    for (_, (x, y), _) in nodes {
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
    for (a, b) in edges {
        let (ax, ay) = map(*a);
        let (bx, by) = map(*b);
        let mut path = netrender::ScenePath::new();
        path.move_to(ax, ay).line_to(bx, by);
        scene.push_shape_stroked(path, edge_color, 1.0);
    }

    let node_color = rgba(theme.body_text, 1.0);
    let selected_color = rgba(theme.strong_text, 1.0);
    for (id, pos, selected) in nodes {
        let (cx, cy) = map(*pos);
        // The focused node draws a touch larger + brighter so it stands out.
        let size = if *selected { NODE + 3.0 } else { NODE };
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
