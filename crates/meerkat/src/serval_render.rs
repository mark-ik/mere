/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `ScriptedDom` → `netrender::Scene` render glue, owned by the host.
//!
//! Formerly consumed from `pelt-live` (a serval-side probe). meerkat now owns
//! this thin assembly directly, depending on serval's *components* (serval-layout,
//! paint_list_render) rather than a serval *port*, so the product's render
//! hot-path no longer rides probe code and is immune to probe churn. The heavy,
//! tested logic stays single-source in serval-layout (cascade / layout / emit /
//! the caret + hit-test query primitives) and paint_list_render (the paint-list →
//! Scene lowering); only the convenience signatures live here. See
//! `design_docs/.../2026-06-11_serval_render_glue_extraction_plan.md`.
//!
//! The stateless helpers that remain (`fragments_from_scripted_dom` for measure,
//! `hit_test_node` for point-to-node) express themselves through a fresh
//! [`IncrementalLayout`]: one cascade+layout, the same work the session path does
//! for a steady frame. The scene path is session-only now. Every pane renders via
//! [`scene_from_session`] over a retained or fresh [`IncrementalLayout`], the
//! equivalence the C3 parity fixture proved.

use std::collections::HashMap;
use std::hash::Hash;

use layout_dom_api::LayoutDom;
use netrender::Scene;
use paint_list_api::{ColorF, DeviceIntSize};
use serval_layout::{FragmentPlane, ImageLoader, IncrementalLayout, ScrollOffsets, ServalPaintList};
use serval_scripted_dom::{NodeId, ScriptedDom};

/// Caret bar thickness, device px.
pub(crate) const CARET_WIDTH: f32 = 2.0;
/// Caret bar fallback colour, used only when the field's text colour can't be
/// resolved. The caret normally tracks the node's cascaded text colour (so it is
/// theme-correct on every theme); this light bar is the legible default if that
/// lookup misses.
const CARET_COLOR: ColorF = ColorF { r: 0.88, g: 0.90, b: 0.96, a: 1.0 };
/// Selection highlight colour (translucent blue; the text shows through). Alpha
/// raised so the highlight actually reads against the chrome background.
const SELECTION_COLOR: ColorF = ColorF { r: 0.42, g: 0.62, b: 0.98, a: 0.55 };
/// Scrollbar thumb colour (translucent dark grey, on the container's right edge).
const SCROLLBAR_COLOR: ColorF = ColorF { r: 0.30, g: 0.30, b: 0.36, a: 0.65 };
/// Scrollbar thumb width, device px.
const SCROLLBAR_WIDTH: f32 = 8.0;
/// Focus-ring colour (the accent blue, drawn as a thin outline on the focused element).
const FOCUS_RING_COLOR: ColorF = ColorF { r: 0.42, g: 0.62, b: 0.98, a: 0.95 };
/// Focus-ring outline thickness, device px.
const FOCUS_RING_WIDTH: f32 = 2.0;

/// What to paint for a focused text field's cursor: the element, the caret's byte
/// offset, and an optional selected byte range. Byte offsets (the layer works in
/// bytes); the host converts from its char-index model.
pub(crate) struct TextCursor {
    pub node: NodeId,
    pub caret: usize,
    pub selection: Option<(usize, usize)>,
    /// Whether the focused node is a text-editable field (an `<input>`). The caret +
    /// selection paint only when true; the focus ring (from `node`) paints regardless, so a
    /// focused button or orrery card rings without a spurious editing caret. (Phase 2a follow-up.)
    pub editable: bool,
}

/// An [`IncrementalLayout`] session → `netrender::Scene`: the chrome / workbench
/// pane path (cheap-path C3/C5). Emits from the session's retained layout plus the
/// focused field's selection + caret and scrollbar overlays, then lowers to a
/// Scene. The caller owns the session (rebuilt on a structural / resize / theme
/// frame, applied otherwise) and guarantees it is on an emittable path.
pub(crate) fn scene_from_session(
    session: &IncrementalLayout<NodeId>,
    dom: &ScriptedDom,
    cursor: Option<TextCursor>,
    scroll: &ScrollOffsets<NodeId>,
    width: u32,
    height: u32,
) -> Scene {
    paint_list_render::translate_paint_list(&paint_list_from_session(
        session, dom, cursor, scroll, width, height,
    ))
}

