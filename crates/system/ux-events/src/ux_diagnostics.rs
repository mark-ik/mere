// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Host-neutral bridge from [`UxEvent`](crate::ux_observability::UxEvent)
//! to a diagnostics channel registry.
//!
//! Hosts already ship various diagnostic-channel registries (the
//! shell crate has a large `pub(crate)` registry behind the
//! `diagnostics` feature; future hosts will have their own). This
//! module defines a portable seam — `DiagnosticsChannelSink` — that
//! every registry can plug into, plus a [`UxChannelObserver`] that
//! converts each `UxEvent` into a stable `(channel_id, severity,
//! payload)` triple and forwards to the sink.
//!
//! The split lets us:
//!
//! - **Lock in channel ids regardless of host.** [`event_channel`]
//!   is the single source of truth: every host emits the *same*
//!   channel id for a given event. The shell crate's egui-era
//!   registry, the iced runtime, and a future Stage-G host all
//!   agree on `"ux.command_palette.opened"`.
//! - **Run observations without a real registry.** Tests use the
//!   built-in [`RecordingChannelSink`] to assert mapping; the
//!   built-in [`NoopChannelSink`] is the default for environments
//!   that don't yet have a registry wired.
//! - **Avoid coupling `kernel` to any host's internal
//!   registry types.** Hosts implement the trait against their
//!   registry; nothing in core has to know that registry's shape.

use std::sync::Mutex;

use crate::ux_observability::{DismissReason, SurfaceId, UxEvent, UxObserver};

/// Severity tier for a diagnostics channel emission. Mirrors the
/// shell crate's existing `ChannelSeverity` taxonomy so a host can
/// translate 1:1 when it plugs in its registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticsSeverity {
    Info,
    Warn,
    Error,
}

/// One channel emission. The id namespace is `"ux.<surface>.<event>"`
/// — the convention the iced host's existing channel registry uses
/// for chrome-surface events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelEmission {
    pub channel_id: &'static str,
    pub severity: DiagnosticsSeverity,
    /// Optional descriptive payload (e.g., `"reason=Confirmed"`).
    /// Hosts that use structured payloads can ignore this and read
    /// the originating event directly via [`UxChannelObserver`]'s
    /// [`event_channel`] mapping.
    pub note: Option<String>,
}

/// Trait every host's diagnostics-channel registry implements to
/// receive UX-event emissions. The trait is `Send + Sync` so
/// future parallel hosts can register thread-safe sinks.
pub trait DiagnosticsChannelSink: Send + Sync {
    fn record(&self, emission: &ChannelEmission);
}

/// Map a `UxEvent` to its canonical channel emission. Pure function;
/// the mapping is the locked-in contract.
pub fn event_channel(event: &UxEvent) -> ChannelEmission {
    match event {
        UxEvent::SurfaceOpened { surface } => ChannelEmission {
            channel_id: surface_open_channel(*surface),
            severity: DiagnosticsSeverity::Info,
            note: None,
        },
        UxEvent::SurfaceDismissed { surface, reason } => ChannelEmission {
            channel_id: surface_dismiss_channel(*surface),
            severity: DiagnosticsSeverity::Info,
            note: Some(format!("reason={}", dismiss_reason_label(*reason))),
        },
        UxEvent::ActionDispatched { action_id, target } => ChannelEmission {
            channel_id: "ux.action.dispatched",
            severity: DiagnosticsSeverity::Info,
            note: Some(match target {
                Some(_) => format!("action={};targeted=true", action_id.key()),
                None => format!("action={};targeted=false", action_id.key()),
            }),
        },
        UxEvent::OpenNodeDispatched { .. } => ChannelEmission {
            channel_id: "ux.open_node.dispatched",
            severity: DiagnosticsSeverity::Info,
            note: None,
        },
    }
}

