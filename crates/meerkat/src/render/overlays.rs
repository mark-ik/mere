/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Chrome-overlay positioning for [`render`](super): the palette / context-menu /
//! submenu scroll-into-view + edge-flip placement, the tear-out ghost, and the
//! shellbar + comms post-layout geometry. Split from `render.rs` to keep files under
//! the workspace 600-LOC ceiling.

use super::*;

impl crate::WindowCtx<'_> {
    /// Position the chrome's floating overlays for this frame and collect their
    /// scroll-into-view targets: the command palette + context menu (bounded, selection
    /// centred, edge-flipped), the open submenu (anchored beside its row), the tear-out
    /// ghost (at the cursor), and the shellbar + comms surfaces (docked at their laid-out
    /// rects). Returns the chrome's `chrome_scroll` targets. (Extracted from `render()`.)
    pub(super) fn position_chrome_overlays(
        &mut self,
        w: u32,
        h: u32,
        toolbar_h: u32,
        orrery_rect: [f32; 4],
        comms_rect: Option<[f32; 4]>,
    ) -> ScrollOffsets<NodeId> {
        // The chrome's own scroll-into-view targets (the command palette / context menu follow
        // their selection); the panes' wheel scroll lives in the session's `element_scroll`
        // now, which `emit_paint_list` folds in, so this carries only the targets. (P2.)
        let mut chrome_scroll = ScrollOffsets::<NodeId>::default();
        if self.chrome().palette_open {
            // Bound the list to the window so a long palette can't overflow it. The
            // overlay floats the panel ~56px down with an input + paddings above the
            // list, so leave generous headroom + a bottom margin — otherwise a small
            // window pushes the last rows past its edge even when scrolled.
            let max_h = (h as f32 - 200.0).max(120.0);
            {
                let mut dom = self.view.dom.borrow_mut();
                let root = dom.document();
                if let Some(node) = first_with_class(&dom, root, "cmd-list") {
                    let attr = QualName::new(None, Namespace::from(""), LocalName::from("style"));
                    dom.set_attribute(
                        node,
                        attr,
                        &format!("overflow: scroll; max-height: {max_h}px;"),
                    );
                }
                // Centre the palette over the orrery area, not the full window, so it does
                // not overlap the side panes: the overlay's flex centring is shifted by the
                // orrery rect's insets (which already shrink when a pane opens), and the panel
                // is capped to the orrery width for a narrow canvas. (Phase 1, step 3.)
                let pad_left = orrery_rect[0].max(0.0);
                let pad_right = (w as f32 - orrery_rect[2]).max(0.0);
                let panel_max = (orrery_rect[2] - orrery_rect[0] - 40.0).max(120.0);
                if let Some(node) = first_with_class(&dom, root, "palette-overlay") {
                    let attr = QualName::new(None, Namespace::from(""), LocalName::from("style"));
                    dom.set_attribute(
                        node,
                        attr,
                        &format!(
                            "display: flex; justify-content: center; padding: 56px {pad_right}px 0 {pad_left}px;"
                        ),
                    );
                }
                if let Some(node) = first_with_class(&dom, root, "palette") {
                    let attr = QualName::new(None, Namespace::from(""), LocalName::from("style"));
                    dom.set_attribute(node, attr, &format!("max-width: {panel_max}px;"));
                }
            }
            // Follow the selection: centre the active row in the bounded viewport,
            // from the prior frame's layout (one-frame lag, like the roster clamp).
            if let Some(session) = &self.view.chrome_session {
                let frags = session.fragments();
                let dom = self.view.dom.borrow();
                let root = dom.document();
                if let (Some(list), Some(active)) = (
                    first_with_class(&dom, root, "cmd-list"),
                    first_with_class(&dom, root, "cmd-row-active"),
                ) {
                    if let (Some(lr), Some(ar)) = (frags.rect_of(list), frags.rect_of(active)) {
                        let viewport_h = lr.size.height;
                        let content_h = lr.content_size.height;
                        let target = (ar.location.y + ar.size.height / 2.0 - viewport_h / 2.0)
                            .clamp(0.0, (content_h - viewport_h).max(0.0));
                        chrome_scroll.insert(list, (0.0, target));
                    }
                }
            }
        }
        // Position the tear-out drag ghost at the live cursor (offset so it sits beside the
        // pointer, not under it). The pill exists in the DOM only while a tear-out drag is
        // active; the stylesheet `.tear-ghost` rule carries its look, this sets left/top.
        // (Tear-out gestures, GA-5.)
        if self.chrome().tear_ghost.is_some() {
            let (gx, gy) = (self.view.cursor.0 + 12.0, self.view.cursor.1 + 12.0);
            let mut dom = self.view.dom.borrow_mut();
            let root = dom.document();
            if let Some(node) = first_with_class(&dom, root, "tear-ghost") {
                let attr = QualName::new(None, Namespace::from(""), LocalName::from("style"));
                dom.set_attribute(
                    node,
                    attr,
                    &format!("position: absolute; left: {gx}px; top: {gy}px;"),
                );
            }
        }
        // The context menu follows its keyboard selection like the palette: bound the panel to the
        // window so a tall menu (the layout submenu) can't spill past the bottom edge, and scroll
        // the highlighted row into view. (Context-menu keyboard nav.)
        if let Some((mx, my)) = self.chrome().context_menu.as_ref().map(|m| (m.x, m.y)) {
            // Open away from whichever edge the panel would overflow: placed down-right of
            // the cursor by default, it flips left / up when that would spill past the
            // right / bottom edge, so the menu never goes offscreen. The panel size comes
            // from the laid-out fragments (the retained prior frame); the first frame a
            // menu opens it is unmeasured (0,0) and lands at the cursor, corrected next
            // frame — the same one-frame settle the submenu anchor uses. (Context-menu
            // edge-flip.)
            // Estimate the panel height from its row count — the measured `size.height` is
            // clamped by the `max-height` below (so it always "fits" and never signals
            // overflow), and `content_size` proved unreliable for this scroll node. One
            // search row + the items, at the context-item row height (~35px) plus the
            // panel's 4px padding top+bottom. Width uses the measured natural width (not
            // height-clamped) when available, else a sane default. (Context-menu edge-flip.)
            let rows = self
                .chrome()
                .context_menu
                .as_ref()
                .map_or(0, |m| m.items.len())
                + 1;
            let menu_h = rows as f32 * 35.0 + 8.0;
            let menu_w = {
                let dom = self.view.dom.borrow();
                let root = dom.document();
                self.view
                    .chrome_session
                    .as_ref()
                    .and_then(|s| {
                        first_with_class(&dom, root, "context-menu")
                            .and_then(|node| s.fragments().rect_of(node))
                    })
                    .map_or(0.0, |r| r.size.width)
            };
            let menu_w = if menu_w > 1.0 { menu_w } else { 240.0 };
            let left = if mx + menu_w > w as f32 {
                (mx - menu_w).max(0.0)
            } else {
                mx
            };
            let top = if my + menu_h > h as f32 {
                (my - menu_h).max(0.0)
            } else {
                my
            };
            let max_h = (h as f32 - top - 16.0).max(120.0);
            {
                let mut dom = self.view.dom.borrow_mut();
                let root = dom.document();
                if let Some(node) = first_with_class(&dom, root, "context-menu") {
                    let attr = QualName::new(None, Namespace::from(""), LocalName::from("style"));
                    dom.set_attribute(
                        node,
                        attr,
                        &format!(
                            "position: absolute; left: {left}px; top: {top}px; overflow: scroll; max-height: {max_h}px;"
                        ),
                    );
                }
            }
            if let Some(session) = &self.view.chrome_session {
                let frags = session.fragments();
                let dom = self.view.dom.borrow();
                let root = dom.document();
                if let (Some(list), Some(active)) = (
                    first_with_class(&dom, root, "context-menu"),
                    first_with_class(&dom, root, "context-item-active"),
                ) {
                    if let (Some(lr), Some(ar)) = (frags.rect_of(list), frags.rect_of(active)) {
                        let viewport_h = lr.size.height;
                        let content_h = lr.content_size.height;
                        let target = (ar.location.y + ar.size.height / 2.0 - viewport_h / 2.0)
                            .clamp(0.0, (content_h - viewport_h).max(0.0));
                        chrome_scroll.insert(list, (0.0, target));
                    }
                }
            }
        }
        // The open submenu panel sits beside its parent row. Anchor it off the parent row's rect
        // (the `.context-submenu-anchor` marker) with Cambium's geometry helper: default to
        // the parent's right (frame-1 correct — RightOf ignores the popup size), flipping left when
        // it would overflow the window. (Nested submenus.)
        let submenu_pos = if self
            .chrome()
            .context_menu
            .as_ref()
            .is_some_and(|m| m.submenu.is_some())
        {
            if let Some(session) = &self.view.chrome_session {
                let frags = session.fragments();
                let dom = self.view.dom.borrow();
                let root = dom.document();
                match (
                    first_with_class(&dom, root, "context-menu"),
                    first_with_class(&dom, root, "context-submenu-anchor"),
                    first_with_class(&dom, root, "context-submenu"),
                ) {
                    (Some(panel), Some(anchor), Some(sub)) => {
                        // Absolute (document-space) origin via the engine's shared parent-chain
                        // accumulation, instead of re-rolling it here. (Host-scroll P1.)
                        let abs_origin = |start| -> (f32, f32) {
                            genet_layout::absolute_origin(&*dom, frags, start)
                                .map_or((0.0, 0.0), |p| (p.x, p.y))
                        };
                        let panel_x = abs_origin(panel).0;
                        let row_y = abs_origin(anchor).1;
                        let panel_w = frags.rect_of(panel).map_or(0.0, |r| r.size.width);
                        let row_h = frags.rect_of(anchor).map_or(0.0, |r| r.size.height);
                        let (sub_w, sub_h) = frags
                            .rect_of(sub)
                            .map_or((0.0, 0.0), |r| (r.size.width, r.size.height));
                        // Place the submenu beside its parent row: right of the root panel by
                        // default, flipping left when it would overflow the window's right edge,
                        // and clamped on-screen — one `anchor_point_clamped`. The y stays at the
                        // parent row's top (the panel scrolls via the max-height set below), so
                        // only the x is taken. (Nested submenus / upstreaming P5.)
                        let (sx, _) = cambium::anchor_point_clamped(
                            (panel_x, row_y, panel_w, row_h),
                            (sub_w, sub_h),
                            cambium::Placement::RightOf,
                            (0.0, 0.0, w as f32, h as f32),
                        );
                        Some((sub, sx, row_y))
                    }
                    _ => None,
                }
            } else {
                None
            }
        } else {
            None
        };
        if let Some((sub, sx, sy)) = submenu_pos {
            let max_h = (h as f32 - sy - 16.0).max(120.0);
            let mut dom = self.view.dom.borrow_mut();
            let attr = QualName::new(None, Namespace::from(""), LocalName::from("style"));
            dom.set_attribute(
                sub,
                attr,
                &format!(
                    "position: absolute; left: {sx}px; top: {sy}px; overflow: scroll; max-height: {max_h}px;"
                ),
            );
        }
        // Scroll the highlighted submenu child into view within its own panel (mirrors the root
        // list scroll). The child carries `context-subitem-active`, distinct from the root rows'
        // `context-item-active`, so this targets the submenu and the root block targets the root.
        // (Nested submenus.)
        if self
            .chrome()
            .context_menu
            .as_ref()
            .is_some_and(|m| m.submenu.is_some())
        {
            if let Some(session) = &self.view.chrome_session {
                let frags = session.fragments();
                let dom = self.view.dom.borrow();
                let root = dom.document();
                if let (Some(sub), Some(active)) = (
                    first_with_class(&dom, root, "context-submenu"),
                    first_with_class(&dom, root, "context-subitem-active"),
                ) {
                    if let (Some(lr), Some(ar)) = (frags.rect_of(sub), frags.rect_of(active)) {
                        let viewport_h = lr.size.height;
                        let content_h = lr.content_size.height;
                        let target = (ar.location.y + ar.size.height / 2.0 - viewport_h / 2.0)
                            .clamp(0.0, (content_h - viewport_h).max(0.0));
                        chrome_scroll.insert(sub, (0.0, target));
                    }
                }
            }
        }
        // Position the shellbar strip at its docked edge. The flex-direction
        // follows the edge so buttons stack vertically (Left/Right) or
        // horizontally (Top/Bottom). (Shellbar F2.1.)
        {
            let sr = shellbar::shellbar_rect(
                self.shared.presentation.shellbar_edge,
                w as f32,
                h as f32,
                toolbar_h as f32,
                self.shared.presentation.ui_scale(),
            );
            let flex_dir = match self.shared.presentation.shellbar_edge {
                session_runtime::ShellbarEdge::Left | session_runtime::ShellbarEdge::Right => {
                    "column"
                }
                session_runtime::ShellbarEdge::Top | session_runtime::ShellbarEdge::Bottom => "row",
            };
            let mut dom = self.view.dom.borrow_mut();
            let root = dom.document();
            let style = overlay_geometry_style(
                sr[0],
                sr[1],
                (sr[2] - sr[0]).max(0.0),
                (sr[3] - sr[1]).max(0.0),
                Some(flex_dir),
            );
            stamp_overlay_style_if_changed(
                &mut self.view.shellbar_style,
                &mut dom,
                root,
                "shellbar",
                style,
            );
        }
        // Position the chrome's comms overlay into its frame leaf (it's chrome-
        // rendered but laid out by the frame tree): set the geometry inline so it
        // fills the reserved Comms leaf rect. (Comms pane.)
        if let Some(cr) = comms_rect {
            let mut dom = self.view.dom.borrow_mut();
            let root = dom.document();
            let style = overlay_geometry_style(
                cr[0],
                cr[1],
                (cr[2] - cr[0]).max(0.0),
                (cr[3] - cr[1]).max(0.0),
                None,
            );
            stamp_overlay_style_if_changed(
                &mut self.view.comms_style,
                &mut dom,
                root,
                "comms-pane",
                style,
            );
        } else {
            self.view.comms_style = None;
        }
        chrome_scroll
    }
}

