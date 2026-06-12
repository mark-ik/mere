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
//! The stateless functions express themselves through a fresh
//! [`IncrementalLayout`] and its session queries, which is exactly the equivalence
//! the C3 parity fixture proved (`scene_from_session` over a fresh session equals
//! the old stateless `scene_from_scripted_dom`), so this is a behaviour-preserving
//! re-expression. A fresh session does one cascade+layout, the same work the old
//! stateless path did.

use std::hash::Hash;

use layout_dom_api::LayoutDom;
use netrender::Scene;
use paint_list_api::{ColorF, DeviceIntSize};
use serval_layout::{FragmentPlane, ImageLoader, IncrementalLayout, ScrollOffsets, ServalPaintList};
use serval_scripted_dom::{NodeId, ScriptedDom};

/// Caret bar thickness, device px.
pub(crate) const CARET_WIDTH: f32 = 2.0;
/// Caret bar colour (near-black, opaque).
const CARET_COLOR: ColorF = ColorF { r: 0.12, g: 0.12, b: 0.20, a: 1.0 };
/// Selection highlight colour (translucent blue; the text shows through).
const SELECTION_COLOR: ColorF = ColorF { r: 0.40, g: 0.60, b: 0.95, a: 0.40 };
/// Scrollbar thumb colour (translucent dark grey, on the container's right edge).
const SCROLLBAR_COLOR: ColorF = ColorF { r: 0.30, g: 0.30, b: 0.36, a: 0.65 };
/// Scrollbar thumb width, device px.
const SCROLLBAR_WIDTH: f32 = 8.0;

/// What to paint for a focused text field's cursor: the element, the caret's byte
/// offset, and an optional selected byte range. Byte offsets (the layer works in
/// bytes); the host converts from its char-index model.
pub(crate) struct TextCursor {
    pub node: NodeId,
    pub caret: usize,
    pub selection: Option<(usize, usize)>,
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

    if let Some(c) = cursor {
        if let Some((start, end)) = c.selection {
            let rects = session.selection_rects(dom, c.node, start, end);
            let highlight = session
                .selection_style(dom, c.node)
                .map(|(bg, _fg)| ColorF { r: bg[0], g: bg[1], b: bg[2], a: bg[3] })
                .unwrap_or(SELECTION_COLOR);
            plist.push_selection(&rects, highlight);
        }
        if let Some(rect) = session.caret_rect(dom, c.node, c.caret, CARET_WIDTH) {
            plist.push_caret(rect, CARET_COLOR);
        }
    }

    push_scrollbars(&mut plist, session.fragments(), scroll);
    plist
}

/// Stateless cascade → layout → paint-emit → Scene over `dom`: the per-frame path
/// for panes without a persistent session (roster / apparatus / utility). A fresh
/// [`IncrementalLayout`] does one cascade+layout, then the same emit + overlays as
/// the session path, so this equals the session render for the same DOM (the C3
/// parity fixture's invariant). meerkat only ever passes `cursor: None` here (the
/// chrome's caret rides `scene_from_session`); the overlay arm stays for parity.
pub(crate) fn scene_from_scripted_dom(
    dom: &ScriptedDom,
    stylesheets: &[&str],
    width: u32,
    height: u32,
    cursor: Option<TextCursor>,
    scroll: &ScrollOffsets<NodeId>,
) -> Scene {
    let session = IncrementalLayout::new(dom, stylesheets, width as f32, height as f32);
    scene_from_session(&session, dom, cursor, scroll, width, height)
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
    scroll: &ScrollOffsets<D::NodeId>,
) -> Scene
where
    D: LayoutDom,
    D::NodeId: Copy + Eq + Hash + 'static,
    L: ImageLoader,
{
    paint_list_render::translate_paint_list(&serval_layout::paint_list_from_layout_dom(
        dom,
        stylesheets,
        loader,
        width,
        height,
        scroll,
    ))
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

/// Append a scrollbar thumb onto `plist` for each scrolled container: a bar on the
/// box's right edge, height ∝ visible/content, position ∝ offset/scrollable.
/// Absolute coords (top-level container ≈ absolute).
fn push_scrollbars(
    plist: &mut ServalPaintList,
    fragments: &FragmentPlane<NodeId>,
    scroll_offsets: &ScrollOffsets<NodeId>,
) {
    for (&node, &(_ox, oy)) in scroll_offsets {
        let Some(r) = fragments.rect_of(node) else { continue };
        let inner_h =
            r.size.height - r.padding.top - r.padding.bottom - r.border.top - r.border.bottom;
        let content_h = r.content_size.height;
        let scrollable = content_h - inner_h;
        if scrollable <= 0.5 {
            continue;
        }
        let thumb_h = (r.size.height * (inner_h / content_h)).max(24.0);
        let thumb_y = r.location.y + (oy / scrollable) * (r.size.height - thumb_h);
        let thumb_x = r.location.x + r.size.width - SCROLLBAR_WIDTH;
        plist.push_fill(thumb_x, thumb_y, SCROLLBAR_WIDTH, thumb_h, SCROLLBAR_COLOR);
    }
}
