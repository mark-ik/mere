/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Chrome: pending-action drains, tab cap, context menu + submenu.

use super::*;

impl Chrome {
    /// Take a pending "connect to peer" request, if one is queued. The host calls
    /// this after running a palette command, then drives the sync actor with it.
    pub fn take_pending_connect(&mut self) -> Option<String> {
        self.pending_connect.take()
    }

    /// Take a pending host action, if one is queued. The host calls this after a
    /// palette run / row click and dispatches it to the matching shell method.
    pub fn take_pending_command(&mut self) -> Option<Command> {
        self.pending_command.take()
    }

    /// Take a pending back/forward step, if one is queued. The host applies it to
    /// the focused node's history and mirrors the revealed page back via
    /// [`Chrome::show_location`] + [`Chrome::content_location`].
    pub fn take_history_step(&mut self) -> Option<HistoryStep> {
        self.history_step.take()
    }

    /// Take the pending pause/play toggle, if the button was clicked this pass. The
    /// host applies it to the orrery's physics. (Physics pause.)
    pub fn take_physics_toggle(&mut self) -> bool {
        std::mem::take(&mut self.physics_toggle)
    }

    /// Raise the active-tab cap by one (bounded, so it can't reach an absurd value).
    /// Driven by the `pelt/appearance` page's cap control; the host applies + persists it.
    pub fn inc_tab_cap(&mut self) {
        self.settings.tab_cap = (self.settings.tab_cap + 1).min(64);
    }

    /// Lower the active-tab cap by one (never below 1).
    pub fn dec_tab_cap(&mut self) {
        self.settings.tab_cap = self.settings.tab_cap.saturating_sub(1).max(1);
    }

    /// Open the right-click context menu at window `(x, y)` with host-computed
    /// `items` (closing the suggestions dropdown so it can't overlap).
    pub fn open_context_menu(&mut self, x: f32, y: f32, items: Vec<ContextItem>) {
        self.close_suggestions();
        self.context_menu = Some(ContextMenu {
            x,
            y,
            items,
            selected: None,
            query: String::new(),
            submenu: None,
        });
    }

    /// Move the context-menu keyboard highlight by `delta`, wrapping within the rows — the
    /// menu's counterpart to [`step_palette`](Self::step_palette). `None` → first row on a step
    /// down, last on a step up. (Context-menu keyboard nav.)
    pub fn step_context_menu(&mut self, delta: isize) {
        if let Some(menu) = &mut self.context_menu {
            // When a submenu is open, the arrows nav the child panel; otherwise the root list.
            if let Some(sub) = &mut menu.submenu {
                let count = menu.items.get(sub.parent).map_or(0, |p| p.children.len());
                if count == 0 {
                    return;
                }
                sub.selected = Some(step_wrapped(sub.selected, delta, count));
            } else {
                let count = menu.items.len();
                if count == 0 {
                    return;
                }
                menu.selected = Some(step_wrapped(menu.selected, delta, count));
            }
        }
    }

    /// Expand row `parent`'s submenu (a no-op if it has no children), highlighting nothing yet.
    /// Idempotent: re-opening the same parent leaves it open. (Nested submenus.)
    pub fn open_submenu(&mut self, parent: usize) {
        if let Some(menu) = &mut self.context_menu {
            if menu.items.get(parent).is_some_and(ContextItem::has_submenu) {
                menu.submenu = Some(SubmenuState {
                    parent,
                    selected: None,
                });
            }
        }
    }

    /// Collapse the open submenu back to the root list (one level). (Nested submenus.)
    pub fn close_submenu(&mut self) {
        if let Some(menu) = &mut self.context_menu {
            menu.submenu = None;
        }
    }

