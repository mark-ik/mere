/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The chrome's sync-status view-model (S5.0).
//!
//! A small, host-neutral projection of a synced lane's status, rendered as a chip
//! in the toolbar band. The host owns the mutation (the M4 contract): it maps the
//! real `moothold::tessera::SyncStatus` into this view-model and folds it into
//! [`Chrome::sync`](crate::Chrome::sync) via the runner, so the chrome domain
//! stays free of any p2p dependency.
//!
//! The summary is deliberately honest (the real-sync-feedback rule): `p2p off`
//! until a lane is joined, then `idle` (joined, nothing caught up — the true
//! state of a lone peer with no one to sync with), `syncing` during a round, and
//! a real operation count once a peer's log has been reconciled. No placebo.

/// A chrome-side snapshot of one synced lane's status, projected from the real
/// `SyncStatus` by the host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncIndicator {
    /// Short label for the lane (e.g. the moot's name).
    pub label: String,
    /// Whether a lane has been joined at all (`false` ⇒ p2p is off / failed).
    pub active: bool,
    /// A reconciliation round is currently in progress.
    pub syncing: bool,
    /// Operations caught up over the lane so far.
    pub ops: u64,
    /// Unix-epoch milliseconds of the last sync activity, if any (reserved for a
    /// future "N ago" readout).
    pub last_activity_ms: Option<u64>,
}

impl SyncIndicator {
    /// The chip text: an honest one-line state. `p2p off` before a lane joins,
    /// then `<label>: idle` (joined, nothing caught up), `<label>: syncing`
    /// (round in progress), or `<label>: N ops` (a peer's log reconciled).
    pub fn summary(&self) -> String {
        if !self.active {
            return "p2p off".to_string();
        }
        if self.syncing {
            return format!("{}: syncing", self.label);
        }
        if self.ops > 0 {
            return format!("{}: {} ops", self.label, self.ops);
        }
        format!("{}: idle", self.label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_until_a_lane_joins() {
        assert_eq!(SyncIndicator::default().summary(), "p2p off");
    }

    #[test]
    fn idle_when_joined_with_no_activity() {
        // The honest state of a lone peer: joined, but nothing to sync with yet.
        let i = SyncIndicator { label: "tessera".into(), active: true, ..Default::default() };
        assert_eq!(i.summary(), "tessera: idle");
    }

    #[test]
    fn syncing_during_a_round() {
        let i = SyncIndicator {
            label: "tessera".into(),
            active: true,
            syncing: true,
            ..Default::default()
        };
        assert_eq!(i.summary(), "tessera: syncing");
    }

    #[test]
    fn shows_the_real_op_count_after_catch_up() {
        let i = SyncIndicator {
            label: "tessera".into(),
            active: true,
            ops: 3,
            last_activity_ms: Some(1_000),
            ..Default::default()
        };
        assert_eq!(i.summary(), "tessera: 3 ops");
    }
}
