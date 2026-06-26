/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! A DOM pane's incremental-layout session — the cheap-path plan's C3 (chrome)
//! and C5 (the remaining panes).
//!
//! A meerkat pane backed by a serval `ScriptedDom` (the chrome toolbar / omnibar /
//! shellbar / dropdowns; the tiled workbench; later roster / apparatus / utility)
//! used to re-run the whole stateless pipeline (`scene_from_scripted_dom`: fresh
//! Stylist + full cascade + box-tree layout + paint emit) every frame — the chrome
//! alone was 53% of every rendered frame at the C0 baseline, and a pane that also
//! queried its layout (the workbench reads slot rects, then hit-tests on click)
//! paid for several. A [`PaneSession`] holds an [`IncrementalLayout`] across frames
//! instead, so a steady (attribute-only) frame restyles incrementally and skips
//! layout (`RepaintOnly`) — the same machine the orrery's node pool rides — and
//! serves the pane's point queries (hit-test, fragment rects) off that one retained
//! layout rather than re-laying-out per query.
//!
//! **Why rebuild-on-structural rather than the session's own splice path.** A
//! structural batch (palette rows / suggestion lists / tile changes insert or
//! remove nodes) drives `IncrementalLayout::apply` to `Spliced`, which invalidates
//! the box-tree side-table (`emit_paint_list` then can't run). Panes splice often,
//! so rather than thread the splice's stale-paint recovery, [`PaneSession::scene`]
//! rebuilds the session whenever the drained batch is structural (or the size /
//! sheet changed) — a full cascade+layout, the *same* cost as the old stateless
//! frame, but only on those frames. Attribute-only frames (the common case: a
//! geometry write, hover-class toggles, an idle pane) take the cheap path. The
//! session is therefore always left on an emittable path; it never observes
//! `Spliced`. Resize and theme switch self-heal through the dims / sheet compare,
//! so no caller has to invalidate it explicitly.

use std::cell::RefCell;
use std::rc::Rc;

use layout_dom_api::{DomMutation, LayoutDomMut};
use netrender::Scene;
use serval_layout::{Applied, FragmentPlane, IncrementalLayout, ScrollOffsets};
use serval_scripted_dom::{NodeId, ScriptedDom};

use crate::serval_render::TextCursor;

/// A persistent cascade+layout session over one of a window's DOM panes, plus the
/// `(dims, sheet)` it was built against so a resize or theme switch triggers a
/// rebuild. One per sessionized pane on `WindowView`, lazily built on first render.
pub(crate) struct PaneSession {
    layout: IncrementalLayout<NodeId>,
    /// The viewport the session was laid out at; a change forces a rebuild.
    dims: (u32, u32),
    /// The stylesheet set the session's (fixed) Stylist was built from; a change
    /// (theme switch) forces a rebuild, since a session's sheets can't be swapped.
    sheet: Vec<String>,
}

impl PaneSession {
    /// Produce this pane's scene for the frame: drain the pane DOM's mutations,
    /// rebuild the session on a structural / resize / theme change (else apply the
    /// attribute-only batch on the cheap `RepaintOnly` path), emit, and lower to a
    /// `netrender::Scene` with the focused field's caret/selection + scrollbar
    /// overlays. Replaces the per-frame `scene_from_scripted_dom` call.
    ///
    /// `slot` is the pane's session field (replaced on rebuild); `dom` is the
    /// shared pane-DOM handle; `sheet` the resolved pane stylesheet; `cursor` the
    /// focused field's caret/selection (`None` for a pane with no editable field).
    pub(crate) fn scene(
        slot: &mut Option<PaneSession>,
        dom: &Rc<RefCell<ScriptedDom>>,
        sheet: &[&str],
        w: u32,
        h: u32,
        cursor: Option<TextCursor>,
        scroll: &ScrollOffsets<NodeId>,
    ) -> Scene {
        // Drain this frame's pane mutations. The session owns the drain (the pane's
        // DOM is no longer drained by the frame-end `discard_dom_mutations`), so the
        // batch reaches `apply`. `&mut` borrow is released before the read.
        let mut muts: Vec<DomMutation<NodeId>> = Vec::new();
        dom.borrow_mut().drain_mutations(&mut muts);
        let structural = muts
            .iter()
            .any(|m| !matches!(m, DomMutation::AttributeChanged { .. }));

        let dom_ref = dom.borrow();
        let dims = (w, h);
        let rebuild = match slot.as_ref() {
            None => true,
            Some(s) => structural || s.dims != dims || !sheet_eq(&s.sheet, sheet),
        };

        if rebuild {
            // Full cascade + layout — same cost as the old stateless frame, but
            // only on a structural / resize / theme frame. The box-tree side-table
            // is fresh, so `emit_paint_list` is valid and the Spliced path is never
            // taken.
            let layout = IncrementalLayout::new(&*dom_ref, sheet, w as f32, h as f32);
            *slot = Some(PaneSession {
                layout,
                dims,
                sheet: sheet.iter().map(|s| s.to_string()).collect(),
            });
        } else {
            // Attribute-only batch → incremental restyle: RepaintOnly (layout
            // skipped) unless an inline geometry write actually moved a box
            // (Restyled re-lays-out). Either way the paint side stays valid.
            let s = slot.as_mut().expect("not rebuilding implies an existing session");
            let applied = s.layout.apply(&*dom_ref, sheet, &muts);
            debug_assert!(
                matches!(
                    applied,
                    Applied::Unchanged | Applied::RepaintOnly | Applied::Restyled
                ),
                "chrome session left the emittable path on an attribute-only batch: {applied:?}",
            );
        }

        let session = &slot.as_ref().expect("session built above").layout;
        crate::serval_render::scene_from_session(session, &dom_ref, cursor, scroll, w, h)
    }

