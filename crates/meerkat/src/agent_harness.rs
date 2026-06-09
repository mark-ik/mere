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
use super::{App, ContentPane};

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

impl App {
    pub(crate) fn agent_observation(&mut self) -> AgentObservation {
        self.refresh_a11y_summary();
        let snapshot = self.apparatus_observability();
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
            self.observability.record_diagnostic(
                "meerkat.agent.action_applied",
                Severity::Info,
                format!("{action_id}: {detail}"),
            );
        } else {
            self.observability.record_diagnostic(
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

    fn agent_observation_from_snapshot(&self, snapshot: ObservabilitySnapshot) -> AgentObservation {
        AgentObservation {
            active_theme_id: self.active_theme_id.clone(),
            focused_node: self.orrery.focused_url().map(str::to_string),
            active_content: match self.active_content {
                ContentPane::Orrery => AgentPane::Orrery,
                ContentPane::Workbench => AgentPane::Workbench,
            },
            surfaces: self.agent_surfaces(),
            enabled_actions: self.agent_enabled_actions(),
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

    fn agent_surfaces(&self) -> Vec<AgentSurface> {
        self.laid_leaves()
            .into_iter()
            .map(|leaf| {
                let pane = AgentPane::from_pane_content(&leaf.content);
                AgentSurface {
                    id: format!("pane:{}", leaf.pane_id.0),
                    pane,
                    label: leaf.content.tag().to_string(),
                    focused: self.agent_surface_focused(pane),
                }
            })
            .collect()
    }

    fn agent_surface_focused(&self, pane: AgentPane) -> bool {
        match (self.active_content, pane) {
            (ContentPane::Orrery, AgentPane::Orrery) => true,
            (ContentPane::Workbench, AgentPane::Workbench) => self.workbench_open(),
            _ => false,
        }
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
            self.open_workbench();
            return (true, action_id, "workbench open".to_string());
        }
        let Some(content) = pane.toggleable_content() else {
            return (false, action_id, "pane is not summonable".to_string());
        };
        if self.pane_of_content(&content).is_none() {
            self.toggle_pane(content);
        }
        (true, action_id, format!("{} open", pane.id()))
    }

    fn agent_toggle_pane(&mut self, pane: AgentPane) -> (bool, String, String) {
        let action_id = format!("pane.toggle.{}", pane.id());
        if pane == AgentPane::Workbench {
            self.toggle_workbench();
            return (true, action_id, "workbench toggled".to_string());
        }
        let Some(content) = pane.toggleable_content() else {
            return (false, action_id, "pane is not toggleable".to_string());
        };
        self.toggle_pane(content);
        (true, action_id, format!("{} toggled", pane.id()))
    }

    fn agent_invoke_command(&mut self, cmd: Command) -> (bool, String, String) {
        let action_id = format!("command.{cmd:?}").to_ascii_lowercase();
        self.runner.update(move |chrome| {
            chrome.run_command_and_close(cmd);
        });
        self.drain_pending_command();
        self.drain_pending_connect();
        self.sync_comms_pane();
        self.sync_settings();
        (true, action_id, cmd.label().to_string())
    }

    fn agent_select_node_by_url(&mut self, url: &str) -> (bool, String, String) {
        let action_id = "node.select_by_url".to_string();
        if !self.orrery.select_by_url(url) {
            return (false, action_id, format!("node not found: {url}"));
        }
        self.active_content = ContentPane::Orrery;
        self.sync_location();
        self.refresh_a11y_summary();
        (true, action_id, url.to_string())
    }

    fn agent_set_theme(&mut self, theme_id: &str) -> (bool, String, String) {
        let action_id = "theme.set".to_string();
        self.set_theme(theme_id);
        (true, action_id, self.active_theme_id.clone())
    }

    fn agent_activate_focused_action(&mut self) -> (bool, String, String) {
        let action_id = "focus.activate".to_string();
        if self.runner.state().palette_open {
            self.runner.update(meerkat::Chrome::run_palette_selection);
            self.drain_pending_command();
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
        if self.focused_member().is_none() {
            return (false, action_id, "no focused node".to_string());
        }
        self.toggle_live_preview();
        (true, action_id, "focused node preview toggled".to_string())
    }

    fn agent_retry_focused_content(&mut self) -> (bool, String, String) {
        let action_id = "content.retry.focused".to_string();
        let Some(url) = self.current_focus_url() else {
            return (false, action_id, "no focused node".to_string());
        };
        if !super::fetch::is_fetchable(&url) {
            return (false, action_id, format!("not fetchable: {url}"));
        }
        self.retry_focused_content();
        (
            true,
            action_id,
            "focused content retry requested".to_string(),
        )
    }

    fn agent_stop_focused_operation(&mut self) -> (bool, String, String) {
        let action_id = "operation.stop.focused".to_string();
        if self.focused_member().is_none() {
            return (false, action_id, "no focused node".to_string());
        }
        self.stop_focused_operation();
        (true, action_id, "focused operation stopped".to_string())
    }

    fn agent_pin_focused_operation(&mut self) -> (bool, String, String) {
        let action_id = "operation.pin.focused".to_string();
        if self.focused_member().is_none() {
            return (false, action_id, "no focused node".to_string());
        }
        self.pin_focused_operation();
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

    fn test_app() -> App {
        let (_tx, rx) = std::sync::mpsc::channel();
        App::new_with_session_dir(test_proxy(), rx, temp_session_dir())
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
        let focused = app.focused_member().expect("welcome is focused");
        assert!(
            app.constellation.is_background(focused),
            "pin marks the focused operation as background"
        );

        let step = app.apply_agent_action(AgentAction::StopFocusedOperation);
        assert!(step.result.applied);
        assert!(
            !app.constellation.is_active(focused),
            "stop reaps the focused operation"
        );
    }

    #[test]
    fn agent_can_select_node_and_report_blocked_actions() {
        let mut app = test_app();
        app.orrery.visit("https://example.test");
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
        app.orrery.visit("https://example.test");
        app.orrery.select_by_url("mere://welcome");
        app.refresh_a11y_summary();
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

        app.apply_a11y_request(crate::a11y_bridge::A11yActionRequest {
            action: Action::Focus,
            target_node: target,
        });

        assert_eq!(app.orrery.focused_url(), Some("https://example.test"));
        let observation = app.agent_observation();
        assert!(
            observation
                .diagnostics
                .iter()
                .any(|record| record.channel == "meerkat.agent.action_applied")
        );
    }
}
