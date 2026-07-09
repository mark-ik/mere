/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Top-level key dispatch (on_key_pressed) and the graph-pane key handler.

use super::*;

impl WindowCtx<'_> {
    /// Handle a pressed key. First the global chords (Ctrl+P palette, Ctrl+K comms,
    /// Ctrl+T workbench, …) and clipboard shortcuts; then the key routes to whichever
    /// field owns the caret — the palette query ([`on_palette_key`](Self::on_palette_key)),
    /// the comms compose box ([`on_comms_key`](Self::on_comms_key)), or the omnibar
    /// ([`on_omnibar_key`](Self::on_omnibar_key)) — each handler scoped to its own
    /// field. A key with no focused field is ignored.
    pub(crate) fn on_key_pressed(&mut self, key: &WinitKey) {
        // Renaming a session captures the keyboard: type into the switcher label,
        // Enter commits, Escape cancels, Backspace deletes. (Host text path.)
        if self.view.renaming.is_some() {
            match key {
                WinitKey::Named(WinitNamedKey::Enter) => self.commit_rename(),
                WinitKey::Named(WinitNamedKey::Escape) => self.cancel_rename(),
                WinitKey::Named(WinitNamedKey::Backspace) => self.rename_backspace(),
                WinitKey::Character(s)
                    if !self.view.modifiers.ctrl
                        && !self.view.modifiers.meta
                        && !s.chars().any(char::is_control) =>
                {
                    self.rename_push(s.as_str());
                }
                _ => {}
            }
            return;
        }
        // Tagging a node captures the keyboard like renaming: type the tag, Enter
        // commits it onto the selection, Escape cancels, Backspace deletes. (Add-tag.)
        if self.view.tagging.is_some() {
            match key {
                WinitKey::Named(WinitNamedKey::Enter) => self.commit_tag(),
                WinitKey::Named(WinitNamedKey::Escape) => self.cancel_tag(),
                WinitKey::Named(WinitNamedKey::Backspace) => self.tag_backspace(),
                WinitKey::Character(s)
                    if !self.view.modifiers.ctrl
                        && !self.view.modifiers.meta
                        && !s.chars().any(char::is_control) =>
                {
                    self.tag_push(s.as_str());
                }
                _ => {}
            }
            return;
        }
        // An open context menu eats Escape to dismiss (other keys fall through). With a submenu
        // open, Escape collapses it one level first, keeping the root menu up. (Nested submenus.)
        if self.chrome().context_menu.is_some()
            && matches!(key, WinitKey::Named(WinitNamedKey::Escape))
        {
            if self
                .chrome()
                .context_menu
                .as_ref()
                .is_some_and(|m| m.submenu.is_some())
            {
                self.chrome_update(Chrome::close_submenu);
                self.view.request_redraw();
            } else {
                self.close_context_menu();
            }
            return;
        }
        // Escape closes an open object card (after a menu, which eats Escape first). A
        // click-away already closes it: an empty-canvas click clears the selection, and the
        // card drops once focus leaves its member. (Object card — explicit close.)
        if self.view.object_card.is_some() && matches!(key, WinitKey::Named(WinitNamedKey::Escape))
        {
            self.view.object_card = None;
            self.view.request_redraw();
            return;
        }
        // F2 renames the focused pane's session (the switcher's keyboard rename
        // affordance; right-clicking a tile renames that one). (Host text path;
        // pane-as-unit — the focused pane's session, not a global active one.)
        if matches!(key, WinitKey::Named(WinitNamedKey::F2)) {
            if let Some((id, _)) = self.session_for_graph(self.view.focused_graph) {
                self.start_rename(id);
            }
            return;
        }
        // Ctrl+L focuses the address bar and selects its whole contents (browser
        // convention) so a new URL can be typed to replace the shown one, without the
        // mouse.
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("l"))
        {
            let omnibar = self.input_under_class("toolbar");
            self.multi.set_focus(self.view.projection_id, omnibar);
            self.chrome_update(|c| c.omnibar.select_all());
            self.view.request_redraw();
            return;
        }
        // F5 reloads the focused node's page — re-fetch, bypassing the durable cache.
        if matches!(key, WinitKey::Named(WinitNamedKey::F5)) {
            self.retry_focused_content();
            return;
        }
        // Alt+Left / Alt+Right step the focused node's own history (browser
        // back/forward). Mirror the toolbar buttons: record the intent on the chrome,
        // then drain it this pass so the revealed page loads at once. A no-op when
        // nothing is focused or the node is at a history end.
        if self.view.modifiers.alt && matches!(key, WinitKey::Named(WinitNamedKey::ArrowLeft)) {
            self.chrome_update(|c| c.history_step = Some(HistoryStep::Back));
            self.drain_history_step();
            return;
        }
        if self.view.modifiers.alt && matches!(key, WinitKey::Named(WinitNamedKey::ArrowRight)) {
            self.chrome_update(|c| c.history_step = Some(HistoryStep::Forward));
            self.drain_history_step();
            return;
        }
        // Command palette: Ctrl+P. Ctrl+K toggles the comms pane (freed up for it).
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("p"))
        {
            self.toggle_palette();
            return;
        }
        // Ctrl +/- zoom the chrome, Ctrl+0 resets it to the baseline (browser
        // convention). '=' and '+' both zoom in (so the unshifted key works); '-' and
        // '_' both zoom out. The chrome sheet rebuilds at the new scale. (UI scale.)
        if self.view.modifiers.ctrl {
            if let WinitKey::Character(s) = key {
                match s.as_str() {
                    "=" | "+" => {
                        self.adjust_user_zoom(0.1);
                        return;
                    }
                    "-" | "_" => {
                        self.adjust_user_zoom(-0.1);
                        return;
                    }
                    "0" => {
                        self.adjust_user_zoom(0.0);
                        return;
                    }
                    _ => {}
                }
            }
        }
        // Ctrl+F opens / closes the find-in-page bar (HTML/serval lane).
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("f"))
        {
            self.toggle_find();
            return;
        }
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("k"))
        {
            self.chrome_update(Chrome::toggle_comms);
            self.drain_comms_intent();
            self.view.request_redraw();
            return;
        }
        // Ctrl+T toggles the tiled workbench (Tree projection) and the orrery
        // (Cartography projection) of the same graph.
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("t"))
        {
            self.toggle_workbench();
            return;
        }
        // Ctrl+B flags the focused node to keep working in the background (its
        // actor outlives the view); Ctrl+Backspace deletes the focused node from
        // the graph. Both are modifier-gated so they don't collide with omnibar
        // editing, and intercepted here before the keystroke reaches the field.
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("b"))
        {
            self.toggle_focus_background();
            return;
        }
        // Ctrl+R summons / closes the roster pane (the graph's node list); Ctrl+,
        // the apparatus pane (settings + system); both split beside the graph pane.
        // Ctrl+M maximizes the pane under the cursor to full-screen and back.
        // (Frame tree, F1 / apparatus.)
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("r"))
        {
            self.toggle_pane(PaneContent::Roster);
            return;
        }
        if self.view.modifiers.ctrl && matches!(key, WinitKey::Character(s) if s.as_str() == ",") {
            self.toggle_pane(PaneContent::Apparatus);
            return;
        }
        // Ctrl+Shift+D dumps the full diagnostics ring to `<mere_root>/diagnostics-dump.txt`
        // (dev-loop escape hatch); the Apparatus pane only shows a small recent window.
        if self.view.modifiers.ctrl
            && self.view.modifiers.shift
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("d"))
        {
            self.dump_diagnostics();
            return;
        }
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("g"))
        {
            self.toggle_pane(PaneContent::Gloss);
            return;
        }
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("m"))
        {
            self.toggle_maximize();
            return;
        }
        // Ctrl+W closes the focused graph pane when a second graph-pane is open (the
        // dismiss for an open-beside graph). A no-op with a single graph view, so it
        // never closes your last graph. (Window composition — pane-as-unit.)
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("w"))
        {
            self.close_focused_graph_pane();
            return;
        }
        // Cmd/Ctrl+Shift+N opens a new OS window over the same shared session (a
        // second view). A per-window handler can't create a window itself (no event
        // loop, no registry access), so it queues a `SpawnWindow` the shell applies in
        // `about_to_wait`. Checked before the unshifted Ctrl+N (new session) so the
        // shift distinguishes the two. (Multi-window MW3.)
        if (self.view.modifiers.ctrl || self.view.modifiers.meta)
            && self.view.modifiers.shift
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("n"))
        {
            self.commands.push(crate::ShellCommand::SpawnWindow);
            return;
        }
        // Ctrl+N mints a new graph session and switches to it; Ctrl+PageDown /
        // Ctrl+PageUp cycle through the open sessions (the interim keyboard switch
        // until the F2.3 shellbar switcher). (Multi-graph MG2 / MG3.)
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("n"))
        {
            self.commands.push(crate::ShellCommand::CreateSession);
            return;
        }
        if self.view.modifiers.ctrl && matches!(key, WinitKey::Named(WinitNamedKey::PageDown)) {
            self.commands.push(crate::ShellCommand::CycleSession(true));
            return;
        }
        if self.view.modifiers.ctrl && matches!(key, WinitKey::Named(WinitNamedKey::PageUp)) {
            self.commands.push(crate::ShellCommand::CycleSession(false));
            return;
        }
        if self.view.modifiers.ctrl && matches!(key, WinitKey::Named(WinitNamedKey::Backspace)) {
            self.delete_focused_node();
            return;
        }
        // Ctrl+E flips the open knot editor between source-edit and rendered-preview
        // (the preview drops the opaque overlay so the live-rendered tile shows through).
        // Only while the editor is open on a bound note; otherwise it falls through.
        // (Djot editor — Phase 2 toggle source/preview.)
        if self.view.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("e"))
            && self.chrome().knot_editor_open
            && self.chrome().knot_target.is_some()
        {
            self.chrome_update(|c| c.toggle_knot_editor_preview());
            self.view.request_redraw();
            return;
        }
        if self.handle_clipboard_shortcut(key) {
            return;
        }
        if self.chrome().palette_open {
            self.on_palette_key(key);
            return;
        }
        // An open context menu owns the keyboard (arrow nav + Enter/Escape), like the palette.
        if self.chrome().context_menu.is_some() {
            self.on_context_menu_key(key);
            return;
        }
        if self.chrome().find_open {
            self.on_find_key(key);
            return;
        }
        // Soft-wrap visual-line nav for the focused knot-editor textarea: ArrowUp/ArrowDown
        // move by parley's wrapped rows (with a sticky goal column) instead of the buffer
        // handler's hard-`\n`-line move, before the field handlers route the key. A no-op for
        // any other focus, so omnibar suggestion nav / comms field moves fall through. (Soft-
        // wrap caret nav.)
        // (Alt+Up/Down is structural expand/shrink, handled in `on_knot_editor_key`, so it
        // is excluded here.)
        if let WinitKey::Named(named) = key {
            let delta = match named {
                WinitNamedKey::ArrowUp => Some(-1),
                WinitNamedKey::ArrowDown => Some(1),
                _ => None,
            };
            // Also excluded while a completion popup is open (Up/Down navigate it instead).
            let vertical_free =
                !self.view.modifiers.alt && self.chrome().knot_completion.is_none();
            if let Some(delta) = delta.filter(|_| vertical_free) {
                if self.soft_wrap_nav(delta, self.view.modifiers.shift) {
                    return;
                }
            }
        }
        // Route the key to whichever field owns the caret. Each focusable field has
        // its own handler, invoked only while it holds focus — so no field's logic
        // (the omnibar's suggestion refresh, its Enter-submits) leaks onto another.
        // A key with no focused field is ignored: the chrome owns the keyboard; the
        // orrery / workbench are pointer-driven.
        if self.comms_input_focused() {
            self.on_comms_key(key);
        } else if self.omnibar_focused() {
            self.on_omnibar_key(key);
        } else if matches!(
            self.focused_field_kind(),
            Some(crate::ime::FocusedField::KnotEditor)
        ) {
            // The knot editor's source field wraps every edit with undo/redo bookkeeping,
            // so it gets its own handler rather than the generic field dispatch below.
            self.on_knot_editor_key(key);
            // Re-detect / re-filter the `/` slash + `[[` completion popup after every key.
            self.refresh_knot_completion();
        } else if self.multi.focus(self.view.projection_id).is_some()
            || matches!(key, WinitKey::Named(WinitNamedKey::Tab))
        {
            // A non-field focusable holds focus (a pane button/row reached via Tab), or Tab
            // starts the traversal from nothing: route to the runner, which applies the
            // keyboard defaults (Tab/Shift+Tab traversal, Enter/Space activation) over the
            // focusable set, so focus moves on through the chrome and folded panes and the
            // focused control activates. (Phase 1, step 3c.)
            if let Some(key_event) = key_event_from_winit(key, self.view.modifiers) {
                self.multi.dispatch_key(self.view.projection_id, key_event);
                // Enter/Space synthesizes a click on the focused control, queuing its intent
                // the same way a pointer click does, so drain + apply it. (Phase 1, step 3c.)
                self.drain_chrome_intents();
                self.view.request_redraw();
            }
        } else {
            // No chrome field or focusable element holds focus: graph-level keys act on
            // the orrery selection.
            self.on_graph_key(key);
        }
    }

    /// Route a key to the focused knot-editor source field, wrapping the buffer edit with
    /// undo/redo bookkeeping. Ctrl+Z undoes and Ctrl+Y / Ctrl+Shift+Z redo; a mutating key
    /// first snapshots the pre-edit source (a typing run coalesced into one entry), then the
    /// key dispatches to the field like any other. (Djot editor — Phase 2 undo/redo.)
    fn on_knot_editor_key(&mut self, key: &WinitKey) {
        let ctrl = self.view.modifiers.ctrl || self.view.modifiers.meta;
        // While a completion popup is open, arrows navigate it, Enter/Tab accept the
        // highlighted item, Escape closes; anything else edits (and re-filters after).
        // (Phase 3 completion.)
        if self.chrome().knot_completion.is_some() {
            match key {
                WinitKey::Named(WinitNamedKey::ArrowDown) => {
                    self.chrome_update(|c| c.move_knot_completion(1));
                    self.view.request_redraw();
                    return;
                }
                WinitKey::Named(WinitNamedKey::ArrowUp) => {
                    self.chrome_update(|c| c.move_knot_completion(-1));
                    self.view.request_redraw();
                    return;
                }
                WinitKey::Named(WinitNamedKey::Enter | WinitNamedKey::Tab) => {
                    let sel = self
                        .chrome()
                        .knot_completion
                        .as_ref()
                        .map(|k| k.selected)
                        .unwrap_or(0);
                    self.chrome_update(|c| c.accept_knot_completion(sel));
                    self.view.request_redraw();
                    return;
                }
                WinitKey::Named(WinitNamedKey::Escape) => {
                    self.chrome_update(|c| c.close_knot_completion());
                    self.view.request_redraw();
                    return;
                }
                _ => {}
            }
        }
        // Undo / redo take the key before it can edit the buffer.
        if ctrl && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("z")) {
            let redo = self.view.modifiers.shift;
            self.chrome_update(|c| {
                if redo {
                    c.knot_redo_apply();
                } else {
                    c.knot_undo_apply();
                }
            });
            self.view.request_redraw();
            return;
        }
        if ctrl && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("y")) {
            self.chrome_update(|c| {
                c.knot_redo_apply();
            });
            self.view.request_redraw();
            return;
        }
        // Structural selection: Alt+Up grows the selection to the enclosing container,
        // Alt+Down shrinks back. (Phase 3.)
        if self.view.modifiers.alt {
            let grow = match key {
                WinitKey::Named(WinitNamedKey::ArrowUp) => Some(true),
                WinitKey::Named(WinitNamedKey::ArrowDown) => Some(false),
                _ => None,
            };
            if let Some(grow) = grow {
                self.chrome_update(|c| {
                    if grow {
                        c.grow_selection();
                    } else {
                        c.shrink_selection();
                    }
                });
                self.view.request_redraw();
                return;
            }
        }
        // Smart list continuation: Enter in a list item continues (or ends) the list; a
        // non-list Enter falls through to a plain newline. (Phase 3.)
        if !ctrl && matches!(key, WinitKey::Named(WinitNamedKey::Enter)) {
            let mut handled = false;
            self.chrome_update(|c| {
                c.knot_edit_snapshot(false);
                handled = c.continue_list_on_enter();
            });
            if !handled {
                if let Some(ev) = key_event_from_winit(key, self.view.modifiers) {
                    self.multi.dispatch_key(self.view.projection_id, ev);
                }
            }
            self.view.request_redraw();
            return;
        }
        // Auto-pair: a wrapping delimiter typed over a selection wraps it (kept selected, so
        // wraps nest). No selection / not a pair char falls through to a normal insert. (Phase 3.)
        if !ctrl {
            if let WinitKey::Character(s) = key {
                let mut ch = s.chars();
                if let (Some(open), None) = (ch.next(), ch.next()) {
                    let mut wrapped = false;
                    self.chrome_update(|c| wrapped = c.wrap_selection_if_pair(open));
                    if wrapped {
                        self.view.request_redraw();
                        return;
                    }
                }
            }
        }
        // Snapshot before a mutating edit, classifying for undo grouping. A caret move ends
        // the coalescing run (so the next insert is a fresh group) without snapshotting.
        match key {
            // Ctrl+<char> (e.g. select-all) is not a text edit; just break the run + dispatch.
            WinitKey::Character(_) if ctrl => self.chrome_update(|c| c.knot_break_coalesce()),
            WinitKey::Character(_) | WinitKey::Named(WinitNamedKey::Space) => {
                self.chrome_update(|c| c.knot_edit_snapshot(true));
            }
            WinitKey::Named(WinitNamedKey::Backspace | WinitNamedKey::Delete) => {
                self.chrome_update(|c| c.knot_edit_snapshot(false))
            }
            WinitKey::Named(
                WinitNamedKey::ArrowLeft
                | WinitNamedKey::ArrowRight
                | WinitNamedKey::Home
                | WinitNamedKey::End,
            ) => self.chrome_update(|c| c.knot_break_coalesce()),
            _ => {}
        }
        // Apply the key to the field (mutating knot_source), the same dispatch the generic
        // focusable path uses. Live-on-change render refresh then re-renders the tile.
        if let Some(key_event) = key_event_from_winit(key, self.view.modifiers) {
            self.multi.dispatch_key(self.view.projection_id, key_event);
            self.view.request_redraw();
        }
    }

    /// Keys handled when no chrome field is focused (the graph has the keyboard).
    /// `Delete` / `Backspace` removes the selection: a selected edge's relation is
    /// retracted; otherwise the focused node is deleted (the same as the legacy
    /// Ctrl+Backspace, now reachable with a bare `Delete`).
    pub(crate) fn on_graph_key(&mut self, key: &WinitKey) {
        if matches!(
            key,
            WinitKey::Named(WinitNamedKey::Delete) | WinitKey::Named(WinitNamedKey::Backspace)
        ) {
            if self.orrery().has_selected_edges() {
                if self.orrery_mut().retract_selected_relation() > 0 {
                    self.save_session();
                    self.view.request_redraw();
                }
            } else if self.focused_member().is_some() {
                self.delete_focused_node();
            }
        }
        // Space pauses / resumes the layout physics, so you can freeze the graph
        // mid-settle (or let a field's pull keep running). (Physics pause.)
        if matches!(key, WinitKey::Named(WinitNamedKey::Space)) {
            self.orrery_mut().toggle_physics_paused();
            self.view.request_redraw();
        }
    }
}
