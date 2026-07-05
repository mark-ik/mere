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
use rustc_hash::FxHashSet;
use serval_layout::{
    FragmentPlane, ImageLoader, IncrementalLayout, ScrollOffsets, ServalPaintList,
};
use serval_scripted_dom::{NodeId, ScriptedDom};

/// Caret bar thickness, device px.
pub(crate) const CARET_WIDTH: f32 = 2.0;
/// Caret bar fallback colour, used only when the field's text colour can't be
/// resolved. The caret normally tracks the node's cascaded text colour (so it is
/// theme-correct on every theme); this light bar is the legible default if that
/// lookup misses.
const CARET_COLOR: ColorF = ColorF {
    r: 0.88,
    g: 0.90,
    b: 0.96,
    a: 1.0,
};
/// Selection highlight colour (translucent blue; the text shows through). Alpha
/// raised so the highlight actually reads against the chrome background.
const SELECTION_COLOR: ColorF = ColorF {
    r: 0.42,
    g: 0.62,
    b: 0.98,
    a: 0.55,
};
/// Focus-ring colour (the accent blue, drawn as a thin outline on the focused element).
const FOCUS_RING_COLOR: ColorF = ColorF {
    r: 0.42,
    g: 0.62,
    b: 0.98,
    a: 0.95,
};
/// Focus-ring outline thickness, device px.
const FOCUS_RING_WIDTH: f32 = 2.0;

/// What to paint for a focused text field's cursor: the element, the caret's byte
/// offset, and an optional selected byte range. Byte offsets (the layer works in
/// bytes); the host converts from its char-index model.
#[derive(Clone, Copy)]
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
    let plist = paint_list_from_session(session, dom, cursor, scroll, width, height);
    lower_timed("full", &plist)
}

pub(crate) fn scene_from_session_excluding_subtrees(
    session: &IncrementalLayout<NodeId>,
    dom: &ScriptedDom,
    cursor: Option<TextCursor>,
    scroll: &ScrollOffsets<NodeId>,
    skipped_subtrees: &FxHashSet<NodeId>,
    width: u32,
    height: u32,
) -> Scene {
    let plist = paint_list_from_session_excluding_subtrees(
        session,
        dom,
        cursor,
        scroll,
        skipped_subtrees,
        width,
        height,
    );
    lower_timed("base", &plist)
}

pub(crate) fn scene_from_session_subtree(
    session: &IncrementalLayout<NodeId>,
    dom: &ScriptedDom,
    root: NodeId,
    cursor: Option<TextCursor>,
    scroll: &ScrollOffsets<NodeId>,
    width: u32,
    height: u32,
) -> Option<Scene> {
    let plist = paint_list_from_session_subtree(session, dom, root, cursor, scroll, width, height)?;
    Some(lower_timed("orrery", &plist))
}

/// Lower a paint list to a `netrender::Scene`, logging the P0 attribution spans
/// (emission is timed by the callers of `paint_list_from_session*`; this logs the
/// lowering half plus the command count so a loaded-session frame can say how much
/// of `chrome_us` was scene production vs paint-list -> Scene translation).
/// (2026-07-03 shell paint emission plan, P0.)
fn lower_timed(lane: &str, plist: &ServalPaintList) -> Scene {
    use paint_list_api::PaintList;
    let t = std::time::Instant::now();
    let scene = paint_list_render::translate_paint_list(plist);
    tracing::debug!(
        target: "meerkat::profile",
        lane,
        lower_us = t.elapsed().as_micros() as u64,
        cmds = plist.commands().len(),
        "paint list lowered"
    );
    scene
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
    let merged = merged_scroll_offsets(session, scroll);
    let t = std::time::Instant::now();
    let mut plist =
        session.emit_paint_list(dom, scroll, DeviceIntSize::new(width as i32, height as i32));
    tracing::debug!(
        target: "meerkat::profile",
        lane = "full",
        emit_us = t.elapsed().as_micros() as u64,
        "paint list emitted"
    );
    append_cursor_and_focus(&mut plist, session, dom, &merged, cursor);
    serval_layout::push_scrollbars(&mut plist, dom, session.fragments(), &merged);
    plist
}

