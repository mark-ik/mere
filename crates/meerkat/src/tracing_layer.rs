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

pub(crate) const TRACE_EVENT_CHANNEL: &str = "meerkat.tracing.event";

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
        // A span can be *re-entered* (a future polled repeatedly, a re-entrant guard), so `on_enter`
        // fires more than once for the same span. Record the start on the first enter only:
        // `ExtensionsMut::insert` **panics** on a duplicate, and that panic poisons the registry's
        // per-span RwLock and cascades into a process-wide crash on busy re-entrant spans. It took
        // down the p2panda address-book SQLite migration (entered/exited per statement on sqlx's
        // worker threads), silently disabling p2p sync + the murm cabal.
        {
            let mut extensions = span.extensions_mut();
            if extensions.get_mut::<SpanStart>().is_none() {
                extensions.insert(SpanStart(Instant::now()));
            }
        }
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
        // The tracing message is recorded under the `message` field; lift it out as the event's
        // message and keep the rest as the structured payload. `target` / `level` are first-class
        // on the `Event` variant, so they are not duplicated into the fields.
        let mut message = String::new();
        let mut fields = Vec::with_capacity(visitor.fields.len());
        for field in visitor.fields {
            if field.name == "message" {
                message = field.value;
            } else {
                fields.push(field);
            }
        }
        self.emit(DiagnosticEvent::Event {
            target: metadata.target(),
            level: level_name(metadata.level()),
            message,
            fields,
        });
    }
}

/// The tracing level's static name, for the portable [`DiagnosticEvent::Event`] variant.
fn level_name(level: &tracing::Level) -> &'static str {
    match *level {
        tracing::Level::ERROR => "ERROR",
        tracing::Level::WARN => "WARN",
        tracing::Level::INFO => "INFO",
        tracing::Level::DEBUG => "DEBUG",
        tracing::Level::TRACE => "TRACE",
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
                DiagnosticEvent::Event { target, .. } => Some(target.to_string()),
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
                    DiagnosticEvent::Event { target, .. } => Some(target.to_string()),
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
    fn the_ring_opts_into_library_completion_debug_but_not_per_frame() {
        use tracing_subscriber::EnvFilter;
        let (ring_tx, ring_rx) = mpsc::channel();
        // Mirror the ring filter in main.rs: an `info` floor plus per-target `=debug` opt-ins for the
        // sibling libraries' per-operation completion traces. `netrender` is left at the floor, so its
        // per-frame `frame rendered` debug is dropped while its faults (warn+) still reach the ring.
        let subscriber = tracing_subscriber::registry().with(
            ApparatusTracingLayer::new(ring_tx)
                .with_filter(EnvFilter::new("info,netfetcher=debug,errand=debug,serval_layout=debug")),
        );
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(target: "netfetcher", "fetch complete"); // opted in -> ring
            tracing::debug!(target: "serval_layout", "lay_out_content complete"); // opted in -> ring
            tracing::debug!(target: "netrender", "frame rendered"); // per-frame, info floor -> dropped
            tracing::warn!(target: "netrender", "render failed"); // fault, warn > info -> ring
            tracing::info!(target: "meerkat", "omnibar submit"); // lifecycle -> ring
        });
        let targets: Vec<String> = std::iter::from_fn(|| ring_rx.try_recv().ok())
            .filter_map(|ev| match ev {
                DiagnosticEvent::Event { target, .. } => Some(target.to_string()),
                _ => None,
            })
            .collect();
        assert!(
            targets.contains(&"netfetcher".to_string()),
            "a library's per-operation debug completion is opted into the ring: {targets:?}",
        );
        assert!(
            targets.contains(&"serval_layout".to_string()),
            "serval_layout's debug completion is opted in: {targets:?}",
        );
        assert!(
            targets.contains(&"meerkat".to_string()),
            "first-party info lifecycle reaches the ring: {targets:?}",
        );
        // netrender appears exactly once — its warn fault — not its per-frame debug (info floor drops it).
        assert_eq!(
            targets.iter().filter(|t| t.as_str() == "netrender").count(),
            1,
            "only netrender's fault reaches the ring, never its per-frame debug: {targets:?}",
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
        let DiagnosticEvent::Event { fields, .. } = ev else {
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

    #[test]
    fn re_entering_a_span_does_not_double_insert_and_panic() {
        let (tx, _rx) = mpsc::channel();
        let subscriber = tracing_subscriber::registry().with(ApparatusTracingLayer::new(tx));
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!(target: "meerkat", "reentrant");
            // Hold two enter guards on the SAME span (a future polled within its own span, a
            // re-entrant guard): `on_enter` fires twice with no exit between. Without the idempotent
            // `SpanStart` insert this panicked on the duplicate extension, poisoning the registry
            // RwLock and crashing the process (it took down sync + cabal via sqlx's worker threads).
            let _enter_once = span.enter();
            let _enter_twice = span.enter();
            // Reaching here without a panic is the assertion.
        });
    }
}
