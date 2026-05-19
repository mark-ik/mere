/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Concrete in-process signal routing implementation. Split out of
//! `signal_bus.rs` to keep the parent module under the workspace's
//! 600-LOC ceiling. The parent owns the vocabulary (signal kinds,
//! envelope, trait, diagnostics shapes); this file owns the
//! [`SignalRoutingLayer`] facade + its sync/async fanout state.

use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

use register_diagnostics::channels::CHANNEL_REGISTER_SIGNAL_ROUTING_LAGGED;
use register_diagnostics::{DiagnosticEvent, emit_event};
use tokio::sync::broadcast;

use super::{
    ASYNC_SIGNAL_BUFFER, DEAD_LETTER_LIMIT, ObserverId, SIGNAL_TRACE_LIMIT, SignalBus,
    SignalDeadLetter, SignalDeadLetterReason, SignalEnvelope, SignalPublishReport,
    SignalRoutingDiagnostics, SignalTopic, SignalTraceEntry, SyncObserverCallback,
};

#[derive(Clone)]
struct SignalObserver {
    id: ObserverId,
    callback: SyncObserverCallback,
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
