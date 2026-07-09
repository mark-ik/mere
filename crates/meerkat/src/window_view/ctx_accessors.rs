/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `WindowCtx` view-state accessors — the choke point for the one-state-N-windows flip.
//!
//! Every per-window view-state read/write funnels through these methods. They route into
//! the shared [`ShellMultiRunner`](super::ShellMultiRunner) (`Shell.multi`) by the window's
//! `projection_id`: a read borrows `multi.state().windows[pid]`, a write is a
//! `multi.update_local(pid, …)` (rebuild just this window's projection). The ~200 call
//! sites reach these off `WindowCtx` and never touch the runner directly, so the state
//! ownership lives in one place. A change to *shared* chrome (sync / crawl) instead uses
//! `multi.update` at its call site, rebuilding every window. (One state, N windows —
//! Slice 3.)

use super::*;

impl crate::WindowCtx<'_> {
    pub(crate) fn chrome(&self) -> &Chrome {
        &self.multi.state().windows[self.view.projection_id.0].chrome
    }

    pub(crate) fn chrome_update(&mut self, f: impl FnOnce(&mut Chrome)) {
        let pid = self.view.projection_id;
        self.multi
            .update_local(pid, |app| f(&mut app.windows[pid.0].chrome));
    }

    pub(crate) fn set_orrery(&mut self, render: OrreryRender) {
        let pid = self.view.projection_id;
        self.multi
            .update_local(pid, |app| app.windows[pid.0].orrery = render);
    }

    pub(crate) fn orrery_render(&self) -> &OrreryRender {
        &self.multi.state().windows[self.view.projection_id.0].orrery
    }

    pub(crate) fn take_object_card_keys(&mut self) -> Vec<String> {
        let pid = self.view.projection_id;
        let mut out = Vec::new();
        self.multi.update_local(pid, |app| {
            out = std::mem::take(&mut app.windows[pid.0].object_card_keys)
        });
        out
    }

    pub(crate) fn take_orrery_wheel(&mut self) -> Option<(f32, f32)> {
        let pid = self.view.projection_id;
        let mut out = None;
        self.multi
            .update_local(pid, |app| out = app.windows[pid.0].orrery_wheel.take());
        out
    }

    pub(crate) fn set_roster(
        &mut self,
        snapshot: crate::roster::RosterSnapshot,
        rect: Option<[f32; 4]>,
    ) {
        let pid = self.view.projection_id;
        self.multi.update_local(pid, |app| {
            let wl = &mut app.windows[pid.0];
            wl.roster.node_rows = snapshot.node_rows;
            wl.roster.link_rows = snapshot.link_rows;
            wl.roster.graphlet_rows = snapshot.graphlet_rows;
            wl.roster.field_rows = snapshot.field_rows;
            wl.roster.detail = snapshot.detail;
            wl.roster_rect = rect;
        });
    }

    pub(crate) fn roster_subject(&self) -> Option<crate::roster::RosterSubject> {
        self.multi.state().windows[self.view.projection_id.0]
            .roster
            .selected_subject
            .clone()
    }

    pub(crate) fn set_roster_subject(&mut self, subject: Option<crate::roster::RosterSubject>) {
        let pid = self.view.projection_id;
        self.multi.update_local(pid, |app| {
            app.windows[pid.0].roster.selected_subject = subject
        });
    }

    pub(crate) fn set_roster_tab(&mut self, tab: crate::roster::RosterTab) {
        let pid = self.view.projection_id;
        self.multi
            .update_local(pid, |app| app.windows[pid.0].roster.active_tab = tab);
    }

    pub(crate) fn roster_open(&self) -> bool {
        self.multi.state().windows[self.view.projection_id.0]
            .roster_rect
            .is_some()
    }

    pub(crate) fn take_roster_intents(&mut self) -> Vec<crate::roster_view::RosterIntent> {
        let pid = self.view.projection_id;
        let mut out = Vec::new();
        self.multi.update_local(pid, |app| {
            out = std::mem::take(&mut app.windows[pid.0].roster.pending)
        });
        out
    }

    pub(crate) fn set_gloss_outline(
        &mut self,
        snapshot: mere::gloss::GlossOutlineSnapshot,
        rect: Option<[f32; 4]>,
    ) {
        let pid = self.view.projection_id;
        self.multi.update_local(pid, |app| {
            let wl = &mut app.windows[pid.0];
            wl.gloss_outline.rows = snapshot.rows;
            wl.gloss_outline.metrics = snapshot.metrics;
            wl.gloss_outline_rect = rect;
        });
    }

    pub(crate) fn gloss_outline_open(&self) -> bool {
        self.multi.state().windows[self.view.projection_id.0]
            .gloss_outline_rect
            .is_some()
    }

    pub(crate) fn gloss_outline_rect(&self) -> Option<[f32; 4]> {
        self.multi.state().windows[self.view.projection_id.0].gloss_outline_rect
    }

    pub(crate) fn take_gloss_outline_intents(&mut self) -> Vec<mere::gloss::GlossRowIntent> {
        let pid = self.view.projection_id;
        let mut out = Vec::new();
        self.multi.update_local(pid, |app| {
            out = std::mem::take(&mut app.windows[pid.0].gloss_outline.pending)
        });
        out
    }

    pub(crate) fn set_gloss_recent(
        &mut self,
        snapshot: crate::gloss_view::GlossRecentSnapshot,
        rect: Option<[f32; 4]>,
    ) {
        let pid = self.view.projection_id;
        self.multi.update_local(pid, |app| {
            let wl = &mut app.windows[pid.0];
            wl.gloss_recent.rows = snapshot.rows;
            wl.gloss_recent_rect = rect;
        });
    }

    pub(crate) fn gloss_recent_open(&self) -> bool {
        self.multi.state().windows[self.view.projection_id.0]
            .gloss_recent_rect
            .is_some()
    }

    pub(crate) fn take_gloss_recent_intents(&mut self) -> Vec<mere::gloss::GlossRowIntent> {
        let pid = self.view.projection_id;
        let mut out = Vec::new();
        self.multi.update_local(pid, |app| {
            out = std::mem::take(&mut app.windows[pid.0].gloss_recent.pending)
        });
        out
    }

    pub(crate) fn set_gloss_minimap(
        &mut self,
        snapshot: crate::gloss_view::GlossMinimapSnapshot,
        rect: Option<[f32; 4]>,
    ) {
        let pid = self.view.projection_id;
        self.multi.update_local(pid, |app| {
            let wl = &mut app.windows[pid.0];
            wl.gloss_minimap.nodes = snapshot.nodes;
            wl.gloss_minimap.w = snapshot.w;
            wl.gloss_minimap.h = snapshot.h;
            wl.gloss_minimap_rect = rect;
        });
    }

    pub(crate) fn gloss_minimap_open(&self) -> bool {
        self.multi.state().windows[self.view.projection_id.0]
            .gloss_minimap_rect
            .is_some()
    }

    pub(crate) fn take_gloss_minimap_intents(&mut self) -> Vec<mere::gloss::GlossRowIntent> {
        let pid = self.view.projection_id;
        let mut out = Vec::new();
        self.multi.update_local(pid, |app| {
            out = std::mem::take(&mut app.windows[pid.0].gloss_minimap.pending)
        });
        out
    }

    pub(crate) fn set_list_pane(
        &mut self,
        which: ShellListPane,
        root_class: &str,
        items: Vec<PaneItem>,
        rect: Option<[f32; 4]>,
    ) {
        let pid = self.view.projection_id;
        let i = which.idx();
        let root_class = root_class.to_string();
        self.multi.update_local(pid, |app| {
            let wl = &mut app.windows[pid.0];
            wl.panes[i].root_class = root_class;
            wl.panes[i].items = items;
            wl.pane_rects[i] = rect;
        });
    }

    pub(crate) fn list_pane_open(&self, which: ShellListPane) -> bool {
        self.multi.state().windows[self.view.projection_id.0].pane_rects[which.idx()].is_some()
    }

    pub(crate) fn take_list_pane_activations(&mut self, which: ShellListPane) -> Vec<String> {
        let pid = self.view.projection_id;
        let i = which.idx();
        let mut out = Vec::new();
        self.multi.update_local(pid, |app| {
            out = std::mem::take(&mut app.windows[pid.0].panes[i].pending)
        });
        out
    }

    pub(crate) fn set_settings_panes(&mut self, panes: Vec<SettingsPane>, panel_bg: String) {
        let pid = self.view.projection_id;
        self.multi.update_local(pid, |app| {
            let wl = &mut app.windows[pid.0];
            wl.settings.panes = panes;
            wl.settings.panel_bg = panel_bg;
        });
    }

    pub(crate) fn settings_panes_open(&self) -> bool {
        !self.multi.state().windows[self.view.projection_id.0]
            .settings
            .panes
            .is_empty()
    }

    pub(crate) fn take_settings_pane_keys(&mut self) -> Vec<(GraphMemberId, String)> {
        let pid = self.view.projection_id;
        let mut out = Vec::new();
        self.multi.update_local(pid, |app| {
            out = std::mem::take(&mut app.windows[pid.0].settings.pending_keys)
        });
        out
    }

    pub(crate) fn take_settings_pane_nav(&mut self) -> Vec<(GraphMemberId, String)> {
        let pid = self.view.projection_id;
        let mut out = Vec::new();
        self.multi.update_local(pid, |app| {
            out = std::mem::take(&mut app.windows[pid.0].settings.pending_nav)
        });
        out
    }
}