fn paint_list_from_session_excluding_subtrees(
    session: &IncrementalLayout<NodeId>,
    dom: &ScriptedDom,
    cursor: Option<TextCursor>,
    scroll: &ScrollOffsets<NodeId>,
    skipped_subtrees: &FxHashSet<NodeId>,
    width: u32,
    height: u32,
) -> ServalPaintList {
    let merged = merged_scroll_offsets_excluding_subtrees(session, dom, scroll, skipped_subtrees);
    let t = std::time::Instant::now();
    let mut plist = session.emit_paint_list_excluding_subtrees(
        dom,
        scroll,
        skipped_subtrees,
        DeviceIntSize::new(width as i32, height as i32),
    );
    tracing::debug!(
        target: "meerkat::profile",
        lane = "base",
        emit_us = t.elapsed().as_micros() as u64,
        "paint list emitted"
    );
    let cursor = cursor.filter(|c| {
        !skipped_subtrees
            .iter()
            .any(|&root| node_under_root(dom, c.node, root))
    });
    append_cursor_and_focus(&mut plist, session, dom, &merged, cursor);
    serval_layout::push_scrollbars(&mut plist, dom, session.fragments(), &merged);
    plist
}

fn paint_list_from_session_subtree(
    session: &IncrementalLayout<NodeId>,
    dom: &ScriptedDom,
    root: NodeId,
    cursor: Option<TextCursor>,
    scroll: &ScrollOffsets<NodeId>,
    width: u32,
    height: u32,
) -> Option<ServalPaintList> {
    let merged = merged_scroll_offsets_under_root(session, dom, scroll, root);
    let t = std::time::Instant::now();
    let plist = session.emit_subtree_paint_list(
        dom,
        root,
        scroll,
        DeviceIntSize::new(width as i32, height as i32),
    );
    tracing::debug!(
        target: "meerkat::profile",
        lane = "orrery",
        emit_us = t.elapsed().as_micros() as u64,
        "paint list emitted"
    );
    let mut plist = plist?;
    let cursor = cursor.filter(|c| node_under_root(dom, c.node, root));
    append_cursor_and_focus(&mut plist, session, dom, &merged, cursor);
    serval_layout::push_scrollbars(&mut plist, dom, session.fragments(), &merged);
    Some(plist)
}

fn merged_scroll_offsets(
    session: &IncrementalLayout<NodeId>,
    scroll: &ScrollOffsets<NodeId>,
) -> ScrollOffsets<NodeId> {
    let mut merged = session.element_scroll().clone();
    for (k, v) in scroll {
        merged.insert(*k, *v);
    }
    merged
}

fn merged_scroll_offsets_under_root(
    session: &IncrementalLayout<NodeId>,
    dom: &ScriptedDom,
    scroll: &ScrollOffsets<NodeId>,
    root: NodeId,
) -> ScrollOffsets<NodeId> {
    merged_scroll_offsets(session, scroll)
        .into_iter()
        .filter(|(node, _)| node_under_root(dom, *node, root))
        .collect()
}

fn merged_scroll_offsets_excluding_subtrees(
    session: &IncrementalLayout<NodeId>,
    dom: &ScriptedDom,
    scroll: &ScrollOffsets<NodeId>,
    skipped_subtrees: &FxHashSet<NodeId>,
) -> ScrollOffsets<NodeId> {
    merged_scroll_offsets(session, scroll)
        .into_iter()
        .filter(|(node, _)| {
            !skipped_subtrees
                .iter()
                .any(|&root| node_under_root(dom, *node, root))
        })
        .collect()
}

