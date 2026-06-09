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

pub(super) struct HostObservability {
    started: Instant,
    capacity: usize,
    registry: DiagnosticsRegistry,
    diagnostics: VecDeque<DiagnosticRecord>,
    ux: VecDeque<UxRecord>,
    actors: VecDeque<ActorRecord>,
    probes: VecDeque<ProbeRecord>,
    traces: VecDeque<TraceRecord>,
    invariant_violations: VecDeque<String>,
    a11y: A11ySnapshot,
}

impl HostObservability {
    pub(super) fn new() -> Self {
        Self {
            started: Instant::now(),
            capacity: DEFAULT_CAPACITY,
            registry: initial_registry(),
            diagnostics: VecDeque::with_capacity(DEFAULT_CAPACITY),
            ux: VecDeque::with_capacity(DEFAULT_CAPACITY),
            actors: VecDeque::with_capacity(DEFAULT_CAPACITY),
            probes: VecDeque::with_capacity(DEFAULT_CAPACITY),
            traces: VecDeque::with_capacity(DEFAULT_CAPACITY),
            invariant_violations: VecDeque::with_capacity(DEFAULT_CAPACITY),
            a11y: A11ySnapshot::default(),
        }
    }

    pub(super) fn record_startup(&mut self, theme: &str, panes: usize) {
        self.record_diagnostic(
            "meerkat.startup.started",
            Severity::Info,
            "app state initialized",
        );
        self.record_diagnostic(
            "meerkat.startup.succeeded",
            Severity::Info,
            format!("theme={theme};panes={panes}"),
        );
    }

    pub(super) fn record_diagnostic(
        &mut self,
        channel: impl Into<String>,
        severity: Severity,
        message: impl Into<String>,
    ) {
        let channel = channel.into();
        let message = message.into();
        if !self.registry.should_emit_channel(&channel) {
            return;
        }
        self.observe_registry_channel(&channel);
        push_bounded(
            &mut self.diagnostics,
            self.capacity,
            DiagnosticRecord {
                channel,
                severity,
                message,
                at: Instant::now(),
            },
        );
    }

    pub(super) fn record_ux_event(&mut self, event: UxEvent) {
        let emission = event_channel(&event);
        let (surface, kind, detail) = ux_parts(&event);
        push_bounded(
            &mut self.ux,
            self.capacity,
            UxRecord {
                surface,
                event: kind,
                detail: detail.or(emission.note.clone()),
                at: Instant::now(),
            },
        );
        self.record_diagnostic(
            emission.channel_id,
            Severity::from(emission.severity),
            emission.note.unwrap_or_else(|| format!("{event:?}")),
        );
    }

    pub(super) fn record_pane_toggle(&mut self, content: &PaneContent, opened: bool) {
        let surface = surface_for_pane(content);
        let event = if opened {
            UxEvent::SurfaceOpened { surface }
        } else {
            UxEvent::SurfaceDismissed {
                surface,
                reason: DismissReason::Cancelled,
            }
        };
        self.record_ux_event(event);
    }

    pub(super) fn record_theme_activated(&mut self, theme_id: &str) {
        self.record_ux_note("theme", "activated", Some(theme_id.to_string()));
        self.record_diagnostic("meerkat.theme.activated", Severity::Info, theme_id);
    }

    pub(super) fn record_frame_layout_changed(&mut self, detail: impl Into<String>) {
        self.record_diagnostic("meerkat.frame.layout_changed", Severity::Info, detail);
    }

    pub(super) fn record_actor(
        &mut self,
        actor: impl Into<String>,
        event: impl Into<String>,
        detail: Option<String>,
    ) {
        let actor = actor.into();
        let event = event.into();
        let message = detail.clone().unwrap_or_default();
        self.record_diagnostic(
            format!("meerkat.actor.{actor}.{event}"),
            Severity::Info,
            message.clone(),
        );
        push_bounded(
            &mut self.actors,
            self.capacity,
            ActorRecord {
                actor,
                event,
                detail,
                at: Instant::now(),
            },
        );
    }

