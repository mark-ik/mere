/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Steady-heat idle-cadence Athanor passes (Alembic B1, Path A): run forgetting
//! (P1) and consolidation (P2) automatically once the app has sat idle a while,
//! instead of only on their manual Alembic-pane triggers. Host-side timers on
//! `Shell` (no actor, no thread) — both passes are light enough (a snapshot + a
//! handful of evictions; a manifest list + a bounded pairwise compare) to ride the
//! existing `about_to_wait` tick rather than earning an armillary actor (the
//! spun-out plan's recommendation; the actor's reason to exist is the heavier
//! facet-extraction pass, not these two). One cadence drives both, per the plan.

use std::time::{Duration, Instant};

use super::*;

/// How long the app must sit with no window input before a pass is eligible. Keeps
/// an active browsing session from ever seeing a pass mid-interaction.
const IDLE_GRACE: Duration = Duration::from_secs(120);

/// The minimum gap between idle passes once eligible — "steady-heat", not a tight
/// poll. A pass is cheap (a snapshot + a handful of evictions), but there is no
/// reason to re-propose every tick once the graph has gone quiet.
const PASS_INTERVAL: Duration = Duration::from_secs(900);

impl Shell {
    /// Record window input as activity, deferring the next idle pass. Called from
    /// [`on_window_event`](Self::on_window_event) — every real input resets the
    /// clock; actor wakes (`user_event`) do not, so a settling physics actor or an
    /// in-flight fetch cannot itself suppress the pass forever.
    pub(crate) fn note_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Run Athanor's forgetting (P1) and consolidation (P2) passes once the app has
    /// been idle past [`IDLE_GRACE`], at most every [`PASS_INTERVAL`] — one shared
    /// gate for both, per the plan's "driven by the same cadence". Called from
    /// `about_to_wait` on every tick; cheap when not due (two `Instant`
    /// comparisons). Runs against the **primary** window's ctx only — every pooled
    /// orrery and the content store are shared, so one pass covers the whole
    /// session (the spun-out plan's "which orrery" note); running it per-window
    /// would re-propose the same urls / engram pairs.
    ///
    /// Does not touch the event loop's `ControlFlow`: this app already runs the
    /// winit default (`Poll`, never set to `Wait`/`WaitUntil` anywhere today), so
    /// `about_to_wait` already ticks continuously and needs no extra wake scheduling
    /// here. (Verified against the live event-loop wiring, not assumed — see the
    /// progress note on this plan.)
    pub(crate) fn maybe_run_idle_forgetting_pass(&mut self) {
        let Some(primary) = self.primary else { return };
        let now = Instant::now();
        if now.duration_since(self.last_activity) < IDLE_GRACE {
            return;
        }
        if self
            .last_forgetting
            .is_some_and(|t| now.duration_since(t) < PASS_INTERVAL)
        {
            return;
        }
        if let Some(mut wc) = self.window_ctx(primary) {
            wc.run_forgetting_pass();
            wc.run_consolidation_pass();
        }
        self.last_forgetting = Some(now);
    }
}
