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
use meerkat::ContextAction;
use meerkat::command::{Command, context_action_from_id, context_action_palette_label};

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
    /// Invoke any registry action by its stable id — a command verb (`"workbench"`) or a
    /// context-action id (`"add_node"`). The one by-id seam automation + agents use, the
    /// same id space the palette and (later) the configurable menu address. (Registry P3.)
    Invoke(String),
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
            AgentAction::Invoke(id) => self.agent_invoke(&id),
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
        self.ctx().view.chrome_update(move |chrome| {
            chrome.run_command_and_close(cmd);
        });
        self.ctx().drain_pending_command();
        self.ctx().drain_pending_connect();
        self.ctx().sync_comms_pane();
        self.ctx().sync_settings();
        (true, action_id, cmd.label().to_string())
    }

    /// Invoke any registry action by its stable id: a command verb routes to
    /// [`agent_invoke_command`](Self::agent_invoke_command), a context-action id to
    /// [`agent_invoke_context`](Self::agent_invoke_context). The agent's view of the one
    /// registry seam — the same ids the palette + menu config use. (Registry P3.)
    fn agent_invoke(&mut self, id: &str) -> (bool, String, String) {
        if let Some(cmd) = Command::from_id(id) {
            self.agent_invoke_command(cmd)
        } else if let Some(action) = context_action_from_id(id) {
            self.agent_invoke_context(action)
        } else {
            (false, format!("invoke.{id}"), format!("unknown registry id: {id}"))
        }
    }

    /// Apply a context action against the live selection — the agent counterpart of the
    /// palette's context path: seed `context_set` from the selection, queue the action, and
    /// run the existing context drain (the same applier the menu uses). (Registry P3.)
    fn agent_invoke_context(&mut self, action: ContextAction) -> (bool, String, String) {
        let action_id = format!("context.{action:?}").to_ascii_lowercase();
        let detail = context_action_palette_label(action).unwrap_or_default().to_string();
        let set = self.ctx().selection_working_set();
        self.ctx().view.context_set = set;
        self.ctx().view.context_origin = None;
        self.ctx().view.chrome_update(move |c| c.pick_context(action));
        self.ctx().drain_pending_context();
        (true, action_id, detail)
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
        if self.ctx().view.chrome().palette_open {
            self.ctx().view.chrome_update(meerkat::Chrome::run_palette_selection);
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
        // The Stage-2 live-preview toggle was retired (d4375d0): a focused node's
        // content now shows automatically as a snapshot card, and a pelt tile is the
        // live view (double-click / compat toggle). There is no preview to toggle.
        (
            false,
            action_id,
            "live preview retired; focused content shows as a snapshot card".to_string(),
        )
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
            // Alembic has no dedicated AgentPane yet; fold into the generic mapping. (Alembic B1.)
            PaneContent::Trail
            | PaneContent::Alembic
            | PaneContent::Tile(_)
            | PaneContent::Custom(_) => Self::Workbench,
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
        let second = app.create_session();
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

        let second = app.create_session();
        // The fresh session is its own (smaller) graph, not the first's.
        assert!(app.orrery().graph().nodes().count() < first_count);

        app.switch_session(first);
        assert_eq!(app.shared.session.active_session_id, first);
        assert_eq!(
            app.orrery().graph().nodes().count(),
            first_count,
            "the first session's graph was restored intact"
        );

        app.switch_session(second);
        assert_eq!(app.shared.session.active_session_id, second);
    }

    #[test]
    fn cycle_session_wraps_through_the_open_sessions() {
        let mut app = test_app();
        let a = app.shared.session.active_session_id;
        let b = app.create_session(); // active = b, two sessions
        app.cycle_session(true);
        assert_eq!(app.shared.session.active_session_id, a, "wrapped forward to the other session");
        app.cycle_session(true);
        assert_eq!(app.shared.session.active_session_id, b, "wrapped forward back to the first");
    }

    #[test]
    fn orrery_pool_is_bounded_and_keeps_the_focused_graph() {
        let mut app = test_app();
        // Mint more sessions than the pool holds; each switches to a fresh graph,
        // growing the pool until the cap evicts the stalest non-focused orrery.
        for _ in 0..(crate::MAX_POOLED_ORRERIES + 3) {
            app.create_session();
        }
        assert_eq!(
            app.orreries.len(),
            crate::MAX_POOLED_ORRERIES,
            "the orrery pool is bounded at the cap, not unbounded per session"
        );
        let focused = app.view().focused_graph;
        assert!(
            app.orreries.contains_key(&focused),
            "the focused graph is never evicted"
        );
    }

    #[test]
    fn focus_pane_graph_moves_focus_without_a_switch_or_clobber() {
        // C1 (pane-as-unit): focus-follows-click is a *pointer* move, not a session
        // switch. It re-keys the focused graph + active session so nav and save follow
        // the clicked pane, but leaves both graphs live in the pool — no reload, no
        // cache clear, no frame re-point (unlike `switch_session`). (Tear-out C1.)
        let mut app = test_app();
        let a_session = app.shared.session.active_session_id;
        let a_graph = app.view().focused_graph;

        // A second graph, switched to: focus is now on B, A parked but still pooled.
        let b_session = app.create_session();
        let b_graph = app.view().focused_graph;
        assert_ne!(a_graph, b_graph, "the second session is its own graph");
        assert_eq!(app.shared.session.active_session_id, b_session);

        // Focus the first graph's pane — the lightweight focus-follows-click path.
        app.ctx().focus_pane_graph(a_graph);

        assert_eq!(app.view().focused_graph, a_graph, "focus moved to the clicked pane's graph");
        assert_eq!(
            app.shared.session.active_session_id, a_session,
            "active session re-keyed to the focused pane, so nav + save target it"
        );
        assert!(
            app.orreries.contains_key(&a_graph) && app.orreries.contains_key(&b_graph),
            "both graphs stay live in the pool — focus is a pointer, not a reload or a clobber"
        );
    }

    #[test]
    fn steward_surfaces_the_live_graph_count() {
        let mut app = test_app();
        app.create_session(); // seed + the new graph = 2 live in the pool
        let rows = app.ctx().steward_rows();
        let live = rows
            .iter()
            .find(|(k, _)| k.as_str() == "Live graphs")
            .expect("Steward shows a Live graphs row");
        assert_eq!(
            live.1,
            format!("2 / {}", crate::MAX_POOLED_ORRERIES),
            "the row reports the real pool count over the cap (the no-placebo tripwire)"
        );
    }

    #[test]
    fn steward_exposes_clickable_action_verbs() {
        // retry / stop / pin are real buttons, not just a typed-verb hint: each row
        // carries the `steward:*` activation key the drain routes to a node-ops verb.
        let mut app = test_app();
        app.create_session();
        let items = app.ctx().steward_items();
        let keys: Vec<&str> = items.iter().filter_map(|i| i.key.as_deref()).collect();
        assert!(keys.contains(&"steward:retry"), "retry is a clickable row: {keys:?}");
        assert!(keys.contains(&"steward:stop"), "stop is a clickable row: {keys:?}");
        assert!(keys.contains(&"steward:pin"), "pin is a clickable row: {keys:?}");
    }

    #[test]
    fn relate_picker_offers_the_semantic_kinds_as_actions() {
        // The two-node relate picker turns the edge vocabulary into clickable rows: each is a
        // "Relate as <kind>" label carrying a RelateAs(kind) action. Previously the kinds were
        // reachable only by typing relate("cites"); every drawn edge was an undifferentiated
        // UserGrouped. (Audit A3 — the top pick.)
        let mut app = test_app();
        app.create_session();
        let items = app.ctx().relate_picker_items();
        assert!(
            items.len() >= 10,
            "the curated semantic vocabulary is offered: {}",
            items.len()
        );
        assert!(
            items
                .iter()
                .all(|i| matches!(i.action, meerkat::ContextAction::RelateAs(_))),
            "every picker row is a RelateAs action",
        );
        let cites = items
            .iter()
            .find(|i| {
                matches!(
                    i.action,
                    meerkat::ContextAction::RelateAs(kernel::graph::SemanticSubKind::Cites)
                )
            })
            .expect("Cites is offered as a relation kind");
        assert_eq!(cites.label, "Relate as Cites");
    }

    #[test]
    fn steward_surfaces_a_recorded_forgetting_pass() {
        let mut app = test_app();
        app.create_session();
        let mut wc = app.ctx();
        // Before any pass, Steward shows the row as not-yet-run.
        let rows = wc.steward_rows();
        assert_eq!(
            rows.iter()
                .find(|(k, _)| k == "Last forgetting")
                .map(|r| r.1.as_str()),
            Some("not run yet"),
            "Steward carries a Last forgetting row, not-run before any pass",
        );
        // Record a pass directly (deterministic — independent of the store / proposal)
        // and confirm Steward surfaces the real dropped count. (No placebo.)
        wc.shared.observability.record_forgetting_pass(2);
        let rows = wc.steward_rows();
        let row = rows
            .iter()
            .find(|(k, _)| k == "Last forgetting")
            .expect("Steward shows a Last forgetting row");
        assert!(
            row.1.contains("dropped 2 page(s)"),
            "the row reports the real pass result: {}",
            row.1
        );
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

        let committed = wc.view.chrome().omnibar.text().to_string();
        wc.handle_ime(Ime::Preedit("ni".to_string(), None));
        assert_eq!(wc.view.chrome().omnibar.preedit(), "ni", "preedit routed to the omnibar");
        assert_eq!(
            wc.view.chrome().omnibar.text().to_string(),
            committed,
            "preedit stays out of the committed buffer",
        );

        // 你好 — commit clears the preedit and inserts the composed text.
        wc.handle_ime(Ime::Commit("\u{4f60}\u{597d}".to_string()));
        assert_eq!(wc.view.chrome().omnibar.preedit(), "", "commit clears the preedit");
        assert!(
            wc.view.chrome().omnibar.text().contains("\u{4f60}\u{597d}"),
            "commit inserts the composed text into the omnibar, got {:?}",
            wc.view.chrome().omnibar.text(),
        );
    }

    #[test]
    fn a_spawned_window_is_a_slim_leaf() {
        // A second window (Cmd/Ctrl+Shift+N → build_window_view) is a leaf: slim
        // chrome (no shellbar / switcher). The primary stays full chrome. (MW3 step 4.)
        let app = test_app();
        let leaf = app.build_window_view();
        assert_eq!(leaf.kind, crate::window_view::WindowKind::Leaf);
        assert!(leaf.runner.state().chrome.slim, "a leaf's chrome is slim");
        assert_eq!(app.view().kind, crate::window_view::WindowKind::Primary);
        assert!(!app.view().runner.state().chrome.slim, "the primary's chrome is full");
    }

    #[test]
    fn a_pane_camera_lives_on_the_view_and_round_trips_through_the_ctx() {
        // Camera on the view: the pooled Orrery is the authority (graph + physics +
        // node positions, shared across windows); the per-pane camera is a `Viewport`
        // on the WindowView. A ctx installs it on build and reads it back on drop, so a
        // move made through one window's ctx lands on *that* window's stored viewport,
        // not the shared orrery. That decoupling is what lets a same-graph co-window be
        // an independent view instead of a mirror. (Camera on the view.)
        let mut app = test_app();
        let gid = app.view().focused_graph;

        // No stored viewport yet; building then dropping a ctx seeds it from the
        // orrery's current framing and reads it back onto the view.
        assert!(!app.view().viewports.contains_key(&gid));
        drop(app.ctx());
        assert!(
            app.view().viewports.contains_key(&gid),
            "the ctx seeds + reads back this window's viewport for the shown graph",
        );

        // A camera move made within a ctx is captured back onto the view on drop,
        // including the pan inertia (per-view, so one window's fling can't drift another).
        let moved = orrery::Viewport {
            offset: (123.0, 45.0),
            zoom: 2.0,
            yaw: 0.0,
            tilt: 1.0,
            pan_velocity: (7.0, -3.0),
        };
        app.ctx().orrery_mut().set_viewport(moved);
        let stored = app.view().viewports.get(&gid).copied().expect("viewport stored");
        assert_eq!(stored.offset, (123.0, 45.0), "pan read back onto the view");
        assert_eq!(stored.zoom, 2.0, "zoom read back onto the view");
        assert_eq!(stored.pan_velocity, (7.0, -3.0), "inertia is per-view too");
    }

    #[test]
    fn ctrl_n_without_shift_makes_a_session_not_a_window() {
        // The unshifted Ctrl+N is the older new-session verb; the shift is the only
        // thing that distinguishes it from the new-window verb above. (Multi-window MW3.)
        // A session op re-keys the orrery pool, so — like spawn/close — it runs on
        // `Shell` via a queued `ShellCommand`, not synchronously in the handler.
        // (Window composition P1, multi-graph.)
        let mut app = test_app();
        {
            let mut wc = app.ctx();
            wc.view.modifiers.ctrl = true;
            wc.on_key_pressed(&winit::keyboard::Key::Character("n".into()));
        }
        assert!(
            matches!(app.commands.first(), Some(crate::ShellCommand::CreateSession)),
            "a new-session command was queued, not a window spawn"
        );
        assert_eq!(app.commands.len(), 1, "exactly the one session command");
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
        app.create_session();
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
        app.switch_session(first);
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
    fn omnibar_ctrl_a_selects_all() {
        // Ctrl+A reaches the focused omnibar (no host shortcut eats it) and selects
        // the whole buffer, so the next keystroke replaces it.
        let mut app = test_app();
        {
            let mut wc = app.ctx();
            let omnibar = wc
                .input_under_class("toolbar")
                .expect("the omnibar input exists in the chrome DOM");
            wc.view.runner.set_focus(Some(omnibar));
            wc.view
                .runner
                .update(|c| c.chrome.omnibar = xilem_serval::TextInput::new("hello world"));
            wc.view.modifiers.ctrl = true;
            wc.on_key_pressed(&winit::keyboard::Key::Character("a".into()));
        }
        let state = &app.view().runner.state().chrome;
        assert!(state.omnibar.has_selection(), "Ctrl+A selected the omnibar text");
        assert_eq!(state.omnibar.selected_text(), "hello world");
    }

    /// Soft-wrap ArrowUp/ArrowDown only claims the key for the focused knot-editor
    /// textarea; with nothing or a single-line field (the omnibar) focused it declines,
    /// so suggestion nav / other-field moves route normally. Guards the interception
    /// against hijacking every field's Up/Down. (Soft-wrap caret nav.)
    #[test]
    fn soft_wrap_nav_declines_outside_the_knot_editor() {
        let mut app = test_app();
        let mut wc = app.ctx();
        wc.view.runner.set_focus(None);
        assert!(!wc.soft_wrap_nav(-1, false), "no focus: soft-wrap nav declines");
        let omnibar = wc.input_under_class("toolbar").expect("the omnibar input exists");
        wc.view.runner.set_focus(Some(omnibar));
        assert!(!wc.soft_wrap_nav(1, false), "single-line omnibar: soft-wrap nav declines");
    }

    /// The knot editor's close × lays out as a small button on the right of the header row
    /// (the `.knot-editor-title` flex rule), not the full-width block bar `button { display:
    /// block }` would otherwise make it. Laid out headlessly through the real `PaneSession::scene`
    /// path, so it verifies the chrome-sheet rule without a window. (Knot editor header.)
    #[test]
    fn knot_editor_close_button_is_small_and_right_of_the_title() {
        use layout_dom_api::LayoutDom;
        use serval_layout::ScrollOffsets;
        let mut app = test_app();
        let mut wc = app.ctx();
        wc.view.chrome_update(|c| c.open_knot_editor("a note"));

        let sheet = wc.shared.presentation.chrome_sheet_refs();
        let scroll = ScrollOffsets::default();
        crate::pane_session::PaneSession::scene(
            &mut wc.view.chrome_session,
            &wc.view.dom,
            &sheet,
            1024,
            600,
            None,
            &scroll,
        );

        let dom = wc.view.dom.borrow();
        let session = wc.view.chrome_session.as_ref().expect("chrome session built");
        let btn = crate::first_with_class(&dom, dom.document(), "knot-editor-btn")
            .expect("the close button exists when the editor is open");
        let title = crate::first_with_class(&dom, dom.document(), "knot-editor-title-text")
            .expect("the title text exists");
        let btn_r = session.fragments().rect_of(btn).expect("close button laid out");
        let title_r = session.fragments().rect_of(title).expect("title laid out");

        // Small (a flex item sized to the × glyph), not the ~600px full-width block bar.
        assert!(
            btn_r.size.width < 80.0,
            "the × shrinks to a small button, not a full-width bar: width={}",
            btn_r.size.width,
        );
        // To the right of the title in the header row (justify-content: space-between).
        assert!(
            btn_r.location.x > title_r.location.x,
            "the × sits right of the title: btn.x={} title.x={}",
            btn_r.location.x,
            title_r.location.x,
        );
    }

    #[test]
    fn omnibar_right_arrow_accepts_the_ghost_completion() {
        // The driven half of ghost autocomplete: with `>ros` typed and the omnibar
        // focused, Right arrow at the buffer end splices the ghost in, giving
        // `>roster`. Enter would then run it; the ghost itself is never evaluated.
        let mut app = test_app();
        {
            let mut wc = app.ctx();
            let omnibar = wc
                .input_under_class("toolbar")
                .expect("the omnibar input exists in the chrome DOM");
            wc.view.runner.set_focus(Some(omnibar));
            wc.view.chrome_update(|c| {
                c.omnibar = xilem_serval::TextInput::new(">ros");
                c.refresh_suggestions();
            });
            assert_eq!(wc.view.chrome().omnibar.ghost(), "ter", "the ghost is shown");
            wc.on_key_pressed(&winit::keyboard::Key::Named(
                winit::keyboard::NamedKey::ArrowRight,
            ));
        }
        assert_eq!(
            app.view().runner.state().chrome.omnibar.text(),
            ">roster",
            "Right arrow accepted the ghost into the buffer"
        );
    }

    #[test]
    fn f5_reloads_the_focused_nodes_page() {
        // F5 re-fetches the focused node's page, putting it back into Loading so the
        // live fetch actor retries (browser reload, bypassing the durable cache).
        // Driven through on_key_pressed so it guards the real key path, not just
        // retry_focused_content in isolation.
        let mut app = test_app();
        app.orrery_mut().visit("gemini://capsule.example/");
        let url = app
            .orrery()
            .focused_url()
            .expect("the visited node is focused")
            .to_string();
        // Start from a clean slate so the assertion is about F5, not the visit.
        app.shared.content.pages.remove(&url);
        {
            let mut wc = app.ctx();
            wc.on_key_pressed(&winit::keyboard::Key::Named(winit::keyboard::NamedKey::F5));
        }
        assert!(
            matches!(
                app.shared.content.pages.get(&url),
                Some(crate::fetch::ContentState::Loading)
            ),
            "F5 queued a fresh fetch (the focused node's page is Loading)"
        );
    }

    #[test]
    fn ctrl_l_focuses_and_selects_the_omnibar() {
        // Ctrl+L moves the caret to the address bar AND selects its whole contents
        // (browser convention) so the next keystroke replaces the shown URL rather
        // than appending to it. (The appending bug was caught on-screen.)
        let mut app = test_app();
        let mut wc = app.ctx();
        let omnibar = wc
            .input_under_class("toolbar")
            .expect("the omnibar input exists in the chrome DOM");
        // Seed the bar with a shown location and put the caret at its end (focused).
        wc.view
            .runner
            .update(|c| c.chrome.omnibar = xilem_serval::TextInput::new("gemini://shown.example/"));
        wc.view.runner.set_focus(None);
        wc.view.modifiers.ctrl = true;
        wc.on_key_pressed(&winit::keyboard::Key::Character("l".into()));
        assert_eq!(
            wc.view.runner.focus(),
            Some(omnibar),
            "Ctrl+L focused the omnibar"
        );
        let state = wc.view.chrome();
        assert!(state.omnibar.has_selection(), "Ctrl+L selected the omnibar text");
        assert_eq!(
            state.omnibar.selected_text(),
            "gemini://shown.example/",
            "the whole address is selected, so typing replaces it"
        );
    }

    #[test]
    fn alt_arrows_step_the_focused_nodes_history() {
        // Alt+Left / Alt+Right walk the focused node's own browse history (browser
        // back/forward), driven through on_key_pressed so the real key path is
        // guarded, not just drain_history_step.
        let mut app = test_app();
        app.orrery_mut().visit("gemini://a.example/");
        let member = app
            .orrery()
            .focused_member()
            .expect("the visited node is focused");
        // Navigate the same node in place a -> b, growing its within-node history.
        app.orrery_mut().navigate_member(member, "gemini://b.example/");
        assert!(app.orrery().member_can_back(member), "a -> b leaves a back step");
        assert!(!app.orrery().member_can_forward(member), "and no forward step yet");

        let press = |app: &mut Shell, named: winit::keyboard::NamedKey| {
            let mut wc = app.ctx();
            wc.view.modifiers.alt = true;
            wc.on_key_pressed(&winit::keyboard::Key::Named(named));
        };

        // Alt+Left steps back to the root (a).
        press(&mut app, winit::keyboard::NamedKey::ArrowLeft);
        assert!(!app.orrery().member_can_back(member), "back from b lands at the root a");
        assert!(app.orrery().member_can_forward(member), "and forward to b is available");

        // Alt+Right steps forward to the tip (b) again.
        press(&mut app, winit::keyboard::NamedKey::ArrowRight);
        assert!(app.orrery().member_can_back(member), "forward returns to b");
        assert!(!app.orrery().member_can_forward(member), "and b is the tip");
    }

    #[test]
    fn mouse_thumb_buttons_step_the_focused_nodes_history() {
        // The dedicated mouse back/forward (thumb) buttons walk the focused node's
        // own history, same intent as Alt+Left / Alt+Right, driven through
        // on_mouse_input.
        let mut app = test_app();
        app.orrery_mut().visit("gemini://a.example/");
        let member = app
            .orrery()
            .focused_member()
            .expect("the visited node is focused");
        app.orrery_mut().navigate_member(member, "gemini://b.example/");
        assert!(app.orrery().member_can_back(member), "a -> b leaves a back step");

        {
            let mut wc = app.ctx();
            wc.on_mouse_input(
                winit::event::ElementState::Pressed,
                winit::event::MouseButton::Back,
            );
        }
        assert!(!app.orrery().member_can_back(member), "thumb-back lands at the root a");
        assert!(app.orrery().member_can_forward(member), "and forward to b is available");

        {
            let mut wc = app.ctx();
            wc.on_mouse_input(
                winit::event::ElementState::Pressed,
                winit::event::MouseButton::Forward,
            );
        }
        assert!(app.orrery().member_can_back(member), "thumb-forward returns to the tip b");
    }

    #[test]
    fn shellbar_roster_button_toggles_the_roster() {
        // The shellbar pane-toggle buttons are real chrome-DOM nodes; activating the
        // roster button runs ToggleRoster through the command spine. (The pointer
        // routing that reaches this — the inert-shellbar fix — is verified on-screen;
        // this guards the button→command→toggle chain it unblocks.)
        let has_roster = |app: &Shell| {
            app.view().frame_layout
                .iter_leaves()
                .any(|(_, c, _)| matches!(c, frame::PaneContent::Roster))
        };
        let mut app = test_app();
        assert!(!has_roster(&app), "no roster before");
        let roster_btn = {
            use layout_dom_api::LayoutDom;
            let dom = app.view().runner.dom();
            let dom = dom.borrow();
            let buttons = crate::all_with_class(&dom, dom.document(), "shellbar-btn");
            assert!(buttons.len() >= 7, "the shellbar carries a button per toggle-able pane");
            buttons[1] // workbench, ROSTER, gloss, apparatus, inspector, steward, comms
        };
        app.ctx().chrome_activate(roster_btn, (0.0, 0.0));
        assert!(has_roster(&app), "the shellbar roster button toggled the roster pane");
    }

    #[test]
    fn empty_space_right_click_adds_a_node() {
        // Right-clicking empty graph space offers "Add node"; running it mints a
        // fresh node at the captured cursor anchor and selects it.
        let mut app = test_app();
        app.orrery_mut().clear_selection();
        let before = app.orrery().graph().nodes().count();
        {
            let mut wc = app.ctx();
            wc.open_context_menu_at(200.0, 300.0);
            assert!(wc.view.context_origin.is_some(), "the empty-space menu captured a cursor anchor");
            wc.view.chrome_update(|c| c.pick_context(meerkat::ContextAction::AddNode));
            wc.drain_pending_context();
        }
        assert_eq!(app.orrery().graph().nodes().count(), before + 1, "AddNode minted a node");
        assert!(app.orrery().focused_url().is_some(), "the new node is selected");
        assert!(app.view().context_origin.is_none(), "the anchor was consumed");
    }

    #[test]
    fn workbench_new_tile_mints_and_opens_a_tile() {
        // The "add tile" affordance: an AddTile context action mints a node and opens
        // it as a tile (the live path the surface "+" routes to via `pick_context`).
        let mut app = test_app();
        let nodes_before = app.orrery().graph().nodes().count();
        let tiles_before = app.view().workbench.open_members().len();
        {
            let mut wc = app.ctx();
            wc.view.chrome_update(|c| c.pick_context(meerkat::ContextAction::AddTile));
            wc.drain_pending_context();
        }
        assert_eq!(app.orrery().graph().nodes().count(), nodes_before + 1, "NewTile minted a node");
        assert_eq!(
            app.view().workbench.open_members().len(),
            tiles_before + 1,
            "and opened it as a tile",
        );
        // The pane is actually shown (not just a model row): NewTile summons + tiles
        // the workbench and focuses the new tile.
        assert!(app.ctx().workbench_open(), "the workbench pane is open");
        assert!(app.view().workbench.is_tiled(), "in the tiled (Tree) projection");
        assert!(app.view().focused_tile.is_some(), "the new tile is focused");
    }

    #[test]
    fn agent_invoke_by_id_runs_commands_and_context_actions() {
        // The one by-id seam: an agent invokes any registry action by its stable id — a
        // command verb or a context-action id — the same id space the palette uses. (P3.)
        let mut app = test_app();
        // A command id routes through the command path.
        let step = app.apply_agent_action(AgentAction::Invoke("workbench".into()));
        assert!(step.result.applied, "a command id (workbench) invokes");
        // A context-action id applies to the graph: `add_node` mints a node by id.
        let before = app.orrery().graph().nodes().count();
        let step = app.apply_agent_action(AgentAction::Invoke("add_node".into()));
        assert!(step.result.applied, "a context-action id (add_node) invokes");
        assert_eq!(
            app.orrery().graph().nodes().count(),
            before + 1,
            "add_node minted a node, invoked purely by id",
        );
        // The invoke is audited through the one observability spine (P3): a
        // `meerkat.command.invoked` diagnostic carrying the registry id.
        assert!(
            step.observation.diagnostics.iter().any(|d| {
                d.channel == "meerkat.command.invoked" && d.message.contains("add_node")
            }),
            "the invoke is audited by its registry id through the observability spine",
        );
        // An unknown id is reported, not applied (no panic, no silent success).
        let step = app.apply_agent_action(AgentAction::Invoke("not_a_real_id".into()));
        assert!(!step.result.applied, "an unknown registry id is not applied");
    }

    #[test]
    fn roster_rows_highlight_a_multi_selection() {
        // The roster highlights by member-id set membership, so a multi-selection
        // (built by Shift-click) shows every selected row — not zero (the focused_url
        // collapse the review caught).
        let mut app = test_app();
        app.orrery_mut().visit("https://a.test");
        let b = app.orrery_mut().visit("https://b.test");
        let b_id = app.orrery().graph().get_node(b).unwrap().id;
        app.orrery_mut().select_by_url("https://a.test");
        app.orrery_mut().toggle_select_member(b_id);
        let rows = app.ctx().roster_rows();
        assert_eq!(
            rows.iter().filter(|r| r.selected).count(),
            2,
            "both selected members are highlighted in the roster",
        );
    }

    #[test]
    fn delete_key_removes_the_focused_node() {
        // With no chrome field focused, a bare Delete removes the focused node (the
        // Ctrl+Backspace muscle-memory, now reachable with Delete).
        let mut app = test_app();
        app.orrery_mut().visit("https://x.test");
        let before = app.orrery().graph().nodes().count();
        {
            let mut wc = app.ctx();
            wc.view.runner.set_focus(None); // graph has the keyboard
            wc.on_key_pressed(&winit::keyboard::Key::Named(winit::keyboard::NamedKey::Delete));
        }
        assert_eq!(
            app.orrery().graph().nodes().count(),
            before - 1,
            "Delete removed the focused node",
        );
    }

    #[test]
    fn omnibar_relate_without_a_pair_reports_a_note() {
        // `>relate` with fewer than two nodes selected no-ops; the bar explains why
        // instead of silently doing nothing.
        let mut app = test_app();
        {
            let mut wc = app.ctx();
            let omnibar = wc
                .input_under_class("toolbar")
                .expect("the omnibar input exists in the chrome DOM");
            wc.view.runner.set_focus(Some(omnibar));
            wc.view.chrome_update(|c| {
                c.omnibar = xilem_serval::TextInput::new(">relate");
                c.refresh_suggestions();
            });
            wc.on_key_pressed(&winit::keyboard::Key::Named(
                winit::keyboard::NamedKey::Enter,
            ));
        }
        assert!(
            app.view().runner.state().chrome.omnibar.text().contains("two nodes"),
            "the bar reports the no-op, got {:?}",
            app.view().runner.state().chrome.omnibar.text()
        );
    }

    #[test]
    fn omnibar_command_expression_drives_the_command_spine() {
        // The fourth driver: a `>`-prefixed omnibar expression reaches the same
        // `Command` spine the palette and the agent harness drive. Type `>roster`,
        // press Enter, and the host runs `ToggleRoster` through the full drain — a
        // Roster frame leaf appears, with no navigation. (Omnibar command shell, S3.)
        let has_roster = |app: &Shell| {
            app.view().frame_layout
                .iter_leaves()
                .any(|(_, c, _)| matches!(c, frame::PaneContent::Roster))
        };
        let mut app = test_app();
        assert!(!has_roster(&app), "no roster pane before the command");
        let entries_before = app.view().runner.state().chrome.history.entries().len();

        {
            let mut wc = app.ctx();
            let omnibar = wc
                .input_under_class("toolbar")
                .expect("the omnibar input exists in the chrome DOM");
            wc.view.runner.set_focus(Some(omnibar));
            wc.view.chrome_update(|c| c.show_location(">roster"));
            wc.on_key_pressed(&winit::keyboard::Key::Named(
                winit::keyboard::NamedKey::Enter,
            ));
        }

        assert!(
            has_roster(&app),
            "the `>roster` command toggled the roster pane through the host drain"
        );
        assert_eq!(
            app.view().runner.state().chrome.history.entries().len(),
            entries_before,
            "a command runs without navigating (no new history entry)"
        );
        // The bar is reset after the command: the typed `>roster` is cleared (a
        // pure action restores the location), never stranded behind a command that
        // already ran.
        assert!(
            !app.view().runner.state().chrome.omnibar.text().starts_with('>'),
            "the omnibar no longer shows the run command, got {:?}",
            app.view().runner.state().chrome.omnibar.text()
        );
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
