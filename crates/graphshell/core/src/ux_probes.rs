/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-neutral UX probes — assertion-shaped consumers of the
//! [`UxEvent`](crate::ux_observability::UxEvent) stream.
//!
//! Where [`UxObserver`](crate::ux_observability::UxObserver) is a
//! passive listener (counting, recording), a `UxProbe` is an active
//! invariant-checker. Each probe encodes one rule that the iced
//! jump-ship plan §4.10 calls out — e.g., "no two modal-like
//! surfaces open at the same time" — and reports
//! [`ProbeFailure`]s when the rule is violated.
//!
//! Probes plug into the same `UxObservers` registry as plain
//! observers via [`UxProbe::as_observer`]. Tests register probes,
//! drive the iced host through a sequence of messages, and
//! `drain_failures()` returns any violations. Hosts can additionally
//! wire a probe in production to surface violations as soft warnings
//! through the diagnostics channel registry.
//!
//! ## Canonical probes
//!
//! - [`MutualExclusionProbe`] (Slice 25) — at most one of the modal-like
//!   surfaces (Command Palette / Node Finder / Context Menu /
//!   Confirm Dialog) is open at a time. The dismissal-before-open
//!   sequencing the iced host emits during supersession satisfies
//!   this invariant; any host that opens a second modal without
//!   first dismissing the prior one trips the probe.
//! - [`OpenDismissBalanceProbe`] (Slice 25) — every `SurfaceOpened`
//!   event is eventually paired with a matching `SurfaceDismissed`.
//!   Used to catch surface leaks where a dismissal path is forgotten.
//! - [`ProductiveSelectionProbe`] (Slice 48) — every Confirmed
//!   dismissal of a configured surface must be followed by a
//!   "productive" event (action dispatch, open-node dispatch, or a
//!   specific successor surface opening). Covers the §4.10 guarantees
//!   that selection-shaped surfaces emit explicit intents on
//!   confirmation. The probe is parameterised by a list of
//!   [`ProductiveRule`]s so callers can express "Palette Confirmed →
//!   ActionDispatched" alongside "NodeFinder Confirmed →
//!   OpenNodeDispatched" in a single probe.
//! - [`DestructiveActionGateProbe`] (Slice 48) — every
//!   [`UxEvent::ActionDispatched`] for a configured-destructive
//!   `ActionId` must be preceded (as the most-recent ConfirmDialog
//!   event) by a Confirmed dismissal of `ConfirmDialog`. Covers the
//!   §4.10 guarantee that destructive actions (Tombstone, Remove
//!   edge, ...) always carry a confirmation step.
//!
//! ## Extensibility
//!
//! Adding a probe is implementing [`UxProbe`] in any crate (no need
//! to land it in core). The probes that ship in core do so because
//! they're generally useful and have no host-specific data
//! dependencies.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::actions::ActionId;
use crate::ux_observability::{DismissReason, SurfaceId, UxEvent, UxObserver};

/// One rule violation reported by a probe.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeFailure {
    /// Stable name of the probe that reported the failure (e.g.,
    /// `"mutual_exclusion"`). Diagnostics surfaces use this to group
    /// related failures.
    pub probe_name: &'static str,
    /// Human-readable description of the violation.
    pub description: String,
    /// The event that triggered the failure (or that immediately
    /// preceded the discovery, if the violation is detected on a
    /// later event).
    pub triggering_event: UxEvent,
}

/// A `UxProbe` observes events and accumulates failures when its
/// invariant is broken. The trait is `Send + Sync` so probes can be
/// shared across threads (iced is single-threaded today, but future
/// hosts may dispatch observers in parallel).
pub trait UxProbe: Send + Sync {
    /// Stable identifier. Used by [`ProbeFailure::probe_name`] and
    /// in diagnostics rollups.
    fn name(&self) -> &'static str;
    /// Observe the next event in the stream.
    fn observe(&self, event: &UxEvent);
    /// Drain accumulated failures since the last drain. Each call
    /// returns failures recorded since the prior call; the probe's
    /// internal failure list resets to empty.
    fn drain_failures(&self) -> Vec<ProbeFailure>;
}

/// Adapter converting a probe `Arc` into a boxed
/// [`UxObserver`](crate::ux_observability::UxObserver) suitable for
/// registration on the runtime's observer registry. The probe stays
/// queryable via the original `Arc` so the test or diagnostics-pane
/// host code can call `drain_failures()` after running messages.
pub fn probe_as_observer(probe: Arc<dyn UxProbe>) -> Box<dyn UxObserver> {
    Box::new(ProbeAdapter(probe))
}

struct ProbeAdapter(Arc<dyn UxProbe>);

impl UxObserver for ProbeAdapter {
    fn observe(&self, event: &UxEvent) {
        self.0.observe(event);
    }
}

