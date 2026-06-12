/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Typed in-process agent harness for Meerkat.
//!
//! This is deliberately not pixel automation. The harness exposes the same
//! semantic state Apparatus and the internal a11y projection consume, then routes
//! actions through the existing host methods.

#![allow(dead_code)]

use frame::PaneContent;
use meerkat::command::Command;

use super::observability::{A11ySnapshot, ObservabilitySnapshot, Severity};
use super::{Shell, ContentPane};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentObservation {
    pub active_theme_id: String,
    pub focused_node: Option<String>,
    pub active_content: AgentPane,
    pub surfaces: Vec<AgentSurface>,
    pub enabled_actions: Vec<AgentActionDescriptor>,
    pub diagnostics: Vec<AgentDiagnostic>,
    pub probes: Vec<AgentProbe>,
    pub a11y: A11ySnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentSurface {
    pub id: String,
    pub pane: AgentPane,
    pub label: String,
    pub focused: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentActionDescriptor {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentDiagnostic {
    pub channel: String,
    pub severity: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentProbe {
    pub name: String,
    pub status: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentStep {
    pub result: AgentActionResult,
    pub observation: AgentObservation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentActionResult {
    pub applied: bool,
    pub action_id: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AgentAction {
    OpenPane(AgentPane),
    TogglePane(AgentPane),
    InvokeCommand(Command),
    SelectNodeByUrl(String),
    SetTheme(String),
    ActivateFocusedAction,
    DragDivider {
        surface: AgentPane,
        ratio_delta: i32,
    },
    RequestContentPreview,
    RetryFocusedContent,
    StopFocusedOperation,
    PinFocusedOperation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentPane {
    Orrery,
    Workbench,
    Roster,
    Inspector,
    Steward,
    Gloss,
    Comms,
    Apparatus,
}

impl Shell {
    pub(crate) fn agent_observation(&mut self) -> AgentObservation {
        self.ctx().refresh_a11y_summary();
        let snapshot = self.ctx().apparatus_observability();
        self.agent_observation_from_snapshot(snapshot)
    }

    pub(crate) fn apply_agent_action(&mut self, action: AgentAction) -> AgentStep {
        let (applied, action_id, detail) = match action {
            AgentAction::OpenPane(pane) => self.agent_open_pane(pane),
            AgentAction::TogglePane(pane) => self.agent_toggle_pane(pane),
            AgentAction::InvokeCommand(cmd) => self.agent_invoke_command(cmd),
            AgentAction::SelectNodeByUrl(url) => self.agent_select_node_by_url(&url),
            AgentAction::SetTheme(theme_id) => self.agent_set_theme(&theme_id),
            AgentAction::ActivateFocusedAction => self.agent_activate_focused_action(),
            AgentAction::DragDivider {
                surface,
                ratio_delta,
            } => self.agent_drag_divider(surface, ratio_delta),
            AgentAction::RequestContentPreview => self.agent_request_content_preview(),
            AgentAction::RetryFocusedContent => self.agent_retry_focused_content(),
            AgentAction::StopFocusedOperation => self.agent_stop_focused_operation(),
            AgentAction::PinFocusedOperation => self.agent_pin_focused_operation(),
        };
        if applied {
            self.shared.observability.record_diagnostic(
                "meerkat.agent.action_applied",
                Severity::Info,
                format!("{action_id}: {detail}"),
            );
        } else {
            self.shared.observability.record_diagnostic(
                "meerkat.agent.intent_dropped",
                Severity::Warn,
                format!("{action_id}: {detail}"),
            );
        }
        AgentStep {
            result: AgentActionResult {
                applied,
                action_id,
                detail,
            },
            observation: self.agent_observation(),
        }
    }

    fn agent_observation_from_snapshot(&mut self, snapshot: ObservabilitySnapshot) -> AgentObservation {
        // Compute each self-derived value into a local first: the struct literal would
        // otherwise hold several borrows of `self` (ctx, orrery, agent_surfaces) at once.
        let active_theme_id = self.shared.presentation.active_theme_id.clone();
        let focused_node = self.orrery().focused_url().map(str::to_string);
        let active_content = match self.ctx().view.active_content {
            ContentPane::Orrery => AgentPane::Orrery,
            ContentPane::Workbench => AgentPane::Workbench,
        };
        let surfaces = self.agent_surfaces();
        let enabled_actions = self.agent_enabled_actions();
        AgentObservation {
            active_theme_id,
            focused_node,
            active_content,
            surfaces,
            enabled_actions,
            diagnostics: snapshot
                .diagnostics
                .into_iter()
                .map(|record| AgentDiagnostic {
                    channel: record.channel,
                    severity: super::observability::severity_label(record.severity).to_string(),
                    message: record.message,
                })
                .collect(),
            probes: snapshot
                .probes
                .into_iter()
                .map(|record| AgentProbe {
                    name: record.name,
                    status: record.status,
                    detail: record.detail,
                })
                .collect(),
            a11y: snapshot.a11y,
        }
    }

    fn agent_surfaces(&mut self) -> Vec<AgentSurface> {
        // Precompute the per-window reads through the ctx before the loop, so the
        // closure borrows neither `self` nor a live ctx. (MW2 (c).)
        let leaves = self.ctx().laid_leaves();
        let active = self.ctx().view.active_content;
        let wb_open = self.ctx().workbench_open();
        leaves
            .into_iter()
            .map(|leaf| {
                let pane = AgentPane::from_pane_content(&leaf.content);
                let focused = match (active, pane) {
                    (ContentPane::Orrery, AgentPane::Orrery) => true,
                    (ContentPane::Workbench, AgentPane::Workbench) => wb_open,
                    _ => false,
                };
                AgentSurface {
                    id: format!("pane:{}", leaf.pane_id.0),
                    pane,
                    label: leaf.content.tag().to_string(),
                    focused,
                }
            })
            .collect()
    }

    fn agent_enabled_actions(&self) -> Vec<AgentActionDescriptor> {
        let mut actions = vec![
            action("pane.open.apparatus", "Open Apparatus"),
            action("pane.open.roster", "Open Roster"),
            action("pane.open.inspector", "Open Inspector"),
            action("pane.open.steward", "Open Steward"),
            action("pane.open.gloss", "Open Gloss"),
            action("pane.open.comms", "Open Comms"),
            action("theme.set", "Set theme"),
            action("node.select_by_url", "Select node by URL"),
            action("content.preview.request", "Request content preview"),
            action("content.retry.focused", "Retry focused content"),
            action("operation.stop.focused", "Stop focused operation"),
            action("operation.pin.focused", "Pin focused operation"),
        ];
        actions.extend(
            Command::ALL
                .into_iter()
                .map(|cmd| action(format!("command.{cmd:?}").to_ascii_lowercase(), cmd.label())),
        );
        actions
    }

    fn agent_open_pane(&mut self, pane: AgentPane) -> (bool, String, String) {
        let action_id = format!("pane.open.{}", pane.id());
        if pane == AgentPane::Workbench {
            self.ctx().open_workbench();
            return (true, action_id, "workbench open".to_string());
        }
        let Some(content) = pane.toggleable_content() else {
            return (false, action_id, "pane is not summonable".to_string());
        };
        if self.ctx().pane_of_content(&content).is_none() {
            self.ctx().toggle_pane(content);
        }
        (true, action_id, format!("{} open", pane.id()))
    }

    fn agent_toggle_pane(&mut self, pane: AgentPane) -> (bool, String, String) {
        let action_id = format!("pane.toggle.{}", pane.id());
        if pane == AgentPane::Workbench {
            self.ctx().toggle_workbench();
            return (true, action_id, "workbench toggled".to_string());
        }
        let Some(content) = pane.toggleable_content() else {
            return (false, action_id, "pane is not toggleable".to_string());
        };
        self.ctx().toggle_pane(content);
        (true, action_id, format!("{} toggled", pane.id()))
    }

    fn agent_invoke_command(&mut self, cmd: Command) -> (bool, String, String) {
        let action_id = format!("command.{cmd:?}").to_ascii_lowercase();
        self.ctx().view.runner.update(move |chrome| {
            chrome.run_command_and_close(cmd);
        });
        self.ctx().drain_pending_command();
        self.ctx().drain_pending_connect();
        self.ctx().sync_comms_pane();
        self.ctx().sync_settings();
        (true, action_id, cmd.label().to_string())
    }

    fn agent_select_node_by_url(&mut self, url: &str) -> (bool, String, String) {
        let action_id = "node.select_by_url".to_string();
        if !self.orrery_mut().select_by_url(url) {
            return (false, action_id, format!("node not found: {url}"));
        }
        self.ctx().view.active_content = ContentPane::Orrery;
        self.ctx().sync_location();
        self.ctx().refresh_a11y_summary();
        (true, action_id, url.to_string())
    }

    fn agent_set_theme(&mut self, theme_id: &str) -> (bool, String, String) {
        let action_id = "theme.set".to_string();
        self.ctx().set_theme(theme_id);
        (true, action_id, self.shared.presentation.active_theme_id.clone())
    }

    fn agent_activate_focused_action(&mut self) -> (bool, String, String) {
        let action_id = "focus.activate".to_string();
        if self.ctx().view.runner.state().palette_open {
            self.ctx().view.runner.update(meerkat::Chrome::run_palette_selection);
            self.ctx().drain_pending_command();
            return (true, action_id, "palette selection activated".to_string());
        }
        (
            false,
            action_id,
            "no focused activatable action".to_string(),
        )
    }

    fn agent_drag_divider(
        &mut self,
        _surface: AgentPane,
        _ratio_delta: i32,
    ) -> (bool, String, String) {
        (
            false,
            "divider.drag".to_string(),
            "semantic divider targets are not exposed yet".to_string(),
        )
    }

    fn agent_request_content_preview(&mut self) -> (bool, String, String) {
        let action_id = "content.preview.request".to_string();
        if self.ctx().focused_member().is_none() {
            return (false, action_id, "no focused node".to_string());
        }
        self.ctx().toggle_live_preview();
        (true, action_id, "focused node preview toggled".to_string())
    }

    fn agent_retry_focused_content(&mut self) -> (bool, String, String) {
        let action_id = "content.retry.focused".to_string();
        let Some(url) = self.ctx().current_focus_url() else {
            return (false, action_id, "no focused node".to_string());
        };
        if !super::fetch::is_fetchable(&url) {
            return (false, action_id, format!("not fetchable: {url}"));
        }
        self.ctx().retry_focused_content();
        (
            true,
            action_id,
            "focused content retry requested".to_string(),
        )
    }

    fn agent_stop_focused_operation(&mut self) -> (bool, String, String) {
        let action_id = "operation.stop.focused".to_string();
        if self.ctx().focused_member().is_none() {
            return (false, action_id, "no focused node".to_string());
        }
        self.ctx().stop_focused_operation();
        (true, action_id, "focused operation stopped".to_string())
    }

    fn agent_pin_focused_operation(&mut self) -> (bool, String, String) {
        let action_id = "operation.pin.focused".to_string();
        if self.ctx().focused_member().is_none() {
            return (false, action_id, "no focused node".to_string());
        }
        self.ctx().pin_focused_operation();
        (true, action_id, "focused operation pinned".to_string())
    }
}

impl AgentPane {
    fn id(self) -> &'static str {
        match self {
            AgentPane::Orrery => "orrery",
            AgentPane::Workbench => "workbench",
            AgentPane::Roster => "roster",
            AgentPane::Inspector => "inspector",
            AgentPane::Steward => "steward",
            AgentPane::Gloss => "gloss",
            AgentPane::Comms => "comms",
            AgentPane::Apparatus => "apparatus",
        }
    }

    fn toggleable_content(self) -> Option<PaneContent> {
        match self {
            AgentPane::Orrery => None,
            AgentPane::Workbench => Some(PaneContent::Workbench),
            AgentPane::Roster => Some(PaneContent::Roster),
            AgentPane::Inspector => Some(PaneContent::Inspector),
            AgentPane::Steward => Some(PaneContent::Steward),
            AgentPane::Gloss => Some(PaneContent::Gloss),
            AgentPane::Comms => Some(PaneContent::Comms),
            AgentPane::Apparatus => Some(PaneContent::Apparatus),
        }
    }

    fn from_pane_content(content: &PaneContent) -> Self {
        match content {
            PaneContent::Orrery => Self::Orrery,
            PaneContent::Workbench => Self::Workbench,
            PaneContent::Roster => Self::Roster,
            PaneContent::Inspector => Self::Inspector,
            PaneContent::Steward => Self::Steward,
            PaneContent::Gloss => Self::Gloss,
            PaneContent::Comms => Self::Comms,
            PaneContent::Apparatus | PaneContent::System => Self::Apparatus,
            PaneContent::Tile(_) | PaneContent::Custom(_) => Self::Workbench,
        }
    }
}

fn action(id: impl Into<String>, label: impl Into<String>) -> AgentActionDescriptor {
    AgentActionDescriptor {
        id: id.into(),
        label: label.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::Action;
    use register_theme::theme::{THEME_ID_DARK, THEME_ID_LIGHT};
    use std::sync::OnceLock;
    use winit::event_loop::EventLoopProxy;

    fn test_app() -> Shell {
        let (_tx, rx) = std::sync::mpsc::channel();
        Shell::new_with_session_dir(test_proxy(), rx, temp_session_dir())
    }

    fn test_proxy() -> EventLoopProxy<()> {
        static PROXY: OnceLock<EventLoopProxy<()>> = OnceLock::new();
        PROXY
            .get_or_init(|| {
                let mut builder = winit::event_loop::EventLoop::builder();
                #[cfg(target_os = "windows")]
                {
                    use winit::platform::windows::EventLoopBuilderExtWindows;
                    builder.with_any_thread(true);
                }
                let event_loop = builder.build().expect("event loop");
                let proxy = event_loop.create_proxy();
                Box::leak(Box::new(event_loop));
                proxy
            })
            .clone()
    }

    fn temp_session_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join("mere-agent-harness-tests")
            .join(format!(
                "{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn create_session_mints_and_activates_a_new_session() {
        let mut app = test_app();
        let first = app.shared.session.active_session_id;
        assert_eq!(app.shared.session.manifests.len(), 1);
        let second = app.ctx().create_session();
        assert_ne!(second, first, "a fresh session id is minted");
        assert_eq!(app.shared.session.active_session_id, second, "the new session is active");
        assert_eq!(app.shared.session.manifests.len(), 2);
        let dir = app.shared.session.mere_root
            .join("sessions")
            .join(second.as_uuid().to_string());
        assert!(
            dir.join(session_runtime::MANIFEST_FILE).exists(),
            "the new session's manifest is on disk"
        );
    }

    #[test]
    fn switch_session_restores_each_sessions_own_graph() {
        let mut app = test_app();
        let first = app.shared.session.active_session_id;
        // Grow the first session's graph, then create + switch to a fresh second.
        app.orrery_mut().visit("mere://added-to-first");
        let first_count = app.orrery().graph().nodes().count();
        assert!(first_count >= 2, "welcome + the added node");

        let second = app.ctx().create_session();
        // The fresh session is its own (smaller) graph, not the first's.
        assert!(app.orrery().graph().nodes().count() < first_count);

        app.ctx().switch_session(first);
        assert_eq!(app.shared.session.active_session_id, first);
        assert_eq!(
            app.orrery().graph().nodes().count(),
            first_count,
            "the first session's graph was restored intact"
        );

        app.ctx().switch_session(second);
        assert_eq!(app.shared.session.active_session_id, second);
    }

    #[test]
    fn cycle_session_wraps_through_the_open_sessions() {
        let mut app = test_app();
        let a = app.shared.session.active_session_id;
        let b = app.ctx().create_session(); // active = b, two sessions
        app.ctx().cycle_session(true);
        assert_eq!(app.shared.session.active_session_id, a, "wrapped forward to the other session");
        app.ctx().cycle_session(true);
        assert_eq!(app.shared.session.active_session_id, b, "wrapped forward back to the first");
    }

    #[test]
    fn ctrl_shift_n_queues_a_spawn_window_command() {
        // The new-window verb can't create a window from a per-window handler (no
        // event loop, no registry access), so it queues a `SpawnWindow` the shell
        // applies in `about_to_wait`. Here we assert the verb→queue step; the actual
        // spawn needs a live event loop (on-screen verify). (Multi-window MW3.)
        let mut app = test_app();
        {
            let mut wc = app.ctx();
            wc.view.modifiers.ctrl = true;
            wc.view.modifiers.shift = true;
            wc.on_key_pressed(&winit::keyboard::Key::Character("n".into()));
        }
        assert!(
            matches!(app.commands.first(), Some(crate::ShellCommand::SpawnWindow)),
            "Ctrl+Shift+N queues a SpawnWindow"
        );
        assert_eq!(app.commands.len(), 1, "exactly one command queued");
        assert_eq!(
            app.shared.session.manifests.len(),
            1,
            "the shifted verb must not also mint a session"
        );
    }

    #[test]
    fn ime_preedit_and_commit_route_to_the_focused_field() {
        // G2.1: the meerkat-specific half of IME — routing `WindowEvent::Ime` to
        // the *focused* chrome field. (The candidate-area call no-ops headlessly:
        // no window / chrome session.) Preedit shows inline (out of the committed
        // buffer); commit clears it and inserts via the focus-routed key path.
        use winit::event::Ime;
        let mut app = test_app();
        let mut wc = app.ctx();
        let omnibar = wc.input_under_class("toolbar");
        assert!(omnibar.is_some(), "the omnibar input exists in the chrome DOM");
        wc.view.runner.set_focus(omnibar);

        let committed = wc.view.runner.state().omnibar.text().to_string();
        wc.handle_ime(Ime::Preedit("ni".to_string(), None));
        assert_eq!(wc.view.runner.state().omnibar.preedit(), "ni", "preedit routed to the omnibar");
        assert_eq!(
            wc.view.runner.state().omnibar.text().to_string(),
            committed,
            "preedit stays out of the committed buffer",
        );

        // 你好 — commit clears the preedit and inserts the composed text.
        wc.handle_ime(Ime::Commit("\u{4f60}\u{597d}".to_string()));
        assert_eq!(wc.view.runner.state().omnibar.preedit(), "", "commit clears the preedit");
        assert!(
            wc.view.runner.state().omnibar.text().contains("\u{4f60}\u{597d}"),
            "commit inserts the composed text into the omnibar, got {:?}",
            wc.view.runner.state().omnibar.text(),
        );
    }

    #[test]
    fn a_spawned_window_is_a_slim_leaf() {
        // A second window (Cmd/Ctrl+Shift+N → build_window_view) is a leaf: slim
        // chrome (no shellbar / switcher). The primary stays full chrome. (MW3 step 4.)
        let app = test_app();
        let leaf = app.build_window_view();
        assert_eq!(leaf.kind, crate::window_view::WindowKind::Leaf);
        assert!(leaf.runner.state().slim, "a leaf's chrome is slim");
        assert_eq!(app.view().kind, crate::window_view::WindowKind::Primary);
        assert!(!app.view().runner.state().slim, "the primary's chrome is full");
    }

    #[test]
    fn ctrl_n_without_shift_makes_a_session_not_a_window() {
        // The unshifted Ctrl+N is the older new-session verb; the shift is the only
        // thing that distinguishes it from the new-window verb above. (Multi-window MW3.)
        let mut app = test_app();
        {
            let mut wc = app.ctx();
            wc.view.modifiers.ctrl = true;
            wc.on_key_pressed(&winit::keyboard::Key::Character("n".into()));
        }
        assert!(app.commands.is_empty(), "no window spawn queued");
        assert_eq!(
            app.shared.session.manifests.len(),
            2,
            "a new session was minted instead"
        );
    }

    #[test]
    fn switching_keeps_the_window_panes_and_resources_graph_bound_leaves() {
        let mut app = test_app();
        let first = app.shared.session.active_session_id;
        let first_graph = app.ctx().active_graph_id();
        // Open a second pane: the window now holds an orrery + a roster.
        app.ctx().toggle_pane(frame::PaneContent::Roster);
        let has_roster = |app: &Shell| {
            app.view().frame_layout
                .iter_leaves()
                .any(|(_, c, _)| matches!(c, frame::PaneContent::Roster))
        };
        let orrery_graph = |app: &Shell| {
            app.view().frame_layout
                .iter_leaves()
                .find(|(_, c, _)| matches!(c, frame::PaneContent::Orrery))
                .map(|(_, _, g)| g)
                .expect("the orrery pane is always present")
        };
        assert!(has_roster(&app), "roster pane opened");
        assert_eq!(orrery_graph(&app), first_graph, "orrery bound to the first graph");

        // Switch to a fresh session: the pane arrangement persists (the frame is
        // window-scoped) and the graph-bound orrery leaf re-sources to the new graph.
        // (Model B, MG5.)
        app.ctx().create_session();
        let second_graph = app.ctx().active_graph_id();
        assert_ne!(second_graph, first_graph, "the new session has its own graph");
        assert!(
            has_roster(&app),
            "the roster pane survived the switch (frame is window-scoped, not swapped)"
        );
        assert_eq!(
            orrery_graph(&app),
            second_graph,
            "the orrery leaf re-sourced to the new active graph"
        );

        // Back to the first: the layout still holds; the orrery follows it again.
        app.ctx().switch_session(first);
        assert!(has_roster(&app), "the pane layout persists across switches");
        assert_eq!(orrery_graph(&app), first_graph);
    }

    #[test]
    fn rename_sets_then_clears_the_session_display_name() {
        let mut app = test_app();
        let id = app.shared.session.active_session_id;
        let name_of = |app: &Shell, id| {
            app.shared.session.manifests
                .get(id)
                .and_then(|m| m.display_name.clone())
        };

        // Rename starts from the current (derived) label, so clear it before typing.
        app.ctx().start_rename(id);
        assert!(app.ctx().view.renaming.is_some());
        for _ in 0..16 {
            app.ctx().rename_backspace(); // clear the seeded label (backspace-on-empty is a no-op)
        }
        app.ctx().rename_push("W");
        app.ctx().rename_push("ork");
        app.ctx().rename_backspace(); // "Work" -> "Wor"
        app.ctx().commit_rename();
        assert!(app.ctx().view.renaming.is_none());
        assert_eq!(name_of(&app, id).as_deref(), Some("Wor"));
        assert_eq!(app.shared.session.session_labels.get(&id).map(String::as_str), Some("Wor"));

        // Emptying the buffer clears the display name (the label reverts to derived).
        app.ctx().start_rename(id);
        for _ in 0..16 {
            app.ctx().rename_backspace();
        }
        app.ctx().commit_rename();
        assert!(name_of(&app, id).is_none(), "an empty rename clears the name");

        // Escape (cancel) leaves the name untouched.
        app.ctx().start_rename(id);
        app.ctx().rename_push("X");
        app.ctx().cancel_rename();
        assert!(app.ctx().view.renaming.is_none());
        assert!(name_of(&app, id).is_none(), "cancel did not persist the edit");
    }

    #[test]
    fn observation_exposes_surfaces_actions_and_a11y() {
        let mut app = test_app();
        let observation = app.agent_observation();
        assert!(
            observation
                .surfaces
                .iter()
                .any(|s| s.pane == AgentPane::Orrery)
        );
        assert!(
            observation
                .enabled_actions
                .iter()
                .any(|a| a.id == "pane.open.apparatus")
        );
        assert!(observation.a11y.nodes > 0);
        assert_eq!(observation.focused_node.as_deref(), Some("mere://welcome"));
    }

    #[test]
    fn agent_can_open_apparatus_switch_theme_and_open_roster() {
        let mut app = test_app();
        let step = app.apply_agent_action(AgentAction::OpenPane(AgentPane::Apparatus));
        assert!(step.result.applied);
        assert!(
            step.observation
                .surfaces
                .iter()
                .any(|s| s.pane == AgentPane::Apparatus)
        );

        let step = app.apply_agent_action(AgentAction::SetTheme(THEME_ID_LIGHT.to_string()));
        assert!(step.result.applied);
        assert_eq!(step.observation.active_theme_id, THEME_ID_LIGHT);

        let step = app.apply_agent_action(AgentAction::OpenPane(AgentPane::Roster));
        assert!(step.result.applied);
        assert!(
            step.observation
                .surfaces
                .iter()
                .any(|s| s.pane == AgentPane::Roster)
        );
    }

    #[test]
    fn agent_can_open_inspector_and_steward_as_d8_panes() {
        let mut app = test_app();
        let step = app.apply_agent_action(AgentAction::OpenPane(AgentPane::Inspector));
        assert!(step.result.applied);
        assert!(
            step.observation
                .surfaces
                .iter()
                .any(|s| s.pane == AgentPane::Inspector)
        );

        let step = app.apply_agent_action(AgentAction::OpenPane(AgentPane::Steward));
        assert!(step.result.applied);
        assert!(
            step.observation
                .surfaces
                .iter()
                .any(|s| s.pane == AgentPane::Steward)
        );
        assert!(
            step.observation
                .enabled_actions
                .iter()
                .any(|action| action.id == "pane.open.inspector")
        );
        assert!(
            step.observation
                .enabled_actions
                .iter()
                .any(|action| action.id == "pane.open.steward")
        );
        assert!(
            step.observation
                .enabled_actions
                .iter()
                .any(|action| action.id == "operation.pin.focused")
        );
    }

    #[test]
    fn agent_can_pin_stop_and_report_blocked_retry_for_focused_operation() {
        let mut app = test_app();
        let step = app.apply_agent_action(AgentAction::RetryFocusedContent);
        assert!(
            !step.result.applied,
            "mere://welcome is not fetchable, so retry is blocked"
        );

        let step = app.apply_agent_action(AgentAction::PinFocusedOperation);
        assert!(step.result.applied);
        let focused = app.ctx().focused_member().expect("welcome is focused");
        assert!(
            app.shared.content.constellation.is_background(focused),
            "pin marks the focused operation as background"
        );

        let step = app.apply_agent_action(AgentAction::StopFocusedOperation);
        assert!(step.result.applied);
        assert!(
            !app.shared.content.constellation.is_active(focused),
            "stop reaps the focused operation"
        );
    }

    #[test]
    fn agent_can_select_node_and_report_blocked_actions() {
        let mut app = test_app();
        app.orrery_mut().visit("https://example.test");
        let step =
            app.apply_agent_action(AgentAction::SelectNodeByUrl("mere://welcome".to_string()));
        assert!(step.result.applied);
        assert_eq!(
            step.observation.focused_node.as_deref(),
            Some("mere://welcome")
        );

        let step = app.apply_agent_action(AgentAction::SelectNodeByUrl(
            "https://missing.example".to_string(),
        ));
        assert!(!step.result.applied);
        assert!(
            step.observation
                .diagnostics
                .iter()
                .any(|d| d.channel == "meerkat.agent.intent_dropped")
        );
    }

    #[test]
    fn agent_can_invoke_command_without_coordinate_scripting() {
        let mut app = test_app();
        let step = app.apply_agent_action(AgentAction::InvokeCommand(Command::ToggleComms));
        assert!(step.result.applied);
        assert!(
            step.observation
                .surfaces
                .iter()
                .any(|s| s.pane == AgentPane::Comms)
        );

        let step = app.apply_agent_action(AgentAction::SetTheme(THEME_ID_DARK.to_string()));
        assert_eq!(step.observation.active_theme_id, THEME_ID_DARK);
    }

    #[test]
    fn accesskit_actions_route_to_semantic_node_selection() {
        let mut app = test_app();
        app.orrery_mut().visit("https://example.test");
        app.orrery_mut().select_by_url("mere://welcome");
        app.ctx().refresh_a11y_summary();
        let target = app
            .a11y_action_routes
            .iter()
            .find_map(|(id, action)| match action {
                crate::A11yHostAction::SelectNodeByUrl(url) if url == "https://example.test" => {
                    Some(*id)
                }
                _ => None,
            })
            .expect("example node has an AccessKit route");

        app.ctx().apply_a11y_request(crate::a11y_bridge::A11yActionRequest {
            action: Action::Focus,
            target_node: target,
        });

        assert_eq!(app.orrery().focused_url(), Some("https://example.test"));
        let observation = app.agent_observation();
        assert!(
            observation
                .diagnostics
                .iter()
                .any(|record| record.channel == "meerkat.agent.action_applied")
        );
    }

    #[test]
    fn accesskit_focus_on_a_chrome_control_routes_to_the_runner() {
        // G2.4 part 2: a screen reader's action on a chrome control routes back to
        // the host's activation path. The route stores the whole `NodeId` keyed by
        // the node's salted a11y id, so the projection's node id and the route key
        // round-trip — the seam the first cut (reversing the salted id) got
        // debug-wrong, a doc-tag riding the salt's high bits.
        use serval_layout::ScrollOffsets;
        let mut app = test_app();
        let mut wc = app.ctx();

        // The omnibar is a real chrome DOM node — an `<input>`, so it advertises
        // `Focus`.
        let omnibar = wc
            .input_under_class("toolbar")
            .expect("the omnibar input exists in the chrome DOM");

        // Build the chrome session so the projection derives from the live chrome
        // DOM (the placeholder fallback carries no actionable nodes). CPU layout, no
        // GPU — the same `PaneSession::scene` the render path runs.
        let sheet = wc.shared.presentation.chrome_sheet_refs();
        let scroll = ScrollOffsets::default();
        crate::pane_session::PaneSession::scene(
            &mut wc.view.chrome_session,
            &wc.view.dom,
            &sheet,
            1200,
            80,
            None,
            &scroll,
        );

        // Project: the omnibar carries a `ChromeNode` route under its own a11y id —
        // the key the bridge targets a request with.
        wc.refresh_a11y_summary();
        let target = crate::serval_a11y::chrome_a11y_id(omnibar);
        assert_eq!(
            wc.a11y_action_routes.get(&target),
            Some(&crate::A11yHostAction::ChromeNode(omnibar)),
            "the omnibar's a11y id keys a ChromeNode route to its own node",
        );

        // Apply a `Focus` request at that id: the runner's focus lands on the
        // omnibar, proving the projection id and the route key round-trip.
        wc.view.runner.set_focus(None);
        wc.apply_a11y_request(crate::a11y_bridge::A11yActionRequest {
            action: Action::Focus,
            target_node: target,
        });
        assert_eq!(
            wc.view.runner.focus(),
            Some(omnibar),
            "Focus routed through the ChromeNode route to the omnibar node",
        );
    }
}
