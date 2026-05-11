/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable command-surface telemetry sink.
//!
//! Consolidates the publish-latest-snapshot cell and the event
//! sequence counter cell that widgets emit into as the user interacts
//! with the command bar, omnibar, and command palette.
//!
//! History:
//! - M4 slice 6 (2026-04-22) removed the `OnceLock<CommandSurfaceTelemetry>`
//!   crate-global and made production call sites take a
//!   `&CommandSurfaceTelemetry` reference.
//! - M4 slice 10 (2026-04-22) moved the whole module to mere-kernel
//!   once its data-shape dependencies (`PaneId`,
//!   `ToolSurfaceReturnTarget`) became portable. Confirmed empirically
//!   that `std::sync::Mutex` compiles to `wasm32-unknown-unknown`
//!   (Rust 1.70+), so no Mutex-related refactor was required.
//!
//! Tests construct per-test instances via
//! [`CommandSurfaceTelemetry::new`] — each test's sink is naturally
//! isolated (no global serialisation needed).

use std::sync::Mutex;

use mere_graphshell::routing::ToolSurfaceReturnTarget;
use mere_kernel::graph::NodeKey;
use mere_kernel::pane::PaneId;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandBarSemanticMetadata {
    pub active_pane: Option<PaneId>,
    pub focused_node: Option<NodeKey>,
    pub location_focused: bool,
    pub route_events: CommandRouteEventSequenceMetadata,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandRouteEventSequenceMetadata {
    pub resolved: u64,
    pub fallback: u64,
    pub no_target: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OmnibarMailboxEventSequenceMetadata {
    pub request_started: u64,
    pub applied: u64,
    pub failed: u64,
    pub stale: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandSurfaceEventSequenceMetadata {
    pub route_events: CommandRouteEventSequenceMetadata,
    pub omnibar_mailbox_events: OmnibarMailboxEventSequenceMetadata,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OmnibarSemanticMetadata {
    pub active: bool,
    pub focused: bool,
    pub query: Option<String>,
    pub match_count: usize,
    pub provider_status: Option<String>,
    pub active_pane: Option<PaneId>,
    pub focused_node: Option<NodeKey>,
    pub mailbox_events: OmnibarMailboxEventSequenceMetadata,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PaletteSurfaceSemanticMetadata {
    pub contextual_mode: bool,
    pub return_target: Option<ToolSurfaceReturnTarget>,
    pub pending_node_context_target: Option<NodeKey>,
    pub pending_frame_context_target: Option<String>,
    pub context_anchor_present: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandSurfaceSemanticSnapshot {
    pub command_bar: CommandBarSemanticMetadata,
    pub omnibar: OmnibarSemanticMetadata,
    pub command_palette: Option<PaletteSurfaceSemanticMetadata>,
    pub context_palette: Option<PaletteSurfaceSemanticMetadata>,
}

/// Consolidated command-surface telemetry sink.
///
/// Holds the "latest published snapshot" cell and the counter cell
/// widgets bump as they route commands and fetch omnibar suggestions.
/// Owned by `GraphshellRuntime`; production call sites receive a
/// `&CommandSurfaceTelemetry` reference rather than reaching a
/// crate-global singleton.
#[derive(Default)]
pub struct CommandSurfaceTelemetry {
    snapshot: Mutex<Option<CommandSurfaceSemanticSnapshot>>,
    events: Mutex<CommandSurfaceEventSequenceMetadata>,
}

impl CommandSurfaceTelemetry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn latest_snapshot(&self) -> Option<CommandSurfaceSemanticSnapshot> {
        self.snapshot.lock().ok().and_then(|slot| slot.clone())
    }

    pub fn publish_snapshot(&self, snapshot: CommandSurfaceSemanticSnapshot) {
        if let Ok(mut slot) = self.snapshot.lock() {
            *slot = Some(snapshot);
        }
    }

    pub fn clear_snapshot(&self) {
        if let Ok(mut slot) = self.snapshot.lock() {
            *slot = None;
        }
        #[cfg(any(test, feature = "test-utils"))]
        self.clear_event_sequence_metadata();
    }

    pub fn latest_event_sequence_metadata(&self) -> CommandSurfaceEventSequenceMetadata {
        self.events.lock().map(|state| *state).unwrap_or_default()
    }

    fn update_events(&self, mutator: impl FnOnce(&mut CommandSurfaceEventSequenceMetadata)) {
        if let Ok(mut state) = self.events.lock() {
            mutator(&mut state);
        }
    }

    pub fn note_route_resolved(&self) {
        self.update_events(|state| {
            state.route_events.resolved = state.route_events.resolved.saturating_add(1);
        });
    }

    pub fn note_route_fallback(&self) {
        self.update_events(|state| {
            state.route_events.fallback = state.route_events.fallback.saturating_add(1);
        });
    }

    pub fn note_route_no_target(&self) {
        self.update_events(|state| {
            state.route_events.no_target = state.route_events.no_target.saturating_add(1);
        });
    }

    pub fn note_omnibar_mailbox_request_started(&self) {
        self.update_events(|state| {
            state.omnibar_mailbox_events.request_started = state
                .omnibar_mailbox_events
                .request_started
                .saturating_add(1);
        });
    }

    pub fn note_omnibar_mailbox_applied(&self) {
        self.update_events(|state| {
            state.omnibar_mailbox_events.applied =
                state.omnibar_mailbox_events.applied.saturating_add(1);
        });
    }

    pub fn note_omnibar_mailbox_failed(&self) {
        self.update_events(|state| {
            state.omnibar_mailbox_events.failed =
                state.omnibar_mailbox_events.failed.saturating_add(1);
        });
    }

    pub fn note_omnibar_mailbox_stale(&self) {
        self.update_events(|state| {
            state.omnibar_mailbox_events.stale =
                state.omnibar_mailbox_events.stale.saturating_add(1);
        });
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn set_event_sequence_metadata_for_tests(
        &self,
        metadata: CommandSurfaceEventSequenceMetadata,
    ) {
        if let Ok(mut state) = self.events.lock() {
            *state = metadata;
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn clear_event_sequence_metadata(&self) {
        if let Ok(mut state) = self.events.lock() {
            *state = CommandSurfaceEventSequenceMetadata::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_telemetry_has_default_state() {
        let t = CommandSurfaceTelemetry::new();
        assert!(t.latest_snapshot().is_none());
        assert_eq!(
            t.latest_event_sequence_metadata(),
            CommandSurfaceEventSequenceMetadata::default()
        );
    }

    #[test]
    fn publish_and_latest_round_trip_snapshot() {
        let t = CommandSurfaceTelemetry::new();
        let snap = CommandSurfaceSemanticSnapshot {
            command_bar: CommandBarSemanticMetadata {
                location_focused: true,
                ..Default::default()
            },
            omnibar: OmnibarSemanticMetadata {
                active: true,
                query: Some("rust".into()),
                match_count: 3,
                ..Default::default()
            },
            command_palette: Some(PaletteSurfaceSemanticMetadata {
                contextual_mode: true,
                ..Default::default()
            }),
            context_palette: None,
        };
        t.publish_snapshot(snap.clone());
        assert_eq!(t.latest_snapshot(), Some(snap));
    }

    #[test]
    fn clear_snapshot_removes_published_value() {
        let t = CommandSurfaceTelemetry::new();
        t.publish_snapshot(CommandSurfaceSemanticSnapshot::default());
        assert!(t.latest_snapshot().is_some());
        t.clear_snapshot();
        assert!(t.latest_snapshot().is_none());
    }

    #[test]
    fn note_route_events_increment_counters() {
        let t = CommandSurfaceTelemetry::new();
        t.note_route_resolved();
        t.note_route_resolved();
        t.note_route_fallback();
        t.note_route_no_target();
        let m = t.latest_event_sequence_metadata();
        assert_eq!(m.route_events.resolved, 2);
        assert_eq!(m.route_events.fallback, 1);
        assert_eq!(m.route_events.no_target, 1);
    }

    #[test]
    fn note_omnibar_mailbox_events_increment_counters() {
        let t = CommandSurfaceTelemetry::new();
        t.note_omnibar_mailbox_request_started();
        t.note_omnibar_mailbox_request_started();
        t.note_omnibar_mailbox_request_started();
        t.note_omnibar_mailbox_applied();
        t.note_omnibar_mailbox_failed();
        t.note_omnibar_mailbox_stale();
        let m = t.latest_event_sequence_metadata();
        assert_eq!(m.omnibar_mailbox_events.request_started, 3);
        assert_eq!(m.omnibar_mailbox_events.applied, 1);
        assert_eq!(m.omnibar_mailbox_events.failed, 1);
        assert_eq!(m.omnibar_mailbox_events.stale, 1);
    }

    #[test]
    fn counters_saturate_at_u64_max() {
        // Defensive: the counters use `saturating_add` because
        // long-running sessions could in principle exceed u64::MAX
        // (not realistic, but pin the contract).
        let t = CommandSurfaceTelemetry::new();
        t.set_event_sequence_metadata_for_tests(CommandSurfaceEventSequenceMetadata {
            route_events: CommandRouteEventSequenceMetadata {
                resolved: u64::MAX,
                ..Default::default()
            },
            ..Default::default()
        });
        t.note_route_resolved();
        let m = t.latest_event_sequence_metadata();
        assert_eq!(m.route_events.resolved, u64::MAX);
    }
}