// ---------------------------------------------------------------------------
// MutualExclusionProbe — at most one modal-like surface open at a time
// ---------------------------------------------------------------------------

/// The set of surfaces that must form a mutually-exclusive group:
/// at most one of these may be open at any given instant. Other
/// surfaces (StatusBar, NavigatorHost, panes) are always present and
/// not subject to this rule.
fn is_modal_like(surface: SurfaceId) -> bool {
    matches!(
        surface,
        SurfaceId::CommandPalette
            | SurfaceId::NodeFinder
            | SurfaceId::ContextMenu
            | SurfaceId::ConfirmDialog
            | SurfaceId::NodeCreate
            | SurfaceId::FrameRename
    )
}

/// Asserts that at most one modal-like surface is open at a time.
/// The iced host's "dismiss-before-open" supersession sequencing
/// satisfies this — opening a second modal must emit a dismissal of
/// the prior one *first*.
pub struct MutualExclusionProbe {
    open_modals: Mutex<Vec<SurfaceId>>,
    failures: Mutex<Vec<ProbeFailure>>,
}

impl MutualExclusionProbe {
    pub fn new() -> Self {
        Self {
            open_modals: Mutex::new(Vec::new()),
            failures: Mutex::new(Vec::new()),
        }
    }
}

impl Default for MutualExclusionProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl UxProbe for MutualExclusionProbe {
    fn name(&self) -> &'static str {
        "mutual_exclusion"
    }

    fn observe(&self, event: &UxEvent) {
        match event {
            UxEvent::SurfaceOpened { surface } if is_modal_like(*surface) => {
                let mut open = self.open_modals.lock().unwrap();
                if !open.is_empty() {
                    self.failures.lock().unwrap().push(ProbeFailure {
                        probe_name: self.name(),
                        description: format!("opened {:?} while {:?} still open", surface, open),
                        triggering_event: event.clone(),
                    });
                }
                open.push(*surface);
            }
            UxEvent::SurfaceDismissed { surface, .. } if is_modal_like(*surface) => {
                let mut open = self.open_modals.lock().unwrap();
                if let Some(pos) = open.iter().position(|s| s == surface) {
                    open.swap_remove(pos);
                }
            }
            _ => {}
        }
    }

    fn drain_failures(&self) -> Vec<ProbeFailure> {
        std::mem::take(&mut *self.failures.lock().unwrap())
    }
}

// ---------------------------------------------------------------------------
// OpenDismissBalanceProbe — every Opened eventually gets Dismissed
// ---------------------------------------------------------------------------

/// Asserts that every `SurfaceOpened` for a given surface gets
/// matched by a `SurfaceDismissed` before the same surface is
/// re-opened. Catches leaks where a dismissal path is forgotten.
///
/// This probe *flags on re-open*, not on stream end (the stream
/// never explicitly ends). To check terminal balance, call
/// [`Self::pending_opens`] after running messages: any non-zero
/// count means a surface is still open.
pub struct OpenDismissBalanceProbe {
    open_counts: Mutex<HashMap<SurfaceId, u32>>,
    failures: Mutex<Vec<ProbeFailure>>,
}

impl OpenDismissBalanceProbe {
    pub fn new() -> Self {
        Self {
            open_counts: Mutex::new(HashMap::new()),
            failures: Mutex::new(Vec::new()),
        }
    }

    /// Snapshot the current per-surface open count (Opens minus
    /// Dismisses). A non-zero entry means that surface is currently
    /// open — useful for terminal-balance assertions in tests.
    pub fn pending_opens(&self) -> HashMap<SurfaceId, u32> {
        self.open_counts
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, c)| **c > 0)
            .map(|(s, c)| (*s, *c))
            .collect()
    }
}

impl Default for OpenDismissBalanceProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl UxProbe for OpenDismissBalanceProbe {
    fn name(&self) -> &'static str {
        "open_dismiss_balance"
    }

    fn observe(&self, event: &UxEvent) {
        match event {
            UxEvent::SurfaceOpened { surface } => {
                let mut counts = self.open_counts.lock().unwrap();
                let entry = counts.entry(*surface).or_insert(0);
                if *entry > 0 {
                    self.failures.lock().unwrap().push(ProbeFailure {
                        probe_name: self.name(),
                        description: format!(
                            "{:?} opened again while previous open is unmatched",
                            surface
                        ),
                        triggering_event: event.clone(),
                    });
                }
                *entry += 1;
            }
            UxEvent::SurfaceDismissed { surface, .. } => {
                let mut counts = self.open_counts.lock().unwrap();
                let entry = counts.entry(*surface).or_insert(0);
                if *entry == 0 {
                    self.failures.lock().unwrap().push(ProbeFailure {
                        probe_name: self.name(),
                        description: format!("{:?} dismissed without a matching open", surface),
                        triggering_event: event.clone(),
                    });
                } else {
                    *entry -= 1;
                }
            }
            _ => {}
        }
    }

    fn drain_failures(&self) -> Vec<ProbeFailure> {
        std::mem::take(&mut *self.failures.lock().unwrap())
    }
}

