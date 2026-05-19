/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Typed action bus — target-scoped, gate-checkable, diagnostics-
//! emitting dispatch.
//!
//! Per the multiplexer framing brief §5.8 and the
//! [typed action bus refactor plan](../../../../../../design_docs/mere_docs/implementation_strategy/2026-05-11_typed_action_bus_plan.md):
//! every keybinding, palette invocation, AccessKit action, drag
//! gesture, and future external IPC routes through one
//! dispatch path. This module owns the **data shape** + the
//! **permission gate trait** — both portable. The host adapter
//! (currently `mere-host`) owns the execute side, which translates
//! a `BusAction` into the host's native action or command surface.
//!
//! ## Architecture
//!
//! ```text
//!   keybinding / palette / AccessKit / IPC / drag
//!                 │
//!                 ▼  BusAction { target, kind }
//!         ┌───────────────┐
//!         │ check_permission  ─► PermissionDecision
//!         │ + emit diag      │
//!         └───────────────┘
//!                 │
//!                 ▼  (allowed only)
//!         ┌───────────────┐
//!         │ BusDispatcher  │  host-side trait impl
//!         │ ::execute      │  ← translates kind → host action,
//!         └───────────────┘     calls cx.dispatch_action /
//!                               HostRoot methods.
//! ```
//!
//! ## What's portable, what's not
//!
//! Portable (this module):
//! - `ActionTarget`, `ActionKind`, `BusAction`, `TearOutMode`
//! - `PermissionGate` trait, `PermissionDecision`, `PermitEverythingGate`
//! - `BusDispatchOutcome` (Allowed / Denied / TargetMissing)
//! - `check_permission` — pure function
//!
//! Host-specific (mere-host or future siblings):
//! - `BusDispatcher` impl (the `execute` side)
//! - Diagnostics emission (currently routes through the apparatus
//!   event buffer; runtime stays unaware of that mechanism)
//! - Target inference (which target a no-target keybinding maps to;
//!   depends on host state like "active workbench")
//!
//! ## v0 scope
//!
//! Types + gate + tests. Migration of existing actions through the
//! bus is the explicit next slice; this module's API is what that
//! migration targets.

use frame::{FrameId, GraphId, InsertSide, PaneId, SessionId};
use mere_identity::PersonaId;
use kernel::graph::{
    EdgeAssertion, NavigationTrigger, NodeKey, RelationKind, RelationSelector,
};

/// Where on the multiplexer's identity tree an action operates.
///
/// Composes with [`ActionKind`] to address any scope from
/// "everything" to "this specific node." Target inference (when a
/// keybinding doesn't carry an explicit target) is host-side, since
/// it depends on host state like "active workbench."
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ActionTarget {
    /// Application-wide. Quit, open-new-window, app-level settings.
    App,
    /// Scoped to a persona — multi-identity work, persona-scoped
    /// engine profiles, persona switching. Reserved; no v0 actions
    /// use it.
    Persona(PersonaId),
    /// Scoped to a session. Most multi-window operations: kill,
    /// fork-from, attach-into-window, consolidate-to-engram.
    Session(SessionId),
    /// Scoped to a frame layout. Save / restore / apply-template.
    Frame(FrameId),
    /// Scoped to a single pane. Close-pane, focus-pane,
    /// tear-out, toggle-panel-here.
    Pane(PaneId),
    /// Scoped to a graph node. Reserved for node-level actions
    /// (rename, pin, follow).
    Node(GraphId, NodeKey),
    /// Scoped to a verso-core surface. Reserved.
    Surface(SurfaceId),
}

/// Placeholder for a verso-core surface identifier. v0: opaque u64
/// stand-in. When verso-core exposes a stable id type, swap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceId(pub u64);

/// Tear-out gesture mode. Three coexisting operations per the
/// [tear-out operations brief](../../../../../../design_docs/mere_docs/research/2026-05-11_tearout_operations_brief.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TearOutMode {
    /// Leaf — UI handle only; donor unchanged; new window holds a
    /// tile facet of the donor's node.
    Leaf,
    /// Branch — new graphlet in donor's graph-tree. Lives or dies
    /// with the donor session.
    Branch,
    /// Fork — new session + new graph. Independent of donor.
    Fork,
    /// Legacy compatibility for the Phase 2 Part 1 actions
    /// (`TearOutTileToNewGraphMinimized` / `Visible`). Maps to
    /// `Fork` but with the minimised orrery layout. Will retire
    /// when the action surface unifies on Leaf/Branch/Fork.
    ForkMinimized,
    /// Legacy compatibility — `Fork` with 50/50 split.
    ForkVisible,
}

