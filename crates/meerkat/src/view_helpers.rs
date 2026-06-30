/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Small view helpers: DOM class queries, theme/orrery palettes, camera <-> snapshot.

use super::*;

/// Lay out the chrome root and return the border-box bottom (px, rounded up) of
/// the first element carrying CSS class `class` — `"toolbar"` for the content
/// split, `"chrome"` for the click-region gate (toolbar + open dropdown).
/// `None` if no such element is laid out.
pub(crate) fn measure_class_bottom(
    dom: &ScriptedDom,
    sheet: &[&str],
    w: u32,
    h: u32,
    class: &str,
) -> Option<u32> {
    let frags = fragments_from_scripted_dom(dom, sheet, w, h);
    class_bottom_in(dom, &frags, class)
}

/// The border-box bottom (px, rounded up) of the first element carrying CSS
/// `class`, read from an **already-computed** fragment plane. Lets a caller that
/// holds a session's retained fragments (`ChromeSession::fragments`) measure the
/// chrome-region gate off the rendered layout instead of re-laying-out (C4).
/// `None` if no such element is laid out, or its bottom is non-positive.
pub(crate) fn class_bottom_in(
    dom: &ScriptedDom,
    frags: &FragmentPlane<NodeId>,
    class: &str,
) -> Option<u32> {
    first_with_class(dom, dom.document(), class)
        .and_then(|node| frags.rect_of(node))
        .map(|layout| (layout.location.y + layout.size.height).ceil() as u32)
        .filter(|&measured| measured > 0)
}

/// The first element carrying CSS class `class` in pre-order under `id` — the engine's
/// [`LayoutDom::first_with_class`](layout_dom_api::LayoutDom::first_with_class), kept as a
/// free-fn entry so the call sites read `first_with_class(&dom, id, class)`.
pub(crate) fn first_with_class(dom: &ScriptedDom, id: NodeId, class: &str) -> Option<NodeId> {
    dom.first_with_class(id, class)
}

/// Every element carrying CSS class `class` in pre-order under `id` (the engine's
/// [`LayoutDom::all_with_class`](layout_dom_api::LayoutDom::all_with_class)). Used to find
/// the workbench root's content placeholders, one per tile.
pub(crate) fn all_with_class(dom: &ScriptedDom, id: NodeId, class: &str) -> Vec<NodeId> {
    dom.all_with_class(id, class)
}

/// The `data-member` attribute of element `id`, parsed as a graph member id — the
/// tile whose content composites at this placeholder's rect.
pub(crate) fn member_attr(dom: &ScriptedDom, id: NodeId) -> Option<GraphMemberId> {
    dom.attributes(id)
        .find(|a| a.name.local.as_ref() == "data-member")
        .and_then(|a| a.value.parse::<GraphMemberId>().ok())
}

/// The orrery's themed palette — `(backdrop, edge)` as straight `[r, g, b, a]`
/// (0..1) — from a resolved theme: the backdrop is the theme background, the edge
/// a translucent default stroke that contrasts with it per theme. (Theming A2.)
pub(crate) fn orrery_palette(
    tokens: &register_theme::theme::ThemeTokenSet,
) -> ([f32; 4], [f32; 4]) {
    let (br, bg, bb) = tokens.theme_data.background_rgb;
    let backdrop = [br as f32 / 255.0, bg as f32 / 255.0, bb as f32 / 255.0, 1.0];
    let [er, eg, eb, _] = tokens.graph_node_chrome.default_stroke.to_array();
    // A higher alpha than the old translucent edges, so the stroke reads on a
    // light backdrop instead of washing out.
    let edge = [
        er as f32 / 255.0,
        eg as f32 / 255.0,
        eb as f32 / 255.0,
        0.85,
    ];
    (backdrop, edge)
}

/// Straight RGBA (0..1) for a chrome `Color32` — the format the document
/// [`document_canvas::ColorVocabulary`] + paint list consume.
pub(crate) fn vocab_color(c: register_theme::theme::Color32) -> [f32; 4] {
    [
        c.r() as f32 / 255.0,
        c.g() as f32 / 255.0,
        c.b() as f32 / 255.0,
        c.a() as f32 / 255.0,
    ]
}

/// The document-lane color palette for the focused-content card, derived from
/// the active theme: the chrome text tiers (`body` / `strong` / `muted`) plus
/// the theme accent for links, so smolweb / markdown / feed cards re-theme with
/// the shell instead of a fixed light-on-dark palette. Code has no dedicated
/// token — it takes `body_text` and leans on the monospace font for the
/// distinction. (Document theming, P3.)
pub(crate) fn document_palette(
    tokens: &register_theme::theme::ThemeTokenSet,
) -> document_canvas::ColorVocabulary {
    let ch = &tokens.chrome;
    let (ar, ag, ab) = tokens.theme_data.accent_rgb;
    let muted = ch.muted_text;
    let muted_rgb = [
        muted.r() as f32 / 255.0,
        muted.g() as f32 / 255.0,
        muted.b() as f32 / 255.0,
    ];
    document_canvas::ColorVocabulary {
        body_text: vocab_color(ch.body_text),
        heading_text: vocab_color(ch.strong_text),
        link_text: [ar as f32 / 255.0, ag as f32 / 255.0, ab as f32 / 255.0, 1.0],
        code_text: vocab_color(ch.body_text),
        badge_text: vocab_color(ch.muted_text),
        rule: vocab_color(ch.muted_text),
        placeholder_text: [muted_rgb[0], muted_rgb[1], muted_rgb[2], 0.12],
        placeholder_image: [muted_rgb[0], muted_rgb[1], muted_rgb[2], 0.20],
    }
}

