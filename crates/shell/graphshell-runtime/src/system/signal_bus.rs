/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Register-owned signal routing layer — the typed signal bus through which
//! system-layer events (navigation, lifecycle, sync, registry events, input
//! events) fan out to sync observers and async broadcast subscribers.
//!
//! Consolidated from the shell-side `shell/desktop/runtime/registries/signal_routing.rs`
//! per Slice 51 (Phase 2 of the workspace architecture proposal). The shell-side
//! file is now a `pub use` shim; the canonical body lives here.
//!
//! Cross-crate retargets vs. the original shell-side file:
//! - `crate::graph::NodeKey` → `graphshell_core::graph::NodeKey`
//! - `crate::shell::desktop::runtime::diagnostics::*` → `register_diagnostics::*`
//! - `super::CHANNEL_REGISTER_SIGNAL_ROUTING_LAGGED` → `register_diagnostics::channels::*`
//! - `pub` → `pub` (items are now the public API of `graphshell-runtime::system::signal_bus`)

use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use graphshell_core::graph::NodeKey;
use register_diagnostics::channels::CHANNEL_REGISTER_SIGNAL_ROUTING_LAGGED;
use register_diagnostics::{DiagnosticEvent, emit_event};
use tokio::sync::broadcast;

/// Topic families used by the Register signal routing layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalTopic {
    Navigation,
    Lifecycle,
    Sync,
    RegistryEvent,
    InputEvent,
}

impl SignalTopic {
    const ALL: [SignalTopic; 5] = [
        SignalTopic::Navigation,
        SignalTopic::Lifecycle,
        SignalTopic::Sync,
        SignalTopic::RegistryEvent,
        SignalTopic::InputEvent,
    ];

