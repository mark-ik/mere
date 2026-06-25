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
    /// This persona's earned tessera standing — the folded ledger score. Replaces the
    /// raw op-count as the chip headline: the op-count is plumbing, the standing is the
    /// point (the whole reason tessera exists). `None` until the lane folds a ledger.
    /// (Tessera ledger fold.)
    pub standing: Option<i64>,
    /// This install's dialable ticket — share it with a peer to connect. Surfaced from
    /// the sync actor (it was previously only `tracing`-logged, so a user had to read
    /// logs to share it). (Tessera ticket surface.)
    pub ticket: Option<String>,
}

impl SyncIndicator {
    /// The chip text: an honest one-line state. `p2p off` before a lane joins, then
    /// `<label>: idle` (joined, nothing caught up), `<label>: syncing` (round in
    /// progress), then `<label>: +N standing` once a ledger folds (the earned standing,
    /// the point of tessera), falling back to `<label>: N ops` when only raw catch-up
    /// is known.
    pub fn summary(&self) -> String {
        if !self.active {
            return "p2p off".to_string();
        }
        if self.syncing {
            return format!("{}: syncing", self.label);
        }
        // Earned standing is the headline once the ledger has folded; the raw op-count
        // is the fallback for "caught up but no ledger yet". (Tessera ledger fold.)
        if let Some(standing) = self.standing {
            return format!("{}: {:+} standing", self.label, standing);
        }
        if self.ops > 0 {
            return format!("{}: {} ops", self.label, self.ops);
        }
        format!("{}: idle", self.label)
    }

    /// The dialable ticket to share with a peer, if the lane produced one (the actor
    /// surfaces it here instead of only logging it). (Tessera ticket surface.)
    pub fn ticket(&self) -> Option<&str> {
        self.ticket.as_deref()
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
        let i = SyncIndicator {
            label: "tessera".into(),
            active: true,
            ..Default::default()
        };
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

    #[test]
    fn standing_is_the_headline_once_the_ledger_folds() {
        // The earned standing replaces the raw op-count as the chip headline — the
        // whole point of tessera, made visible. (Tessera ledger fold.)
        let i = SyncIndicator {
            label: "tessera".into(),
            active: true,
            ops: 5, // op-count still known, but standing takes the headline
            standing: Some(3),
            ..Default::default()
        };
        assert_eq!(i.summary(), "tessera: +3 standing");

        // Debt reads honestly (negative standing).
        let owing = SyncIndicator {
            standing: Some(-2),
            ..i.clone()
        };
        assert_eq!(owing.summary(), "tessera: -2 standing");
    }

    #[test]
    fn ticket_is_surfaced_for_sharing() {
        let i = SyncIndicator {
            label: "tessera".into(),
            active: true,
            ticket: Some("dial:abc123".into()),
            ..Default::default()
        };
        assert_eq!(i.ticket(), Some("dial:abc123"));
    }
}