/// What the action does. One variant per current and reserved
/// action. Variants carry only the arguments not already encoded
/// in the target.
#[derive(Clone, Debug, PartialEq)]
pub enum ActionKind {
    // ---- App lifecycle (target = App)
    Quit,
    OpenNewWindow,

    // ---- Chrome (target = App or Frame)
    FocusOmnibar,
    OpenPalette,
    CycleShellbarPosition,
    CycleWorkbenchStripPosition,

    // ---- Navigation (target = Pane, usually the active workbench)
    NavigateTo {
        address: String,
        new_tile: bool,
    },
    GoBack,
    GoForward,
    Reload,
    FocusTile {
        index: usize,
    },
    CloseTile {
        index: usize,
    },

    // ---- Pane management (target = Frame / Pane)
    ClosePane,
    /// Move the `source` pane to sit adjacent to the *target* pane
    /// (target = the dispatch's `ActionTarget::Pane`) on `side`.
    /// Source's content + pane_id + per-pane state survive the
    /// move. Per the pane-UX brief §1.
    ReparentPane {
        source: PaneId,
        side: InsertSide,
    },
    ToggleWorkbench,
    ToggleGloss,
    ToggleApparatus,
    SummonOrreryForNewGraph,
    SummonOrreryForGraph(GraphId),
    ToggleGraphSwitcher,

    // ---- Tear-out (target = Pane; mode picks leaf/branch/fork)
    /// Tear out a tile from a workbench. `tile_index = None` means
    /// "use the donor pane's currently-active tile" (the usual
    /// case for keybinding-driven tear-out). `tile_index = Some(i)`
    /// names a specific tile in the donor's strip — drag-drop
    /// gestures and right-click-on-tile-row use this so the
    /// torn-out tile is the one the user *grabbed*, not whatever
    /// happens to be active at drop time.
    TearOutTile {
        mode: TearOutMode,
        #[allow(dead_code)]
        tile_index: Option<usize>,
    },
    /// Toast button — promote an existing leaf window to a branch
    /// or fork. Target is the leaf window's workbench pane.
    PromoteLeafToBranch,
    PromoteLeafToFork,

    /// Pin a graph node as a standalone tile leaf in the frame —
    /// renders the document body without a workbench strip. Per
    /// the pane-UX brief §3 frametree side-by-side rendering.
    /// Source: the active workbench's active tile (or explicit
    /// node when called from a context-menu builder). The new
    /// `PaneContent::Tile(LeafNodeRef)` leaf summons to the right
    /// of the target pane.
    PinTileToFrame {
        node: frame::LeafNodeRef,
        graph_id: GraphId,
    },

    // ---- Phase 3 fork machinery (target = Session)
    ForkStickyNoteSession,

    // ---- Graph mutation (target = App; the kind carries graph_id
    // because graph truth is shared across windows, not pane-scoped)
    /// Add a typed relation to the `from → to` edge in `graph_id`.
    AssertRelation {
        graph_id: GraphId,
        from: NodeKey,
        to: NodeKey,
        assertion: EdgeAssertion,
    },
    /// Remove a relation matching `selector` from the `from → to`
    /// edge — mutates graph truth. Per the relation-taxonomy plan
    /// §6.1 only user-authored Semantic / AgentDerived relations are
    /// retractable; the host gates which menu entries offer this.
    RetractRelation {
        graph_id: GraphId,
        from: NodeKey,
        to: NodeKey,
        selector: RelationSelector,
    },
    /// Append a traversal event to the `from → to` edge.
    AppendTraversal {
        graph_id: GraphId,
        from: NodeKey,
        to: NodeKey,
        trigger: NavigationTrigger,
    },
    /// Remove a node and all its incident edges from `graph_id`.
    RemoveNode {
        graph_id: GraphId,
        node: NodeKey,
    },

    // ---- View-local relation policy (target = Pane; the orrery
    // leaf whose view should hide the relation)
    /// Toggle a relation's hidden state in one orrery pane's view.
    /// View-local — never mutates graph truth. Per the
    /// relation-taxonomy plan §6.2 the host stores this per-orrery.
    HideRelation {
        graph_id: GraphId,
        from: NodeKey,
        to: NodeKey,
        kind: RelationKind,
    },

    // ---- Manifest / session lifecycle (target = Session)
    KillSession,
    SnapshotSessionToEngram,
    ConsolidateBranchToEngram,
    ConsolidateForkToEngram,

