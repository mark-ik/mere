/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HostObservability methods (record/snapshot/forget).

use super::*;

impl HostObservability {
    pub(crate) fn new() -> Self {
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
            last_forgetting: None,
        }
    }

    pub(crate) fn record_startup(&mut self, theme: &str, panes: usize) {
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

    pub(crate) fn record_diagnostic(
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

    /// Record a forgetting pass for Steward's live-ops view: the dropped count and
    /// when it ran. Complements the `alembic.forget` diagnostic (which lands in the
    /// Apparatus log) with a structured "last pass" Steward reads directly. (B2.)
    pub(crate) fn record_forgetting_pass(&mut self, dropped: usize) {
        self.last_forgetting = Some(ForgettingPass {
            dropped,
            at: Instant::now(),
        });
    }

    /// The most recent forgetting pass, if one has run this session. (B2.)
    pub(crate) fn last_forgetting(&self) -> Option<&ForgettingPass> {
        self.last_forgetting.as_ref()
    }

    pub(crate) fn record_ux_event(&mut self, event: UxEvent) {
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

    pub(crate) fn record_pane_toggle(&mut self, content: &PaneContent, opened: bool) {
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

    pub(crate) fn record_theme_activated(&mut self, theme_id: &str) {
        self.record_ux_note("theme", "activated", Some(theme_id.to_string()));
        self.record_diagnostic("meerkat.theme.activated", Severity::Info, theme_id);
    }

    pub(crate) fn record_frame_layout_changed(&mut self, detail: impl Into<String>) {
        self.record_diagnostic("meerkat.frame.layout_changed", Severity::Info, detail);
    }

    pub(crate) fn record_actor(
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

    pub(crate) fn record_portable_event(&mut self, event: DiagnosticEvent) {
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

    pub(crate) fn record_probe(
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

    pub(crate) fn set_a11y_snapshot(&mut self, snapshot: A11ySnapshot) {
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

    pub(crate) fn snapshot(&self) -> ObservabilitySnapshot {
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

    pub(crate) fn record_ux_note(&mut self, surface: &str, event: &str, detail: Option<String>) {
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

    pub(crate) fn observe_registry_channel(&mut self, channel: &str) {
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
