/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-local observability cache for Apparatus.
//!
//! This is an observation sink, not an authority: graph, frame, actor, chrome,
//! and accessibility state remain owned by their existing modules.

use std::collections::VecDeque;
use std::time::{Instant, SystemTime};

use frame::PaneContent;
use register_diagnostics::{
    ChannelRegistrationPolicy, DiagnosticEvent, DiagnosticsCapability, DiagnosticsChannelOwner,
    DiagnosticsInvariant, DiagnosticsRegistry, RuntimeChannelDescriptor, SpanPhase,
    StructuredPayloadField,
};
use ux_events::ux_diagnostics::{DiagnosticsSeverity, event_channel};
use ux_events::ux_observability::{DismissReason, SurfaceId, UxEvent};

mod registry;
mod state_impl;

use registry::*;

const DEFAULT_CAPACITY: usize = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Severity {
    Info,
    Warn,
    Error,
}

impl Severity {
    fn label(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

impl From<DiagnosticsSeverity> for Severity {
    fn from(value: DiagnosticsSeverity) -> Self {
        match value {
            DiagnosticsSeverity::Info => Self::Info,
            DiagnosticsSeverity::Warn => Self::Warn,
            DiagnosticsSeverity::Error => Self::Error,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct DiagnosticRecord {
    pub channel: String,
    pub severity: Severity,
    pub message: String,
    pub at: Instant,
}

/// A user-facing **notification** (distinct from a dev-facing `DiagnosticRecord`): the
/// Steward surfaces the log, and the chrome renders recent `transient` ones as toasts. The
/// notification subsystem's record; actionable toasts (buttons → host verbs) are a later
/// layer held in the chrome. (Notification subsystem; tear-out ambiguous-drag toast.)
#[derive(Clone, Debug)]
pub(super) struct NotificationRecord {
    pub severity: Severity,
    pub title: String,
    pub body: String,
    pub at: Instant,
    /// Whether this also surfaces as a transient toast (the chrome drains these); `false`
    /// = Steward-log only.
    pub transient: bool,
}

#[derive(Clone, Debug)]
pub(super) struct UxRecord {
    pub surface: String,
    pub event: String,
    pub detail: Option<String>,
    pub at: Instant,
}

#[derive(Clone, Debug)]
pub(super) struct ActorRecord {
    pub actor: String,
    pub event: String,
    pub detail: Option<String>,
    pub at: Instant,
}

#[derive(Clone, Debug)]
pub(super) struct ProbeRecord {
    pub name: String,
    pub status: String,
    pub detail: String,
    pub at: Instant,
}

#[derive(Clone, Debug)]
pub(super) struct TraceRecord {
    pub name: String,
    pub event: String,
    pub detail: Option<String>,
    pub at: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct A11ySnapshot {
    pub surfaces: usize,
    pub degraded: usize,
    pub nodes: usize,
    pub missing_labels: usize,
    pub missing_bounds: usize,
    pub duplicate_ids: usize,
    pub root: String,
    pub focus: String,
    pub audit: Vec<String>,
}

impl Default for A11ySnapshot {
    fn default() -> Self {
        Self {
            surfaces: 0,
            degraded: 0,
            nodes: 0,
            missing_labels: 0,
            missing_bounds: 0,
            duplicate_ids: 0,
            root: "unwired".to_string(),
            focus: "unwired".to_string(),
            audit: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct ObservabilitySnapshot {
    pub uptime: String,
    pub diagnostics: Vec<DiagnosticRecord>,
    pub ux: Vec<UxRecord>,
    pub actors: Vec<ActorRecord>,
    pub probes: Vec<ProbeRecord>,
    pub traces: Vec<TraceRecord>,
    pub a11y: A11ySnapshot,
    pub registry: RegistrySnapshot,
}

#[derive(Clone, Debug)]
pub(super) struct RegistrySnapshot {
    pub registered_channels: usize,
    pub orphan_channels: Vec<(String, u64)>,
    pub invariant_violations: Vec<String>,
}

/// The most recent forgetting pass (Athanor), surfaced live in Steward. The
/// dropped count plus when it ran make the pass a real tracked op, not only a
/// line in the Apparatus diagnostics log. (Alembic B2.)
#[derive(Clone, Debug)]
pub(super) struct ForgettingPass {
    pub dropped: usize,
    pub at: Instant,
}

pub(super) struct HostObservability {
    started: Instant,
    capacity: usize,
    registry: DiagnosticsRegistry,
    diagnostics: VecDeque<DiagnosticRecord>,
    /// User-facing notifications (the Steward-accounted subsystem; the chrome toasts the
    /// `transient` ones). (Notification subsystem.)
    notifications: VecDeque<NotificationRecord>,
    ux: VecDeque<UxRecord>,
    actors: VecDeque<ActorRecord>,
    probes: VecDeque<ProbeRecord>,
    traces: VecDeque<TraceRecord>,
    invariant_violations: VecDeque<String>,
    a11y: A11ySnapshot,
    /// The last forgetting pass, for Steward's live-ops view. (Alembic B2.)
    last_forgetting: Option<ForgettingPass>,
}

fn push_bounded<T>(buf: &mut VecDeque<T>, capacity: usize, value: T) {
    if capacity == 0 {
        return;
    }
    if buf.len() == capacity {
        buf.pop_front();
    }
    buf.push_back(value);
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn format_fields(fields: &[StructuredPayloadField]) -> String {
    fields
        .iter()
        .map(|field| format!("{}={}", field.name, field.value))
        .collect::<Vec<_>>()
        .join(";")
}

fn recent<T: Clone>(buf: &VecDeque<T>, limit: usize) -> Vec<T> {
    buf.iter()
        .rev()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn ux_parts(event: &UxEvent) -> (String, String, Option<String>) {
    match event {
        UxEvent::SurfaceOpened { surface } => (
            surface_label(*surface).to_string(),
            "opened".to_string(),
            None,
        ),
        UxEvent::SurfaceDismissed { surface, reason } => (
            surface_label(*surface).to_string(),
            "dismissed".to_string(),
            Some(format!("reason={reason:?}")),
        ),
        UxEvent::ActionDispatched { action_id, target } => (
            "action".to_string(),
            "dispatched".to_string(),
            Some(format!(
                "action={};targeted={}",
                action_id.key(),
                target.is_some()
            )),
        ),
        UxEvent::OpenNodeDispatched { .. } => {
            ("node".to_string(), "open_dispatched".to_string(), None)
        }
    }
}

fn surface_for_pane(content: &PaneContent) -> SurfaceId {
    match content {
        PaneContent::Workbench => SurfaceId::WorkbenchPane,
        PaneContent::Orrery => SurfaceId::CanvasPane,
        PaneContent::Gloss => SurfaceId::GlossPane,
        PaneContent::Roster => SurfaceId::RosterPane,
        PaneContent::Inspector => SurfaceId::InspectorPane,
        // Trail reuses the generic host surface for telemetry; a dedicated
        // SurfaceId::TrailPane is a later refinement.
        PaneContent::Trail => SurfaceId::NavigatorHost,
        // Alembic reuses the generic host surface for telemetry; a dedicated
        // SurfaceId::AlembicPane is a later refinement.
        PaneContent::Alembic => SurfaceId::NavigatorHost,
        PaneContent::Steward => SurfaceId::StewardPane,
        PaneContent::Comms => SurfaceId::CommsPane,
        PaneContent::Apparatus | PaneContent::System => SurfaceId::ApparatusPane,
        PaneContent::Tile(_) => SurfaceId::TilePane,
        PaneContent::Custom(_) => SurfaceId::NavigatorHost,
    }
}

fn surface_label(surface: SurfaceId) -> &'static str {
    match surface {
        SurfaceId::Omnibar => "omnibar",
        SurfaceId::CommandPalette => "command_palette",
        SurfaceId::NodeFinder => "node_finder",
        SurfaceId::ContextMenu => "context_menu",
        SurfaceId::ConfirmDialog => "confirm_dialog",
        SurfaceId::NodeCreate => "node_create",
        SurfaceId::FrameRename => "frame_rename",
        SurfaceId::StatusBar => "status_bar",
        SurfaceId::TreeSpine => "tree_spine",
        SurfaceId::NavigatorHost => "navigator_host",
        SurfaceId::TilePane => "tile_pane",
        SurfaceId::CanvasPane => "canvas_pane",
        SurfaceId::BaseLayer => "base_layer",
        SurfaceId::RosterPane => "roster",
        SurfaceId::InspectorPane => "inspector",
        SurfaceId::StewardPane => "steward",
        SurfaceId::GlossPane => "gloss",
        SurfaceId::ApparatusPane => "apparatus",
        SurfaceId::CommsPane => "comms",
        SurfaceId::WorkbenchPane => "workbench",
    }
}

fn format_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h {m}m {s}s")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}

pub(super) fn age(recorded_at: Instant) -> String {
    let secs = recorded_at.elapsed().as_secs();
    if secs < 1 {
        "now".to_string()
    } else {
        format!("{secs}s ago")
    }
}

pub(super) fn severity_label(severity: Severity) -> &'static str {
    severity.label()
}
