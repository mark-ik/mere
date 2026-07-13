/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! A DOM pane's incremental-layout session — the cheap-path plan's C3 (chrome)
//! and C5 (the remaining panes).
//!
//! A meerkat pane backed by a genet `ScriptedDom` (the chrome toolbar / omnibar /
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
//! **Structural batches ride the session's splice path now.** A structural
//! batch (palette rows / suggestion lists / tile changes / a text write) drives
//! `IncrementalLayout::apply` to `Spliced`, and since the paint-side graft
//! landed (genet `BoxTree::graft_subtree`, shell paint plan 2026-07-03) a
//! splice keeps the box-tree side-table + shaped text valid, so the session
//! stays emittable and the pane no longer rebuilds it per structural frame
//! (P0 receipts: 34ms of stylist + full cascade + layout per batch, paid even
//! for one text mutation). The rebuild triggers that remain are the ones a
//! retained session genuinely can't absorb: no session yet, a resize (relayout
//! width/height are fixed per session here), a theme switch (the persistent
//! Stylist's sheets are fixed for the session's life), or — belt and braces —
//! an apply that somehow left the paint side stale (`paint_ready` false).

use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;

use layout_dom_api::{DomMutation, LayoutDom, LayoutDomMut, NodeKind};
use netrender::Scene;
use genet_layout::{Applied, FragmentPlane, IncrementalLayout, ScrollOffsets, GenetPaintList};
use genet_scripted_dom::{NodeId, ScriptedDom};

use crate::genet_render::TextCursor;

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
    /// The `prefers-color-scheme` the session last evaluated its media rules at.
    /// With the scheme pair baked into one sheet (`bake_scheme_pair`), a
    /// light/dark mode flip changes only this — the session survives via
    /// `IncrementalLayout::set_prefers_color_scheme` (media re-evaluation over
    /// the persistent Stylist), not a rebuild. (Theme-modes T2.)
    scheme_dark: bool,
}

pub(crate) struct PaneSessionRefresh {
    pub(crate) rebuild: bool,
    pub(crate) structural: bool,
    pub(crate) mut_count: usize,
}

impl PaneSession {
    /// Produce this pane's scene for the frame: drain the pane DOM's mutations,
    /// rebuild the session on a resize / theme change (else apply the batch —
    /// attribute-only rides `RepaintOnly`, structural rides the splice + paint
    /// graft), emit, and lower to a `netrender::Scene` with the focused field's
    /// caret/selection + scrollbar overlays. Replaces the per-frame
    /// `scene_from_scripted_dom` call.
    ///
    /// `slot` is the pane's session field (replaced on rebuild); `dom` is the
    /// shared pane-DOM handle; `sheet` the resolved pane stylesheet;
    /// `scheme_dark` the active `prefers-color-scheme` (the mode flip's cheap
    /// axis); `cursor` the focused field's caret/selection (`None` for a pane
    /// with no editable field).
    pub(crate) fn scene(
        slot: &mut Option<PaneSession>,
        dom: &Rc<RefCell<ScriptedDom>>,
        sheet: &[&str],
        scheme_dark: bool,
        w: u32,
        h: u32,
        cursor: Option<TextCursor>,
        scroll: &ScrollOffsets<NodeId>,
    ) -> Scene {
        let mut muts: Vec<DomMutation<NodeId>> = Vec::new();
        dom.borrow_mut().drain_mutations(&mut muts);
        let dom_ref = dom.borrow();
        Self::refresh(slot, &dom_ref, sheet, scheme_dark, w, h, &muts);
        let session = &slot.as_ref().expect("session built above").layout;
        crate::genet_render::scene_from_session(session, &dom_ref, cursor, scroll, w, h)
    }