/// A `wgpu::Color` (opaque) for a chrome `Color32` — the host-cleared content
/// card background, taken from the theme's floated-panel surface so the card
/// reads as a raised surface in every theme. (Document theming, P3.)
pub(crate) fn chrome_to_wgpu(c: register_theme::theme::Color32) -> wgpu::Color {
    wgpu::Color {
        r: c.r() as f64 / 255.0,
        g: c.g() as f64 / 255.0,
        b: c.b() as f64 / 255.0,
        a: 1.0,
    }
}

/// The first element with local tag `local` in pre-order under `id` — the engine's
/// [`LayoutDom::first_tag`](layout_dom_api::LayoutDom::first_tag).
pub(crate) fn first_tag(dom: &ScriptedDom, id: NodeId, local: &str) -> Option<NodeId> {
    dom.first_tag(id, local)
}

/// Whether element `id` carries CSS class `class` — the engine's
/// [`LayoutDom::has_class`](layout_dom_api::LayoutDom::has_class).
pub(crate) fn has_class(dom: &ScriptedDom, id: NodeId, class: &str) -> bool {
    dom.has_class(id, class)
}

/// Map the orrery camera to a serialized [`CameraSnapshot`] — the kurbo `Affine`
/// coefficient order `[a, b, c, d, e, f]` (a point maps to `(a*x + c*y + e,
/// b*x + d*y + f)`). The orrery camera is rotation(`yaw`) . non-uniform-scale(`zoom`,
/// `tilt*zoom`) . translate(`offset`), which the six coefficients carry exactly; a
/// top-down camera (`yaw 0`, `tilt 1`) reduces to `a = d = zoom, b = c = 0` (the prior
/// form), so old snapshots load unchanged. (Isometric camera — persist yaw/tilt.)
pub(crate) fn camera_to_snapshot(
    camera: CameraView,
    yaw: f32,
    tilt: f32,
) -> session_runtime::CameraSnapshot {
    let (sn, cs) = (yaw.sin() as f64, yaw.cos() as f64);
    let z = camera.zoom as f64;
    let tz = (tilt * camera.zoom) as f64;
    session_runtime::CameraSnapshot {
        coefficients: [
            cs * z,
            sn * tz,
            -sn * z,
            cs * tz,
            camera.offset.0 as f64,
            camera.offset.1 as f64,
        ],
    }
}

/// Recover pan + zoom from the affine coefficients: `offset` from `(e, f)`, `zoom`
/// from the first row's magnitude (`sqrt(a^2 + c^2)`, which is `zoom` for the orrery's
/// rotation+scale affine). The yaw/tilt half is [`snapshot_yaw_tilt`].
pub(crate) fn snapshot_to_camera(snapshot: &session_runtime::CameraSnapshot) -> CameraView {
    let m = snapshot.coefficients;
    let zoom = (m[0] * m[0] + m[2] * m[2]).sqrt();
    CameraView {
        offset: (m[4] as f32, m[5] as f32),
        zoom: zoom as f32,
    }
}

/// Recover the isometric orbit (`yaw`, radians) and vertical foreshorten (`tilt`) from
/// the affine coefficients: `yaw = atan2(-c, a)`, `tilt = |row2| / |row1|`. An old
/// top-down snapshot (`b = c = 0`, `a = d = zoom`) yields `(0, 1)`. (Isometric camera.)
pub(crate) fn snapshot_yaw_tilt(snapshot: &session_runtime::CameraSnapshot) -> (f32, f32) {
    let m = snapshot.coefficients;
    let row1 = (m[0] * m[0] + m[2] * m[2]).sqrt();
    let row2 = (m[1] * m[1] + m[3] * m[3]).sqrt();
    let yaw = (-m[2]).atan2(m[0]);
    let tilt = if row1 > 1e-6 { row2 / row1 } else { 1.0 };
    (yaw as f32, tilt as f32)
}

/// A durably-cached entry as a [`fetch::Fetched`], decoding the stored body as
/// text (lossily). Binary subresources are served from the resource cache as
/// bytes; this text view is for the page-document lane.
pub(crate) fn fetched_from(
    stored: session_runtime::content_store::StoredContent,
) -> fetch::Fetched {
    fetch::Fetched {
        content_type: stored.content_type,
        body: String::from_utf8_lossy(&stored.body).into_owned(),
    }
}
