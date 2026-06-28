/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Bridge selected `tracing` spans/events into the Apparatus diagnostics ring.

use std::fmt;
use std::sync::OnceLock;
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

impl FieldVisitor {
    fn push(&mut self, field: &Field, value: String) {
        // `Field::name()` is `&'static str` (compile-time metadata), so the *real* field name
        // survives losslessly — no whitelist normalization (which collapsed unknown names to the
        // literal `"field"`). `StructuredPayloadField.name` is `&'static str`, which this satisfies.
        self.fields.push(StructuredPayloadField { name: field.name(), value });
    }
}

impl Visit for FieldVisitor {
    // Typed records first, so a `str`/number/bool field keeps its natural value (`5`, `true`,
    // `text`) instead of the `Debug`-quoted form (`"text"`); `record_debug` is the catch-all.
    fn record_str(&mut self, field: &Field, value: &str) {
        self.push(field, value.to_string());
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.push(field, value.to_string());
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.push(field, value.to_string());
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.push(field, value.to_string());
    }
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.push(field, format!("{value:?}"));
    }
}

/// Whether a span/event `target` is mirrored into the diagnostics ring. The default set covers the
/// first-party components (host, actor substrate, kernel, engines, content, comms, memory).
/// `MEERKAT_TRACE_TARGETS` (comma-separated prefixes) replaces the default for a dev session, so a
/// target can be narrowed to one component or widened to a custom one without a rebuild.
fn interesting_target(target: &str) -> bool {
    trace_target_prefixes()
        .iter()
        .any(|prefix| target.starts_with(prefix.as_str()))
}

/// The active target-prefix allowlist, parsed once. `MEERKAT_TRACE_TARGETS` overrides
/// [`DEFAULT_TRACE_TARGETS`] when set and non-empty.
fn trace_target_prefixes() -> &'static [String] {
    static PREFIXES: OnceLock<Vec<String>> = OnceLock::new();
    PREFIXES.get_or_init(|| {
        std::env::var("MEERKAT_TRACE_TARGETS")
            .ok()
            .map(|raw| {
                raw.split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
            })
            .filter(|parsed| !parsed.is_empty())
            .unwrap_or_else(|| DEFAULT_TRACE_TARGETS.iter().map(|s| s.to_string()).collect())
    })
}