/// Stable channel-id mapping for `SurfaceOpened` events. One static
/// `&'static str` per surface so the channel registry can register
/// these at startup.
fn surface_open_channel(surface: SurfaceId) -> &'static str {
    match surface {
        SurfaceId::Omnibar => "ux.omnibar.opened",
        SurfaceId::CommandPalette => "ux.command_palette.opened",
        SurfaceId::NodeFinder => "ux.node_finder.opened",
        SurfaceId::ContextMenu => "ux.context_menu.opened",
        SurfaceId::ConfirmDialog => "ux.confirm_dialog.opened",
        SurfaceId::NodeCreate => "ux.node_create.opened",
        SurfaceId::FrameRename => "ux.frame_rename.opened",
        SurfaceId::StatusBar => "ux.status_bar.opened",
        SurfaceId::TreeSpine => "ux.tree_spine.opened",
        SurfaceId::NavigatorHost => "ux.navigator_host.opened",
        SurfaceId::TilePane => "ux.tile_pane.opened",
        SurfaceId::CanvasPane => "ux.canvas_pane.opened",
        SurfaceId::BaseLayer => "ux.base_layer.opened",
        SurfaceId::RosterPane => "ux.roster_pane.opened",
        SurfaceId::InspectorPane => "ux.inspector_pane.opened",
        SurfaceId::StewardPane => "ux.steward_pane.opened",
        SurfaceId::GlossPane => "ux.gloss_pane.opened",
        SurfaceId::ApparatusPane => "ux.apparatus_pane.opened",
        SurfaceId::CommsPane => "ux.comms_pane.opened",
        SurfaceId::WorkbenchPane => "ux.workbench_pane.opened",
    }
}

fn surface_dismiss_channel(surface: SurfaceId) -> &'static str {
    match surface {
        SurfaceId::Omnibar => "ux.omnibar.dismissed",
        SurfaceId::CommandPalette => "ux.command_palette.dismissed",
        SurfaceId::NodeFinder => "ux.node_finder.dismissed",
        SurfaceId::ContextMenu => "ux.context_menu.dismissed",
        SurfaceId::ConfirmDialog => "ux.confirm_dialog.dismissed",
        SurfaceId::NodeCreate => "ux.node_create.dismissed",
        SurfaceId::FrameRename => "ux.frame_rename.dismissed",
        SurfaceId::StatusBar => "ux.status_bar.dismissed",
        SurfaceId::TreeSpine => "ux.tree_spine.dismissed",
        SurfaceId::NavigatorHost => "ux.navigator_host.dismissed",
        SurfaceId::TilePane => "ux.tile_pane.dismissed",
        SurfaceId::CanvasPane => "ux.canvas_pane.dismissed",
        SurfaceId::BaseLayer => "ux.base_layer.dismissed",
        SurfaceId::RosterPane => "ux.roster_pane.dismissed",
        SurfaceId::InspectorPane => "ux.inspector_pane.dismissed",
        SurfaceId::StewardPane => "ux.steward_pane.dismissed",
        SurfaceId::GlossPane => "ux.gloss_pane.dismissed",
        SurfaceId::ApparatusPane => "ux.apparatus_pane.dismissed",
        SurfaceId::CommsPane => "ux.comms_pane.dismissed",
        SurfaceId::WorkbenchPane => "ux.workbench_pane.dismissed",
    }
}

fn dismiss_reason_label(reason: DismissReason) -> &'static str {
    match reason {
        DismissReason::Confirmed => "confirmed",
        DismissReason::Cancelled => "cancelled",
        DismissReason::Superseded => "superseded",
        DismissReason::Programmatic => "programmatic",
    }
}