    /// Like [`scene`](Self::scene) but stops at the engine-agnostic
    /// [`GenetPaintList`] instead of lowering to a `netrender::Scene` — for a
    /// caller that composes the pane's paint list into *another* document rather
    /// than rasterizing it standalone. The overlay-slot satellite path uses this:
    /// a host `ViewPane` produces its paint list here, and the content actor
    /// composites it under an anchor via `ContentLayout::set_overlay`. Same
    /// refresh cadence as `scene` (drain → rebuild-or-apply → emit). No caret
    /// overlay (a satellite chip has no editable field yet). (Overlay-roots P1.)
    pub(crate) fn paint_list(
        slot: &mut Option<PaneSession>,
        dom: &Rc<RefCell<ScriptedDom>>,
        sheet: &[&str],
        scheme_dark: bool,
        w: u32,
        h: u32,
        scroll: &ScrollOffsets<NodeId>,
    ) -> GenetPaintList {
        let mut muts: Vec<DomMutation<NodeId>> = Vec::new();
        dom.borrow_mut().drain_mutations(&mut muts);
        let dom_ref = dom.borrow();
        Self::refresh(slot, &dom_ref, sheet, scheme_dark, w, h, &muts);
        let session = &slot.as_ref().expect("session built above").layout;
        crate::genet_render::paint_list_from_session(session, &dom_ref, None, scroll, w, h)
    }

    pub(crate) fn refresh(
        slot: &mut Option<PaneSession>,
        dom: &ScriptedDom,
        sheet: &[&str],
        scheme_dark: bool,
        w: u32,
        h: u32,
        muts: &[DomMutation<NodeId>],
    ) -> PaneSessionRefresh {
        let structural = muts
            .iter()
            .any(|m| !matches!(m, DomMutation::AttributeChanged { .. }));
        let dims = (w, h);
        let rebuild = match slot.as_ref() {
            None => true,
            Some(s) => s.dims != dims || !sheet_eq(&s.sheet, sheet),
        };
        let structural_summary = structural.then(|| summarize_structural_batch(dom, muts));
        tracing::debug!(
            target: "meerkat::profile",
            rebuild,
            mut_count = muts.len(),
            structural,
            inserted = structural_summary.as_ref().map(|s| s.inserted),
            removed = structural_summary.as_ref().map(|s| s.removed),
            attr_changed = structural_summary.as_ref().map(|s| s.attr_changed),
            text_changed = structural_summary.as_ref().map(|s| s.text_changed),
            subtree_replaced = structural_summary.as_ref().map(|s| s.subtree_replaced),
            moved = structural_summary.as_ref().map(|s| s.moved),
            structural_samples = structural_summary
                .as_ref()
                .map(|s| s.samples.as_str()),
            "chrome session rebuild decision"
        );

        if rebuild {
            let t = std::time::Instant::now();
            let mut layout = IncrementalLayout::new(dom, sheet, w as f32, h as f32);
            if let Some(prev) = slot.as_ref() {
                layout.set_element_scroll(prev.layout.element_scroll().clone());
            }
            // A fresh session evaluates media at the engine default (light);
            // seed the active scheme so a session built while dark mode is on
            // lands the dark rules. (An extra recascade over the just-built
            // session; a `new`-time scheme would save it — genet follow-up.)
            if scheme_dark {
                layout.set_prefers_color_scheme(dom, true);
            }
            tracing::debug!(
                target: "meerkat::profile",
                rebuild_us = t.elapsed().as_micros() as u64,
                "chrome session rebuilt (stylist + full cascade + layout)"
            );
            *slot = Some(PaneSession {
                layout,
                dims,
                sheet: sheet.iter().map(|s| s.to_string()).collect(),
                scheme_dark,
            });
        } else {
            let s = slot
                .as_mut()
                .expect("not rebuilding implies an existing session");
            let t = std::time::Instant::now();
            let applied = s.layout.apply(dom, sheet, muts);
            // The mode flip's cheap axis: sheet unchanged + scheme changed
            // rides the engine's media re-evaluation over the persistent
            // Stylist — the session (and its element scroll) survives, no
            // rebuild. (Theme-modes T2, the W3C adoption plan's P3 host half.)
            if s.scheme_dark != scheme_dark {
                let flip = std::time::Instant::now();
                s.layout.set_prefers_color_scheme(dom, scheme_dark);
                s.scheme_dark = scheme_dark;
                tracing::debug!(
                    target: "meerkat::profile",
                    flip_us = flip.elapsed().as_micros() as u64,
                    scheme_dark,
                    "chrome session scheme flipped (media re-evaluation)"
                );
            }
            tracing::debug!(
                target: "meerkat::profile",
                apply_us = t.elapsed().as_micros() as u64,
                ?applied,
                "chrome session applied"
            );
            debug_assert!(
                s.layout.paint_ready(),
                "chrome session left the emittable path: {applied:?}",
            );
            // Belt and braces for release builds: the splice graft leaves every
            // apply path emittable, but if that contract ever breaks, heal with
            // the old full rebuild instead of tripping the emit assert.
            if !s.layout.paint_ready() {
                let t = std::time::Instant::now();
                let mut layout = IncrementalLayout::new(dom, sheet, w as f32, h as f32);
                layout.set_element_scroll(s.layout.element_scroll().clone());
                tracing::warn!(
                    target: "meerkat::profile",
                    rebuild_us = t.elapsed().as_micros() as u64,
                    ?applied,
                    "chrome session healed by rebuild (paint side stale after apply)"
                );
                s.layout = layout;
            }
        }

        PaneSessionRefresh {
            rebuild,
            structural,
            mut_count: muts.len(),
        }
    }

