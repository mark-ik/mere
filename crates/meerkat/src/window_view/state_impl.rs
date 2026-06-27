/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! WindowView methods (chrome access, dispatch, lifecycle).

use super::*;

impl WindowView {
    /// The chrome view-state, read-only. Phase 1 insulation seam: call sites read the
    /// chrome through this rather than `runner.state()` directly, so the runner's state
    /// type can later widen from `Chrome` to a composed shell state without touching
    /// them. (Unified document host — Phase 1.)
    pub(crate) fn chrome(&self) -> &Chrome {
        &self.runner.state().chrome
    }

    /// Mutate the chrome view-state; the runner re-renders and diffs the change. The
    /// mutation counterpart to [`chrome`](Self::chrome), and the same insulation seam.
    pub(crate) fn chrome_update(&mut self, f: impl FnOnce(&mut Chrome)) {
        self.runner.update(|s| f(&mut s.chrome));
    }

    /// Replace the orrery element's node cards with a fresh gyre snapshot. The runner
    /// re-renders, re-placing the cards by their transforms on the RepaintOnly path.
    /// (Orrery-as-element — Phase 2.)
    pub(crate) fn set_orrery(&mut self, render: OrreryRender) {
        self.runner.update(|s| s.orrery = render);
    }

    /// The orrery render snapshot currently in the shell state, so the host can skip
    /// the rebuild when a fresh snapshot is identical (a settled orrery costs no
    /// per-frame view re-run). (Orrery-as-element — Phase 2.)
    pub(crate) fn orrery_render(&self) -> &OrreryRender {
        &self.runner.state().orrery
    }

    /// Drain the activation keys the object card's widget controls queued, for the host to
    /// dispatch to its member. (Object card — P1.)
    pub(crate) fn take_node_card_keys(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        self.runner.update(|s| out = std::mem::take(&mut s.node_card_keys));
        out
    }

    /// Drain the most recent orrery wheel delta the pane element's `on_wheel` queued (device px),
    /// for the host to apply to gyre's pan / Ctrl-zoom. (cond 5 input bridge.)
    pub(crate) fn take_orrery_wheel(&mut self) -> Option<(f32, f32)> {
        let mut out = None;
        self.runner.update(|s| out = s.orrery_wheel.take());
        out
    }

    /// Fold the roster pane into the shell document: replace its rows + window rect (or
    /// `None` to close it). The runner re-renders, diffing the roster subtree into the one
    /// shell DOM, so it lays out, hit-tests, and projects a11y with the chrome. (Phase 1.)
    pub(crate) fn set_roster(
        &mut self,
        rows: Vec<crate::roster::RosterRow>,
        field_rows: Vec<crate::roster::FieldRow>,
        rect: Option<[f32; 4]>,
    ) {
        self.runner.update(|s| {
            s.roster.rows = rows;
            s.roster.field_rows = field_rows;
            s.roster_rect = rect;
        });
    }

    /// Whether the roster subtree is currently in the shell document (the pane is open),
    /// so the host can skip the per-frame `set_roster` while it stays closed. (Phase 1.)
    pub(crate) fn roster_open(&self) -> bool {
        self.runner.state().roster_rect.is_some()
    }

    /// Drain the selections / field intents the roster's row handlers queued through the
    /// shell runner's dispatch, for the host to apply. (Phase 1.)
    pub(crate) fn take_roster_intents(&mut self) -> Vec<crate::roster_view::RosterIntent> {
        let mut out = Vec::new();
        self.runner.update(|s| out = std::mem::take(&mut s.roster.pending));
        out
    }

    /// Fold a list pane (apparatus / steward / inspector / trail) into the shell document:
    /// set its root class + items + window rect (or `None` to close it). The runner
    /// re-renders, diffing the pane subtree into the one shell DOM so it lays out, scrolls,
    /// and dispatches button clicks with the chrome. (Phase 1, step 2.)
    pub(crate) fn set_list_pane(
        &mut self,
        which: ShellListPane,
        root_class: &str,
        items: Vec<PaneItem>,
        rect: Option<[f32; 4]>,
    ) {
        let i = which.idx();
        let root_class = root_class.to_string();
        self.runner.update(|s| {
            s.panes[i].root_class = root_class;
            s.panes[i].items = items;
            s.pane_rects[i] = rect;
        });
    }

    /// Whether a list pane's subtree is currently in the shell document (the pane is open),
    /// so the host can skip the per-frame `set_list_pane` while it stays closed. (Phase 1.)
    pub(crate) fn list_pane_open(&self, which: ShellListPane) -> bool {
        self.runner.state().pane_rects[which.idx()].is_some()
    }

