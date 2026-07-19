// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests for the probe family. Split out of `ux_probes.rs` to keep the
//! parent module under the workspace's 600-LOC ceiling.

use std::sync::Arc;

use kernel::actions::ActionId;

use super::*;
use crate::ux_observability::{DismissReason, SurfaceId, UxEvent, UxObservers};

#[test]
fn mutual_exclusion_passes_when_dismissals_precede_opens() {
    let probe = Arc::new(MutualExclusionProbe::new());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    // Open palette, dismiss-superseded, open finder. The host
    // emits the dismissal *before* the new open — invariant holds.
    observers.emit(UxEvent::SurfaceOpened {
        surface: SurfaceId::CommandPalette,
    });
    observers.emit(UxEvent::SurfaceDismissed {
        surface: SurfaceId::CommandPalette,
        reason: DismissReason::Superseded,
    });
    observers.emit(UxEvent::SurfaceOpened {
        surface: SurfaceId::NodeFinder,
    });

    assert!(probe.drain_failures().is_empty());
}

#[test]
fn mutual_exclusion_flags_overlapping_modals() {
    let probe = Arc::new(MutualExclusionProbe::new());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    observers.emit(UxEvent::SurfaceOpened {
        surface: SurfaceId::CommandPalette,
    });
    observers.emit(UxEvent::SurfaceOpened {
        surface: SurfaceId::NodeFinder,
    });

    let failures = probe.drain_failures();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].probe_name, "mutual_exclusion");
    assert!(failures[0].description.contains("NodeFinder"));
}

#[test]
fn mutual_exclusion_ignores_non_modal_surfaces() {
    let probe = Arc::new(MutualExclusionProbe::new());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    observers.emit(UxEvent::SurfaceOpened {
        surface: SurfaceId::CommandPalette,
    });
    observers.emit(UxEvent::SurfaceOpened {
        surface: SurfaceId::StatusBar,
    });

    assert!(probe.drain_failures().is_empty());
}

#[test]
fn open_dismiss_balance_passes_for_paired_events() {
    let probe = Arc::new(OpenDismissBalanceProbe::new());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    observers.emit(UxEvent::SurfaceOpened {
        surface: SurfaceId::ContextMenu,
    });
    observers.emit(UxEvent::SurfaceDismissed {
        surface: SurfaceId::ContextMenu,
        reason: DismissReason::Cancelled,
    });

    assert!(probe.drain_failures().is_empty());
    assert!(probe.pending_opens().is_empty());
}

#[test]
fn open_dismiss_balance_flags_double_open() {
    let probe = Arc::new(OpenDismissBalanceProbe::new());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    observers.emit(UxEvent::SurfaceOpened {
        surface: SurfaceId::ContextMenu,
    });
    observers.emit(UxEvent::SurfaceOpened {
        surface: SurfaceId::ContextMenu,
    });

    let failures = probe.drain_failures();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].description.contains("opened again"));
}

#[test]
fn open_dismiss_balance_flags_unmatched_dismiss() {
    let probe = Arc::new(OpenDismissBalanceProbe::new());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    observers.emit(UxEvent::SurfaceDismissed {
        surface: SurfaceId::ContextMenu,
        reason: DismissReason::Cancelled,
    });

    let failures = probe.drain_failures();
    assert_eq!(failures.len(), 1);
    assert!(
        failures[0]
            .description
            .contains("dismissed without a matching open")
    );
}

#[test]
fn open_dismiss_balance_pending_reports_unclosed_surfaces() {
    let probe = Arc::new(OpenDismissBalanceProbe::new());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    observers.emit(UxEvent::SurfaceOpened {
        surface: SurfaceId::CommandPalette,
    });
    // Forgot the dismissal — pending_opens reports it.
    let pending = probe.pending_opens();
    assert_eq!(pending.get(&SurfaceId::CommandPalette), Some(&1));
}

// ProductiveSelectionProbe ----------------------------------------------

#[test]
fn productive_selection_palette_with_action_dispatch_passes() {
    let probe = Arc::new(ProductiveSelectionProbe::iced_default());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    observers.emit(UxEvent::SurfaceOpened {
        surface: SurfaceId::CommandPalette,
    });
    observers.emit(UxEvent::SurfaceDismissed {
        surface: SurfaceId::CommandPalette,
        reason: DismissReason::Confirmed,
    });
    observers.emit(UxEvent::ActionDispatched {
        action_id: ActionId::GraphTogglePhysics,
        target: None,
    });

    assert!(probe.drain_failures().is_empty());
}

#[test]
fn productive_selection_palette_routed_to_node_create_passes() {
    let probe = Arc::new(ProductiveSelectionProbe::iced_default());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    // Palette confirms a host-routed action that opens NodeCreate.
    observers.emit(UxEvent::SurfaceDismissed {
        surface: SurfaceId::CommandPalette,
        reason: DismissReason::Confirmed,
    });
    observers.emit(UxEvent::SurfaceOpened {
        surface: SurfaceId::NodeCreate,
    });

    assert!(probe.drain_failures().is_empty());
}

#[test]
fn productive_selection_finder_must_emit_open_node() {
    let probe = Arc::new(ProductiveSelectionProbe::iced_default());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    observers.emit(UxEvent::SurfaceDismissed {
        surface: SurfaceId::NodeFinder,
        reason: DismissReason::Confirmed,
    });
    // ActionDispatched is NOT a productive outcome for NodeFinder —
    // only OpenNodeDispatched satisfies the rule.
    observers.emit(UxEvent::ActionDispatched {
        action_id: ActionId::GraphTogglePhysics,
        target: None,
    });

    let failures = probe.drain_failures();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].description.contains("NodeFinder"));
}