    /// Scroll the nearest nested `overflow: scroll/auto` container under scene point
    /// `(x, y)` by `(dx, dy)` device px, recording it in the retained layout's
    /// `element_scroll` (the wheel default action; the engine hit-tests the point,
    /// walks to the scroll container, clamps, and chains). The next [`scene`](Self::scene)
    /// paints + [`hit_test`](Self::hit_test)s at the new offset. Returns whether anything
    /// scrolled (a pane, or the document fallback). (Host-scroll P2.)
    pub(crate) fn scroll_at(&mut self, dom: &ScriptedDom, x: f32, y: f32, dx: f32, dy: f32) -> bool {
        self.layout.scroll_at(dom, x, y, dx, dy)
    }

    /// The retained per-container nested scroll offsets the host reads for its **own**
    /// geometry (a11y row bounds, the scrollbar overlay, mapping a pointer into a scrolled
    /// pane). [`scene`](Self::scene) and [`hit_test`](Self::hit_test) already fold these into
    /// paint + hit-test, so those callers pass empty host offsets. (Host-scroll P2.)
    pub(crate) fn element_scroll(&self) -> &ScrollOffsets<NodeId> {
        self.layout.element_scroll()
    }

    /// Hit-test scene point `(x, y)` against the session's **retained** chrome
    /// layout — the C4 path that lets a click / region probe reuse the render's
    /// cascade+layout instead of re-running it (`hit_test_node`'s fresh pipeline).
    /// `None` if no fragment covers the point. The caller falls back to the
    /// stateless probe only when the session hasn't been built yet.
    pub(crate) fn hit_test(
        &self,
        dom: &ScriptedDom,
        x: f32,
        y: f32,
        scroll: &ScrollOffsets<NodeId>,
    ) -> Option<NodeId> {
        self.layout.hit_test(dom, x, y, scroll)
    }

    /// The session's retained fragment plane — for class-bottom / anchor measures
    /// that read box geometry off the rendered layout without re-laying-out (C4).
    pub(crate) fn fragments(&self) -> &FragmentPlane<NodeId> {
        self.layout.fragments()
    }

    /// The caret rect for `byte` within `node`, from the session's retained layout
    /// — the same one [`scene`](Self::scene) paints the caret from, so the IME
    /// candidate area matches the visible caret. `(x, y, w, h)` in the chrome's
    /// (window) coordinate space; `None` before the session is built or when the
    /// node carries no laid-out caret. (G2.1 IME T3.)
    pub(crate) fn caret_rect(
        &self,
        dom: &ScriptedDom,
        node: NodeId,
        byte: usize,
        width: f32,
    ) -> Option<(f32, f32, f32, f32)> {
        self.layout
            .caret_rect(dom, node, byte, width)
            .map(|r| (r.x, r.y, r.width, r.height))
    }

    /// The caret byte nearest scene point `(x, y)` within `node`'s retained text
    /// layout — the click-to-place / drag-select primitive, the inverse of
    /// [`caret_rect`](Self::caret_rect). `(x, y)` are in the chrome's (window) space,
    /// the same `caret_rect` returns. `None` before the session is built or when the
    /// node carries no laid-out text. The host maps a pointer event to a byte here,
    /// then drives [`TextInput::set_caret_byte`](xilem_serval::TextInput::set_caret_byte).
    pub(crate) fn caret_byte_at_point(
        &self,
        dom: &ScriptedDom,
        node: NodeId,
        x: f32,
        y: f32,
    ) -> Option<usize> {
        self.layout.caret_byte_at_point(dom, node, x, y)
    }

    /// The caret byte one visual line up (`delta < 0`) or down (`delta > 0`) from
    /// `byte` within `node`'s retained text layout, honouring soft-wrap rows — for
    /// ArrowUp / ArrowDown in a multi-line (textarea) field. `None` when the node
    /// carries no laid-out text.
    pub(crate) fn caret_byte_vertical(
        &self,
        node: NodeId,
        byte: usize,
        delta: isize,
    ) -> Option<usize> {
        self.layout.caret_byte_vertical::<ScriptedDom>(node, byte, delta)
    }

    /// The accumulated CSS `translate` of `node` (its own plus its ancestors'), which the
    /// fragment plane omits. Added to a fragment origin, it lands a transform-positioned
    /// element (an orrery node-card) where it actually paints — the same offset the focus
    /// ring reads, so the orrery a11y rect tracks the painted card. Delegates to the
    /// retained layout. (Slice 4.)
    pub(crate) fn accumulated_translate(&self, dom: &ScriptedDom, node: NodeId) -> (f32, f32) {
        self.layout.accumulated_translate(dom, node)
    }
}

/// Whether the session's stored sheet set still matches the frame's resolved
/// sheet (a theme switch changes the strings). Cheap per-frame compare that
/// avoids any caller having to invalidate the session on theme change.
fn sheet_eq(stored: &[String], current: &[&str]) -> bool {
    stored.len() == current.len() && stored.iter().zip(current).all(|(a, b)| a == b)
}