// ---------------------------------------------------------------------------
// ProductiveSelectionProbe — Confirmed dismissal must produce an outcome
// ---------------------------------------------------------------------------

/// One outcome that satisfies a [`ProductiveRule`]. A rule is satisfied
/// when the next event after the configured surface's Confirmed dismissal
/// matches any of its outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductiveOutcome {
    /// Any [`UxEvent::ActionDispatched`] satisfies the rule. Use for
    /// surfaces whose only effect is dispatching a `HostIntent::Action`
    /// (Command Palette, Confirm Dialog).
    AnyAction,
    /// A [`UxEvent::SurfaceOpened`] for `surface` satisfies the rule.
    /// Use for surfaces that route to a successor modal (Context Menu's
    /// destructive path opens ConfirmDialog; Command Palette host-routed
    /// actions open NodeCreate / FrameRename).
    Open(SurfaceId),
    /// Any [`UxEvent::OpenNodeDispatched`] satisfies the rule. Use for
    /// surfaces whose effect is opening a node (Node Finder).
    OpenNode,
}

/// Pairs a surface with the set of outcomes that count as productive
/// when that surface emits a Confirmed dismissal.
#[derive(Debug, Clone)]
pub struct ProductiveRule {
    pub surface: SurfaceId,
    pub outcomes: Vec<ProductiveOutcome>,
}

impl ProductiveRule {
    pub fn new(surface: SurfaceId, outcomes: Vec<ProductiveOutcome>) -> Self {
        Self { surface, outcomes }
    }
}

/// Asserts that every Confirmed dismissal of a configured surface is
/// followed (as the very next observable event) by a matching
/// [`ProductiveOutcome`]. The strictness of "very next event" relies
/// on the host emitting Dismissed → productive in the same update arm,
/// which the iced host satisfies for all five gs::Modal-backed surfaces.
///
/// Cancelled / Superseded / Programmatic dismissals are ignored —
/// only Confirmed dismissals carry the productive expectation.
pub struct ProductiveSelectionProbe {
    rules: Vec<ProductiveRule>,
    pending: Mutex<Option<Pending>>,
    failures: Mutex<Vec<ProbeFailure>>,
}

struct Pending {
    rule_idx: usize,
    triggering_event: UxEvent,
}

impl ProductiveSelectionProbe {
    pub fn new(rules: Vec<ProductiveRule>) -> Self {
        Self {
            rules,
            pending: Mutex::new(None),
            failures: Mutex::new(Vec::new()),
        }
    }

    /// Default rule set wiring the four selection-shaped surfaces the
    /// iced host emits today: Command Palette and Confirm Dialog
    /// confirm via `ActionDispatched`; Node Finder via
    /// `OpenNodeDispatched`; Context Menu via either `ActionDispatched`
    /// (immediate path) or `SurfaceOpened { ConfirmDialog }`
    /// (destructive gate path).
    pub fn iced_default() -> Self {
        Self::new(vec![
            ProductiveRule::new(
                SurfaceId::CommandPalette,
                vec![
                    ProductiveOutcome::AnyAction,
                    ProductiveOutcome::Open(SurfaceId::NodeCreate),
                    ProductiveOutcome::Open(SurfaceId::FrameRename),
                    ProductiveOutcome::Open(SurfaceId::CommandPalette),
                ],
            ),
            ProductiveRule::new(SurfaceId::NodeFinder, vec![ProductiveOutcome::OpenNode]),
            ProductiveRule::new(SurfaceId::ConfirmDialog, vec![ProductiveOutcome::AnyAction]),
            ProductiveRule::new(
                SurfaceId::ContextMenu,
                vec![
                    ProductiveOutcome::AnyAction,
                    ProductiveOutcome::Open(SurfaceId::ConfirmDialog),
                ],
            ),
        ])
    }

    fn outcome_matches(outcome: ProductiveOutcome, event: &UxEvent) -> bool {
        match (outcome, event) {
            (ProductiveOutcome::AnyAction, UxEvent::ActionDispatched { .. }) => true,
            (ProductiveOutcome::OpenNode, UxEvent::OpenNodeDispatched { .. }) => true,
            (ProductiveOutcome::Open(target), UxEvent::SurfaceOpened { surface }) => {
                target == *surface
            }
            _ => false,
        }
    }
}