/// The [`ServalPaintList`] half of [`scene_from_session`]: emit from the session,
/// then append the focused-field selection (under) + caret (over) and scrollbar
/// overlays, all sourced from the session's retained layout so they match the body.
fn paint_list_from_session(
    session: &IncrementalLayout<NodeId>,
    dom: &ScriptedDom,
    cursor: Option<TextCursor>,
    scroll: &ScrollOffsets<NodeId>,
    width: u32,
    height: u32,
) -> ServalPaintList {
    let mut plist =
        session.emit_paint_list(dom, scroll, DeviceIntSize::new(width as i32, height as i32));
    // The cursor's node is `runner.focus()` (the host builds it from there), so the focus
    // ring below reads it without a separate focus parameter threaded through.
    let focus_node = cursor.as_ref().map(|c| c.node);

    // Caret + selection are text-field overlays: paint them only for editable focus. The
    // focus ring below still reads `focus_node`, so a focused button / orrery card rings
    // without a spurious editing caret. (Phase 2a follow-up.)
    if let Some(c) = cursor.filter(|c| c.editable) {
        if let Some((start, end)) = c.selection {
            let rects = session.selection_rects(dom, c.node, start, end);
            let highlight = session
                .selection_style(dom, c.node)
                .map(|(bg, _fg)| ColorF { r: bg[0], g: bg[1], b: bg[2], a: bg[3] })
                .unwrap_or(SELECTION_COLOR);
            plist.push_selection(&rects, highlight);
        }
        if let Some(rect) = session.caret_rect(dom, c.node, c.caret, CARET_WIDTH) {
            // The caret tracks the field's cascaded text colour (`caret-color:
            // auto`), so it stays legible on every theme; fall back to the light
            // default if the colour can't be resolved.
            let caret = session
                .caret_color(dom, c.node)
                .map(|[r, g, b, a]| ColorF { r, g, b, a })
                .unwrap_or(CARET_COLOR);
            plist.push_caret(rect, caret);
        }
    }

    push_scrollbars(&mut plist, dom, session.fragments(), scroll);
    push_focus_ring(&mut plist, session, dom, scroll, focus_node);
    plist
}

/// Any `LayoutDom` document (with image decode) → `netrender::Scene`, through
/// serval-layout's shared content pipeline + the paint-list lowering. The content
/// lane (fetched pages, the content card). No caret/selection/scrollbar overlays.
pub(crate) fn scene_from_layout_dom<D, L>(
    dom: &D,
    stylesheets: &[&str],
    loader: &L,
    width: u32,
    height: u32,
    band_y: u32,
    band_h: u32,
    scroll: &ScrollOffsets<D::NodeId>,
) -> (Scene, Vec<paint_list_render::BoxShadowMaskRequest>, u32, Vec<(String, [f32; 4])>)
where
    D: LayoutDom,
    // serval-layout's Send-ification (parallel shaping pre-pass) requires
    // `Send + Sync` on the node id; both real DOM node ids are usize newtypes,
    // so no concrete caller is restricted.
    D::NodeId: Copy + Eq + Hash + Send + Sync + 'static,
    L: ImageLoader,
{
    use paint_list_api::PaintList;
    // Lay out at the viewport, then emit ONE band (`band_y`..`band_y + band_h`) of the
    // page plus the document scroll range and the page's `<a href>` hit rects, so the
    // host knows the full height, can request the next band, and can hit-test a click
    // against the links (the flat serval scene is not queryable). A flat serval scene
    // the host cannot window, so the actor does the windowing here. Lower through
    // `translate_paint_cmd_stream` (not the scene-only `translate_paint_list`) so the
    // box-shadow mask requests survive for the host to build. The link rects are
    // full-document px (band-independent). (HTML scroll; box-shadow; inline-link nav.)
    let (list, scroll_range, links) = serval_layout::paint_list_band_from_layout_dom(
        dom, stylesheets, loader, width, height, band_y, band_h, scroll,
    );
    let tdl = paint_list_render::translate_paint_cmd_stream(
        list.viewport(),
        list.commands(),
        list.fonts(),
        list.images(),
    );
    let content_height = (height as f32 + scroll_range.1).ceil().max(1.0) as u32;
    (tdl.scene, tdl.box_shadow_masks, content_height, links)
}