/// Enumerate every channel id this module emits to. Hosts use this
/// to pre-register the channel descriptors at startup so the
/// "registered channels" tab in the diagnostics pane lists every
/// UX channel even if none have fired yet.
pub fn all_channel_ids() -> &'static [&'static str] {
    &[
        // SurfaceOpened
        "ux.omnibar.opened",
        "ux.command_palette.opened",
        "ux.node_finder.opened",
        "ux.context_menu.opened",
        "ux.confirm_dialog.opened",
        "ux.node_create.opened",
        "ux.frame_rename.opened",
        "ux.status_bar.opened",
        "ux.tree_spine.opened",
        "ux.navigator_host.opened",
        "ux.tile_pane.opened",
        "ux.canvas_pane.opened",
        "ux.base_layer.opened",
        "ux.roster_pane.opened",
        "ux.gloss_pane.opened",
        "ux.apparatus_pane.opened",
        "ux.comms_pane.opened",
        "ux.workbench_pane.opened",
        // SurfaceDismissed
        "ux.omnibar.dismissed",
        "ux.command_palette.dismissed",
        "ux.node_finder.dismissed",
        "ux.context_menu.dismissed",
        "ux.confirm_dialog.dismissed",
        "ux.node_create.dismissed",
        "ux.frame_rename.dismissed",
        "ux.status_bar.dismissed",
        "ux.tree_spine.dismissed",
        "ux.navigator_host.dismissed",
        "ux.tile_pane.dismissed",
        "ux.canvas_pane.dismissed",
        "ux.base_layer.dismissed",
        "ux.roster_pane.dismissed",
        "ux.gloss_pane.dismissed",
        "ux.apparatus_pane.dismissed",
        "ux.comms_pane.dismissed",
        "ux.workbench_pane.dismissed",
        // Action / OpenNode
        "ux.action.dispatched",
        "ux.open_node.dispatched",
    ]
}

// ---------------------------------------------------------------------------
// UxChannelObserver — adapter forwarding UxEvents to a sink
// ---------------------------------------------------------------------------

/// `UxObserver` adapter that translates each event into a
/// [`ChannelEmission`] via [`event_channel`] and forwards to the
/// supplied sink. Hosts wrap their registry-emit in a sink and
/// register this observer once; UxEvents start flowing into their
/// diagnostics infrastructure with no further wiring.
pub struct UxChannelObserver<S: DiagnosticsChannelSink> {
    sink: S,
}

impl<S: DiagnosticsChannelSink> UxChannelObserver<S> {
    pub fn new(sink: S) -> Self {
        Self { sink }
    }
}

impl<S: DiagnosticsChannelSink + 'static> UxObserver for UxChannelObserver<S> {
    fn observe(&self, event: &UxEvent) {
        let emission = event_channel(event);
        self.sink.record(&emission);
    }
}

// ---------------------------------------------------------------------------
// Built-in sinks
// ---------------------------------------------------------------------------

/// Drops every emission. Default sink for environments that don't
/// yet have a registry wired (e.g., the iced-host build with no
/// diagnostics feature). Useful as the unconfigured zero state.
#[derive(Debug, Default)]
pub struct NoopChannelSink;

impl DiagnosticsChannelSink for NoopChannelSink {
    fn record(&self, _emission: &ChannelEmission) {}
}

/// Records every emission into a bounded ring. Used by tests to
/// assert mapping; can also back a "recent UX channel emissions"
/// view in the diagnostics pane until the host's real registry is
/// wired.
pub struct RecordingChannelSink {
    capacity: usize,
    emissions: Mutex<std::collections::VecDeque<ChannelEmission>>,
}

impl RecordingChannelSink {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            emissions: Mutex::new(std::collections::VecDeque::with_capacity(capacity)),
        }
    }

    pub fn snapshot(&self) -> Vec<ChannelEmission> {
        self.emissions.lock().unwrap().iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.emissions.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.emissions.lock().unwrap().is_empty()
    }
}