    pub(super) fn record_portable_event(&mut self, event: DiagnosticEvent) {
        match event {
            DiagnosticEvent::Span {
                name,
                phase,
                duration_us,
            } => {
                let event = match phase {
                    SpanPhase::Enter => "enter",
                    SpanPhase::Exit => "exit",
                };
                push_bounded(
                    &mut self.traces,
                    self.capacity,
                    TraceRecord {
                        name: name.to_string(),
                        event: event.to_string(),
                        detail: duration_us.map(|us| format!("{us}us")),
                        at: Instant::now(),
                    },
                );
            }
            DiagnosticEvent::MessageSent {
                channel_id,
                byte_len,
            } => {
                self.record_diagnostic(channel_id, Severity::Info, format!("sent {byte_len} bytes"))
            }
            DiagnosticEvent::MessageSentStructured {
                channel_id,
                byte_len,
                fields,
            } => self.record_diagnostic(
                channel_id,
                Severity::Info,
                format!("sent {byte_len} bytes; {}", format_fields(&fields)),
            ),
            DiagnosticEvent::MessageReceived {
                channel_id,
                latency_us,
            } => self.record_diagnostic(
                channel_id,
                Severity::Info,
                format!("received latency={latency_us}us"),
            ),
            DiagnosticEvent::MessageReceivedStructured {
                channel_id,
                latency_us,
                fields,
            } => self.record_diagnostic(
                channel_id,
                Severity::Info,
                format!(
                    "received latency={latency_us}us; {}",
                    format_fields(&fields)
                ),
            ),
        }
    }

    pub(super) fn record_probe(
        &mut self,
        name: impl Into<String>,
        status: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let name = name.into();
        let status = status.into();
        let detail = detail.into();
        let channel = if status == "failed" {
            "meerkat.probe.failed"
        } else {
            "meerkat.probe.degraded"
        };
        self.record_diagnostic(channel, Severity::Info, format!("{name}: {detail}"));
        push_bounded(
            &mut self.probes,
            self.capacity,
            ProbeRecord {
                name,
                status,
                detail,
                at: Instant::now(),
            },
        );
    }

    pub(super) fn set_a11y_snapshot(&mut self, snapshot: A11ySnapshot) {
        if self.a11y == snapshot {
            return;
        }
        self.a11y = snapshot;
        self.record_diagnostic(
            "meerkat.a11y.tree_built",
            Severity::Info,
            format!(
                "surfaces={};degraded={}",
                self.a11y.surfaces, self.a11y.degraded
            ),
        );
    }

    pub(super) fn snapshot(&self) -> ObservabilitySnapshot {
        ObservabilitySnapshot {
            uptime: format_duration(self.started.elapsed().as_secs()),
            diagnostics: recent(&self.diagnostics, 8),
            ux: recent(&self.ux, 8),
            actors: recent(&self.actors, 8),
            probes: recent(&self.probes, 6),
            traces: recent(&self.traces, 6),
            a11y: self.a11y.clone(),
            registry: RegistrySnapshot {
                registered_channels: self.registry.list_channel_configs().len(),
                orphan_channels: self.registry.list_orphan_channels(),
                invariant_violations: recent(&self.invariant_violations, 6),
            },
        }
    }

    fn record_ux_note(&mut self, surface: &str, event: &str, detail: Option<String>) {
        push_bounded(
            &mut self.ux,
            self.capacity,
            UxRecord {
                surface: surface.to_string(),
                event: event.to_string(),
                detail,
                at: Instant::now(),
            },
        );
    }

    fn observe_registry_channel(&mut self, channel: &str) {
        let violations = self
            .registry
            .observe_channel_event(channel, current_unix_ms());
        for violation in violations {
            let summary = format!(
                "{} timed out after {}",
                violation.invariant_id, violation.start_channel
            );
            push_bounded(
                &mut self.invariant_violations,
                self.capacity,
                summary.clone(),
            );
            push_bounded(
                &mut self.probes,
                self.capacity,
                ProbeRecord {
                    name: "diagnostics_invariant".to_string(),
                    status: "failed".to_string(),
                    detail: summary,
                    at: Instant::now(),
                },
            );
        }
    }
}

