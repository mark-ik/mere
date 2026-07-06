/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Chrome: session, location, suggestions, palette, find, command dispatch.

use super::*;

impl Chrome {
    /// A chrome state seeded with `initial_location` (classified the same way a
    /// submission is, so a bare host normalizes) across the history, the reused
    /// toolbar session state, and the live omnibar buffer.
    pub fn new(initial_location: impl Into<String>) -> Self {
        let raw = initial_location.into();
        // A blank cold-start stays blank (not a search for the empty string).
        let location = if raw.trim().is_empty() {
            String::new()
        } else {
            nav::classify(&raw).resolve()
        };
        Self {
            toolbar: ToolbarState::with_initial_location(location.clone()),
            omnibar: TextInput::new(location.clone()),
            history: History::new(location.clone()),
            content_location: location,
            history_step: None,
            physics_toggle: false,
            physics_paused: false,
            open_as_new_node: false,
            suggest: Vec::new(),
            suggest_active: None,
            palette_open: false,
            palette: CommandPaletteSession::default(),
            palette_input: TextInput::new(""),
            find_open: false,
            find_input: TextInput::new(""),
            find_active: 0,
            find_count: 0,
            pending_connect: None,
            pending_command: None,
            settings: Settings::default(),
            context_menu: None,
            tear_ghost: None,
            branch_label: None,
            pending_context: None,
            pending_palette_action: None,
            comms: CommsPane::new(),
            comms_draft: TextInput::new(""),
            comms_intent: None,
            comms_new_to: TextInput::new(""),
            comms_new_body: TextInput::new(""),
            shellbar_panes: ShellbarPaneStates::default(),
            shellbar_edge: ShellbarEdge::default(),
            shellbar_hidden: false,
            slim: false,
            knot_source: TextInput::new(""),
            knot_editor_open: false,
            knot_target: None,
            knot_editor_label: String::new(),
            knot_editor_rect: None,
            knot_save_requested: false,
            sessions: Vec::new(),
            sessions_overflow_open: false,
            session_intent: None,
        }
    }

    /// A session chip click: activate it (the host drains to a switch, or open-beside
    /// when Shift is held). Closes the overflow dropdown. (Chrome bar P4.)
    pub fn pick_session(&mut self, id: SessionId) {
        self.session_intent = Some(SessionIntent::Activate(id));
        self.sessions_overflow_open = false;
    }

    /// A chip's × : close that session. (Chrome bar P4.)
    pub fn request_close_session(&mut self, id: SessionId) {
        self.session_intent = Some(SessionIntent::Close(id));
        self.sessions_overflow_open = false;
    }

    /// The session strip's `+` : mint a new session. (Chrome bar P4.)
    pub fn request_create_session(&mut self) {
        self.session_intent = Some(SessionIntent::Create);
    }

    /// Toggle the `+N ⌄` overflow dropdown of sessions past the inline cap. (Chrome bar P4.)
    pub fn toggle_sessions_overflow(&mut self) {
        self.sessions_overflow_open = !self.sessions_overflow_open;
    }

    /// The URL the content root should display. The host's `sync_orrery` reads
    /// this and navigates the focused node to it; the chrome never renders it. Set
    /// by `submit_omnibar` / `navigate_suggestion` (fresh nav) and by the host on
    /// a back/forward step. Independent of [`History`] (the suggestions log).
    pub fn content_location(&self) -> &str {
        &self.content_location
    }

    /// Display `url` as the current location (toolbar text + omnibar) without
    /// touching history — the omnibar follows the focused tile / node. The host
    /// gates this so it does not clobber the omnibar mid-edit.
    pub fn show_location(&mut self, url: &str) {
        self.toolbar.editable.location = url.to_string();
        self.toolbar.editable.location_dirty = false;
        self.toolbar.editable.location_submitted = false;
        self.omnibar = TextInput::new(url.to_string());
        self.close_suggestions();
    }

    /// Regenerate the omnibar suggestions from the current omnibar text and the
    /// history, resetting the highlight. The host calls this after each omnibar
    /// edit (keystroke / caret move).
    pub fn refresh_suggestions(&mut self) {
        self.suggest = suggest::suggestions(self.omnibar.text(), &self.history);
        self.suggest_active = None;
        self.refresh_ghost();
    }

