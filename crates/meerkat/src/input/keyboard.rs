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
        if self.view.chrome().context_menu.is_some()
            && matches!(key, WinitKey::Named(WinitNamedKey::Escape))
        {
            if self.view.chrome().context_menu.as_ref().is_some_and(|m| m.submenu.is_some()) {
                self.view.chrome_update(Chrome::close_submenu);
                self.view.request_redraw();
            } else {
                self.close_context_menu();
            }
            return;
        }
        // Escape closes an open object card (after a menu, which eats Escape first). A
        // click-away already closes it: an empty-canvas click clears the selection, and the
        // card drops once focus leaves its member. (Object card — explicit close.)
        if self.view.object_card.is_some()
            && matches!(key, WinitKey::Named(WinitNamedKey::Escape))
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
            self.view.runner.set_focus(omnibar);
            self.view.chrome_update(|c| c.omnibar.select_all());
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
        if self.view.modifiers.alt
            && matches!(key, WinitKey::Named(WinitNamedKey::ArrowLeft))
        {
            self.view.chrome_update(|c| c.history_step = Some(HistoryStep::Back));
            self.drain_history_step();
            return;
        }
        if self.view.modifiers.alt
            && matches!(key, WinitKey::Named(WinitNamedKey::ArrowRight))
        {
            self.view.chrome_update(|c| c.history_step = Some(HistoryStep::Forward));
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
            self.view.chrome_update(Chrome::toggle_comms);
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
        if self.handle_clipboard_shortcut(key) {
            return;
        }
        if self.view.chrome().palette_open {
            self.on_palette_key(key);
            return;
        }
        // An open context menu owns the keyboard (arrow nav + Enter/Escape), like the palette.
        if self.view.chrome().context_menu.is_some() {
            self.on_context_menu_key(key);
            return;
        }
        if self.view.chrome().find_open {
            self.on_find_key(key);
            return;
        }
        // Soft-wrap visual-line nav for the focused knot-editor textarea: ArrowUp/ArrowDown
        // move by parley's wrapped rows (with a sticky goal column) instead of the buffer
        // handler's hard-`\n`-line move, before the field handlers route the key. A no-op for
        // any other focus, so omnibar suggestion nav / comms field moves fall through. (Soft-
        // wrap caret nav.)
        if let WinitKey::Named(named) = key {
            let delta = match named {
                WinitNamedKey::ArrowUp => Some(-1),
                WinitNamedKey::ArrowDown => Some(1),
                _ => None,
            };
            if let Some(delta) = delta {
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
        tracing::info!(target: "meerkat", key = ?key, omnibar_focused = self.omnibar_focused(), focus = ?self.view.runner.focus(), toolbar = ?self.input_under_class("toolbar"), suggest_open = !self.view.chrome().suggest.is_empty(), "KEY DISPATCH probe");
        if self.comms_input_focused() {
            self.on_comms_key(key);
        } else if self.omnibar_focused() {
            self.on_omnibar_key(key);
        } else if self.view.runner.focus().is_some()
            || matches!(key, WinitKey::Named(WinitNamedKey::Tab))
        {
            // A non-field focusable holds focus (a pane button/row reached via Tab), or Tab
            // starts the traversal from nothing: route to the runner, which applies the
            // keyboard defaults (Tab/Shift+Tab traversal, Enter/Space activation) over the
            // focusable set, so focus moves on through the chrome and folded panes and the
            // focused control activates. (Phase 1, step 3c.)
            if let Some(key_event) = key_event_from_winit(key, self.view.modifiers) {
                self.view.runner.dispatch_key(key_event);
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
