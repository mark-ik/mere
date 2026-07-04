/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Steady-heat idle-cadence snapshot refresh (node/card summoning design, §5 item
//! 4): redeposit every open workbench tile's thumbnail once the app has sat idle a
//! while, on top of the always-on boundary-triggered deposits (tile close, blur,
//! navigation-away, app suspend). Mirrors [`idle_forgetting`](super::idle_forgetting)
//! exactly — a host-side timer on `Shell` (no actor, no thread), ticked from
//! `about_to_wait`, with its own cadence and trigger so it is independently
//! tunable/disableable from the Athanor passes.
//!
//! Unlike the forgetting pass, this one walks **every** open window, not just
//! primary: `run_forgetting_pass` can stay primary-only because the graph and
//! content store are shared, but a tile's cached band and scroll position live on
//! that window's own `WindowView`, so a secondary window's open tiles would never
//! refresh otherwise.

use std::time::{Duration, Instant};

use super::*;

/// How long the app must sit with no window input before a pass is eligible.
/// Shares the forgetting pass's grace period — both read the same idle signal.
const IDLE_GRACE: Duration = Duration::from_secs(120);

/// The minimum gap between snapshot-refresh passes once eligible. Rasterizing +
/// reading back + PNG-encoding every open tile is heavier than a graph-metadata
/// evict, so this rides its own (not shared) steady-heat interval.
const PASS_INTERVAL: Duration = Duration::from_secs(900);

impl Shell {
    /// Redeposit every open workbench tile's thumbnail, across every open window,
    /// once the app has been idle past [`IDLE_GRACE`], at most every
    /// [`PASS_INTERVAL`], and only while `snapshot_idle_refresh` is on and the
    /// session's thumbnail byte budget isn't already spent. Called from
    /// `about_to_wait` on every tick; cheap when not due.
    ///
    /// The byte cap gates only this pass — boundary-triggered deposits (close,
    /// blur, navigation-away, suspend) and the on-demand snapshot-card render both
    /// keep depositing unconditionally, since those are the correctness-critical
    /// paths the summoning-design fix depends on. This is the optional, best-effort
    /// extra that keeps a long-open tile's preview fresh without a boundary crossing.
    pub(crate) fn maybe_run_idle_snapshot_refresh(&mut self) {
        if !self.shared.presentation.snapshot_idle_refresh {
            return;
        }
        let now = Instant::now();
        if now.duration_since(self.last_activity) < IDLE_GRACE {
            return;
        }
        if self
            .last_snapshot_refresh
            .is_some_and(|t| now.duration_since(t) < PASS_INTERVAL)
        {
            return;
        }
        if self.shared.session.thumbnail_bytes_this_session
            >= self.shared.session.thumbnail_byte_cap
        {
            return;
        }
        let window_ids: Vec<_> = self.windows.keys().copied().collect();
        for id in window_ids {
            if let Some(mut wc) = self.window_ctx(id) {
                wc.persist_workbench_boundary_thumbnails();
            }
        }
        self.last_snapshot_refresh = Some(now);
    }
}
