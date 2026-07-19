// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

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
//! observers via [`probe_as_observer`]. Tests register probes,
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
//!
//! The probe family is split across child modules to keep each file
//! under the workspace's 600-LOC ceiling — the trait + adapter live
//! here; each concrete probe gets its own file.

use std::sync::Arc;

use crate::ux_observability::{UxEvent, UxObserver};

mod destructive_action_gate;
mod mutual_exclusion;
mod open_dismiss_balance;
mod productive_selection;

#[cfg(test)]
mod tests;

pub use destructive_action_gate::DestructiveActionGateProbe;
pub use mutual_exclusion::MutualExclusionProbe;
pub use open_dismiss_balance::OpenDismissBalanceProbe;
pub use productive_selection::{ProductiveOutcome, ProductiveRule, ProductiveSelectionProbe};

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
