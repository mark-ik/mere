/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Workbench tile pointer drags and tile-event application.

use super::*;

impl WindowCtx<'_> {
    /// Route a left press in the workbench pane into the host-authoritative pelt shell:
    /// set its cursor (pane-local), press, and apply each emitted gesture to the
    /// `Workbench` — the authority. Marks a gesture in flight so subsequent moves feed
    /// the shell (a drag continues past the pane edge). The shell does the frame
    /// hit-testing (tab / divider / close) internally; tile-content link clicks were
    /// already resolved earlier (`tile_link_at`). Re-projection is on the next render
    /// (`Workbench::to_tile_tree`), so the shell stays a driven view. (Drag via TileEvents.)
    pub(crate) fn workbench_pointer_down(&mut self, x: f32, y: f32) {
        let Some(wr) = self.workbench_leaf_rect() else {
            return;
        };
        let (lx, ly) = (x - wr[0], y - wr[1]);
        let events = {
            let Some(shell) = self.view.pelt_shell.as_mut() else {
                return;
            };
            shell.pointer_move(lx, ly);
            shell.pointer_down();
            shell.take_events()
        };
        self.view.workbench_gesture = true;
        for event in events {
            self.apply_tile_event(event);
        }
        self.view.request_redraw();
    }

    /// Feed a pointer move to the shell while a workbench gesture is in flight (advances
    /// a divider resize / tab drag), applying any emitted gesture. (Drag via TileEvents.)
    pub(crate) fn workbench_pointer_move(&mut self, x: f32, y: f32) {
        let Some(wr) = self.workbench_leaf_rect() else {
            return;
        };
        let (lx, ly) = (x - wr[0], y - wr[1]);
        let events = {
            let Some(shell) = self.view.pelt_shell.as_mut() else {
                return;
            };
            shell.pointer_move(lx, ly);
            shell.take_events()
        };
        for event in events {
            self.apply_tile_event(event);
        }
        self.view.request_redraw();
    }

    /// End a workbench gesture: release the shell (resolving a tab drop / activate),
    /// apply the emitted gesture, and clear the in-flight flag. Returns whether the
    /// shell consumed the release (it emitted a gesture); `false` means the press was a
    /// tile-content click that should fall through to the link/card paths. (Drag via
    /// TileEvents.)
    pub(crate) fn workbench_pointer_up(&mut self, x: f32, y: f32) -> bool {
        self.view.workbench_gesture = false;
        let Some(wr) = self.workbench_leaf_rect() else {
            return false;
        };
        let (lx, ly) = (x - wr[0], y - wr[1]);
        let events = {
            let Some(shell) = self.view.pelt_shell.as_mut() else {
                return false;
            };
            shell.pointer_move(lx, ly);
            shell.pointer_up();
            shell.take_events()
        };
        let consumed = !events.is_empty();
        for event in events {
            self.apply_tile_event(event);
        }
        self.view.request_redraw();
        consumed
    }

    /// The workbench member a pelt [`TileId`](pelt_core::tile::TileId) addresses, keyed
    /// by the UUID's low 64 bits (the encoding `Workbench::to_tile_tree` mints in
    /// `render.rs`). `None` if no open member matches.
    pub(crate) fn tile_member(&self, id: pelt_core::tile::TileId) -> Option<GraphMemberId> {
        self.view
            .workbench
            .open_members()
            .iter()
            .copied()
            .find(|m| m.as_u128() as u64 == id.0)
    }

    /// Apply one pelt-surface [`TileEvent`](pelt_core::tile::TileEvent) to the
    /// `Workbench` (the single tiling authority). The one seam every tile gesture
    /// funnels through: a tab activates / closes its member today; drag + divider
    /// resize fill the remaining arms once the surface's pointer state machine drives
    /// them (B3). `press` is the window-space press point for the host's transitional
    /// tab-drag candidate (a click sets it; a surface-emitted gesture passes `None`).
    /// Re-projection happens on the next render (`Workbench::to_tile_tree`), so the
    /// surface stays a driven view. (Tile-event seam.)
    pub(crate) fn apply_tile_event(&mut self, event: pelt_core::tile::TileEvent) {
        use pelt_core::tile::{DropTarget, Edge, SplitAxis, TileEvent};
        match event {
            TileEvent::Activated(id) => {
                if let Some(m) = self.tile_member(id) {
                    self.view.workbench.activate(m);
                    self.view.focused_tile = Some(m);
                }
            }
            TileEvent::Closed(id) => {
                if let Some(m) = self.tile_member(id) {
                    self.view.workbench.close_tile(m);
                    self.shared.content.constellation.reap(m);
                    if self.view.workbench.open_members().is_empty() {
                        // Closing the last tile closes the workbench pane entirely
                        // (back to just the orrery). (Workbench-as-pane.)
                        self.close_workbench();
                    } else if self.view.focused_tile == Some(m) {
                        self.view.focused_tile =
                            self.view.workbench.open_members().first().copied();
                    }
                }
            }
            // A tab dropped past the slop: onto a tab bar (Stack) it merges into that
            // stack; onto another tile's content (Edge) it splits that pane on the
            // dropped edge; outside the surface it tears that tile into its own leaf
            // window. The drag itself + the drop-zone resolution live in the pelt
            // shell; here each resolved DropTarget maps to a Workbench mutation or a
            // shell command.
            TileEvent::Dragged { tile, to } => {
                let Some(dragged) = self.tile_member(tile) else {
                    return;
                };
                match to {
                    DropTarget::Stack { stack, .. } => {
                        // Any member in the target stack identifies the slot; insertion
                        // index is appended for now (DropTarget::Stack{index} is a
                        // follow-on for precise reorder).
                        let target_id = self
                            .view
                            .pelt_shell
                            .as_ref()
                            .and_then(|sh| member_at_path(sh.tree(), &stack.0));
                        let target = target_id.and_then(|tid| self.tile_member(tid));
                        if let Some(target) = target {
                            if self.view.workbench.move_to_slot_of(dragged, target) {
                                self.view.focused_tile = Some(dragged);
                            }
                        }
                    }
                    DropTarget::Edge {
                        tile: target_id,
                        edge,
                    } => {
                        let Some(target) = self.tile_member(target_id) else {
                            return;
                        };
                        let (axis, after) = match edge {
                            Edge::Left => (SplitAxis::Row, false),
                            Edge::Right => (SplitAxis::Row, true),
                            Edge::Top => (SplitAxis::Column, false),
                            Edge::Bottom => (SplitAxis::Column, true),
                        };
                        let moved = if target == dragged {
                            self.view.workbench.split_out(dragged, axis, after)
                        } else {
                            self.view
                                .workbench
                                .split_beside_axis(dragged, target, axis, after)
                        };
                        if moved {
                            self.view.focused_tile = Some(dragged);
                        }
                    }
                    DropTarget::Outside => {
                        self.commands.push(crate::ShellCommand::TearOut {
                            node: dragged,
                            from: self.view.focused_graph,
                        });
                    }
                }
            }
            // A divider drag: reweight the addressed split. Path-addressed, so nested
            // splits resize too (the host divider path only reached the top level).
            TileEvent::DividerMoved { split, fractions } => {
                self.view
                    .workbench
                    .set_split_fractions(&split.0, &fractions);
            }
        }
    }
}