impl DiagnosticsChannelSink for RecordingChannelSink {
    fn record(&self, emission: &ChannelEmission) {
        let mut buf = self.emissions.lock().unwrap();
        if buf.len() == self.capacity && self.capacity > 0 {
            buf.pop_front();
        }
        buf.push_back(emission.clone());
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ux_observability::UxObservers;
    use kernel::actions::ActionId;
    use kernel::graph::NodeKey;
    use std::sync::Arc;

    #[test]
    fn surface_opened_maps_to_open_channel() {
        let emission = event_channel(&UxEvent::SurfaceOpened {
            surface: SurfaceId::CommandPalette,
        });
        assert_eq!(emission.channel_id, "ux.command_palette.opened");
        assert_eq!(emission.severity, DiagnosticsSeverity::Info);
    }

    #[test]
    fn surface_dismissed_carries_reason_in_note() {
        let emission = event_channel(&UxEvent::SurfaceDismissed {
            surface: SurfaceId::ConfirmDialog,
            reason: DismissReason::Confirmed,
        });
        assert_eq!(emission.channel_id, "ux.confirm_dialog.dismissed");
        assert_eq!(emission.note.as_deref(), Some("reason=confirmed"));
    }

    #[test]
    fn action_dispatched_carries_action_key_and_targeted_flag() {
        let untargeted = event_channel(&UxEvent::ActionDispatched {
            action_id: ActionId::GraphTogglePhysics,
            target: None,
        });
        assert_eq!(untargeted.channel_id, "ux.action.dispatched");
        assert!(
            untargeted
                .note
                .as_deref()
                .unwrap()
                .contains("action=graph:toggle_physics")
        );
        assert!(
            untargeted
                .note
                .as_deref()
                .unwrap()
                .contains("targeted=false")
        );

        let targeted = event_channel(&UxEvent::ActionDispatched {
            action_id: ActionId::NodePinToggle,
            target: Some(NodeKey::new(7)),
        });
        assert!(targeted.note.as_deref().unwrap().contains("targeted=true"));
    }

    #[test]
    fn open_node_dispatched_uses_canonical_channel() {
        let emission = event_channel(&UxEvent::OpenNodeDispatched {
            node_key: NodeKey::new(3),
        });
        assert_eq!(emission.channel_id, "ux.open_node.dispatched");
    }

    #[test]
    fn ux_channel_observer_forwards_to_sink() {
        struct ProxySink(Arc<RecordingChannelSink>);
        impl DiagnosticsChannelSink for ProxySink {
            fn record(&self, emission: &ChannelEmission) {
                self.0.record(emission);
            }
        }

        let recorder = Arc::new(RecordingChannelSink::with_capacity(16));
        let observer = UxChannelObserver::new(ProxySink(Arc::clone(&recorder)));

        let mut observers = UxObservers::new();
        observers.register(Box::new(observer));
        observers.emit(UxEvent::SurfaceOpened {
            surface: SurfaceId::NodeFinder,
        });
        observers.emit(UxEvent::SurfaceDismissed {
            surface: SurfaceId::NodeFinder,
            reason: DismissReason::Cancelled,
        });

        let snap = recorder.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].channel_id, "ux.node_finder.opened");
        assert_eq!(snap[1].channel_id, "ux.node_finder.dismissed");
    }

    #[test]
    fn all_channel_ids_covers_every_surface_and_dispatch_kind() {
        let ids = all_channel_ids();
        // Every SurfaceId variant has both an opened and dismissed
        // channel (2 each), plus action.dispatched and
        // open_node.dispatched. 18 surfaces x 2 + 2 = 38.
        assert_eq!(ids.len(), 38);
        assert!(ids.contains(&"ux.command_palette.opened"));
        assert!(ids.contains(&"ux.command_palette.dismissed"));
        assert!(ids.contains(&"ux.node_create.opened"));
        assert!(ids.contains(&"ux.apparatus_pane.opened"));
        assert!(ids.contains(&"ux.action.dispatched"));
        assert!(ids.contains(&"ux.open_node.dispatched"));
    }

    #[test]
    fn noop_sink_is_silent() {
        let sink = NoopChannelSink;
        sink.record(&ChannelEmission {
            channel_id: "ux.command_palette.opened",
            severity: DiagnosticsSeverity::Info,
            note: None,
        });
        // Nothing to assert — Noop ate the emission.
    }
}