/// Cascade + lay out `dom` and return its per-node fragment plane (no paint). The
/// measure-only path (chrome band measure, roster scroll clamp).
pub(crate) fn fragments_from_scripted_dom(
    dom: &ScriptedDom,
    stylesheets: &[&str],
    width: u32,
    height: u32,
) -> FragmentPlane<NodeId> {
    serval_layout::render(dom, stylesheets, width as f32, height as f32)
}

/// Lay out `dom` and hit-test scene point `(x, y)`, returning the topmost
/// (paint-order) node containing it. The stateless `point → NodeId` probe (the
/// fallback before a pane's session exists). `None` outside every fragment.
pub(crate) fn hit_test_node(
    dom: &ScriptedDom,
    stylesheets: &[&str],
    width: u32,
    height: u32,
    x: f32,
    y: f32,
    scroll: &ScrollOffsets<NodeId>,
) -> Option<NodeId> {
    IncrementalLayout::new(dom, stylesheets, width as f32, height as f32).hit_test(dom, x, y, scroll)
}

/// Accumulate each laid-out node's absolute origin into `out`, walking from `node`
/// whose parent sits at `parent_origin`. Taffy fragment locations are parent-
/// relative, so absolute coords require summing the chain — mirrors the a11y
/// tree's bounds accumulation (`serval_a11y::build`). Shared with the roster a11y
/// row bounds, which face the same nested-pane offset.
pub(crate) fn accumulate_origins(
    dom: &ScriptedDom,
    fragments: &FragmentPlane<NodeId>,
    node: NodeId,
    parent_origin: (f32, f32),
    out: &mut HashMap<NodeId, (f32, f32)>,
) {
    let origin = match fragments.rect_of(node) {
        Some(l) => (parent_origin.0 + l.location.x, parent_origin.1 + l.location.y),
        None => parent_origin,
    };
    out.insert(node, origin);
    for child in dom.dom_children(node) {
        accumulate_origins(dom, fragments, child, origin, out);
    }
}

/// Append a scrollbar thumb onto `plist` for each scrolled container: a bar on the
/// box's right edge, height ∝ visible/content, position ∝ offset/scrollable.
///
/// The thumb is placed at the container's **absolute** origin. Taffy locations are
/// parent-relative, so a single `rect_of` is only ≈ absolute for a top-level
/// container; a scroll container nested in a positioned ancestor (the roster pane,
/// the command palette) would otherwise land at `container_width` from the
/// document's left edge (left-aligned). We accumulate origins down the tree once,
/// the same parent + location sum the a11y tree does. (Nested scroll containers
/// are not offset by an ancestor's scroll here; the host's panes are single-level.)
fn push_scrollbars(
    plist: &mut ServalPaintList,
    dom: &ScriptedDom,
    fragments: &FragmentPlane<NodeId>,
    scroll_offsets: &ScrollOffsets<NodeId>,
) {
    if scroll_offsets.is_empty() {
        return;
    }
    let mut origins: HashMap<NodeId, (f32, f32)> = HashMap::new();
    accumulate_origins(dom, fragments, dom.document(), (0.0, 0.0), &mut origins);
    for (&node, &(_ox, oy)) in scroll_offsets {
        let Some(r) = fragments.rect_of(node) else { continue };
        let inner_h =
            r.size.height - r.padding.top - r.padding.bottom - r.border.top - r.border.bottom;
        let content_h = r.content_size.height;
        let scrollable = content_h - inner_h;
        if scrollable <= 0.5 {
            continue;
        }
        let Some(&(abs_x, abs_y)) = origins.get(&node) else { continue };
        let thumb_h = (r.size.height * (inner_h / content_h)).max(24.0);
        let thumb_y = abs_y + (oy / scrollable) * (r.size.height - thumb_h);
        let thumb_x = abs_x + r.size.width - SCROLLBAR_WIDTH;
        plist.push_fill(thumb_x, thumb_y, SCROLLBAR_WIDTH, thumb_h, SCROLLBAR_COLOR);
    }
}