    fn label(self) -> &'static str {
        match self {
            SignalTopic::Navigation => "navigation",
            SignalTopic::Lifecycle => "lifecycle",
            SignalTopic::Sync => "sync",
            SignalTopic::RegistryEvent => "registry_event",
            SignalTopic::InputEvent => "input_event",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationSignal {
    Resolved {
        uri: String,
        viewer_id: String,
    },
    NodeActivated {
        key: NodeKey,
        uri: String,
        title: String,
    },
    MimeResolved {
        key: NodeKey,
        uri: String,
        mime_hint: Option<String>,
        viewer_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleSignal {
    SemanticIndexUpdated {
        indexed_nodes: usize,
    },
    MimeResolved {
        node_key: NodeKey,
        mime: String,
    },
    WorkflowActivated {
        workflow_id: String,
    },
    MemoryPressureChanged {
        level: String,
        available_mib: u64,
        total_mib: u64,
    },
    /// Emitted when no user gesture has been produced for longer than the
    /// configured idle threshold. Tier 1 workers enter low-frequency mode.
    UserIdle {
        /// Milliseconds since UNIX epoch of the last observed user gesture.
        since_ms: u64,
    },
    /// Emitted when a user gesture is observed after a `UserIdle` period.
    UserResumed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncSignal {
    RemoteEntriesQueued,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryEventSignal {
    ThemeChanged { new_theme_id: String },
    LensChanged { new_lens_id: String },
    WorkflowChanged { new_workflow_id: String },
    PhysicsProfileChanged { new_profile_id: String },
    CanvasProfileChanged { new_profile_id: String },
    WorkbenchSurfaceChanged { new_profile_id: String },
    SemanticIndexUpdated { indexed_nodes: usize },
    SettingsRouteRequested { url: String },
    ModLoaded { mod_id: String },
    ModUnloaded { mod_id: String },
    AgentSpawned { agent_id: String },
    IdentityRotated { identity_id: String },
    WorkbenchProjectionRefreshRequested { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEventSignal {
    ContextChanged { new_context: String },
    BindingRemapped { action_id: String },
    BindingsReset,
}

/// Typed signal kinds emitted through Register-owned routing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalKind {
    Navigation(NavigationSignal),
    Lifecycle(LifecycleSignal),
    Sync(SyncSignal),
    RegistryEvent(RegistryEventSignal),
    InputEvent(InputEventSignal),
}

impl SignalKind {
    pub fn topic(&self) -> SignalTopic {
        match self {
            Self::Navigation(..) => SignalTopic::Navigation,
            Self::Lifecycle(..) => SignalTopic::Lifecycle,
            Self::Sync(..) => SignalTopic::Sync,
            Self::RegistryEvent(..) => SignalTopic::RegistryEvent,
            Self::InputEvent(..) => SignalTopic::InputEvent,
        }
    }
}

/// Producer identity for tracing and causality debugging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalSource {
    RegistryRuntime,
    ControlPanel,
}

/// Typed signal envelope with source metadata and optional causality stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalEnvelope {
    pub kind: SignalKind,
    pub source: SignalSource,
    pub emitted_at: Instant,
    pub causality_stamp: Option<u64>,
}

impl SignalEnvelope {
    pub fn new(kind: SignalKind, source: SignalSource, causality_stamp: Option<u64>) -> Self {
        Self {
            kind,
            source,
            emitted_at: Instant::now(),
            causality_stamp,
        }
    }
}

/// Stable identifier for a registered observer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObserverId(u64);

pub type SyncObserverCallback = Arc<dyn Fn(&SignalEnvelope) -> Result<(), String> + Send + Sync>;

pub trait SignalBus: Send + Sync {
    fn publish(&self, envelope: SignalEnvelope) -> SignalPublishReport;
    fn subscribe_sync(&self, topic: SignalTopic, callback: SyncObserverCallback) -> ObserverId;
    fn unsubscribe(&self, topic: SignalTopic, id: ObserverId) -> bool;
    fn subscribe_async(&self, topic: SignalTopic) -> AsyncSignalSubscription;
    fn subscribe_all(&self) -> AsyncSignalSubscription;
    fn diagnostics(&self) -> SignalRoutingDiagnostics;
    fn dead_letters(&self) -> Vec<SignalDeadLetter>;
    fn signal_trace(&self) -> Vec<SignalTraceEntry>;
}

const DEAD_LETTER_LIMIT: usize = 64;
const SIGNAL_TRACE_LIMIT: usize = 128;
const ASYNC_SIGNAL_BUFFER: usize = 64;

#[derive(Clone)]
struct SignalObserver {
    id: ObserverId,
    callback: SyncObserverCallback,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SignalRoutingDiagnostics {
    pub published_signals: u64,
    pub routed_deliveries: u64,
    pub unrouted_signals: u64,
    pub observer_failures: u64,
    pub lagged_receivers: u64,
    pub queue_depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignalPublishReport {
    pub observers_notified: usize,
    pub observer_failures: usize,
    pub dead_letters_added: usize,
    pub queue_depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalDeadLetterReason {
    Unrouted,
    ObserverFailed,
    ObserverPanicked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalDeadLetter {
    pub envelope: SignalEnvelope,
    pub observer_id: Option<ObserverId>,
    pub reason: SignalDeadLetterReason,
    pub detail: String,
}

/// A single entry in the signal trace ring, recording what was published and how it was routed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalTraceEntry {
    pub kind: SignalKind,
    pub source: SignalSource,
    pub emitted_at: Instant,
    pub causality_stamp: Option<u64>,
    pub observers_notified: usize,
    pub observer_failures: usize,
}

#[derive(Default)]
struct SignalRoutingState {
    next_observer_id: u64,
    observers: HashMap<SignalTopic, Vec<SignalObserver>>,
    diagnostics: SignalRoutingDiagnostics,
    dead_letters: VecDeque<SignalDeadLetter>,
    signal_trace: VecDeque<SignalTraceEntry>,
}

pub struct AsyncSignalSubscription {
    label: &'static str,
    receiver: broadcast::Receiver<SignalEnvelope>,
    state: Arc<Mutex<SignalRoutingState>>,
}

impl AsyncSignalSubscription {
    pub async fn recv(&mut self) -> Option<SignalEnvelope> {
        loop {
            match self.receiver.recv().await {
                Ok(envelope) => return Some(envelope),
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    let mut guard = self.state.lock().expect("signal routing lock poisoned");
                    guard.diagnostics.lagged_receivers = guard
                        .diagnostics
                        .lagged_receivers
                        .saturating_add(skipped as u64);
                    drop(guard);
                    emit_event(DiagnosticEvent::MessageSent {
                        channel_id: CHANNEL_REGISTER_SIGNAL_ROUTING_LAGGED,
                        byte_len: skipped as usize,
                    });
                    log::warn!(
                        "signal_routing: async subscriber for {} lagged and skipped {} signal(s)",
                        self.label,
                        skipped
                    );
                }
            }
        }
    }
}

/// SR2/SR3 transitional Register-owned signal routing facade and in-process fabric.
#[derive(Clone)]
pub struct SignalRoutingLayer {
    state: Arc<Mutex<SignalRoutingState>>,
    topic_broadcast_tx: Arc<HashMap<SignalTopic, broadcast::Sender<SignalEnvelope>>>,
    all_broadcast_tx: broadcast::Sender<SignalEnvelope>,
}

impl Default for SignalRoutingLayer {
    fn default() -> Self {
        Self::with_async_capacity(ASYNC_SIGNAL_BUFFER)
    }
}

impl SignalRoutingLayer {
    pub fn with_async_capacity(async_capacity: usize) -> Self {
        let topic_broadcast_tx = SignalTopic::ALL
            .into_iter()
            .map(|topic| {
                let (tx, _rx) = broadcast::channel(async_capacity);
                (topic, tx)
            })
            .collect::<HashMap<_, _>>();
        let (all_broadcast_tx, _all_rx) = broadcast::channel(async_capacity);
        Self {
            state: Arc::new(Mutex::new(SignalRoutingState::default())),
            topic_broadcast_tx: Arc::new(topic_broadcast_tx),
            all_broadcast_tx,
        }
    }

    pub fn subscribe(
        &self,
        topic: SignalTopic,
        callback: impl Fn(&SignalEnvelope) -> Result<(), String> + Send + Sync + 'static,
    ) -> ObserverId {
        let mut guard = self.state.lock().expect("signal routing lock poisoned");
        guard.next_observer_id = guard.next_observer_id.saturating_add(1);
        let id = ObserverId(guard.next_observer_id);
        let observer = SignalObserver {
            id,
            callback: Arc::new(callback),
        };
        guard.observers.entry(topic).or_default().push(observer);
        id
    }

    pub fn unsubscribe(&self, topic: SignalTopic, id: ObserverId) -> bool {
        let mut guard = self.state.lock().expect("signal routing lock poisoned");
        let Some(observers) = guard.observers.get_mut(&topic) else {
            return false;
        };
        let len_before = observers.len();
        observers.retain(|entry| entry.id != id);
        len_before != observers.len()
    }

    pub fn subscribe_async(&self, topic: SignalTopic) -> AsyncSignalSubscription {
        let sender = self
            .topic_broadcast_tx
            .get(&topic)
            .expect("signal topic sender missing");
        AsyncSignalSubscription {
            label: topic.label(),
            receiver: sender.subscribe(),
            state: Arc::clone(&self.state),
        }
    }

    pub fn subscribe_all(&self) -> AsyncSignalSubscription {
        AsyncSignalSubscription {
            label: "all_topics",
            receiver: self.all_broadcast_tx.subscribe(),
            state: Arc::clone(&self.state),
        }
    }

    pub fn publish(&self, envelope: SignalEnvelope) -> SignalPublishReport {
        let topic = envelope.kind.topic();
        let topic_async_receivers = self
            .topic_broadcast_tx
            .get(&topic)
            .map(|sender| sender.receiver_count())
            .unwrap_or(0);
        let all_async_receivers = self.all_broadcast_tx.receiver_count();
        let observers = {
            let mut guard = self.state.lock().expect("signal routing lock poisoned");
            guard.diagnostics.published_signals =
                guard.diagnostics.published_signals.saturating_add(1);
            let Some(observers) = guard.observers.get(&topic) else {
                if topic_async_receivers > 0 || all_async_receivers > 0 {
                    let async_deliveries = self.publish_async(&envelope, topic);
                    guard.diagnostics.routed_deliveries = guard
                        .diagnostics
                        .routed_deliveries
                        .saturating_add(async_deliveries as u64);
                    guard.diagnostics.queue_depth = self.max_queue_depth();
                    push_signal_trace(&mut guard.signal_trace, &envelope, async_deliveries, 0);
                    return SignalPublishReport {
                        observers_notified: async_deliveries,
                        observer_failures: 0,
                        dead_letters_added: 0,
                        queue_depth: guard.diagnostics.queue_depth,
                    };
                }
                guard.diagnostics.unrouted_signals =
                    guard.diagnostics.unrouted_signals.saturating_add(1);
                push_dead_letter(
                    &mut guard.dead_letters,
                    SignalDeadLetter {
                        envelope: envelope.clone(),
                        observer_id: None,
                        reason: SignalDeadLetterReason::Unrouted,
                        detail: "no observers registered for topic".to_string(),
                    },
                );
                push_signal_trace(&mut guard.signal_trace, &envelope, 0, 0);
                log::warn!(
                    "signal_routing: signal {:?} has no observers (source: {:?})",
                    envelope.kind,
                    envelope.source
                );
                return SignalPublishReport {
                    observers_notified: 0,
                    observer_failures: 0,
                    dead_letters_added: 1,
                    queue_depth: guard.diagnostics.queue_depth,
                };
            };
            if observers.is_empty() {
                if topic_async_receivers > 0 || all_async_receivers > 0 {
                    let async_deliveries = self.publish_async(&envelope, topic);
                    guard.diagnostics.routed_deliveries = guard
                        .diagnostics
                        .routed_deliveries
                        .saturating_add(async_deliveries as u64);
                    guard.diagnostics.queue_depth = self.max_queue_depth();
                    push_signal_trace(&mut guard.signal_trace, &envelope, async_deliveries, 0);
                    return SignalPublishReport {
                        observers_notified: async_deliveries,
                        observer_failures: 0,
                        dead_letters_added: 0,
                        queue_depth: guard.diagnostics.queue_depth,
                    };
                }
                guard.diagnostics.unrouted_signals =
                    guard.diagnostics.unrouted_signals.saturating_add(1);
                push_dead_letter(
                    &mut guard.dead_letters,
                    SignalDeadLetter {
                        envelope: envelope.clone(),
                        observer_id: None,
                        reason: SignalDeadLetterReason::Unrouted,
                        detail: "observer list empty for topic".to_string(),
                    },
                );
                push_signal_trace(&mut guard.signal_trace, &envelope, 0, 0);
                log::warn!(
                    "signal_routing: signal {:?} has no observers (source: {:?})",
                    envelope.kind,
                    envelope.source
                );
                return SignalPublishReport {
                    observers_notified: 0,
                    observer_failures: 0,
                    dead_letters_added: 1,
                    queue_depth: guard.diagnostics.queue_depth,
                };
            }
            observers.clone()
        };

        let mut failures = 0usize;
        let mut dead_letters = Vec::new();
        for observer in &observers {
            match catch_unwind(AssertUnwindSafe(|| (observer.callback)(&envelope))) {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    failures = failures.saturating_add(1);
                    log::error!(
                        "signal_routing: observer {:?} failed on {:?}: {}",
                        observer.id,
                        envelope.kind,
                        error
                    );
                    dead_letters.push(SignalDeadLetter {
                        envelope: envelope.clone(),
                        observer_id: Some(observer.id),
                        reason: SignalDeadLetterReason::ObserverFailed,
                        detail: error,
                    });
                }
                Err(payload) => {
                    failures = failures.saturating_add(1);
                    let detail = panic_payload_message(payload);
                    log::error!(
                        "signal_routing: observer {:?} panicked on {:?}: {}",
                        observer.id,
                        envelope.kind,
                        detail
                    );
                    dead_letters.push(SignalDeadLetter {
                        envelope: envelope.clone(),
                        observer_id: Some(observer.id),
                        reason: SignalDeadLetterReason::ObserverPanicked,
                        detail,
                    });
                }
            }
        }

        let async_deliveries = self.publish_async(&envelope, topic);
        let total_notified = observers.len() + async_deliveries;
        let mut guard = self.state.lock().expect("signal routing lock poisoned");
        guard.diagnostics.routed_deliveries = guard
            .diagnostics
            .routed_deliveries
            .saturating_add(total_notified as u64);
        guard.diagnostics.observer_failures = guard
            .diagnostics
            .observer_failures
            .saturating_add(failures as u64);
        guard.diagnostics.queue_depth = self.max_queue_depth();
        for dead_letter in &dead_letters {
            push_dead_letter(&mut guard.dead_letters, dead_letter.clone());
        }
        push_signal_trace(&mut guard.signal_trace, &envelope, total_notified, failures);

        SignalPublishReport {
            observers_notified: total_notified,
            observer_failures: failures,
            dead_letters_added: dead_letters.len(),
            queue_depth: guard.diagnostics.queue_depth,
        }
    }

    pub fn diagnostics_snapshot(&self) -> SignalRoutingDiagnostics {
        self.state
            .lock()
            .expect("signal routing lock poisoned")
            .diagnostics
    }

    pub fn dead_letters_snapshot(&self) -> Vec<SignalDeadLetter> {
        self.state
            .lock()
            .expect("signal routing lock poisoned")
            .dead_letters
            .iter()
            .cloned()
            .collect()
    }

    pub fn signal_trace_snapshot(&self) -> Vec<SignalTraceEntry> {
        self.state
            .lock()
            .expect("signal routing lock poisoned")
            .signal_trace
            .iter()
            .cloned()
            .collect()
    }

    fn publish_async(&self, envelope: &SignalEnvelope, topic: SignalTopic) -> usize {
        let mut delivered = 0usize;
        if let Some(sender) = self.topic_broadcast_tx.get(&topic) {
            if let Ok(count) = sender.send(envelope.clone()) {
                delivered = delivered.saturating_add(count);
            }
        }
        if let Ok(count) = self.all_broadcast_tx.send(envelope.clone()) {
            delivered = delivered.saturating_add(count);
        }
        delivered
    }

    fn max_queue_depth(&self) -> usize {
        let topic_depth = self
            .topic_broadcast_tx
            .values()
            .map(broadcast::Sender::len)
            .max()
            .unwrap_or(0);
        topic_depth.max(self.all_broadcast_tx.len())
    }
}

impl SignalBus for SignalRoutingLayer {
    fn publish(&self, envelope: SignalEnvelope) -> SignalPublishReport {
        SignalRoutingLayer::publish(self, envelope)
    }

    fn subscribe_sync(&self, topic: SignalTopic, callback: SyncObserverCallback) -> ObserverId {
        let mut guard = self.state.lock().expect("signal routing lock poisoned");
        guard.next_observer_id = guard.next_observer_id.saturating_add(1);
        let id = ObserverId(guard.next_observer_id);
        let observer = SignalObserver { id, callback };
        guard.observers.entry(topic).or_default().push(observer);
        id
    }

    fn unsubscribe(&self, topic: SignalTopic, id: ObserverId) -> bool {
        SignalRoutingLayer::unsubscribe(self, topic, id)
    }

    fn subscribe_async(&self, topic: SignalTopic) -> AsyncSignalSubscription {
        SignalRoutingLayer::subscribe_async(self, topic)
    }

    fn subscribe_all(&self) -> AsyncSignalSubscription {
        SignalRoutingLayer::subscribe_all(self)
    }

    fn diagnostics(&self) -> SignalRoutingDiagnostics {
        SignalRoutingLayer::diagnostics_snapshot(self)
    }

    fn dead_letters(&self) -> Vec<SignalDeadLetter> {
        SignalRoutingLayer::dead_letters_snapshot(self)
    }

    fn signal_trace(&self) -> Vec<SignalTraceEntry> {
        SignalRoutingLayer::signal_trace_snapshot(self)
    }
}

fn push_dead_letter(dead_letters: &mut VecDeque<SignalDeadLetter>, dead_letter: SignalDeadLetter) {
    if dead_letters.len() >= DEAD_LETTER_LIMIT {
        dead_letters.pop_front();
    }
    dead_letters.push_back(dead_letter);
}

fn push_signal_trace(
    trace: &mut VecDeque<SignalTraceEntry>,
    envelope: &SignalEnvelope,
    observers_notified: usize,
    observer_failures: usize,
) {
    if trace.len() >= SIGNAL_TRACE_LIMIT {
        trace.pop_front();
    }
    trace.push_back(SignalTraceEntry {
        kind: envelope.kind.clone(),
        source: envelope.source,
        emitted_at: envelope.emitted_at,
        causality_stamp: envelope.causality_stamp,
        observers_notified,
        observer_failures,
    });
}

fn panic_payload_message(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_string(),
            Err(_) => "observer panicked with non-string payload".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn signal_routing_notifies_two_observers_for_single_producer_publish() {
        let layer = SignalRoutingLayer::default();
        let observer_a = Arc::new(AtomicUsize::new(0));
        let observer_b = Arc::new(AtomicUsize::new(0));

        {
            let observer_a = Arc::clone(&observer_a);
            layer.subscribe(SignalTopic::Navigation, move |_| {
                observer_a.fetch_add(1, Ordering::Relaxed);
                Ok(())
            });
        }

        {
            let observer_b = Arc::clone(&observer_b);
            layer.subscribe(SignalTopic::Navigation, move |_| {
                observer_b.fetch_add(1, Ordering::Relaxed);
                Ok(())
            });
        }

        let report = layer.publish(SignalEnvelope::new(
            SignalKind::Navigation(NavigationSignal::Resolved {
                uri: "https://example.com".to_string(),
                viewer_id: "viewer:webview".to_string(),
            }),
            SignalSource::RegistryRuntime,
            Some(7),
        ));

        assert_eq!(report.observers_notified, 2);
        assert_eq!(report.observer_failures, 0);
        assert_eq!(report.dead_letters_added, 0);
        assert_eq!(observer_a.load(Ordering::Relaxed), 1);
        assert_eq!(observer_b.load(Ordering::Relaxed), 1);

        let diagnostics = layer.diagnostics_snapshot();
        assert_eq!(diagnostics.published_signals, 1);
        assert_eq!(diagnostics.routed_deliveries, 2);
        assert_eq!(diagnostics.unrouted_signals, 0);
        assert_eq!(diagnostics.observer_failures, 0);
        assert_eq!(diagnostics.lagged_receivers, 0);
        assert_eq!(diagnostics.queue_depth, 0);
    }

    #[test]
    fn signal_routing_tracks_unrouted_and_failed_deliveries() {
        let layer = SignalRoutingLayer::default();

        let unrouted = layer.publish(SignalEnvelope::new(
            SignalKind::Lifecycle(LifecycleSignal::MemoryPressureChanged {
                level: "warning".to_string(),
                available_mib: 512,
                total_mib: 2048,
            }),
            SignalSource::ControlPanel,
            None,
        ));
        assert_eq!(unrouted.observers_notified, 0);
        assert_eq!(unrouted.dead_letters_added, 1);

        layer.subscribe(SignalTopic::Sync, |_| Err("forced failure".to_string()));
        let failed = layer.publish(SignalEnvelope::new(
            SignalKind::Sync(SyncSignal::RemoteEntriesQueued),
            SignalSource::ControlPanel,
            None,
        ));
        assert_eq!(failed.observers_notified, 1);
        assert_eq!(failed.observer_failures, 1);
        assert_eq!(failed.dead_letters_added, 1);

        let diagnostics = layer.diagnostics_snapshot();
        assert_eq!(diagnostics.published_signals, 2);
        assert_eq!(diagnostics.routed_deliveries, 1);
        assert_eq!(diagnostics.unrouted_signals, 1);
        assert_eq!(diagnostics.observer_failures, 1);

        let dead_letters = layer.dead_letters_snapshot();
        assert_eq!(dead_letters.len(), 2);
        assert_eq!(dead_letters[0].reason, SignalDeadLetterReason::Unrouted);
        assert_eq!(
            dead_letters[1].reason,
            SignalDeadLetterReason::ObserverFailed
        );
    }

    #[test]
    fn signal_routing_captures_panicking_observer_as_dead_letter() {
        let layer = SignalRoutingLayer::default();
        layer.subscribe(SignalTopic::Navigation, |_| panic!("boom"));

        let report = layer.publish(SignalEnvelope::new(
            SignalKind::Navigation(NavigationSignal::Resolved {
                uri: "https://example.com".to_string(),
                viewer_id: "viewer:webview".to_string(),
            }),
            SignalSource::RegistryRuntime,
            None,
        ));

        assert_eq!(report.observers_notified, 1);
        assert_eq!(report.observer_failures, 1);
        let dead_letters = layer.dead_letters_snapshot();
        assert_eq!(dead_letters.len(), 1);
        assert_eq!(
            dead_letters[0].reason,
            SignalDeadLetterReason::ObserverPanicked
        );
        assert_eq!(dead_letters[0].observer_id, Some(ObserverId(1)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn signal_routing_async_topic_subscriber_receives_published_signal() {
        let layer = SignalRoutingLayer::default();
        let mut receiver = layer.subscribe_async(SignalTopic::Navigation);

        let report = layer.publish(SignalEnvelope::new(
            SignalKind::Navigation(NavigationSignal::Resolved {
                uri: "https://example.com".to_string(),
                viewer_id: "viewer:webview".to_string(),
            }),
            SignalSource::RegistryRuntime,
            None,
        ));

        let received = receiver
            .recv()
            .await
            .expect("async receiver should stay open");
        assert_eq!(report.observers_notified, 1);
        assert!(matches!(
            received.kind,
            SignalKind::Navigation(NavigationSignal::Resolved { ref uri, .. })
                if uri == "https://example.com"
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn signal_routing_async_all_subscriber_receives_cross_topic_signal() {
        let layer = SignalRoutingLayer::default();
        let mut receiver = layer.subscribe_all();

        layer.publish(SignalEnvelope::new(
            SignalKind::Lifecycle(LifecycleSignal::WorkflowActivated {
                workflow_id: "workflow:research".to_string(),
            }),
            SignalSource::ControlPanel,
            None,
        ));

        let received = receiver
            .recv()
            .await
            .expect("all-topics receiver should stay open");
        assert!(matches!(
            received.kind,
            SignalKind::Lifecycle(LifecycleSignal::WorkflowActivated { ref workflow_id })
                if workflow_id == "workflow:research"
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn signal_routing_async_receiver_reports_lagged_delivery() {
        let layer = SignalRoutingLayer::with_async_capacity(1);
        let mut receiver = layer.subscribe_async(SignalTopic::Navigation);

        for index in 0..3 {
            layer.publish(SignalEnvelope::new(
                SignalKind::Navigation(NavigationSignal::Resolved {
                    uri: format!("https://example.com/{index}"),
                    viewer_id: "viewer:webview".to_string(),
                }),
                SignalSource::RegistryRuntime,
                None,
            ));
        }

        let received = receiver.recv().await.expect("receiver should stay open");
        assert!(matches!(
            received.kind,
            SignalKind::Navigation(NavigationSignal::Resolved { ref uri, .. })
                if uri == "https://example.com/2"
        ));
        assert!(
            layer.diagnostics_snapshot().lagged_receivers > 0,
            "lagged receiver count should increment after skipped messages"
        );
    }
}