    pub(crate) fn layout(&self) -> &IncrementalLayout<NodeId> {
        &self.layout
    }

    /// Scroll the nearest nested `overflow: scroll/auto` container under scene point
    /// `(x, y)` by `(dx, dy)` device px, recording it in the retained layout's
    /// `element_scroll` (the wheel default action; the engine hit-tests the point,
    /// walks to the scroll container, clamps, and chains). The next [`scene`](Self::scene)
    /// paints + [`hit_test`](Self::hit_test)s at the new offset. Returns whether anything
    /// scrolled (a pane, or the document fallback). (Host-scroll P2.)
    pub(crate) fn scroll_at(
        &mut self,
        dom: &ScriptedDom,
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    ) -> bool {
        self.layout.scroll_at(dom, x, y, dx, dy)
    }

    /// The retained per-container nested scroll offsets the host reads for its **own**
    /// geometry (a11y row bounds, the scrollbar overlay, mapping a pointer into a scrolled
    /// pane). [`scene`](Self::scene) and [`hit_test`](Self::hit_test) already fold these into
    /// paint + hit-test, so those callers pass empty host offsets. (Host-scroll P2.)
    pub(crate) fn element_scroll(&self) -> &ScrollOffsets<NodeId> {
        self.layout.element_scroll()
    }

