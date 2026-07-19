// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Portable signal-subscription seam for frame-bound inbox consumers.

use futures_util::stream::BoxStream;

/// Host-provided subscription router for shell-facing runtime signals.
pub trait SignalRouter: Send + Sync {
    fn subscribe(&self, topic: SignalTopic) -> BoxStream<'static, SignalEnvelope>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalTopic {
    Lifecycle,
    RegistryEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalEnvelope {
    Lifecycle(LifecycleSignal),
    RegistryEvent(RegistrySignal),
}

impl SignalEnvelope {
    pub fn topic(&self) -> SignalTopic {
        match self {
            Self::Lifecycle(..) => SignalTopic::Lifecycle,
            Self::RegistryEvent(..) => SignalTopic::RegistryEvent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleSignal {
    SemanticIndexUpdated { indexed_nodes: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrySignal {
    WorkbenchProjectionRefreshRequested,
    SettingsRouteRequested { url: String },
    ThemeChanged,
    LensChanged,
    PhysicsProfileChanged,
    CanvasProfileChanged,
    WorkbenchSurfaceChanged,
}

#[cfg(test)]
mod tests {
    use super::{LifecycleSignal, RegistrySignal, SignalEnvelope, SignalTopic};

    #[test]
    fn lifecycle_envelope_reports_lifecycle_topic() {
        let envelope =
            SignalEnvelope::Lifecycle(LifecycleSignal::SemanticIndexUpdated { indexed_nodes: 3 });

        assert_eq!(envelope.topic(), SignalTopic::Lifecycle);
    }

    #[test]
    fn registry_envelope_reports_registry_topic() {
        let envelope =
            SignalEnvelope::RegistryEvent(RegistrySignal::WorkbenchProjectionRefreshRequested);

        assert_eq!(envelope.topic(), SignalTopic::RegistryEvent);
    }
}