#[test]
fn productive_selection_finder_with_open_node_passes() {
    let probe = Arc::new(ProductiveSelectionProbe::iced_default());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    let dummy = kernel::graph::NodeKey::new(0);
    observers.emit(UxEvent::SurfaceDismissed {
        surface: SurfaceId::NodeFinder,
        reason: DismissReason::Confirmed,
    });
    observers.emit(UxEvent::OpenNodeDispatched { node_key: dummy });

    assert!(probe.drain_failures().is_empty());
}

#[test]
fn productive_selection_ignores_cancelled_dismissals() {
    let probe = Arc::new(ProductiveSelectionProbe::iced_default());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    // Cancelled dismissals carry no productive expectation — the user
    // chose not to act and the probe must not flag.
    observers.emit(UxEvent::SurfaceDismissed {
        surface: SurfaceId::CommandPalette,
        reason: DismissReason::Cancelled,
    });

    assert!(probe.drain_failures().is_empty());
}

#[test]
fn productive_selection_context_menu_destructive_path_passes() {
    let probe = Arc::new(ProductiveSelectionProbe::iced_default());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    observers.emit(UxEvent::SurfaceDismissed {
        surface: SurfaceId::ContextMenu,
        reason: DismissReason::Confirmed,
    });
    observers.emit(UxEvent::SurfaceOpened {
        surface: SurfaceId::ConfirmDialog,
    });

    assert!(probe.drain_failures().is_empty());
}

// DestructiveActionGateProbe -------------------------------------------

#[test]
fn destructive_gate_passes_when_confirm_dialog_grants() {
    let probe = Arc::new(DestructiveActionGateProbe::iced_default());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    // Standard destructive flow: ConfirmDialog Confirmed → destructive
    // action dispatched.
    observers.emit(UxEvent::SurfaceDismissed {
        surface: SurfaceId::ConfirmDialog,
        reason: DismissReason::Confirmed,
    });
    observers.emit(UxEvent::ActionDispatched {
        action_id: ActionId::NodeMarkTombstone,
        target: None,
    });

    assert!(probe.drain_failures().is_empty());
}

#[test]
fn destructive_gate_flags_unconfirmed_destructive() {
    let probe = Arc::new(DestructiveActionGateProbe::iced_default());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    // Destructive action fires without any preceding ConfirmDialog.
    observers.emit(UxEvent::ActionDispatched {
        action_id: ActionId::NodeMarkTombstone,
        target: None,
    });

    let failures = probe.drain_failures();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].description.contains("NodeMarkTombstone"));
}

#[test]
fn destructive_gate_consumes_grant_after_one_destructive() {
    // A confirmation grants ONE destructive dispatch, not many. A
    // second destructive without re-confirmation must trip the probe.
    let probe = Arc::new(DestructiveActionGateProbe::iced_default());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    observers.emit(UxEvent::SurfaceDismissed {
        surface: SurfaceId::ConfirmDialog,
        reason: DismissReason::Confirmed,
    });
    observers.emit(UxEvent::ActionDispatched {
        action_id: ActionId::NodeMarkTombstone,
        target: None,
    });
    // First passed; second fires without a fresh confirm.
    observers.emit(UxEvent::ActionDispatched {
        action_id: ActionId::NodeMarkTombstone,
        target: None,
    });

    let failures = probe.drain_failures();
    assert_eq!(failures.len(), 1);
}

#[test]
fn destructive_gate_cancelled_confirm_does_not_grant() {
    let probe = Arc::new(DestructiveActionGateProbe::iced_default());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    observers.emit(UxEvent::SurfaceDismissed {
        surface: SurfaceId::ConfirmDialog,
        reason: DismissReason::Cancelled,
    });
    observers.emit(UxEvent::ActionDispatched {
        action_id: ActionId::NodeMarkTombstone,
        target: None,
    });

    let failures = probe.drain_failures();
    assert_eq!(failures.len(), 1);
}

#[test]
fn destructive_gate_ignores_non_destructive_actions() {
    let probe = Arc::new(DestructiveActionGateProbe::iced_default());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    // Non-destructive action without a confirm — fine.
    observers.emit(UxEvent::ActionDispatched {
        action_id: ActionId::GraphTogglePhysics,
        target: None,
    });

    assert!(probe.drain_failures().is_empty());
}

#[test]
fn destructive_gate_intervening_action_consumes_grant() {
    // ConfirmDialog Confirmed grants the very next destructive
    // dispatch. A non-destructive ActionDispatched between confirm
    // and the destructive consumes the grant defensively, so the
    // destructive that follows trips the probe.
    let probe = Arc::new(DestructiveActionGateProbe::iced_default());
    let mut observers = UxObservers::new();
    observers.register(probe_as_observer(Arc::clone(&probe) as Arc<dyn UxProbe>));

    observers.emit(UxEvent::SurfaceDismissed {
        surface: SurfaceId::ConfirmDialog,
        reason: DismissReason::Confirmed,
    });
    observers.emit(UxEvent::ActionDispatched {
        action_id: ActionId::GraphTogglePhysics,
        target: None,
    });
    observers.emit(UxEvent::ActionDispatched {
        action_id: ActionId::NodeMarkTombstone,
        target: None,
    });

    let failures = probe.drain_failures();
    assert_eq!(failures.len(), 1);
}