    /// `ArrowRight`: when a root parent row is highlighted and no submenu is open, expand it and
    /// move the highlight to its first child. A no-op otherwise. (Nested submenus.)
    pub fn enter_submenu(&mut self) {
        let target = self.context_menu.as_ref().and_then(|menu| {
            if menu.submenu.is_some() {
                return None;
            }
            let i = menu.selected?;
            menu.items.get(i).filter(|it| it.has_submenu()).map(|_| i)
        });
        if let Some(i) = target {
            self.open_submenu(i);
            if let Some(sub) = self.context_menu.as_mut().and_then(|m| m.submenu.as_mut()) {
                sub.selected = Some(0);
            }
        }
    }

    /// `Escape` (and the early key intercept): collapse the open submenu first, else close the
    /// whole menu — one level at a time. (Nested submenus.)
    pub fn escape_context_menu(&mut self) {
        if self
            .context_menu
            .as_ref()
            .is_some_and(|m| m.submenu.is_some())
        {
            self.close_submenu();
        } else {
            self.close_context_menu();
        }
    }

    /// Run the highlighted context-menu row (or the first, if none is highlighted) and close —
    /// the keyboard `Enter` counterpart to a row click. A no-op close on an empty menu.
    /// (Context-menu keyboard nav.)
    pub fn run_context_selection(&mut self) {
        // Decide first (immutable borrow), then act — a submenu child is picked, a parent row is
        // expanded, a leaf row runs. (Nested submenus.)
        enum Run {
            Pick(ContextAction),
            Open(usize),
            Close,
            Nothing,
        }
        let run = match &self.context_menu {
            None => Run::Nothing,
            Some(menu) => match &menu.submenu {
                Some(sub) => match menu
                    .items
                    .get(sub.parent)
                    .and_then(|p| p.children.get(sub.selected.unwrap_or(0)))
                {
                    Some(child) => Run::Pick(child.action),
                    None => Run::Close,
                },
                None => {
                    let pick = menu.selected.unwrap_or(0);
                    match menu.items.get(pick) {
                        Some(item) if item.has_submenu() => Run::Open(pick),
                        Some(item) => Run::Pick(item.action),
                        None => Run::Close,
                    }
                }
            },
        };
        match run {
            Run::Pick(action) => self.pick_context(action),
            Run::Open(parent) => {
                // Enter-to-open focuses the first child, matching ArrowRight (`enter_submenu`).
                self.open_submenu(parent);
                if let Some(sub) = self.context_menu.as_mut().and_then(|m| m.submenu.as_mut()) {
                    sub.selected = Some(0);
                }
            }
            Run::Close => self.close_context_menu(),
            Run::Nothing => {}
        }
    }

    /// Open the add-pill's menu at `(x, y)`: add a node, a tile, or a session. The
    /// three rows reuse the context-menu machinery; the host drains the chosen
    /// `ContextAction`.
    pub fn open_add_menu(&mut self, x: f32, y: f32) {
        self.open_context_menu(
            x,
            y,
            // No "Add session" — the toolbar session strip owns session creation now
            // (Chrome bar P5). This menu is the split-button's overflow (tile / field).
            vec![
                ContextItem::new("Add node", ContextAction::AddNode),
                ContextItem::new("Add tile", ContextAction::AddTile),
                ContextItem::new("Add field", ContextAction::AddField),
            ],
        );
    }

    /// Close the context menu without running anything.
    pub fn close_context_menu(&mut self) {
        self.context_menu = None;
    }

    /// Capture `action` from a clicked menu row and close the menu; the host drains
    /// it and applies it to the menu's member set.
    pub fn pick_context(&mut self, action: ContextAction) {
        self.pending_context = Some(action);
        self.close_context_menu();
    }

    /// Capture a pin toggle from a search result **without** closing the menu — the host pins /
    /// unpins `id`, persists, and rebuilds the open menu, so several can be pinned in a row.
    /// (Searchable context menu S2.)
    pub fn pin_from_menu(&mut self, id: &'static str) {
        self.pending_context = Some(ContextAction::PinToMenu(id));
    }

    /// Take the pending context-menu action, if any.
    pub fn take_pending_context(&mut self) -> Option<ContextAction> {
        self.pending_context.take()
    }
}
