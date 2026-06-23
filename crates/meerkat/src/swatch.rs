/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The **swatch**: a portable, chrome-understood representation of a graph element (gloss
//! design §2a). It renders as **DOM** — serval lays it out, themes it, hit-tests it — not an
//! opaque `netrender::Scene`, so it can embed anywhere a graph element wants representing (a
//! node facet pane, a menu, a djot script block, an orrery card).
//!
//! First embedding: a **single node** scoped swatch — the node's sprite face plus its editable
//! collider hull — in the `node:<id>/appearance` facet pane (the shape editor). Stage A here is
//! the read-only render (sprite + hull polygon + a dot per vertex); the vertex-drag editing
//! (Stage B) routes through the shell hit-test, the object-card pattern. The view is concrete
//! over `SettingsPanesView` for this first embedder; generalizing over the host state is the
//! reuse step when the menu / djot embeddings land. (Node-rep — sprite shape editor.)

use xilem_serval::el;

use crate::settings_pane_view::{SettingsPanesState, SettingsPanesView};

/// What a node swatch shows: the sprite face (a PNG data-URI) and its collider hull (the
/// opaque-region convex polygon in face-normalized coords, `[-0.5, 0.5]`). A node without a
/// sprite renders an empty swatch (the silhouette case joins later).
pub(crate) struct SwatchSpec {
    pub sprite: Option<String>,
    pub hull: Vec<(f32, f32)>,
}

/// The swatch's on-screen edge length (px) in the facet pane.
const SWATCH: f32 = 220.0;
/// A vertex handle's diameter (px).
const HANDLE: f32 = 12.0;

/// Map a face-normalized coord (`-0.5..=0.5`) to a swatch-local pixel (`0..=SWATCH`).
pub(crate) fn norm_to_swatch_px(n: f32) -> f32 {
    (n + 0.5) * SWATCH
}

/// The swatch's edge length (px), so the host hit-test can place the swatch rect. (Stage B.)
pub(crate) fn swatch_edge_px() -> f32 {
    SWATCH
}

/// Build the node swatch as DOM: the sprite image, its collider hull as a translucent
/// clip-path polygon over it, and a dot at each hull vertex. Read-only (Stage A). The hull
/// is mapped from normalized `[-0.5, 0.5]` to the swatch's `0..100%` (clip-path) / `0..SWATCH`
/// (dots). (Swatch — node shape editor.)
pub(crate) fn swatch_view(spec: &SwatchSpec) -> SettingsPanesView {
    let mut children: Vec<SettingsPanesView> = Vec::new();

    // The sprite fills the swatch (cover-fit) — the surface the hull is traced over.
    if let Some(uri) = &spec.sprite {
        children.push(Box::new(el::<_, SettingsPanesState, ()>("img", ()).attr("src", uri.clone()).attr(
            "style",
            format!(
                "position:absolute;left:0;top:0;width:{SWATCH}px;height:{SWATCH}px;\
                 object-fit:cover;border-radius:6px;display:block"
            ),
        )));
    }

    if spec.hull.len() >= 3 {
        // The collider region: a translucent polygon clipped to the hull, so you see exactly
        // what the physics body covers.
        let pts: Vec<String> = spec
            .hull
            .iter()
            .map(|&(nx, ny)| format!("{:.2}% {:.2}%", (nx + 0.5) * 100.0, (ny + 0.5) * 100.0))
            .collect();
        children.push(Box::new(el::<_, SettingsPanesState, ()>("div", ()).attr(
            "style",
            format!(
                "position:absolute;left:0;top:0;width:{SWATCH}px;height:{SWATCH}px;\
                 background-color:rgba(120,170,255,0.28);clip-path:polygon({})",
                pts.join(", ")
            ),
        )));
        // A dot per vertex — the drag handles Stage B activates.
        let half = HANDLE / 2.0;
        for &(nx, ny) in &spec.hull {
            let cx = norm_to_swatch_px(nx);
            let cy = norm_to_swatch_px(ny);
            children.push(Box::new(el::<_, SettingsPanesState, ()>("div", ()).attr(
                "style",
                format!(
                    "position:absolute;left:{cx:.1}px;top:{cy:.1}px;width:{HANDLE}px;height:{HANDLE}px;\
                     margin-left:-{half}px;margin-top:-{half}px;border-radius:50%;\
                     background-color:#ffffff;border:1px solid rgba(0,0,0,0.55);box-sizing:border-box"
                ),
            )));
        }
    }

    // The swatch container: a positioned, sized box the children layer inside.
    Box::new(el::<_, SettingsPanesState, ()>("div", children).attr(
        "style",
        format!(
            "position:relative;width:{SWATCH}px;height:{SWATCH}px;margin-top:8px;\
             background-color:rgba(0,0,0,0.25);border-radius:6px"
        ),
    ))
}