    /// Drain the activation keys a list pane's button handlers queued through the shell
    /// runner's dispatch, for the host to apply (theme / engine / physics / recover).
    /// (Phase 1, step 2.)
    pub(crate) fn take_list_pane_activations(&mut self, which: ShellListPane) -> Vec<String> {
        let i = which.idx();
        let mut out = Vec::new();
        self.runner.update(|s| out = std::mem::take(&mut s.panes[i].pending));
        out
    }

    /// Replace the open settings tiles' rendered state (one [`SettingsPane`] per open
    /// `settings://` tile) + the panel background, folding them into the shell document.
    /// Empty `panes` keeps the subtree out. (Settings lane P1.)
    pub(crate) fn set_settings_panes(&mut self, panes: Vec<SettingsPane>, panel_bg: String) {
        self.runner.update(|s| {
            s.settings.panes = panes;
            s.settings.panel_bg = panel_bg;
        });
    }

    /// Whether any settings tile subtree is currently in the shell document, so the host
    /// can skip the per-frame `set_settings_panes` while none are open. (Settings lane P1.)
    pub(crate) fn settings_panes_open(&self) -> bool {
        !self.runner.state().settings.panes.is_empty()
    }

    /// Drain the `(member, key)` page-control activations the settings panes queued, for the
    /// host to apply like an apparatus activation (theme / engine / physics). (Settings lane P1.)
    pub(crate) fn take_settings_pane_keys(&mut self) -> Vec<(GraphMemberId, String)> {
        let mut out = Vec::new();
        self.runner.update(|s| out = std::mem::take(&mut s.settings.pending_keys));
        out
    }

    /// Drain the `(member, url)` spine navigations the settings panes queued, for the host
    /// to retarget that settings tile's node to the chosen page. (Settings lane P1.)
    pub(crate) fn take_settings_pane_nav(&mut self) -> Vec<(GraphMemberId, String)> {
        let mut out = Vec::new();
        self.runner.update(|s| out = std::mem::take(&mut s.settings.pending_nav));
        out
    }

    /// Mint a window's view over a fresh pair of serval runners. Everything else
    /// starts at its rest value (empty caches, no in-progress gesture, the default
    /// 1024×600 surface); the caller overrides the view-session bits it restored
    /// (`centered`, `content_location`, `frame_layout`, `next_pane_id`). A second
    /// window is a second `new(...)` over the same shared session. (MW2.)
    pub(crate) fn new(
        kind: WindowKind,
        focused_graph: GraphId,
        dom: Rc<RefCell<ScriptedDom>>,
        runner: ShellRunner,
        workbench: Workbench,
    ) -> Self {
        Self {
            kind,
            focused_graph,
            viewports: HashMap::new(),
            selections: HashMap::new(),
            dom,
            runner,
            chrome_session: None,
            workbench,
            pelt_shell: None,
            pelt_theme: None,
            session_row_rects: Default::default(),
            session_close_rects: Default::default(),
            session_add_rect: Default::default(),
            gloss_node_rects: Default::default(),
            gloss_recent_rects: Default::default(),
            tile_rects: Default::default(),
            content_rects: Default::default(),
            settings_rects: Default::default(),
            script_caps: None,
            find_matches: Default::default(),
            find_member: None,
            find_gen: 0,
            tile_textures: Default::default(),
            tile_bands: Default::default(),
            snapshot_data_uris: Default::default(),
            window_controls_tex: Default::default(),
            divider_tex: Default::default(),
            scroll: Default::default(),
            soft_wrap_goal: None,
            last_left_release: Default::default(),
            workbench_gesture: false,
            frame_divider_drag: Default::default(),
            resize_drag: Default::default(),
            titlebar_press: Default::default(),
            swatch_drag: Default::default(),
            row_reorder_drag: Default::default(),
            tear_out_drag: Default::default(),
            branch_graphlet: None,
            cursor_icon: Default::default(),
            pending_exit: Default::default(),
            context_set: Default::default(),
            context_origin: Default::default(),
            context_link: Default::default(),
            object_card: Default::default(),
            context_field: Default::default(),
            renaming: Default::default(),
            tagging: Default::default(),
            centered: Default::default(),
            healed: Default::default(),
            content_location: Default::default(),
            shown_location: Default::default(),
            focused_tile: Default::default(),
            frame_layout: Default::default(),
            next_pane_id: Default::default(),
            mirror_tiles: false,
            maximized_pane: Default::default(),
            active_content: Default::default(),
            window: Default::default(),
            surface: Default::default(),
            toolbar_h: Default::default(),
            width: 1024,
            height: 600,
            cursor: Default::default(),
            modifiers: Default::default(),
            scrying: Default::default(),
            scrying_rects: Default::default(),
            scrying_input_focus: Default::default(),
        }
    }

    /// Request a redraw of this window, if it exists. A per-window operation: each
    /// window drives its own surface, so the registry's event handlers call this on
    /// the target view directly. (MW2 (c).)
    pub(crate) fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}
