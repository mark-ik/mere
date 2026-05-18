/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Tests for the signal routing layer. Split out of `signal_bus.rs`
//! to keep the parent module under the workspace's 600-LOC ceiling.

use std::sync::Arc;
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
