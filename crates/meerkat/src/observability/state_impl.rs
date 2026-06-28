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
            notifications: VecDeque::with_capacity(DEFAULT_CAPACITY),
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

    /// Record a user-facing **notification** (the Steward-accounted subsystem). `transient`
    /// notifications also surface as chrome toasts (drained by the host); all are kept in the
    /// log the Steward renders. Unlike `record_diagnostic` this is not registry-gated (a
    /// notification is an intentional user-facing event, not a dev channel). (Notifications.)
    pub(crate) fn record_notification(
        &mut self,
        severity: Severity,
        title: impl Into<String>,
        body: impl Into<String>,
        transient: bool,
    ) {
        push_bounded(
            &mut self.notifications,
            self.capacity,
            NotificationRecord {
                severity,
                title: title.into(),
                body: body.into(),
                at: Instant::now(),
                transient,
            },
        );
    }

    /// Steward rows for the notification log (the Steward-accounted subsystem): a header with
    /// the total count, then the most recent `limit` newest-first, each title →
    /// "[body ·] severity · age". Formatting lives here because `Severity::label` is
    /// module-private. (Notification subsystem.)
    pub(crate) fn notification_rows(&self, limit: usize) -> Vec<(String, String)> {
        let mut rows = vec![("Notifications".to_string(), self.notifications.len().to_string())];
        for n in self.notifications.iter().rev().take(limit) {
            let body = if n.body.is_empty() {
                String::new()
            } else {
                format!("{} \u{b7} ", n.body)
            };
            rows.push((
                format!("  {}", n.title),
                format!("{}{} \u{b7} {}", body, n.severity.label(), age(n.at)),
            ));
        }
        rows
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
            // A `tracing` event mirrored in by ApparatusTracingLayer is *not* a channel
            // message: recover its severity from the `level` field (so a warn/error fault reads
            // as a fault instead of being flattened to Info) and present it as a clean
            // "target: message (fields)" log line rather than the messaging-shaped
            // "received latency=…us" frame.
            DiagnosticEvent::MessageReceivedStructured {
                channel_id,
                fields,
                ..
            } if channel_id == crate::tracing_layer::TRACE_EVENT_CHANNEL => {
                let find = |name: &str| {
                    fields
                        .iter()
                        .find(|f| f.name == name)
                        .map(|f| f.value.as_str())
                };
                let severity = match find("level") {
                    Some("ERROR") | Some("error") => Severity::Error,
                    Some("WARN") | Some("warn") => Severity::Warn,
                    _ => Severity::Info,
                };
                let target = find("target").unwrap_or("trace");
                let message = find("message").unwrap_or("");
                let rest: Vec<_> = fields
                    .iter()
                    .filter(|f| !matches!(f.name, "target" | "level" | "message"))
                    .cloned()
                    .collect();
                let detail = if rest.is_empty() {
                    format!("{target}: {message}")
                } else {
                    format!("{target}: {message} ({})", format_fields(&rest))
                };
                self.record_diagnostic(channel_id, severity, detail);
            }
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
        // The a11y *state* has its own Accessibility section in the apparatus, so a healthy
        // per-interaction rebuild (focus move, pane toggle, nav) needn't also land in the general
        // diagnostics stream — it only crowds out the browsing pulse (fetch / layout / faults) in
        // the recent window. Only a *degraded* tree is a fault worth a diagnostic, logged at Warn.
        if self.a11y.degraded > 0 {
            self.record_diagnostic(
                "meerkat.a11y.tree_built",
                Severity::Warn,
                format!(
                    "surfaces={};degraded={}",
                    self.a11y.surfaces, self.a11y.degraded
                ),
            );
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_log_and_surface_for_the_steward() {
        let mut obs = HostObservability::new();
        obs.record_notification(Severity::Info, "Torn out", "kept as leaf", true);
        obs.record_notification(Severity::Warn, "Sync", String::new(), false);
        let rows = obs.notification_rows(5);
        // A header carrying the total count, then one row per recent notification.
        assert!(rows.iter().any(|(k, v)| k == "Notifications" && v == "2"));
        assert!(
            rows.iter().any(|(k, v)| k.contains("Torn out") && v.contains("kept as leaf")),
            "the leaf notification surfaces with its body"
        );
        assert!(
            rows.iter().any(|(k, _)| k.contains("Sync")),
            "the bodyless notification still surfaces"
        );
    }

    #[test]
    fn a_trace_fault_recovers_its_severity_and_reads_as_a_log_line() {
        use register_diagnostics::StructuredPayloadField;
        let mut obs = HostObservability::new();
        // A warn-level `tracing` event mirrored in by the bridge: target/level/message carried as
        // fields on the synthetic trace channel.
        obs.record_portable_event(DiagnosticEvent::MessageReceivedStructured {
            channel_id: crate::tracing_layer::TRACE_EVENT_CHANNEL,
            latency_us: 0,
            fields: vec![
                StructuredPayloadField { name: "target", value: "netfetcher".to_string() },
                StructuredPayloadField { name: "level", value: "WARN".to_string() },
                StructuredPayloadField { name: "message", value: "fetch network error".to_string() },
                StructuredPayloadField { name: "url", value: "https://x".to_string() },
            ],
        });
        let snap = obs.snapshot();
        let rec = snap
            .diagnostics
            .iter()
            .find(|d| d.channel == crate::tracing_layer::TRACE_EVENT_CHANNEL)
            .expect("the trace event was recorded as a diagnostic");
        assert!(
            matches!(rec.severity, Severity::Warn),
            "a warn-level trace fault recovers Warn severity, not the old hardcoded Info",
        );
        assert!(
            rec.message.starts_with("netfetcher: fetch network error"),
            "presented as a clean `target: message` log line: {}",
            rec.message,
        );
        assert!(
            rec.message.contains("url=https://x"),
            "the structured rest is appended: {}",
            rec.message,
        );
        assert!(
            !rec.message.contains("received latency"),
            "no messaging frame leaks through: {}",
            rec.message,
        );
    }

    #[test]
    fn a11y_snapshot_logs_a_diagnostic_only_when_degraded() {
        let mut obs = HostObservability::new();
        // A healthy rebuild updates the Accessibility state but stays out of the diagnostics
        // stream, so it can't crowd out the browsing pulse.
        obs.set_a11y_snapshot(A11ySnapshot { surfaces: 4, degraded: 0, ..Default::default() });
        assert!(
            obs.snapshot()
                .diagnostics
                .iter()
                .all(|d| d.channel != "meerkat.a11y.tree_built"),
            "a healthy a11y rebuild logs no diagnostic",
        );
        // A degraded tree is a real fault and surfaces, at Warn.
        obs.set_a11y_snapshot(A11ySnapshot { surfaces: 4, degraded: 2, ..Default::default() });
        let rec = obs
            .snapshot()
            .diagnostics
            .into_iter()
            .find(|d| d.channel == "meerkat.a11y.tree_built")
            .expect("a degraded a11y tree logs a diagnostic");
        assert!(matches!(rec.severity, Severity::Warn), "degraded a11y logs at Warn");
    }
}