/// Accumulate each laid-out node's **painted** origin: parent-relative taffy locations
/// summed, with each scroll container's offset applied to its descendants (the way paint
/// shifts scrolled content), so an overlay lands where an element actually renders. Distinct
/// from [`accumulate_origins`], which is unscrolled (the scrollbar thumb sits at the
/// container's own, unscrolled, edge). (Phase 1, step 3c.)
fn accumulate_painted_origins(
    dom: &ScriptedDom,
    fragments: &FragmentPlane<NodeId>,
    scroll: &HashMap<NodeId, (f32, f32)>,
    node: NodeId,
    parent_origin: (f32, f32),
    out: &mut HashMap<NodeId, (f32, f32)>,
) {
    let origin = match fragments.rect_of(node) {
        Some(l) => (parent_origin.0 + l.location.x, parent_origin.1 + l.location.y),
        None => parent_origin,
    };
    out.insert(node, origin);
    let (sx, sy) = scroll.get(&node).copied().unwrap_or((0.0, 0.0));
    let child_origin = (origin.0 - sx, origin.1 - sy);
    for child in dom.dom_children(node) {
        accumulate_painted_origins(dom, fragments, scroll, child, child_origin, out);
    }
}

/// Draw a focus ring (a thin outline) on the focused node at its painted bounds. The host
/// builds the cursor from `runner.focus()`, so this rings whatever is focused, chrome control
/// or folded-pane row/button (the engine leaves `:focus` styling to the host). No-op with
/// nothing focused or no laid-out box. (Phase 1, step 3c.)
fn push_focus_ring(
    plist: &mut ServalPaintList,
    session: &IncrementalLayout<NodeId>,
    dom: &ScriptedDom,
    scroll_offsets: &ScrollOffsets<NodeId>,
    focus: Option<NodeId>,
) {
    let Some(node) = focus else { return };
    let fragments = session.fragments();
    let Some(r) = fragments.rect_of(node) else { return };
    let scroll: HashMap<NodeId, (f32, f32)> =
        scroll_offsets.into_iter().map(|(n, o)| (*n, *o)).collect();
    let mut origins: HashMap<NodeId, (f32, f32)> = HashMap::new();
    accumulate_painted_origins(dom, fragments, &scroll, dom.document(), (0.0, 0.0), &mut origins);
    let Some(&(x, y)) = origins.get(&node) else { return };
    // A transform-positioned element (an orrery card) paints at its fragment slot plus its
    // accumulated CSS transform, which the fragments omit; add it so the ring tracks the card
    // where it paints, not at its pre-transform origin. Zero for untransformed chrome. (cond 1 interim.)
    let (tx, ty) = session.accumulated_translate(dom, node);
    let (x, y) = (x + tx, y + ty);
    let (w, h, t) = (r.size.width, r.size.height, FOCUS_RING_WIDTH);
    plist.push_fill(x, y, w, t, FOCUS_RING_COLOR);
    plist.push_fill(x, y + h - t, w, t, FOCUS_RING_COLOR);
    plist.push_fill(x, y, t, h, FOCUS_RING_COLOR);
    plist.push_fill(x + w - t, y, t, h, FOCUS_RING_COLOR);
}