    // ---- Broadcast (target = Frame; fans out to children)
    BroadcastNavigate {
        address: String,
    },
}

/// A bus dispatch — target + kind + nothing else. Diagnostics and
/// permission decisions attach context as the dispatch flows
/// through the host's dispatcher.
#[derive(Clone, Debug, PartialEq)]
pub struct BusAction {
    pub target: ActionTarget,
    pub kind: ActionKind,
}

impl BusAction {
    pub fn new(target: ActionTarget, kind: ActionKind) -> Self {
        Self { target, kind }
    }

    /// Sugar for App-scoped actions.
    pub fn app(kind: ActionKind) -> Self {
        Self::new(ActionTarget::App, kind)
    }

    /// Sugar for Pane-scoped actions.
    pub fn pane(pane_id: PaneId, kind: ActionKind) -> Self {
        Self::new(ActionTarget::Pane(pane_id), kind)
    }

    /// Sugar for Session-scoped actions.
    pub fn session(session_id: SessionId, kind: ActionKind) -> Self {
        Self::new(ActionTarget::Session(session_id), kind)
    }
}

/// A gate's verdict on whether an action may proceed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    Deny(DenyReason),
}

/// Why a gate denied an action. Distinguishes coarse categories so
/// diagnostics can surface why without leaking gate internals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DenyReason {
    /// Target session / pane / node didn't exist or wasn't
    /// addressable.
    UnknownTarget,
    /// Capability not granted for this target.
    CapabilityMissing(&'static str),
    /// Action explicitly blocked by user policy or persona policy.
    PolicyDenied(&'static str),
    /// Host-specific failure — e.g. cross-session attach with a
    /// dangling reference. Free-text for now; gate catalogue will
    /// classify these as it grows.
    Other(&'static str),
}

/// The runtime-side dispatch outcome. `Allowed` means the gate let
/// the action through; the host adapter still has to execute it.
/// `TargetMissing` distinguishes "the target identifier didn't
/// resolve to anything live" from "permission denied."
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusDispatchOutcome {
    Allowed,
    Denied(DenyReason),
    /// The gate said yes, but the target resolution against current
    /// state showed there's nothing to act on (closed pane, killed
    /// session). Surfaced separately so callers can distinguish
    /// user-policy denial from stale references.
    TargetMissing,
}

/// Permission gate trait. The bus calls `check` for every action;
/// the host can plug in any policy.
///
/// v0 wiring uses [`PermitEverythingGate`] so existing behaviour
/// is unchanged. The capability-gate catalogue brief lands a real
/// `SessionPolicyGate` that reads `SessionPolicy.overrides`.
pub trait PermissionGate {
    fn check(&self, action: &BusAction) -> PermissionDecision;
}

/// Open-everything gate. v0 default.
pub struct PermitEverythingGate;

impl PermissionGate for PermitEverythingGate {
    fn check(&self, _action: &BusAction) -> PermissionDecision {
        PermissionDecision::Allow
    }
}

/// Refuse-everything gate. Useful for tests and for "user toggled
/// the app into read-only mode."
pub struct RefuseEverythingGate;

impl PermissionGate for RefuseEverythingGate {
    fn check(&self, _action: &BusAction) -> PermissionDecision {
        PermissionDecision::Deny(DenyReason::PolicyDenied("refuse-everything gate"))
    }
}

/// Run an action through a gate. Pure function — the host wraps
/// this with target resolution and diag emission to produce a full
/// dispatch.
pub fn check_permission(action: &BusAction, gate: &dyn PermissionGate) -> PermissionDecision {
    gate.check(action)
}

/// Listener function the bus calls for every allowed action.
/// `&BusAction` is borrowed; listeners that need to keep the action
/// clone it inside themselves. `Send + Sync` so a bus may be shared
/// across threads even though v0 dispatch is single-threaded.
pub type ActionListener = Box<dyn Fn(&BusAction) + Send + Sync + 'static>;

/// Action bus — owns the permission gate + listener list, performs
/// dispatch.
///
/// v0 single-threaded; the trait bounds (`Send + Sync` on listeners,
/// `&self` dispatch) leave space for a threaded host without
/// breaking the API. The bus is **policy + fan-out only** — it
/// neither resolves targets to live state nor executes
/// `ActionKind`s; that's the host adapter's responsibility (per the
/// module's documented split: runtime vocabulary + bus, host
/// dispatcher).
///
/// Listener ordering: listeners are called in the order they were
/// registered, sequentially, with `&BusAction` (not `&mut`).
/// Listeners run only when the gate returned `Allow` — denied
/// actions short-circuit before any listener sees them.
pub struct ActionBus {
    gate: Box<dyn PermissionGate>,
    listeners: Vec<ActionListener>,
}

impl ActionBus {
    /// Construct a bus with the supplied gate and no listeners.
    pub fn new(gate: Box<dyn PermissionGate>) -> Self {
        Self {
            gate,
            listeners: Vec::new(),
        }
    }

    /// Convenience: bus with [`PermitEverythingGate`] installed.
    /// v0 default; hosts wanting real policy call
    /// [`Self::set_gate`].
    pub fn with_permit_everything() -> Self {
        Self::new(Box::new(PermitEverythingGate))
    }

    /// Replace the permission gate. Subsequent dispatches use the
    /// new gate; in-flight calls are unaffected (`&self` dispatch).
    pub fn set_gate(&mut self, gate: Box<dyn PermissionGate>) {
        self.gate = gate;
    }

    /// Register a listener. Listeners receive every allowed
    /// `BusAction` in registration order.
    pub fn add_listener<F>(&mut self, listener: F)
    where
        F: Fn(&BusAction) + Send + Sync + 'static,
    {
        self.listeners.push(Box::new(listener));
    }

    /// Number of registered listeners. Test hook + diagnostics.
    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }

    /// Dispatch `action`:
    /// 1. Check permission against the installed gate.
    /// 2. If `Allow`: call each listener in registration order,
    ///    return `Allowed`.
    /// 3. If `Deny(reason)`: return `Denied(reason)`, skip listeners.
    ///
    /// Returns `BusDispatchOutcome`. Hosts that want
    /// `TargetMissing` semantics layer that resolution on top of
    /// this dispatch — the bus itself doesn't know what's live.
    pub fn dispatch(&self, action: &BusAction) -> BusDispatchOutcome {
        match self.gate.check(action) {
            PermissionDecision::Allow => {
                for listener in &self.listeners {
                    listener(action);
                }
                BusDispatchOutcome::Allowed
            }
            PermissionDecision::Deny(reason) => BusDispatchOutcome::Denied(reason),
        }
    }
}

impl Default for ActionBus {
    fn default() -> Self {
        Self::with_permit_everything()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn fixture_session(idx: u128) -> SessionId {
        SessionId::from_uuid(Uuid::from_u128(0x1000 + idx))
    }

    fn fixture_pane(idx: u64) -> PaneId {
        PaneId(idx)
    }

    #[test]
    fn app_sugar_constructs_app_targeted_action() {
        let a = BusAction::app(ActionKind::Quit);
        assert_eq!(a.target, ActionTarget::App);
        assert_eq!(a.kind, ActionKind::Quit);
    }

    #[test]
    fn pane_sugar_uses_pane_target() {
        let a = BusAction::pane(fixture_pane(7), ActionKind::Reload);
        assert_eq!(a.target, ActionTarget::Pane(fixture_pane(7)));
    }

    #[test]
    fn session_sugar_uses_session_target() {
        let sid = fixture_session(1);
        let a = BusAction::session(sid, ActionKind::KillSession);
        assert_eq!(a.target, ActionTarget::Session(sid));
    }

    #[test]
    fn permit_everything_gate_allows_every_action() {
        let gate = PermitEverythingGate;
        for action in sample_actions() {
            assert_eq!(check_permission(&action, &gate), PermissionDecision::Allow);
        }
    }

    #[test]
    fn refuse_everything_gate_denies_every_action() {
        let gate = RefuseEverythingGate;
        for action in sample_actions() {
            assert!(matches!(
                check_permission(&action, &gate),
                PermissionDecision::Deny(_)
            ));
        }
    }

    #[test]
    fn action_bus_dispatch_with_permit_calls_listeners() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let mut bus = ActionBus::with_permit_everything();
        let counter = Arc::new(AtomicUsize::new(0));
        let captured = counter.clone();
        bus.add_listener(move |_action| {
            captured.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(bus.listener_count(), 1);

        let action = BusAction::app(ActionKind::Quit);
        let outcome = bus.dispatch(&action);
        assert_eq!(outcome, BusDispatchOutcome::Allowed);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn action_bus_dispatch_with_refuse_short_circuits() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let mut bus = ActionBus::new(Box::new(RefuseEverythingGate));
        let counter = Arc::new(AtomicUsize::new(0));
        let captured = counter.clone();
        bus.add_listener(move |_action| {
            captured.fetch_add(1, Ordering::SeqCst);
        });
        let action = BusAction::app(ActionKind::Quit);
        let outcome = bus.dispatch(&action);
        match outcome {
            BusDispatchOutcome::Denied(_) => {}
            other => panic!("expected Denied, got {:?}", other),
        }
        assert_eq!(
            counter.load(Ordering::SeqCst),
            0,
            "listener should not fire on deny"
        );
    }

    #[test]
    fn action_bus_listeners_run_in_registration_order() {
        use std::sync::{Arc, Mutex};

        let mut bus = ActionBus::with_permit_everything();
        let log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
        let l1 = log.clone();
        let l2 = log.clone();
        bus.add_listener(move |_| l1.lock().unwrap().push("first"));
        bus.add_listener(move |_| l2.lock().unwrap().push("second"));

        let action = BusAction::app(ActionKind::OpenNewWindow);
        bus.dispatch(&action);
        assert_eq!(*log.lock().unwrap(), vec!["first", "second"]);
    }

    #[test]
    fn action_bus_set_gate_changes_policy_for_subsequent_dispatch() {
        let mut bus = ActionBus::with_permit_everything();
        let action = BusAction::app(ActionKind::Quit);
        assert_eq!(bus.dispatch(&action), BusDispatchOutcome::Allowed);
        bus.set_gate(Box::new(RefuseEverythingGate));
        assert!(matches!(
            bus.dispatch(&action),
            BusDispatchOutcome::Denied(_)
        ));
    }

    #[test]
    fn deny_reason_round_trips_through_pattern_match() {
        let r = DenyReason::CapabilityMissing("broadcast.cross_session");
        match r {
            DenyReason::CapabilityMissing(s) => assert_eq!(s, "broadcast.cross_session"),
            _ => panic!("wrong arm"),
        }
    }

    #[test]
    fn tear_out_mode_distinguishes_three_operations() {
        let leaf = BusAction::pane(
            fixture_pane(1),
            ActionKind::TearOutTile {
                mode: TearOutMode::Leaf,
                tile_index: None,
            },
        );
        let branch = BusAction::pane(
            fixture_pane(1),
            ActionKind::TearOutTile {
                mode: TearOutMode::Branch,
                tile_index: None,
            },
        );
        let fork = BusAction::pane(
            fixture_pane(1),
            ActionKind::TearOutTile {
                mode: TearOutMode::Fork,
                tile_index: None,
            },
        );
        assert_ne!(leaf, branch);
        assert_ne!(branch, fork);
        assert_ne!(leaf, fork);
    }

    /// Conditional gate that refuses any action targeting a
    /// specific session — fixture for testing custom policies.
    struct RefuseSessionGate {
        forbidden: SessionId,
    }

    impl PermissionGate for RefuseSessionGate {
        fn check(&self, action: &BusAction) -> PermissionDecision {
            if let ActionTarget::Session(s) = &action.target {
                if *s == self.forbidden {
                    return PermissionDecision::Deny(DenyReason::PolicyDenied(
                        "session quarantined",
                    ));
                }
            }
            PermissionDecision::Allow
        }
    }

    #[test]
    fn custom_gate_selects_on_target_session() {
        let bad = fixture_session(2);
        let good = fixture_session(3);
        let gate = RefuseSessionGate { forbidden: bad };
        let denied = BusAction::session(bad, ActionKind::KillSession);
        let allowed = BusAction::session(good, ActionKind::KillSession);
        assert!(matches!(
            check_permission(&denied, &gate),
            PermissionDecision::Deny(DenyReason::PolicyDenied("session quarantined"))
        ));
        assert_eq!(check_permission(&allowed, &gate), PermissionDecision::Allow);
    }

    fn sample_actions() -> Vec<BusAction> {
        vec![
            BusAction::app(ActionKind::Quit),
            BusAction::app(ActionKind::OpenNewWindow),
            BusAction::pane(fixture_pane(1), ActionKind::Reload),
            BusAction::pane(
                fixture_pane(1),
                ActionKind::NavigateTo {
                    address: "mere://intro".to_string(),
                    new_tile: false,
                },
            ),
            BusAction::pane(
                fixture_pane(1),
                ActionKind::TearOutTile {
                    mode: TearOutMode::Leaf,
                    tile_index: None,
                },
            ),
            BusAction::session(fixture_session(1), ActionKind::KillSession),
            BusAction::session(fixture_session(1), ActionKind::SnapshotSessionToEngram),
        ]
    }
}
