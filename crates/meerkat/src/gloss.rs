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
//!
//! Outline and recent are real DOM now (`gloss_outline_view.rs` / `gloss_view.rs`);
//! this file keeps the pane-split math (`gloss_sections`) plus the minimap's Scene
//! raster, still pending its own DOM-nodes-plus-embedded-Scene-edges migration (the
//! [Scene-to-DOM migration plan](../../../design_docs/mere_docs/implementation_strategy/2026-07-01_gloss_scene_to_dom_migration_plan.md)'s
//! Phase 2).

use netrender::Scene;
use register_theme::chrome::{ChromeTheme, Color32};

/// Inset (px) of the swatch from the pane edges.
const PAD: f32 = 16.0;
/// Edge length (px) of a minimap node square.
const NODE: f32 = 7.0;
/// Fixed height (px) of the gloss "recent" section — the DOM recent lens
/// (`gloss_view::recent_view`) still gets a fixed band; only its rendering
/// mechanism changed, not its sizing.
const RECENT_H: f32 = 110.0;

/// Intent a gloss row (outline / minimap / recent) queues on click: select + focus
/// the node at this URL — the shared `Orrery::select_by_url` primitive every gloss
/// section (and the roster's non-additive click) drives, so a click's effect is
/// identical everywhere. One shared type instead of three near-identical
/// single-variant enums. (gloss-outline plan; Scene-to-DOM migration P1.)
#[derive(Clone, Debug, PartialEq)]
pub enum GlossRowIntent {
    Select(String),
}

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

/// A chrome token as a CSS `rgb(...)` string, for DOM node squares (the minimap's
/// nodes are DOM now; only its edges/rings backdrop stays a Scene raster).
pub fn theme_rgb_css(c: Color32) -> String {
    let [r, g, b, _] = c.to_array();
    format!("rgb({r}, {g}, {b})")
}

/// A minimap node's edge length (px): the focused node draws a touch larger so it
/// stands out; `size_factor` (1.0 unless the gloss sizes by importance) scales it
/// further. (Graph signals — P6c.) Shared by the DOM node squares and, previously,
/// the Scene-drawn ones — kept as one function so they can't drift.
pub fn minimap_node_size(selected: bool, size_factor: f32) -> f32 {
    (if selected { NODE + 3.0 } else { NODE }) * size_factor
}

/// The minimap's fit transform: uniform-scale + center the node bounding box into
/// `w`x`h` with `PAD` inset. Computed once from the (world-space) node positions so
/// the backdrop Scene (edges/rings, [`minimap_backdrop_scene`]) and the DOM node
/// squares (`gloss_view::minimap_view`) map through the identical transform and
/// can't disagree about where anything sits. (Scene-to-DOM migration P2.)
pub struct MinimapFit {
    scale: f32,
    off_x: f32,
    off_y: f32,
    min_x: f32,
    min_y: f32,
}

impl MinimapFit {
    /// `None` for an empty node set — nothing to fit, nothing to draw.
    pub fn compute(positions: &[(f32, f32)], w: u32, h: u32) -> Option<Self> {
        if positions.is_empty() {
            return None;
        }
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (x, y) in positions {
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
        let off_x = PAD + (avail_w - bbox_w * scale) * 0.5;
        let off_y = PAD + (avail_h - bbox_h * scale) * 0.5;
        Some(Self {
            scale,
            off_x,
            off_y,
            min_x,
            min_y,
        })
    }

    pub fn apply(&self, (x, y): (f32, f32)) -> (f32, f32) {
        (
            (x - self.min_x) * self.scale + self.off_x,
            (y - self.min_y) * self.scale + self.off_y,
        )
    }
}

/// Build the minimap's backdrop Scene: edges + signal rings only — no node squares
/// (those are DOM now, `gloss_view::minimap_view`). `edges`/`rings` are already
/// mapped to pane-local coordinates via the same [`MinimapFit`] the DOM nodes used,
/// so this only draws, it never positions. (Scene-to-DOM migration P2; trimmed from
/// the old `minimap_scene`, which also drew the nodes.)
pub fn minimap_backdrop_scene(
    edges: &[((f32, f32), (f32, f32), f32)],
    rings: &[((f32, f32), f32, [f32; 4])],
    w: u32,
    h: u32,
    theme: &ChromeTheme,
) -> Scene {
    let mut scene = Scene::new(w.max(1), h.max(1));

    let edge_color = rgba(theme.muted_text, 0.7);
    for (a, b, weight) in edges {
        let mut path = netrender::ScenePath::new();
        path.move_to(a.0, a.1).line_to(b.0, b.1);
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
        let r = NODE * *factor;
        let [cr, cg, cb, ca] = *color;
        let premul = [cr * ca, cg * ca, cb * ca, ca];
        let mut path = netrender::ScenePath::new();
        for i in 0..=RING_SEGMENTS {
            let t = (i as f32 / RING_SEGMENTS as f32) * std::f32::consts::TAU;
            let (x, y) = (center.0 + r * t.cos(), center.1 + r * t.sin());
            if i == 0 {
                path.move_to(x, y);
            } else {
                path.line_to(x, y);
            }
        }
        scene.push_shape_stroked(path, premul, 1.5);
    }
    scene
}