    /// The scrollable extent `(mx, my)` of container `node` — how far its content overflows
    /// its scrollport. Delegates to [`IncrementalLayout::scroll_extent`]. The live wheel path
    /// never needs this (`scroll_at` clamps to it internally); it is here for the test panes'
    /// overflow assertions, so they read the engine's extent instead of re-deriving it.
    #[cfg(test)]
    pub(crate) fn scroll_extent(&self, dom: &ScriptedDom, node: NodeId) -> (f32, f32) {
        self.layout.scroll_extent(dom, node)
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
    /// ArrowUp / ArrowDown in a multi-line (textarea) field, with a sticky goal column.
    /// Pass `goal_x` `None` to seed it from the caret, or the previous call's returned
    /// value to keep the column across a run; returns `(new_byte, goal_x)`. `None` when
    /// the node carries no laid-out text.
    pub(crate) fn caret_byte_vertical(
        &self,
        node: NodeId,
        byte: usize,
        delta: isize,
        goal_x: Option<f32>,
    ) -> Option<(usize, f32)> {
        self.layout
            .caret_byte_vertical::<ScriptedDom>(node, byte, delta, goal_x)
    }

    /// The accumulated CSS `translate` of `node` (its own plus its ancestors'), which the
    /// fragment plane omits. Added to a fragment origin, it lands a transform-positioned
    /// element (an orrery gnode) where it actually paints — the same offset the focus
    /// ring reads, so the orrery a11y rect tracks the painted gnode. Delegates to the
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

struct StructuralBatchSummary {
    inserted: usize,
    removed: usize,
    attr_changed: usize,
    text_changed: usize,
    subtree_replaced: usize,
    moved: usize,
    samples: String,
}

fn summarize_structural_batch(
    dom: &ScriptedDom,
    muts: &[DomMutation<NodeId>],
) -> StructuralBatchSummary {
    let mut summary = StructuralBatchSummary {
        inserted: 0,
        removed: 0,
        attr_changed: 0,
        text_changed: 0,
        subtree_replaced: 0,
        moved: 0,
        samples: String::new(),
    };
    for (index, mutation) in muts.iter().enumerate() {
        match mutation {
            DomMutation::Inserted { node, parent } => {
                summary.inserted += 1;
                if index < 6 {
                    push_sample(
                        &mut summary.samples,
                        format_args!(
                            "insert {} -> {}",
                            describe_node_brief(dom, *node),
                            describe_node_brief(dom, *parent)
                        ),
                    );
                }
            }
            DomMutation::Removed {
                node,
                former_parent,
            } => {
                summary.removed += 1;
                if index < 6 {
                    push_sample(
                        &mut summary.samples,
                        format_args!(
                            "remove dead({node:?}) <- {}",
                            describe_node_brief(dom, *former_parent)
                        ),
                    );
                }
            }
            DomMutation::AttributeChanged { node, name, .. } => {
                summary.attr_changed += 1;
                if index < 6 {
                    push_sample(
                        &mut summary.samples,
                        format_args!("attr {} @ {}", name.local, describe_node_brief(dom, *node)),
                    );
                }
            }
            DomMutation::CharacterDataChanged { node } => {
                summary.text_changed += 1;
                if index < 6 {
                    push_sample(
                        &mut summary.samples,
                        format_args!("text {}", describe_node_brief(dom, *node)),
                    );
                }
            }
            DomMutation::SubtreeReplaced { node } => {
                summary.subtree_replaced += 1;
                if index < 6 {
                    push_sample(
                        &mut summary.samples,
                        format_args!("subtree {}", describe_node_brief(dom, *node)),
                    );
                }
            }
            DomMutation::Moved {
                node,
                from_parent,
                to_parent,
            } => {
                summary.moved += 1;
                if index < 6 {
                    push_sample(
                        &mut summary.samples,
                        format_args!(
                            "move {} : {} -> {}",
                            describe_node_brief(dom, *node),
                            describe_node_brief(dom, *from_parent),
                            describe_node_brief(dom, *to_parent)
                        ),
                    );
                }
            }
        }
    }
    summary
}

fn push_sample(dst: &mut String, sample: std::fmt::Arguments<'_>) {
    if !dst.is_empty() {
        dst.push_str(" | ");
    }
    let _ = dst.write_fmt(sample);
}

fn describe_node_brief(dom: &ScriptedDom, node: NodeId) -> String {
    // A batch entry may name a node already dropped within the same batch;
    // the read accessors panic on dead ids by contract, so gate on `is_live`.
    if !dom.is_live(node) {
        return format!("#dead({node:?})");
    }
    match dom.kind(node) {
        NodeKind::Text => return format!("#text({node:?})"),
        NodeKind::Document => return format!("#document({node:?})"),
        NodeKind::Comment => return format!("#comment({node:?})"),
        NodeKind::Doctype => return format!("#doctype({node:?})"),
        NodeKind::ProcessingInstruction => return format!("#pi({node:?})"),
        NodeKind::DocumentFragment => return format!("#fragment({node:?})"),
        NodeKind::Element => {}
    }

    let mut desc = dom
        .element_name(node)
        .map(|name| name.local.to_string())
        .unwrap_or_else(|| format!("element({node:?})"));
    let mut id_attr = None;
    let mut class_attr = None;
    let mut data_element = None;
    let mut data_member = None;
    for attr in dom.attributes(node) {
        match attr.name.local.as_ref() {
            "id" => id_attr = Some(attr.value.to_owned()),
            "class" => class_attr = Some(attr.value.to_owned()),
            "data-element" => data_element = Some(attr.value.to_owned()),
            "data-member" => data_member = Some(attr.value.to_owned()),
            _ => {}
        }
    }
    if let Some(id_attr) = id_attr.filter(|s| !s.is_empty()) {
        let _ = write!(desc, "#{id_attr}");
    }
    if let Some(class_attr) = class_attr.filter(|s| !s.is_empty()) {
        let mut tokens = class_attr.split_ascii_whitespace();
        if let Some(first) = tokens.next() {
            let _ = write!(desc, ".{first}");
        }
        if let Some(second) = tokens.next() {
            let _ = write!(desc, ".{second}");
        }
        if tokens.next().is_some() {
            desc.push_str(".+");
        }
    }
    if let Some(data_element) = data_element.filter(|s| !s.is_empty()) {
        let _ = write!(desc, "[data-element={data_element}]");
    }
    if let Some(data_member) = data_member.filter(|s| !s.is_empty()) {
        let _ = write!(desc, "[data-member={data_member}]");
    }
    desc
}
