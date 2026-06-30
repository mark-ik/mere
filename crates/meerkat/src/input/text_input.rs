/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Field-scoped key handlers (omnibar / palette / context menu / comms / find), clipboard, and caret helpers.

use super::*;

impl WindowCtx<'_> {
    /// Route a key to the focused omnibar: Enter submits (Ctrl/Cmd-Enter opens the
    /// address as a *new* node, a browsing surface linked from the focused one),
    /// Arrow Up/Down + Escape drive the suggestions dropdown, anything else edits
    /// the address and regenerates suggestions. Called only while the omnibar holds
    /// the caret, so it refreshes unconditionally — the source of the old "any
    /// focused field is the omnibar" bug, now scoped to the omnibar's own handler.
    pub(crate) fn on_omnibar_key(&mut self, key: &WinitKey) {
        let suggestions_open = !self.view.chrome().suggest.is_empty();
        match key {
            WinitKey::Named(WinitNamedKey::Enter) => {
                // A `>`-prefixed expression with no highlighted suggestion is a
                // command, not an address: route it to the command shell instead of
                // navigating. (Omnibar command shell, S3.)
                let chrome = self.view.chrome();
                let command_expr = if chrome.suggest_active.is_none() {
                    match nav::classify(chrome.omnibar.text()) {
                        nav::NavTarget::Command(expr) => Some(expr),
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(expr) = command_expr {
                    self.submit_omnibar_command(&expr);
                    return;
                }
                let as_new_node = self.view.modifiers.ctrl || self.view.modifiers.meta;
                self.view.chrome_update(move |c| {
                    submit_omnibar(c);
                    c.open_as_new_node = as_new_node;
                });
                tracing::info!(
                    location = %self.view.chrome().toolbar.editable.location,
                    as_new_node,
                    "omnibar submit"
                );
                self.sync_orrery();
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::ArrowDown) if suggestions_open => {
                self.view.chrome_update(|c| c.step_suggestion(1));
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::ArrowUp) if suggestions_open => {
                self.view.chrome_update(|c| c.step_suggestion(-1));
                self.view.request_redraw();
            }
            // Accept the inline ghost completion. Right arrow only at the buffer
            // end (otherwise it is an ordinary caret move); Tab whenever a ghost is
            // present. Either splices `>ros` + "ter" → `>roster`; Enter still
            // evaluates only what is in the buffer, never the ghost.
            WinitKey::Named(WinitNamedKey::ArrowRight) if self.omnibar_ghost_acceptable(true) => {
                self.accept_omnibar_ghost();
            }
            WinitKey::Named(WinitNamedKey::Tab) if self.omnibar_ghost_acceptable(false) => {
                self.accept_omnibar_ghost();
            }
            other => {
                if let Some(key_event) = key_event_from_winit(other, self.view.modifiers) {
                    self.view.runner.dispatch_key(key_event);
                    self.view.chrome_update(Chrome::refresh_suggestions);
                    self.view.request_redraw();
                }
            }
        }
    }

    /// Whether the omnibar has a ghost completion to accept. `at_end_only` gates
    /// the Right-arrow case to a caret at the buffer end (so a mid-text → is still
    /// a plain caret move); Tab passes `false` (it has no other omnibar meaning).
    pub(crate) fn omnibar_ghost_acceptable(&self, at_end_only: bool) -> bool {
        let omnibar = &self.view.chrome().omnibar;
        !omnibar.ghost().is_empty()
            && (!at_end_only || omnibar.caret() == omnibar.text().chars().count())
    }

    /// Splice the omnibar's ghost completion into the buffer and recompute (which
    /// clears the now-complete ghost and refreshes suggestions).
    pub(crate) fn accept_omnibar_ghost(&mut self) {
        self.view.chrome_update(|c| {
            c.omnibar.accept_ghost();
            c.refresh_suggestions();
        });
        self.view.request_redraw();
    }

    /// Ctrl/Cmd + C / X / V on the active text editor — the command-palette query
    /// when the palette is open, else the omnibar (only while it holds the caret).
    /// Returns `true` if it consumed the key, so the shortcut never also lands as a
    /// typed character. No-op without the modifier, an editor, or a clipboard.
    pub(crate) fn handle_clipboard_shortcut(&mut self, key: &WinitKey) -> bool {
        if !(self.view.modifiers.ctrl || self.view.modifiers.meta) {
            return false;
        }
        let WinitKey::Character(s) = key else {
            return false;
        };
        let palette = self.view.chrome().palette_open;
        // The clipboard shortcuts act on the palette query or the omnibar; when
        // another field (the comms compose box) holds the caret, let the key fall
        // through to that field's handler rather than editing the omnibar.
        if !palette && !self.omnibar_focused() {
            return false;
        }
        match s.as_str() {
            "c" => self.clipboard_copy(palette),
            "x" => self.clipboard_cut(palette),
            "v" => self.clipboard_paste(palette),
            _ => return false,
        }
        true
    }

    /// Copy the active editor's selection to the system clipboard.
    pub(crate) fn clipboard_copy(&mut self, palette: bool) {
        let text = {
            let c = self.view.chrome();
            if palette {
                c.palette_input.selected_text()
            } else {
                c.omnibar.selected_text()
            }
            .to_string()
        };
        if text.is_empty() {
            return;
        }
        if let Some(cb) = self.clipboard.as_mut() {
            let _ = cb.set_text(text);
        }
    }

    /// Cut: copy the selection, then delete it. No-op without a selection.
    pub(crate) fn clipboard_cut(&mut self, palette: bool) {
        let has = {
            let c = self.view.chrome();
            if palette {
                c.palette_input.has_selection()
            } else {
                c.omnibar.has_selection()
            }
        };
        if !has {
            return;
        }
        self.clipboard_copy(palette);
        self.view.chrome_update(|c| {
            if palette {
                c.palette_input.backspace();
            } else {
                c.omnibar.backspace();
            }
        });
        self.after_field_edit(palette);
    }

    /// Paste: insert the clipboard text at the caret, replacing any selection.
    pub(crate) fn clipboard_paste(&mut self, palette: bool) {
        let Some(text) = self.clipboard.as_mut().and_then(|cb| cb.get_text().ok()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        self.view.chrome_update(|c| {
            if palette {
                c.palette_input.insert_str(&text);
            } else {
                c.omnibar.insert_str(&text);
            }
        });
        self.after_field_edit(palette);
    }

    /// Mirror an edit of the active field back into its session state, exactly as a
    /// typed keystroke does (palette query sync, or omnibar suggestions), then redraw.
    pub(crate) fn after_field_edit(&mut self, palette: bool) {
        if palette {
            self.view.chrome_update(Chrome::sync_palette_query);
        } else {
            self.view.chrome_update(Chrome::refresh_suggestions);
        }
        self.view.request_redraw();
    }

    /// Route a key to the open command palette: Enter runs the selection, Arrow
    /// Up/Down step it, Escape closes, anything else edits the query.
    pub(crate) fn on_palette_key(&mut self, key: &WinitKey) {
        match key {
            WinitKey::Named(WinitNamedKey::Enter) => {
                self.view.chrome_update(Chrome::run_palette_selection);
                self.drain_pending_connect();
                self.drain_pending_command();
                self.drain_comms_intent();
                // A palette-invoked context action applies to the live selection (registry P2):
                // seed `context_set` from it, then drain the context action — the same pair
                // `drain_chrome_intents` runs for the click path.
                self.drain_palette_context_action();
                self.drain_pending_context();
                self.drain_history_step();
                self.drain_physics_toggle();
                self.sync_orrery();
                self.focus_after_palette_close();
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::Escape) => {
                self.view.chrome_update(Chrome::close_palette);
                self.focus_after_palette_close();
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::ArrowDown) => {
                self.view.chrome_update(|c| c.step_palette(1));
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::ArrowUp) => {
                self.view.chrome_update(|c| c.step_palette(-1));
                self.view.request_redraw();
            }
            other => {
                if let Some(key_event) = key_event_from_winit(other, self.view.modifiers) {
                    self.view.runner.dispatch_key(key_event);
                    self.view.chrome_update(Chrome::sync_palette_query);
                    self.view.request_redraw();
                }
            }
        }
    }

    /// Keyboard navigation for the open context menu, mirroring the palette: Down / Up move the
    /// highlight (wrapping), Enter runs the highlighted row (or the first when none is highlighted),
    /// Escape closes. Other keys are swallowed so they don't leak to the canvas while the menu is
    /// up. The chosen action is drained immediately, the same pair the click path runs. (Context-
    /// menu keyboard nav.)
    pub(crate) fn on_context_menu_key(&mut self, key: &WinitKey) {
        match key {
            WinitKey::Named(WinitNamedKey::ArrowDown) => {
                self.view.chrome_update(|c| c.step_context_menu(1));
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::ArrowUp) => {
                self.view.chrome_update(|c| c.step_context_menu(-1));
                self.view.request_redraw();
            }
            // ArrowRight expands the highlighted parent's submenu; ArrowLeft collapses it back to
            // the root list. (Nested submenus.)
            WinitKey::Named(WinitNamedKey::ArrowRight) => {
                self.view.chrome_update(Chrome::enter_submenu);
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::ArrowLeft) => {
                self.view.chrome_update(Chrome::close_submenu);
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::Enter) => {
                self.view.chrome_update(Chrome::run_context_selection);
                self.drain_pending_context();
                self.drain_pending_command();
                self.sync_orrery();
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::Escape) => {
                // Escape is normally consumed by the early intercept in `on_key_pressed` (which owns
                // the full WindowCtx cleanup); this arm mirrors it exactly as a fallback so the two
                // paths can't diverge: collapse one submenu level if open, else close the whole
                // menu via the cleanup-clearing wrapper. (Nested submenus.)
                if self
                    .view
                    .chrome()
                    .context_menu
                    .as_ref()
                    .is_some_and(|m| m.submenu.is_some())
                {
                    self.view.chrome_update(Chrome::close_submenu);
                    self.view.request_redraw();
                } else {
                    self.close_context_menu();
                }
            }
            // Typing searches the menu (the cursor palette): edit its query buffer and rebuild the
            // rows. Backspace deletes; Space (a named key) inserts a space. (Searchable context menu S1.)
            WinitKey::Named(WinitNamedKey::Backspace) => {
                self.view.chrome_update(|c| {
                    if let Some(menu) = &mut c.context_menu {
                        menu.query.pop();
                    }
                });
                self.rebuild_context_menu();
            }
            WinitKey::Named(WinitNamedKey::Space) => {
                self.view.chrome_update(|c| {
                    if let Some(menu) = &mut c.context_menu {
                        menu.query.push(' ');
                    }
                });
                self.rebuild_context_menu();
            }
            WinitKey::Character(s) => {
                let s = s.to_string();
                self.view.chrome_update(move |c| {
                    if let Some(menu) = &mut c.context_menu {
                        menu.query.push_str(&s);
                    }
                });
                self.rebuild_context_menu();
            }
            _ => {}
        }
    }

    /// Route a key to the focused comms compose field: Enter sends the draft,
    /// Escape blurs back to the omnibar, anything else edits the field — never
    /// touching the omnibar suggestions (the dropdown stays closed while chatting).
    pub(crate) fn on_comms_key(&mut self, key: &WinitKey) {
        let composing_new = self.view.chrome().comms.new_message_open();
        match key {
            WinitKey::Named(WinitNamedKey::Enter) => {
                // Enter sends: the compose-new form when it's open, else the reply.
                if composing_new {
                    self.view.chrome_update(Chrome::send_new_message);
                } else {
                    self.view.chrome_update(Chrome::send_comms);
                }
                self.drain_comms_intent();
                self.view.request_redraw();
            }
            WinitKey::Named(WinitNamedKey::Escape) => {
                // Escape closes the compose-new form, else blurs to the omnibar.
                if composing_new {
                    self.view.chrome_update(Chrome::close_new_message);
                }
                self.focus_after_palette_close();
                self.view.request_redraw();
            }
            other => {
                if let Some(key_event) = key_event_from_winit(other, self.view.modifiers) {
                    self.view.runner.dispatch_key(key_event);
                    self.view.request_redraw();
                }
            }
        }
    }

    /// Whether any comms-pane `<input>` (the reply compose box, or the compose-new
    /// recipient / body) currently holds the keyboard caret.
    pub(crate) fn comms_input_focused(&self) -> bool {
        let focus = self.view.runner.focus();
        focus.is_some()
            && [
                self.input_under_class("comms-compose"),
                self.input_under_class("comms-new-to"),
                self.input_under_class("comms-new-body"),
            ]
            .contains(&focus)
    }

    /// Whether the omnibar `<input>` currently holds the keyboard caret.
    pub(crate) fn omnibar_focused(&self) -> bool {
        let focus = self.view.runner.focus();
        focus.is_some() && focus == self.input_under_class("toolbar")
    }

    /// The text field whose caret the host paints, by the focused DOM `node`: a
    /// comms compose / new-message field, the palette query, else the omnibar.
    /// Whether `node` is a text-editable field (an `<input>`: the omnibar, palette query, find
    /// bar, comms fields). The caret + selection overlay paints only for these; a focused button
    /// or orrery card (focusable since Phase 1 / 2a) rings but shows no editing caret. (Phase 2a.)
    pub(crate) fn is_text_input(&self, node: NodeId) -> bool {
        let dom = self.view.dom.borrow();
        let name = dom.element_name(node).map(|q| q.local.as_ref());
        name == Some("input") || name == Some("textarea")
    }

    pub(crate) fn caret_field(&self, node: NodeId) -> &xilem_serval::TextInput {
        let focus = Some(node);
        let c = self.view.chrome();
        if focus == self.input_under_class("comms-new-to") {
            &c.comms_new_to
        } else if focus == self.input_under_class("comms-new-body") {
            &c.comms_new_body
        } else if focus == self.input_under_class("comms-compose") {
            &c.comms_draft
        } else if focus == self.input_under_class("knot-editor-source") {
            &c.knot_source
        } else if c.palette_open {
            &c.palette_input
        } else {
            &c.omnibar
        }
    }

    /// Toggle the palette and move focus to match: into the palette query when
    /// it opens, back to the omnibar when it closes.
    pub(crate) fn toggle_palette(&mut self) {
        self.view.chrome_update(Chrome::toggle_palette);
        if self.view.chrome().palette_open {
            if let Some(node) = self.input_under_class("palette") {
                self.view.runner.set_focus(Some(node));
            }
        } else {
            self.focus_after_palette_close();
        }
        self.view.request_redraw();
    }

    /// Restore focus to the omnibar after the palette closes (so keyboard use
    /// continues there).
    pub(crate) fn focus_after_palette_close(&mut self) {
        let omnibar = self.input_under_class("toolbar");
        self.view.runner.set_focus(omnibar);
    }

    /// Toggle the find bar and move focus to match: into the find query when it
    /// opens, back to the omnibar when it closes (clearing the actor's matches).
    pub(crate) fn toggle_find(&mut self) {
        self.view.chrome_update(Chrome::toggle_find);
        if self.view.chrome().find_open {
            if let Some(node) = self.input_under_class("find-bar") {
                self.view.runner.set_focus(Some(node));
            }
        } else {
            self.clear_find_matches();
            self.focus_after_palette_close();
        }
        self.view.request_redraw();
    }

    /// Route a key to the open find bar: Enter = next match, Shift+Enter = prev,
    /// Escape closes, anything else edits the query and re-runs the search.
    pub(crate) fn on_find_key(&mut self, key: &WinitKey) {
        match key {
            WinitKey::Named(WinitNamedKey::Enter) => {
                let delta = if self.view.modifiers.shift { -1 } else { 1 };
                self.step_find_match(delta);
            }
            WinitKey::Named(WinitNamedKey::Escape) => {
                self.view.chrome_update(Chrome::close_find);
                self.clear_find_matches();
                self.focus_after_palette_close();
                self.view.request_redraw();
            }
            other => {
                if let Some(key_event) = key_event_from_winit(other, self.view.modifiers) {
                    self.view.runner.dispatch_key(key_event);
                    self.submit_find_query();
                    self.view.request_redraw();
                }
            }
        }
    }

    /// Push the edited find query to the content actor for the focused node, and
    /// reset the active match to the first. (The actor dedups and clears on empty.)
    pub(crate) fn submit_find_query(&mut self) {
        let query = self.view.chrome().find_input.text().to_string();
        self.recompute_find(&query);
        self.view.chrome_update(|c| c.find_active = 0);
    }

    /// Clear the find matches (an empty query / bar closed), so highlights vanish.
    pub(crate) fn clear_find_matches(&mut self) {
        self.clear_find();
    }

    /// Cycle the active match by `delta` (wrapping within the live count) and, if the
    /// newly-active match falls outside the focused card / tile's visible band, scroll
    /// it into view (placed ~20% down). The highlight overlay then tints it. (Find S2.)
    pub(crate) fn step_find_match(&mut self, delta: isize) {
        let Some(member) = self.focused_member() else {
            return;
        };
        let matches = self.find_matches_for(member);
        let count = matches.len();
        if count == 0 {
            return;
        }
        let cur = self.view.chrome().find_active.min(count - 1) as isize;
        let next = (cur + delta).rem_euclid(count as isize) as usize;
        // The active match's vertical extent (full-document px), for the auto-scroll.
        let (match_top, match_bot) = matches[next]
            .iter()
            .fold((f32::MAX, f32::MIN), |(t, b), r| (t.min(r[1]), b.max(r[3])));
        self.view.chrome_update(move |c| c.find_active = next);
        if let Some((vis_h, content_h)) = self.find_member_viewport(member) {
            let scroll = self.view.scroll.get(&member).copied().unwrap_or(0.0);
            let out_of_view = match_top < scroll || match_bot > scroll + vis_h;
            if out_of_view && match_top.is_finite() {
                let target = (match_top - vis_h * 0.2).clamp(0.0, (content_h - vis_h).max(0.0));
                self.view.scroll.insert(member, target);
            }
        }
        self.view.request_redraw();
    }

    /// The focused find surface's `(visible_height, content_height)` in px — the live
    /// card or workbench tile showing `member`, 1:1 so the dest height is the visible
    /// document window. Drives the find auto-scroll. (Find S2.)
    pub(crate) fn find_member_viewport(&self, member: GraphMemberId) -> Option<(f32, f32)> {
        let rect = self
            .view
            .content_rects
            .iter()
            .chain(self.view.tile_rects.iter())
            .find(|(m, _)| *m == member)
            .map(|(_, r)| *r)?;
        let vis_h = (rect[3] - rect[1]).max(1.0);
        let content_h = self.member_content_height(member, vis_h);
        Some((vis_h, content_h))
    }

    /// The first `<input>` or `<textarea>` under the first element carrying CSS
    /// class `class` (the omnibar under `.toolbar`, the knot source under
    /// `.knot-editor-source`).
    pub(crate) fn input_under_class(&self, class: &str) -> Option<NodeId> {
        let dom = self.view.dom.borrow();
        let container = first_with_class(&dom, dom.document(), class)?;
        first_tag(&dom, container, "input").or_else(|| first_tag(&dom, container, "textarea"))
    }
}