fn append_cursor_and_focus(
    plist: &mut ServalPaintList,
    session: &IncrementalLayout<NodeId>,
    dom: &ScriptedDom,
    merged_scroll: &ScrollOffsets<NodeId>,
    cursor: Option<TextCursor>,
) {
    if let Some(c) = cursor.filter(|c| c.editable) {
        if let Some((start, end)) = c.selection {
            let rects = session.selection_rects(dom, c.node, start, end);
            let highlight = session
                .selection_style(dom, c.node)
                .map(|(bg, _fg)| ColorF {
                    r: bg[0],
                    g: bg[1],
                    b: bg[2],
                    a: bg[3],
                })
                .unwrap_or(SELECTION_COLOR);
            plist.push_selection(&rects, highlight);
        }
        if let Some(rect) = session.caret_rect(dom, c.node, c.caret, CARET_WIDTH) {
            let caret = session
                .caret_color(dom, c.node)
                .map(|[r, g, b, a]| ColorF { r, g, b, a })
                .unwrap_or(CARET_COLOR);
            plist.push_caret(rect, caret);
        }
    }
    push_focus_ring(
        plist,
        session,
        dom,
        merged_scroll,
        cursor.as_ref().map(|c| c.node),
    );
}

pub(crate) fn node_under_root(dom: &ScriptedDom, node: NodeId, root: NodeId) -> bool {
    let mut cursor = Some(node);
    while let Some(id) = cursor {
        if id == root {
            return true;
        }
        // A drained mutation may reference a node already dropped later in the
        // same batch (insert-then-remove churn); the read accessors panic on
        // dead ids by contract, so gate on the never-panicking `is_live`.
        // Unclassifiable counts as not-under-root: callers fold that into
        // base-dirty, costing one base repaint, never a missed orrery repaint.
        if !dom.is_live(id) {
            return false;
        }
        cursor = dom.parent(id);
    }
    false
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
) -> (
    Scene,
    Vec<paint_list_render::BoxShadowMaskRequest>,
    u32,
    Vec<(String, [f32; 4])>,
)
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
        dom,
        stylesheets,
        loader,
        width,
        height,
        band_y,
        band_h,
        scroll,
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

/// The retained-layout twin of [`scene_from_layout_dom`]: emit one band off a pre-built
/// [`serval_layout::ContentLayout`] (cascade once, emit many) and lower it to a
/// `netrender::Scene`. The content actor holds the layout across scroll bands / find
/// keystrokes so a re-band does not re-cascade. `height` is the layout viewport height (for
/// the content-height report). (Slice 1, content-lane incremental.)
pub(crate) fn scene_from_content_band<D>(
    layout: &serval_layout::ContentLayout<D::NodeId>,
    dom: &D,
    height: u32,
    band_y: u32,
    band_h: u32,
    scroll: &ScrollOffsets<D::NodeId>,
) -> (
    Scene,
    Vec<paint_list_render::BoxShadowMaskRequest>,
    u32,
    Vec<(String, [f32; 4])>,
)
where
    D: LayoutDom,
    D::NodeId: Copy + Eq + Hash + Send + Sync + 'static,
{
    use paint_list_api::PaintList;
    let (list, scroll_range, links) = layout.emit_band(dom, band_y, band_h, scroll);
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
    IncrementalLayout::new(dom, stylesheets, width as f32, height as f32)
        .hit_test(dom, x, y, scroll)
}

/// Accumulate each laid-out node's absolute origin into `out`, walking from `node`
/// whose parent sits at `parent_origin`. Taffy fragment locations are parent-
/// relative, so absolute coords require summing the chain — mirrors the a11y
/// tree's bounds accumulation (`serval_a11y::build`). Shared with the roster a11y
/// row bounds, which face the same nested-pane offset.
pub(crate) fn accumulate_origins(
    dom: &ScriptedDom,
    fragments: &FragmentPlane<NodeId>,
) -> HashMap<NodeId, (f32, f32)> {
    // The engine owns the parent-chain accumulation (upstreaming P2); this host entry just
    // adapts serval-layout's `accumulate_origins` (a `Point` map, walked from the document
    // root) to the call sites' `(f32, f32)` map.
    serval_layout::accumulate_origins(dom, fragments)
        .into_iter()
        .map(|(id, p)| (id, (p.x, p.y)))
        .collect()
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
    let Some(r) = fragments.rect_of(node) else {
        return;
    };
    // Painted origin (the node's origin minus its ancestors' scroll) via the engine's shared
    // walk, instead of a host copy of the painted accumulation. (Upstreaming P2.)
    let origins = serval_layout::accumulate_painted_origins(dom, fragments, scroll_offsets);
    let Some(p) = origins.get(&node) else { return };
    let (x, y) = (p.x, p.y);
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
