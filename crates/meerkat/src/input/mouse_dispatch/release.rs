/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Mouse release handling (extracted from on_mouse_input).

use super::*;

impl WindowCtx<'_> {
    pub(crate) fn on_mouse_release(&mut self, button: MouseButton) {
        let orrery_button = match button {
            MouseButton::Left => Some(PointerButton::Left),
            MouseButton::Middle => Some(PointerButton::Middle),
            MouseButton::Right => Some(PointerButton::Right),
            _ => None,
        };
        let (x, y) = self.view.cursor;
            // A tear-out drag (G1): a Shift-drag of a node past the slop tears it out
            // into a new leaf window (queued; the registry op runs after the ctx borrow
            // ends). A non-moved press (a Shift-click that never dragged) just clears.
            // Handled first, like the other drags below. (Tear-out gestures.)
            if let Some(drag) = self.view.tear_out_drag.take() {
                const TEAR_SLOP: f32 = 6.0;
                let moved = (self.view.cursor.0 - drag.origin.0)
                    .hypot(self.view.cursor.1 - drag.origin.1)
                    > TEAR_SLOP;
                if moved {
                    // Drop-target grammar (OQ-1): a drop on an orrery pane of a
                    // DIFFERENT graph is a cross-graph copy (G5); anything else (the
                    // source pane, chrome, off-window) tears into a new leaf window.
                    let dest = self
                        .orrery_pane_at(self.view.cursor.0, self.view.cursor.1)
                        .map(|(gid, _)| gid)
                        .filter(|&gid| gid != drag.source_graph);
                    let cmd = match dest {
                        // Cross-graph copy (G5): dropped on a different graph's pane.
                        Some(to) => crate::ShellCommand::CopyNodeAcross {
                            node: drag.node,
                            from: drag.source_graph,
                            to,
                        },
                        // Tear axis: the operation was fixed at press by the modifier.
                        // Fork mints an independent session + graph snapshot; branch
                        // mints a `Branched` graphlet in the donor's session (sharing
                        // its graph + nodes) and opens a window scoped to it.
                        None => match drag.op {
                            crate::window_view::TearOp::Fork => {
                                crate::ShellCommand::ForkNode {
                                    node: drag.node,
                                    from: drag.source_graph,
                                }
                            }
                            crate::window_view::TearOp::Branch => {
                                crate::ShellCommand::BranchNode {
                                    node: drag.node,
                                    from: drag.source_graph,
                                }
                            }
                        },
                    };
                    self.commands.push(cmd);
                }
                self.view.chrome_update(|c| c.tear_ghost = None);
                self.view.request_redraw();
                return;
            }
            // End an in-progress swatch vertex drag before any other release routing. The
            // collider was reshaped live on each move, so the release only clears the
            // gesture — but it must clear FIRST: a release that lands on a scrying tile (or
            // any other branch with its own early return) would otherwise leave the drag
            // armed, and the next no-button move would keep reshaping the hull. Any button
            // ends it (the drag is left-initiated and has no multi-button meaning).
            // (Swatch — node shape editor, Stage B.)
            if self.view.swatch_drag.take().is_some() {
                // The hull is graph-truth geometry: persist the edit on release (the
                // cartography sidecar), so a hitbox edit survives a reload even on a
                // non-graceful exit. (Node body & face — the Body axis persists.)
                self.save_session();
                self.view.request_redraw();
                return;
            }
            // End an in-progress row-reorder drag (the configurable menu list): drop the
            // grabbed row onto the row under the cursor (reposition + persist). Like the
            // swatch drag it clears first and consumes the release; a sub-threshold release
            // (a grip click that never moved) just clears. (Command registry B2.)
            if let Some(drag) = self.view.row_reorder_drag.take() {
                if drag.moved {
                    if let Some(target) = drag.target {
                        if target != drag.id {
                            self.reorder_menu_action_to(&drag.id, &target);
                        }
                    }
                }
                self.view.request_redraw();
                return;
            }
            // A release over the focused compatibility-view tile forwards into
            // its WebView (button-up to complete a click). (Scrying X2.)
            if let Some((member, lx, ly)) = self.scrying_at(x, y) {
                if let Some(btn) = scrying_btn(button) {
                    self.view
                        .scrying
                        .forward_mouse(member, lx, ly, scrying_host::MousePress::Up(btn));
                    self.view.request_redraw();
                }
                return;
            }
            // Resolve a pending titlebar press that never became a window drag:
            // it was a click on the toolbar bar (a button / the omnibar). Run it
            // now, and skip the orrery / double-click paths — the press never
            // reached them. (Custom titlebar.)
            if button == MouseButton::Left {
                if let Some((px, py)) = self.view.titlebar_press.take() {
                    self.chrome_click(px, py);
                    return;
                }
                // A manual window resize ends on release; skip the orrery /
                // double-click paths (the press never reached them).
                if self.view.resize_drag.take().is_some() {
                    return;
                }
            }
            // A release over the focused card belongs to the card, not the
            // orrery — releasing on the card must not deselect the node (that
            // would break the card's double-click promote). Elsewhere the release
            // reaches the orrery, which acts only if it owns an in-progress pan /
            // drag / marquee; a click-release selects the node under the cursor.
            let over_card = self.point_over_card(x, y);
            // A left click on a link inside a content card follows it: map the
            // click into the card's content-local space (its rect origin + the
            // card's scroll), resolve the link URL (relative gemtext / markdown
            // links join the card's own URL), and navigate the focused node to
            // it — the omnibar's record-the-visit path. Consumes the release so
            // it doesn't fall through to the card's live-preview toggle.
            // (Inline-link nav; the document lane carries link regions today.)
            //
            // A workbench tab / divider gesture resolves through the pelt shell
            // wherever the release lands (a drop can end outside the pane). If the
            // shell consumed it (tab activate / close / drop / divider end) we're
            // done; otherwise the press was a tile-content click — fall through to
            // the link paths below. (Drag via pelt TileEvents.)
            if button == MouseButton::Left && self.view.workbench_gesture {
                if self.workbench_pointer_up(x, y) {
                    return;
                }
            }
            if button == MouseButton::Left {
                if let Some((base, href)) = self.card_link_at(x, y) {
                    let url = nav::resolve_href(&base, &href);
                    self.view.chrome_update(|c| c.follow_link(url));
                    self.sync_orrery();
                    self.view.request_redraw();
                    return;
                }
            }
            // A click on a link in a workbench tile: Ctrl+click opens it in a new
            // background tab (browser flow); a plain click navigates *that tile's
            // member* in place — focus it so the omnibar + `sync_orrery` target it
            // (`nav_target_member` is the focused tile in Tree), then follow the link.
            if button == MouseButton::Left {
                if let Some((member, base, href)) = self.tile_link_at(x, y) {
                    let url = nav::resolve_href(&base, &href);
                    if self.view.modifiers.ctrl {
                        self.open_link_in_new_tab(member, url);
                    } else {
                        self.view.workbench.activate(member);
                        self.view.focused_tile = Some(member);
                        self.view.chrome_update(|c| c.follow_link(url));
                        self.sync_orrery();
                        self.view.request_redraw();
                    }
                    return;
                }
            }
            // A click on a scripted-rung tile/card is interactive page content: it
            // routes to the live document (script listeners run) and is consumed,
            // exactly as a link click is — not passed through to the orrery. Other
            // lanes emit no scripted hit and fall through. (Render ladder phase 3.)
            #[cfg(feature = "scripted")]
            if button == MouseButton::Left
                && (self.tile_scripted_click(x, y) || self.card_scripted_click(x, y))
            {
                return;
            }
            if let Some(b) = orrery_button {
                let (ox, oy) = self.orrery_point(x, y);
                // A field move / resize ends here; its geometry is graph truth, so
                // persist the session on release. (Field regions — move/resize.)
                let was_field_drag = self.orrery().dragging_field();
                if !over_card && self.orrery_mut().pointer_up(b, ox, oy) {
                    if was_field_drag {
                        self.save_session();
                    }
                    self.view.request_redraw();
                }
            }
            // A frame-divider (host FrameLayout) resize ends on release. The pelt
            // tile-divider resize ended in the workbench-gesture path above.
            if button == MouseButton::Left {
                self.view.frame_divider_drag = None;
            }
            // Double-click routing (orrery pane): on the focused card it toggles
            // the live preview (snapshot -> live actor, or back); on a node it
            // summons the workbench pane with that node + its active neighbors
            // (the contextual-staging gesture). Skip when the release is in the
            // workbench pane (tiles handle their own double-clicks).
            let released_in_workbench = self
                .workbench_leaf_rect()
                .is_some_and(|wr| x >= wr[0] && x < wr[2] && y >= wr[1] && y < wr[3]);
            if button == MouseButton::Left && !released_in_workbench {
                let now = Instant::now();
                let double = self.view.last_left_release.is_some_and(|(t, (lx, ly))| {
                    now.duration_since(t) < Duration::from_millis(400)
                        && (x - lx).hypot(y - ly) < 6.0
                });
                self.view.last_left_release = Some((now, (x, y)));
                if double {
                    self.view.last_left_release = None; // don't chain a triple-click
                    // Double-click opens the node in pelt (a workbench tile), on a node
                    // or its snapshot card, replacing the retired promote-to-live-preview.
                    // (Node-rep P4.) But the object card's − / + are tier steps, not a
                    // node-open: a double-tap on + must step twice, never launch pelt.
                    // (Object card P0.)
                    if !self.point_over_object_card(x, y)
                        && (over_card || !self.orrery().selected_members().is_empty())
                    {
                        self.toggle_workbench();
                    }
                }
            }
    }
}
