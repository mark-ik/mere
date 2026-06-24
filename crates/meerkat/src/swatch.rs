/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The **swatch**: a portable, chrome-understood representation of a graph element (gloss
//! design §2a). It renders as **DOM** — serval lays it out, themes it, hit-tests it — not an
//! opaque `netrender::Scene`, so it can embed anywhere a graph element wants representing (a
//! node facet pane, a menu, a djot script block, an orrery card).
//!
//! First embedding: a **single node** scoped swatch — the node's sprite face plus its editable
//! collider hull (the Body axis) — in the `node:<id>/appearance` facet pane (the shape editor).
//! It renders the sprite (optional, a tracing underlay) + the hull polygon + a dot per vertex,
//! and is a full **body designer**: the host hit-tests the swatch through the chrome session
//! (the object-card press-gate pattern), walks up to the `node-swatch` container, reads its
//! `data-subject`, and drives editing from the cursor (serval has no native DOM pointer-drag):
//! drag a vertex to move it, click a hull edge to add a corner, right-click a vertex to remove
//! it. A node with no sprite can seed a default hull and shape it from scratch. The view is
//! concrete over `SettingsPanesView` for this first embedder; generalizing over the host state
//! is the reuse step when the menu / djot embeddings land. (Node body & face — the shape editor.)

use xilem_serval::el;

use crate::settings_pane_view::{SettingsPanesState, SettingsPanesView};

/// What a node swatch shows: the sprite face (a PNG data-URI) and its collider hull (the
/// opaque-region convex polygon in face-normalized coords, `[-0.5, 0.5]`). A node without a
/// sprite renders an empty swatch (the silhouette case joins later).
pub(crate) struct SwatchSpec {
    pub sprite: Option<String>,
    pub hull: Vec<(f32, f32)>,
    /// The graph node this swatch is scoped to, when it is editable: emitted as the
    /// container's `data-subject` so the host vertex-drag knows whose hull to rewrite.
    /// `None` for a non-node or read-only swatch. (Swatch — Stage B.)
    pub subject: Option<uuid::Uuid>,
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
        // what the physics body covers. Stage-B note: a vertex dragged inward makes the stored
        // polygon concave — this clip-path follows it, but parry reconvexifies the collider, so
        // the visual and the physics body diverge on a concave edit (expanding/convex edits stay
        // in lockstep). Accepted for now; a convexity constraint or dual-shape draw comes later.
        let pts: Vec<String> = spec
            .hull
            .iter()
            .map(|&(nx, ny)| format!("{:.2}% {:.2}%", (nx + 0.5) * 100.0, (ny + 0.5) * 100.0))
            .collect();
        children.push(Box::new(el::<_, SettingsPanesState, ()>("div", ()).attr(
            "style",
            format!(
                "position:absolute;left:0;top:0;width:{SWATCH}px;height:{SWATCH}px;\
                 background-color:rgba(120,170,255,0.16);clip-path:polygon({})",
                pts.join(", ")
            ),
        )));
        // A dot per vertex — the editor's handles: drag to move, right-click to remove; click a
        // bare edge to add a new one. (Node body & face — the shape editor.)
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

    // The swatch container: a positioned, sized box the children layer inside. The
    // `node-swatch` class + `data-subject` let the host hit-test walk up to it and learn
    // whose hull a vertex drag edits. (Swatch — Stage B.)
    let mut container = el::<_, SettingsPanesState, ()>("div", children)
        .attr("class", "node-swatch")
        .attr(
            "style",
            format!(
                "position:relative;width:{SWATCH}px;height:{SWATCH}px;margin-top:8px;\
                 background-color:rgba(0,0,0,0.25);border-radius:6px"
            ),
        );
    if let Some(subject) = spec.subject {
        container = container.attr("data-subject", subject.to_string());
    }
    Box::new(container)
}