fn stamp_overlay_style_if_changed(
    cache: &mut Option<String>,
    dom: &mut genet_scripted_dom::ScriptedDom,
    root: NodeId,
    class_name: &str,
    style: String,
) {
    let Some(node) = first_with_class(dom, root, class_name) else {
        *cache = None;
        return;
    };
    if cache.as_deref() == Some(style.as_str()) {
        return;
    }
    tracing::trace!(
        target: "meerkat::profile",
        class_name,
        previous = cache.as_deref().unwrap_or("<none>"),
        next = style.as_str(),
        "overlay geometry style changed"
    );
    let attr = QualName::new(None, Namespace::from(""), LocalName::from("style"));
    dom.set_attribute(node, attr, &style);
    *cache = Some(style);
}

#[cfg(test)]
mod tests {
    use super::*;
    use layout_dom_api::LayoutDomMut;
    use genet_scripted_dom::ScriptedDom;

    fn qual(local: &str) -> QualName {
        QualName::new(None, Namespace::from(""), LocalName::from(local))
    }

    #[test]
    fn overlay_style_stamps_once_for_identical_values() {
        let mut dom = ScriptedDom::new();
        let root = dom.document();
        let node = dom.create_element(qual("div"));
        dom.set_attribute(node, qual("class"), "shellbar");
        dom.append_child(root, node);
        let mut drained = Vec::new();
        dom.drain_mutations(&mut drained);
        let mut cache = None;

        stamp_overlay_style_if_changed(
            &mut cache,
            &mut dom,
            root,
            "shellbar",
            "position:absolute; left: 0px; top: 0px;".to_string(),
        );
        let mut muts = Vec::new();
        dom.drain_mutations(&mut muts);
        assert_eq!(muts.len(), 1);

        stamp_overlay_style_if_changed(
            &mut cache,
            &mut dom,
            root,
            "shellbar",
            "position:absolute; left: 0px; top: 0px;".to_string(),
        );
        let mut again = Vec::new();
        dom.drain_mutations(&mut again);
        assert!(again.is_empty());
    }