fn initial_registry() -> DiagnosticsRegistry {
    let mut registry = DiagnosticsRegistry::default();
    for (channel, severity, description) in INITIAL_CHANNELS {
        let descriptor = match severity {
            Severity::Info => RuntimeChannelDescriptor::info(
                *channel,
                1,
                DiagnosticsChannelOwner::runtime(),
                Some((*description).to_string()),
            ),
            Severity::Warn => RuntimeChannelDescriptor::warn(
                *channel,
                1,
                DiagnosticsChannelOwner::runtime(),
                Some((*description).to_string()),
            ),
            Severity::Error => RuntimeChannelDescriptor::error(
                *channel,
                1,
                DiagnosticsChannelOwner::runtime(),
                Some((*description).to_string()),
            ),
        };
        let _ =
            registry.register_runtime_channel(descriptor, ChannelRegistrationPolicy::KeepExisting);
    }
    for invariant in INITIAL_INVARIANTS {
        let _ = registry.register_invariant(
            DiagnosticsInvariant {
                invariant_id: invariant.id.to_string(),
                start_channel: invariant.start.to_string(),
                terminal_channels: invariant
                    .terminal
                    .iter()
                    .map(|channel| (*channel).to_string())
                    .collect(),
                timeout_ms: invariant.timeout_ms,
                owner: DiagnosticsChannelOwner::runtime(),
                enabled: true,
            },
            &[DiagnosticsCapability::RegisterInvariants],
        );
    }
    registry
}

struct InitialInvariant {
    id: &'static str,
    start: &'static str,
    terminal: &'static [&'static str],
    timeout_ms: u64,
}