/// First-party component target prefixes captured into Apparatus by default. Broadened from the
/// original `meerkat`/`frame`/`uxtree` so the actor substrate, kernel, and engines reach the ring.
const DEFAULT_TRACE_TARGETS: &[&str] = &[
    "meerkat",
    "frame",
    "uxtree",
    "armillary",
    "graph",
    "inker",
    "intel",
    "orrery",
    "mesh",
    "moot",
    "murm",
    "persona",
    "verso",
    "serval",
    "netfetcher",
    "netrender",
    "errand",
    "eidetic",
    "import",
    "forme",
    "shell",
    "platen",
    "session_runtime",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn interesting_target_covers_first_party_components_not_vendored() {
        for t in ["meerkat::input", "armillary::actor", "graph_kernel::graph", "verso_serval::flip"] {
            assert!(interesting_target(t), "{t} should be captured");
        }
        for t in ["blitz_dom::layout", "tokio::runtime", "wgpu_hal::vulkan"] {
            assert!(!interesting_target(t), "{t} should not be captured");
        }
    }

    #[test]
    fn the_env_filter_is_the_real_reach_gate() {
        use tracing_subscriber::EnvFilter;
        let (tx, rx) = mpsc::channel();
        // The real subscriber stack (main.rs): the default env filter as a global layer, then the
        // Apparatus bridge. `meerkat=info` is what ships when RUST_LOG is unset.
        let subscriber = tracing_subscriber::registry()
            .with(EnvFilter::new("meerkat=info"))
            .with(ApparatusTracingLayer::new(tx));
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "armillary", "actor started"); // first-party, allowlisted by the bridge
            tracing::info!(target: "meerkat", "host event");
        });
        let targets: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok())
            .filter_map(|ev| match ev {
                DiagnosticEvent::MessageReceivedStructured { fields, .. } => fields
                    .iter()
                    .find(|f| f.name == "target")
                    .map(|f| f.value.clone()),
                _ => None,
            })
            .collect();
        // The armillary event never reaches the bridge under `meerkat=info`: the env filter gates
        // it globally, upstream of the layer's allowlist. So broadening `interesting_target` (T1)
        // and instrumenting armillary (T2) do NOT reach Apparatus until the env-filter default
        // widens. The env filter, not `interesting_target`, is the real reach gate.
        assert!(targets.contains(&"meerkat".to_string()), "meerkat passes: {targets:?}");
        assert!(
            !targets.iter().any(|t| t.starts_with("armillary")),
            "armillary is gated by meerkat=info despite the bridge allowlist: {targets:?}",
        );
    }

    #[test]
    fn per_layer_split_feeds_the_ring_while_the_console_stays_quiet() {
        use tracing_subscriber::EnvFilter;
        use tracing_subscriber::Layer;
        use tracing_subscriber::filter::LevelFilter;
        let (console_tx, console_rx) = mpsc::channel();
        let (ring_tx, ring_rx) = mpsc::channel();
        // Mirror the fixed main.rs stack: a console-side layer carrying the RUST_LOG env filter
        // (here `meerkat=info`), and the Apparatus ring with its own LevelFilter::INFO. A second
        // bridge stands in for fmt so the console side is assertable. With per-layer filters there
        // is no global gate, so the ring is no longer starved (contrast the gate test above).
        let subscriber = tracing_subscriber::registry()
            .with(ApparatusTracingLayer::new(console_tx).with_filter(EnvFilter::new("meerkat=info")))
            .with(ApparatusTracingLayer::new(ring_tx).with_filter(LevelFilter::INFO));
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "armillary", "actor started");
            tracing::info!(target: "meerkat", "host event");
        });
        let targets = |rx: mpsc::Receiver<DiagnosticEvent>| -> Vec<String> {
            std::iter::from_fn(move || rx.try_recv().ok())
                .filter_map(|ev| match ev {
                    DiagnosticEvent::MessageReceivedStructured { fields, .. } => fields
                        .iter()
                        .find(|f| f.name == "target")
                        .map(|f| f.value.clone()),
                    _ => None,
                })
                .collect()
        };
        let console = targets(console_rx);
        let ring = targets(ring_rx);
        // The console (RUST_LOG=meerkat=info) still sees only meerkat; the ring now captures the
        // first-party armillary event too, regardless of RUST_LOG. This is the reach fix.
        assert!(
            console.contains(&"meerkat".to_string())
                && !console.iter().any(|t| t.starts_with("armillary")),
            "console stays scoped by RUST_LOG: {console:?}",
        );
        assert!(
            ring.iter().any(|t| t.starts_with("armillary")) && ring.contains(&"meerkat".to_string()),
            "ring captures first-party regardless of RUST_LOG: {ring:?}",
        );
    }

    #[test]
    fn event_fields_keep_their_real_names() {
        let (tx, rx) = mpsc::channel();
        let subscriber = tracing_subscriber::registry().with(ApparatusTracingLayer::new(tx));
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "armillary", custom_field = 7_i64, "hello");
        });
        let ev = rx.try_recv().expect("the event reaches the diagnostics ring (target broadened)");
        let DiagnosticEvent::MessageReceivedStructured { fields, .. } = ev else {
            panic!("an event becomes a structured diagnostic");
        };
        let custom = fields
            .iter()
            .find(|f| f.name == "custom_field")
            .expect("the field survives by its real name, not the old `field` placeholder");
        assert_eq!(custom.value, "7", "an i64 field keeps its natural value");
        assert!(
            fields.iter().all(|f| f.name != "field"),
            "no field collapsed to the retired whitelist placeholder",
        );
    }
}