    #[test]
    fn overlay_style_cache_resets_when_node_leaves_the_document() {
        let mut dom = ScriptedDom::new();
        let root = dom.document();
        let node = dom.create_element(qual("div"));
        dom.set_attribute(node, qual("class"), "comms-pane");
        dom.append_child(root, node);
        let mut drained = Vec::new();
        dom.drain_mutations(&mut drained);
        let mut cache = None;
        let style = "position:absolute; left: 10px; top: 20px;".to_string();

        stamp_overlay_style_if_changed(&mut cache, &mut dom, root, "comms-pane", style.clone());
        dom.drain_mutations(&mut drained);

        dom.remove_child(node);
        dom.drain_mutations(&mut drained);
        stamp_overlay_style_if_changed(&mut cache, &mut dom, root, "comms-pane", style.clone());
        assert!(cache.is_none());

        let replacement = dom.create_element(qual("div"));
        dom.set_attribute(replacement, qual("class"), "comms-pane");
        dom.append_child(root, replacement);
        dom.drain_mutations(&mut drained);

        stamp_overlay_style_if_changed(&mut cache, &mut dom, root, "comms-pane", style);
        let mut muts = Vec::new();
        dom.drain_mutations(&mut muts);
        assert_eq!(muts.len(), 1);
    }
}
