/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Bridge selected `tracing` spans/events into the Apparatus diagnostics ring.

use std::fmt;
use std::sync::mpsc::Sender;
use std::time::Instant;

use register_diagnostics::{DiagnosticEvent, SpanPhase, StructuredPayloadField};
use tracing::field::{Field, Visit};
use tracing::{Event, Id, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

const TRACE_EVENT_CHANNEL: &str = "meerkat.tracing.event";

pub(crate) struct ApparatusTracingLayer {
    tx: Sender<DiagnosticEvent>,
}

impl ApparatusTracingLayer {
    pub(crate) fn new(tx: Sender<DiagnosticEvent>) -> Self {
        Self { tx }
    }

    fn emit(&self, event: DiagnosticEvent) {
        let _ = self.tx.send(event);
    }
}

impl<S> Layer<S> for ApparatusTracingLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let metadata = span.metadata();
        if !interesting_target(metadata.target()) {
            return;
        }
        span.extensions_mut().insert(SpanStart(Instant::now()));
        self.emit(DiagnosticEvent::Span {
            name: metadata.name(),
            phase: SpanPhase::Enter,
            duration_us: None,
        });
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let metadata = span.metadata();
        if !interesting_target(metadata.target()) {
            return;
        }
        let duration_us = span
            .extensions_mut()
            .remove::<SpanStart>()
            .map(|start| start.0.elapsed().as_micros() as u64);
        self.emit(DiagnosticEvent::Span {
            name: metadata.name(),
            phase: SpanPhase::Exit,
            duration_us,
        });
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        if !interesting_target(metadata.target()) {
            return;
        }
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        visitor.fields.push(StructuredPayloadField {
            name: "target",
            value: metadata.target().to_string(),
        });
        visitor.fields.push(StructuredPayloadField {
            name: "level",
            value: metadata.level().to_string(),
        });
        self.emit(DiagnosticEvent::MessageReceivedStructured {
            channel_id: TRACE_EVENT_CHANNEL,
            latency_us: 0,
            fields: visitor.fields,
        });
    }
}

#[derive(Debug)]
struct SpanStart(Instant);

#[derive(Default)]
struct FieldVisitor {
    fields: Vec<StructuredPayloadField>,
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.fields.push(StructuredPayloadField {
            name: static_field_name(field.name()),
            value: format!("{value:?}"),
        });
    }
}

fn interesting_target(target: &str) -> bool {
    target.starts_with("meerkat") || target.starts_with("frame") || target.starts_with("uxtree")
}

fn static_field_name(name: &str) -> &'static str {
    match name {
        "message" => "message",
        "err" => "err",
        "error" => "error",
        "path" => "path",
        "url" => "url",
        "ticket" => "ticket",
        "member" => "member",
        "background" => "background",
        "frame_id" => "frame_id",
        "node_count" => "node_count",
        "root_path" => "root_path",
        _ => "field",
    }
}