    /// Recompute the omnibar's inline ghost completion. In command mode
    /// (`>token`) the best-matching command verb / query completes the partial
    /// token, shown dim until → / Tab accept it (the same `Command` vocabulary the
    /// palette lists); anything else clears the ghost. Only a clean token at the
    /// buffer end gets a ghost, so accepting — which appends — always lands right.
    pub fn refresh_ghost(&mut self) {
        let ghost = match nav::classify(self.omnibar.text()) {
            nav::NavTarget::Command(expr) if self.omnibar.text().ends_with(&expr) => {
                shell_eval::complete(&expr)
                    .map(|full| full[expr.len()..].to_string())
                    .unwrap_or_default()
            }
            _ => String::new(),
        };
        self.omnibar.set_ghost(ghost);
    }

    /// Move the suggestion highlight by `delta` (wrapping), opening the
    /// highlight at the first/last row when nothing is highlighted yet. A no-op
    /// when there are no suggestions.
    pub fn step_suggestion(&mut self, delta: isize) {
        let count = self.suggest.len();
        if count == 0 {
            self.suggest_active = None;
            return;
        }
        let n = count as isize;
        self.suggest_active = Some(match self.suggest_active {
            Some(cur) => ((cur as isize + delta).rem_euclid(n)) as usize,
            None if delta >= 0 => 0,
            None => (n - 1) as usize,
        });
    }

    /// Close the suggestion dropdown without navigating.
    pub fn close_suggestions(&mut self) {
        self.suggest.clear();
        self.suggest_active = None;
    }

    /// Navigate to suggestion row `i` (clicked or Enter-on-highlight): resolve
    /// its URL, push history, and mirror into the toolbar + omnibar.
    pub fn navigate_suggestion(&mut self, i: usize) {
        if let Some(url) = self.suggest.get(i).and_then(suggest::resolve_match) {
            self.content_location = url.clone();
            self.history.visit(url);
            sync_chrome_from_history(self, true);
        }
    }

    /// Navigate the content to a fully-resolved link URL — a click on a link inside
    /// a rendered content card. Records the visit and mirrors it into the toolbar +
    /// omnibar, the same record-the-visit path as a suggestion; the host resolves the
    /// href (relative links join the card's own URL) before calling. (Inline-link nav.)
    pub fn follow_link(&mut self, url: String) {
        self.content_location = url.clone();
        self.history.visit(url);
        sync_chrome_from_history(self, true);
    }

    /// Toggle the command palette open/closed.
    pub fn toggle_palette(&mut self) {
        if self.palette_open {
            self.close_palette();
        } else {
            self.open_palette();
        }
    }

    /// Open the command palette: arm the reused session (fresh query, no
    /// selection), clear the editing buffer, and close any omnibar dropdown.
    pub fn open_palette(&mut self) {
        self.palette_open = true;
        self.palette.open_fresh(SearchPaletteScope::default());
        self.palette_input = TextInput::new("");
        self.close_suggestions();
    }

    /// Close the command palette without running anything.
    pub fn close_palette(&mut self) {
        self.palette_open = false;
        self.palette.selected_index = None;
    }

    /// Toggle the find-in-page bar open/closed.
    pub fn toggle_find(&mut self) {
        if self.find_open {
            self.close_find();
        } else {
            self.open_find();
        }
    }

    /// Open the find bar: a fresh empty query, the active match reset, the omnibar
    /// dropdown closed.
    pub fn open_find(&mut self) {
        self.find_open = true;
        self.find_input = TextInput::new("");
        self.find_active = 0;
        self.close_suggestions();
    }

    /// Close the find bar (the host clears the actor's matches separately).
    pub fn close_find(&mut self) {
        self.find_open = false;
    }

    /// The commands matching the current palette query.
    pub fn palette_commands(&self) -> Vec<Command> {
        command::filter(self.palette_input.text())
    }

    /// The palette's unified items for the current query — commands plus the
    /// palette-exposed context actions. The registry-driven list the palette renders,
    /// steps, and runs (P2: "every action in the palette"). (Command registry P2.)
    pub fn palette_items(&self) -> Vec<PaletteItem> {
        command::palette_items(self.palette_input.text())
    }

    /// Mirror the edited palette buffer into the reused session query and reset
    /// the selection (the filtered list just changed). Called after each edit.
    pub fn sync_palette_query(&mut self) {
        self.palette.query = self.palette_input.text().to_string();
        self.palette.selected_index = None;
    }

