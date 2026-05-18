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
//! - `crate::graph::NodeKey` → `mere_kernel::graph::NodeKey`
//! - `crate::shell::desktop::runtime::diagnostics::*` → `register_diagnostics::*`
//! - `super::CHANNEL_REGISTER_SIGNAL_ROUTING_LAGGED` → `register_diagnostics::channels::*`
//! - `pub` → `pub` (items are now the public API of `mere-host-contract::system::signal_bus`)
//!
//! Split across submodules to keep each file under the workspace's
//! 600-LOC ceiling: this module owns the vocabulary (signal kinds,
//! envelope, trait, diagnostics shapes); [`routing`] owns the
//! concrete [`SignalRoutingLayer`] implementation.

use std::sync::Arc;
use std::time::Instant;

use mere_kernel::graph::NodeKey;

mod routing;

#[cfg(test)]
mod tests;

pub use routing::{AsyncSignalSubscription, SignalRoutingLayer};

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
