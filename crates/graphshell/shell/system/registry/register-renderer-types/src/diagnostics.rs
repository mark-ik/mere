// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Diagnostic events the renderer registry emits, plus the sink trait
//! a host consumes them through.
//!
//! Per the renderer-registry contract brief §8, registry-level
//! diagnostics observe lifecycle and routing without coupling the
//! substrate to a specific transport. v0a vocabulary covers the
//! state-change events that fire when something actually happens
//! (registration, misroute); the per-frame routing events
//! (`engine.route_chosen` from the brief) wait on a host-side
//! mechanism for deduping the per-frame firehose.

use crate::identity::RendererId;
use crate::scene::{NodeContentKind, NodeIdentity};

/// Why a route resolution was degraded — i.e., the substrate's
/// dispatch couldn't proceed cleanly through the expected path. The
/// host typically surfaces these to the user as warnings, or routes
/// them to telemetry.
#[derive(Clone, Debug, PartialEq)]
pub enum RouteDegradedReason {
    /// No registered renderer handles the node's `NodeContentKind`.
    /// Host typically paints a placeholder for the node.
    NoCandidates,
    /// The selector resolved a renderer whose `composition_mode()` /
    /// downcast claim doesn't match the dispatch the substrate
    /// attempted. Renderer impl bug: `composition_mode()` returns
    /// `InScenePaint` but `as_in_scene_paint()` returns `None` (or
    /// the dual for EmbeddedFrame). The wrong renderer id is
    /// included for the host to flag.
    WrongCompositionMode { renderer: RendererId },
}

/// Diagnostic events emitted by the renderer registry and substrate
/// dispatch. Non-exhaustive — new variants may be added without a
/// major version bump as new event classes are wired (route_chosen
/// dedupe, hot_swap, surface_attach_failed).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum DiagnosticEvent {
    /// `renderer.registered` — a new renderer was added to the
    /// registry. Carries the id and the set of content kinds the
    /// renderer claims to handle.
    RendererRegistered {
        id: RendererId,
        kinds: Vec<NodeContentKind>,
    },
    /// `renderer.unregistered` — a renderer was removed from the
    /// registry. Fires only when the id was actually registered.
    RendererUnregistered { id: RendererId },
    /// `renderer.hot_swapped` — a registered renderer was replaced
    /// in-place by a new implementation at the same id. Carries the
    /// new renderer's handled content kinds (may differ from the
    /// pre-swap impl's kinds). Producer handles owned by the
    /// substrate-host are stale after this event; hosts typically
    /// re-call `SubstrateHost::ensure_embedded_producers` on the
    /// next frame to refresh.
    RendererHotSwapped {
        id: RendererId,
        kinds: Vec<NodeContentKind>,
    },
    /// `engine.route_degraded` — substrate dispatch attempted to
    /// route `node` but the path didn't resolve cleanly. Fires once
    /// per affected dispatch call.
    RouteDegraded {
        node: NodeIdentity,
        reason: RouteDegradedReason,
    },
}

/// Sink the registry hands diagnostic events to. Substrate-side: the
/// trait. Host-side: the implementation that routes events to the
/// host's action bus / telemetry / log.
///
/// Sink methods are `&self` — implementations needing mutable state
/// should use interior mutability (Cell, RefCell, Mutex,
/// AtomicUsize, etc.). This keeps the registry's emit path infallible
/// and lock-free at the trait level; concurrency is the sink's
/// concern.
pub trait DiagnosticSink {
    /// Record `event`. The default impl drops it.
    fn record(&self, event: DiagnosticEvent) {
        let _ = event;
    }
}

/// Default no-op sink. Used when the registry is constructed without
/// an explicit sink — keeps the diagnostic call path inert.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopSink;

impl DiagnosticSink for NoopSink {}

/// Sink that records every event into a thread-local-ish buffer for
/// test inspection. Use [`RecordingSink::events`] to read back what
/// was emitted.
#[derive(Debug, Default)]
pub struct RecordingSink {
    events: std::sync::Mutex<Vec<DiagnosticEvent>>,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the recorded events (clones the buffer).
    pub fn events(&self) -> Vec<DiagnosticEvent> {
        self.events.lock().expect("RecordingSink lock").clone()
    }

    /// Number of events recorded so far.
    pub fn len(&self) -> usize {
        self.events.lock().expect("RecordingSink lock").len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.lock().expect("RecordingSink lock").is_empty()
    }
}

impl DiagnosticSink for RecordingSink {
    fn record(&self, event: DiagnosticEvent) {
        self.events.lock().expect("RecordingSink lock").push(event);
    }
}