    /// Move the palette selection by `delta`, wrapping within the filtered
    /// items — the reused [`CommandPaletteSession::step_selection`] cursor.
    pub fn step_palette(&mut self, delta: isize) {
        let count = self.palette_items().len();
        self.palette.step_selection(delta, count);
    }

    /// Run the highlighted palette item (or the first, if none is highlighted) and
    /// close. A no-op close when nothing matches.
    pub fn run_palette_selection(&mut self) {
        let items = self.palette_items();
        let pick = self.palette.selected_index.unwrap_or(0);
        if let Some(&item) = items.get(pick) {
            self.run_palette_item(item);
        }
        self.close_palette();
    }

    /// Dispatch a palette item by kind: a command runs through `run_command`; a context
    /// action is recorded for the host to apply to the current selection (the host seeds
    /// `context_set` + drains it). (Command registry P2.)
    fn run_palette_item(&mut self, item: PaletteItem) {
        match item {
            PaletteItem::Command(cmd) => self.run_command(cmd),
            PaletteItem::Context(action) => self.pending_palette_action = Some(action),
        }
    }

    /// Run a clicked palette item and close the palette (the click counterpart to
    /// [`run_palette_selection`](Self::run_palette_selection)). (Command registry P2.)
    pub fn run_palette_item_and_close(&mut self, item: PaletteItem) {
        self.run_palette_item(item);
        self.close_palette();
    }

    /// Run `cmd` (e.g. a clicked palette row) and close the palette.
    pub fn run_command_and_close(&mut self, cmd: Command) {
        self.run_command(cmd);
        self.close_palette();
    }

    /// Run `cmd` as a menu / programmatic intent — the pub entry the configurable context menu's
    /// [`RunCommand`](ContextAction::RunCommand) action drains through. No palette involvement.
    /// (Command registry P4.)
    pub fn run_command_intent(&mut self, cmd: Command) {
        self.run_command(cmd);
    }

    /// Apply a command to the chrome state.
    pub(crate) fn run_command(&mut self, cmd: Command) {
        match cmd {
            // Back / forward step the focused node's own history, not a
            // chrome-global one — record the intent; the host drains it via the
            // orrery (the chrome cannot reach it).
            Command::Back => self.history_step = Some(HistoryStep::Back),
            Command::Forward => self.history_step = Some(HistoryStep::Forward),
            Command::Home => {
                let url = "mere://welcome".to_string();
                self.content_location = url.clone();
                self.history.visit(url);
                sync_chrome_from_history(self, true);
            }
            Command::ConnectPeer => {
                // Record the intent; the host executes it (the chrome cannot reach
                // the sync actor). The ticket is whatever is in the address bar.
                self.pending_connect = Some(self.omnibar.text().trim().to_string());
            }
            // Chrome-level: toggle the docked comms pane in place.
            Command::ToggleComms => self.toggle_comms(),
            // Settings now opens as a workbench tile (the pelt settings lane), so it is a
            // host action like the other pane toggles, not the chrome overlay it was in P0.
            // (Settings lane P1.)
            Command::OpenSettings
            | Command::OpenNodeSettings
            | Command::ToggleWorkbench
            | Command::ToggleRoster
            | Command::ToggleGloss
            | Command::ToggleApparatus
            | Command::DeleteNode
            | Command::BackgroundNode
            | Command::HideSelectedEdge
            | Command::ShowAllEdges
            | Command::ToggleProjection
            | Command::ToggleInspector
            | Command::ToggleTrail
            | Command::ToggleSteward
            | Command::ToggleAlembic
            | Command::RetryFocusedContent
            | Command::StopFocusedOperation
            | Command::PinFocusedOperation
            | Command::ToggleCompatView
            | Command::AssertEdge
            | Command::RetractEdge
            | Command::CloseGraphPane
            | Command::ExportGraph
            | Command::SaveGraphEngram
            | Command::MaterializeFocused
            | Command::ToggleKnotEditor
            | Command::ClipFocused
            | Command::CrawlFocused
            | Command::StopCrawl
            | Command::ToggleShellbar => {
                // Host actions over the frame, orrery, workbench, or actor pool:
                // record the intent; the host drains it and runs the matching
                // method.
                self.pending_command = Some(cmd);
            }
        }
    }
}