impl UxProbe for ProductiveSelectionProbe {
    fn name(&self) -> &'static str {
        "productive_selection"
    }

    fn observe(&self, event: &UxEvent) {
        // First: if a productive expectation is pending, check whether
        // *this* event satisfies it. If so, clear the pending slot. If
        // not, record a failure (the dismissal was unproductive).
        let mut pending = self.pending.lock().unwrap();
        if let Some(p) = pending.as_ref() {
            let rule = &self.rules[p.rule_idx];
            let satisfied = rule
                .outcomes
                .iter()
                .any(|o| Self::outcome_matches(*o, event));
            if satisfied {
                *pending = None;
            } else {
                self.failures.lock().unwrap().push(ProbeFailure {
                    probe_name: self.name(),
                    description: format!(
                        "{:?} Confirmed dismissal not followed by a productive \
                         event (saw {:?} instead of {:?})",
                        rule.surface, event, rule.outcomes
                    ),
                    triggering_event: p.triggering_event.clone(),
                });
                *pending = None;
            }
        }

        // Second: if this event is a Confirmed dismissal of a configured
        // surface, arm a new expectation for the next event.
        if let UxEvent::SurfaceDismissed {
            surface,
            reason: DismissReason::Confirmed,
        } = event
        {
            if let Some(rule_idx) = self.rules.iter().position(|r| r.surface == *surface) {
                *pending = Some(Pending {
                    rule_idx,
                    triggering_event: event.clone(),
                });
            }
        }
    }

    fn drain_failures(&self) -> Vec<ProbeFailure> {
        std::mem::take(&mut *self.failures.lock().unwrap())
    }
}

// ---------------------------------------------------------------------------
// DestructiveActionGateProbe — destructive ActionDispatched needs ConfirmDialog
// ---------------------------------------------------------------------------

/// Asserts that every [`UxEvent::ActionDispatched`] for a destructive
/// `ActionId` is preceded (as the most recent ConfirmDialog event) by
/// a `ConfirmDialog` Confirmed dismissal. Covers the §4.10 guarantee
/// that destructive actions (Tombstone, Remove edge, ...) always carry
/// a confirmation step.
///
/// The probe is parameterised by the list of `ActionId`s the caller
/// considers destructive. Today the iced host marks `NodeMarkTombstone`
/// destructive in `items_for_target`; future destructive actions are
/// added by extending this list (and the corresponding
/// `ContextMenuEntry::destructive()` flag).
pub struct DestructiveActionGateProbe {
    destructive: Vec<ActionId>,
    /// True if the most recent ConfirmDialog event was a Confirmed
    /// dismissal. Cleared by Cancelled / Superseded dismissals or by
    /// any subsequent ActionDispatched (the grant is consumed).
    confirm_grant: Mutex<bool>,
    failures: Mutex<Vec<ProbeFailure>>,
}

impl DestructiveActionGateProbe {
    pub fn new(destructive: Vec<ActionId>) -> Self {
        Self {
            destructive,
            confirm_grant: Mutex::new(false),
            failures: Mutex::new(Vec::new()),
        }
    }

    /// Default wiring with the iced host's currently-known destructive
    /// actions. Extend this list as new destructive actions land.
    pub fn iced_default() -> Self {
        Self::new(vec![ActionId::NodeMarkTombstone])
    }
}

impl UxProbe for DestructiveActionGateProbe {
    fn name(&self) -> &'static str {
        "destructive_action_gate"
    }

    fn observe(&self, event: &UxEvent) {
        match event {
            UxEvent::SurfaceDismissed {
                surface: SurfaceId::ConfirmDialog,
                reason,
            } => {
                let mut grant = self.confirm_grant.lock().unwrap();
                *grant = matches!(reason, DismissReason::Confirmed);
            }
            UxEvent::ActionDispatched { action_id, .. } => {
                let mut grant = self.confirm_grant.lock().unwrap();
                if self.destructive.contains(action_id) && !*grant {
                    self.failures.lock().unwrap().push(ProbeFailure {
                        probe_name: self.name(),
                        description: format!(
                            "destructive action {:?} dispatched without a \
                             preceding ConfirmDialog Confirmed dismissal",
                            action_id
                        ),
                        triggering_event: event.clone(),
                    });
                }
                // Either way, consume the grant. A lingering grant must
                // not authorise a later, unrelated destructive dispatch.
                *grant = false;
            }
            _ => {}
        }
    }

    fn drain_failures(&self) -> Vec<ProbeFailure> {
        std::mem::take(&mut *self.failures.lock().unwrap())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ux_observability::{DismissReason, UxEvent, UxObservers};

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

        // Open palette, then open finder *without* dismissing palette.
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

        // StatusBar / NavigatorHost are always-present surfaces; the
        // mutual-exclusion rule shouldn't apply to them.
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

        let dummy = crate::graph::NodeKey::new(0);
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
}