const INITIAL_CHANNELS: &[(&str, Severity, &str)] = &[
    (
        "meerkat.startup.started",
        Severity::Info,
        "Meerkat startup began",
    ),
    (
        "meerkat.startup.succeeded",
        Severity::Info,
        "Meerkat startup completed",
    ),
    (
        "meerkat.startup.failed",
        Severity::Error,
        "Meerkat startup failed",
    ),
    (
        "meerkat.frame.layout_changed",
        Severity::Info,
        "Frame layout changed",
    ),
    (
        "meerkat.frame.pane_summoned",
        Severity::Info,
        "Frame pane opened",
    ),
    (
        "meerkat.frame.pane_closed",
        Severity::Info,
        "Frame pane closed",
    ),
    (
        "meerkat.frame.divider_dragged",
        Severity::Info,
        "Frame divider moved",
    ),
    (
        "meerkat.ui.action_dispatched",
        Severity::Info,
        "UI action dispatched",
    ),
    (
        "meerkat.ui.surface_opened",
        Severity::Info,
        "UI surface opened",
    ),
    (
        "meerkat.ui.surface_dismissed",
        Severity::Info,
        "UI surface dismissed",
    ),
    (
        "meerkat.ui.focus_changed",
        Severity::Info,
        "UI focus changed",
    ),
    ("meerkat.theme.activated", Severity::Info, "Theme activated"),
    (
        "meerkat.actor.fetch.started",
        Severity::Info,
        "Fetch actor work started",
    ),
    (
        "meerkat.actor.fetch.succeeded",
        Severity::Info,
        "Fetch actor work succeeded",
    ),
    (
        "meerkat.actor.fetch.failed",
        Severity::Warn,
        "Fetch actor work failed",
    ),
    (
        "meerkat.actor.fetch.subresource",
        Severity::Info,
        "Fetch actor subresource completed",
    ),
    (
        "meerkat.actor.sync.status",
        Severity::Info,
        "Sync actor status changed",
    ),
    (
        "meerkat.actor.sync.started",
        Severity::Info,
        "Sync actor work started",
    ),
    (
        "meerkat.actor.sync.succeeded",
        Severity::Info,
        "Sync actor work succeeded",
    ),
    (
        "meerkat.actor.sync.failed",
        Severity::Warn,
        "Sync actor work failed",
    ),
    (
        "meerkat.actor.comms.inbox",
        Severity::Info,
        "Comms inbox updated",
    ),
    (
        "meerkat.actor.comms.thread",
        Severity::Info,
        "Comms thread updated",
    ),
    (
        "meerkat.actor.comms.sent",
        Severity::Info,
        "Comms draft sent",
    ),
    (
        "meerkat.actor.comms.send_outcome",
        Severity::Info,
        "Comms send outcome",
    ),
    (
        "meerkat.actor.comms.identity",
        Severity::Info,
        "Comms identity updated",
    ),
    (
        "meerkat.actor.comms.started",
        Severity::Info,
        "Comms actor work started",
    ),
    (
        "meerkat.actor.comms.succeeded",
        Severity::Info,
        "Comms actor work succeeded",
    ),
    (
        "meerkat.actor.comms.failed",
        Severity::Warn,
        "Comms actor work failed",
    ),
    (
        "meerkat.actor.content.respawned",
        Severity::Warn,
        "Content actor respawned",
    ),
    (
        "meerkat.actor.content.failed",
        Severity::Warn,
        "Content actor failed",
    ),
    (
        "meerkat.a11y.tree_built",
        Severity::Info,
        "Accessibility summary built",
    ),
    (
        "meerkat.a11y.bounds_missing",
        Severity::Warn,
        "Accessibility bounds missing",
    ),
    (
        "meerkat.a11y.label_missing",
        Severity::Warn,
        "Accessibility label missing",
    ),
    (
        "meerkat.a11y.focus_missing",
        Severity::Warn,
        "Accessibility focus missing",
    ),
    ("meerkat.probe.failed", Severity::Warn, "Probe failed"),
    ("meerkat.probe.degraded", Severity::Info, "Probe degraded"),
    (
        "meerkat.tracing.event",
        Severity::Info,
        "Tracing event bridged into Apparatus",
    ),
    ("meerkat.agent.spawned", Severity::Info, "Agent spawned"),
    (
        "meerkat.agent.intent_dropped",
        Severity::Warn,
        "Agent intent dropped",
    ),
    (
        "meerkat.agent.action_applied",
        Severity::Info,
        "Agent action applied",
    ),
    (
        "ux.roster_pane.opened",
        Severity::Info,
        "Roster pane opened",
    ),
    (
        "ux.roster_pane.dismissed",
        Severity::Info,
        "Roster pane dismissed",
    ),
    ("ux.gloss_pane.opened", Severity::Info, "Gloss pane opened"),
    (
        "ux.gloss_pane.dismissed",
        Severity::Info,
        "Gloss pane dismissed",
    ),
    (
        "ux.apparatus_pane.opened",
        Severity::Info,
        "Apparatus pane opened",
    ),
    (
        "ux.apparatus_pane.dismissed",
        Severity::Info,
        "Apparatus pane dismissed",
    ),
    ("ux.comms_pane.opened", Severity::Info, "Comms pane opened"),
    (
        "ux.comms_pane.dismissed",
        Severity::Info,
        "Comms pane dismissed",
    ),
    (
        "ux.workbench_pane.opened",
        Severity::Info,
        "Workbench pane opened",
    ),
    (
        "ux.workbench_pane.dismissed",
        Severity::Info,
        "Workbench pane dismissed",
    ),
];

const INITIAL_INVARIANTS: &[InitialInvariant] = &[
    InitialInvariant {
        id: "invariant.meerkat.startup.completes",
        start: "meerkat.startup.started",
        terminal: &["meerkat.startup.succeeded", "meerkat.startup.failed"],
        timeout_ms: 3_000,
    },
    InitialInvariant {
        id: "invariant.meerkat.fetch.completes",
        start: "meerkat.actor.fetch.started",
        terminal: &[
            "meerkat.actor.fetch.succeeded",
            "meerkat.actor.fetch.failed",
        ],
        timeout_ms: 10_000,
    },
    InitialInvariant {
        id: "invariant.meerkat.comms.completes",
        start: "meerkat.actor.comms.started",
        terminal: &[
            "meerkat.actor.comms.succeeded",
            "meerkat.actor.comms.failed",
        ],
        timeout_ms: 10_000,
    },
];

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
