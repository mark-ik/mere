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
mod tests;
