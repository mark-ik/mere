// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Lease vocabulary: the facts a lending device signs, and what the fold keeps.
//!
//! **Authority (settled at the M3a gate).** The job author signs [`LeaseTerms`]
//! into the spec once, at post time. The deterministic claim winner for an
//! epoch then authors its *own* grant, valid only inside that envelope. Author
//! authority is therefore exercised while the author is online and stays valid
//! after it goes away — the alternative, granting each lease by hand, stalls
//! every job whose author closed the lid.
//!
//! **No clocks in the fold.** Every rule here compares one *signed* timestamp
//! with another: a grant for epoch N+1 is admissible when its `granted_at_ms`
//! is at or past epoch N's signed end. Liveness — the only question that needs
//! "now" — lives in [`crate::projection`] and takes the observation time as an
//! argument.
//!
//! **Trusted-ring assumption.** A device could claim with a far-future `at_ms`
//! and reserve later epochs for itself. Inside the own-devices ring that is a
//! misconfiguration, not an attack, and the projection will not treat the
//! matching grant as live until that time actually arrives. The kith plan, which
//! widens the ring beyond one owner, has to revisit it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A lease's identity: the hash of the operation that granted it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LeaseId(pub [u8; 32]);

/// Longest lease window a job may authorize (24 hours).
pub const MAX_LEASE_DURATION_MS: u64 = 24 * 60 * 60 * 1000;
/// Shortest heartbeat interval a job may demand.
pub const MIN_HEARTBEAT_MS: u64 = 1_000;
/// Most consecutive missed heartbeats a job may tolerate.
pub const MAX_MISS_ALLOWANCE: u32 = 60;

/// The envelope the job author signs. A holder picks its window inside this and
/// cannot exceed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseTerms {
    /// Longest window a single grant may claim.
    pub max_duration_ms: u64,
    /// How often the holder must publish a heartbeat.
    pub heartbeat_ms: u64,
    /// Consecutive intervals of silence an observer tolerates before calling
    /// the lease lapsed.
    pub miss_allowance: u32,
}

impl LeaseTerms {
    /// A window with heartbeats every `heartbeat_ms` and three misses allowed.
    pub const fn new(max_duration_ms: u64, heartbeat_ms: u64) -> Self {
        Self {
            max_duration_ms,
            heartbeat_ms,
            miss_allowance: 3,
        }
    }

    pub fn validate(&self) -> Result<(), LeaseTermsError> {
        if self.max_duration_ms == 0 || self.max_duration_ms > MAX_LEASE_DURATION_MS {
            return Err(LeaseTermsError::Duration(self.max_duration_ms));
        }
        if self.heartbeat_ms < MIN_HEARTBEAT_MS || self.heartbeat_ms > self.max_duration_ms {
            return Err(LeaseTermsError::Heartbeat(self.heartbeat_ms));
        }
        if self.miss_allowance == 0 || self.miss_allowance > MAX_MISS_ALLOWANCE {
            return Err(LeaseTermsError::MissAllowance(self.miss_allowance));
        }
        Ok(())
    }

    /// How long an observer waits after the last sign of life before calling a
    /// lease silent.
    pub fn silence_window_ms(&self) -> u64 {
        self.heartbeat_ms
            .saturating_mul(u64::from(self.miss_allowance))
    }
}

/// Why lease terms were refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LeaseTermsError {
    #[error("lease duration {0}ms is outside 1..={MAX_LEASE_DURATION_MS}")]
    Duration(u64),
    #[error("heartbeat interval {0}ms is below the floor or above the lease duration")]
    Heartbeat(u64),
    #[error("miss allowance {0} is outside 1..={MAX_MISS_ALLOWANCE}")]
    MissAllowance(u32),
}

/// What a holder reports about how far it has got. Resource-defined units; the
/// mesh only carries them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseProgress {
    pub done: u64,
    /// `0` when the resource cannot say how much work there is.
    pub total: u64,
    /// Whether a resumable checkpoint exists on the holder's device. It is
    /// **local**: no other device can start from it until a blob lane exists.
    pub checkpoint_held: bool,
}

/// Why a holder gave the lease back. About the *work*.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReleaseReason {
    /// The holder decided not to continue, without failing.
    Yielded,
    /// The run errored.
    Failed,
    /// A granted input is not held by this device. Not unreliability: blob
    /// delivery is a host lane the mesh does not yet provide.
    InputUnavailable,
}

/// Why a device's own owner took the hardware back. About the *device*, never
/// about the worker — none of these is a reliability signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReclaimReason {
    ForegroundActivity,
    Battery,
    Thermal,
    Network,
    QuietHours,
    AtCapacity,
    PolicyChange,
    Manual,
}

/// How an epoch ended, by its own signed fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeaseEnd {
    Released { reason: ReleaseReason, at_ms: u64 },
    Reclaimed { reason: ReclaimReason, at_ms: u64 },
    Completed { at_ms: u64 },
}

impl LeaseEnd {
    pub fn at_ms(&self) -> u64 {
        match self {
            Self::Released { at_ms, .. }
            | Self::Reclaimed { at_ms, .. }
            | Self::Completed { at_ms } => *at_ms,
        }
    }

    /// Whether this end closes the job rather than opening the next epoch.
    pub fn is_final(&self) -> bool {
        matches!(self, Self::Completed { .. })
    }
}

/// One granted lease and everything admissible that happened under it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseEpoch {
    pub epoch: u32,
    pub lease: LeaseId,
    /// The granting operation's author — the holder. Never a wire field, so it
    /// cannot be forged.
    pub holder: [u8; 32],
    pub granted_at_ms: u64,
    pub expires_at_ms: u64,
    /// Latest admissible heartbeat, or `granted_at_ms` when none has arrived.
    pub last_seen_ms: u64,
    pub progress: LeaseProgress,
    pub end: Option<LeaseEnd>,
}

impl LeaseEpoch {
    /// The signed instant at which this epoch stopped binding the job: its end
    /// fact if it has one, otherwise its expiry.
    pub fn boundary_ms(&self) -> u64 {
        self.end.map_or(self.expires_at_ms, |end| end.at_ms())
    }

    /// Whether `at_ms` falls inside the granted window. Used to refuse facts
    /// authored outside the lease they name — a comparison of signed values.
    pub fn covers(&self, at_ms: u64) -> bool {
        at_ms >= self.granted_at_ms && at_ms <= self.expires_at_ms
    }

    /// Whether this epoch has stopped being live by `at_ms`, and why. The only
    /// function here that takes an observation time; everything else compares
    /// signed values. `skew_ms` slack always favours the holder.
    pub fn lapse_at(&self, at_ms: u64, terms: &LeaseTerms, skew_ms: u64) -> Option<LapseWindow> {
        if at_ms >= self.expires_at_ms.saturating_add(skew_ms) {
            return Some(LapseWindow::Expired);
        }
        let silent_from = self
            .last_seen_ms
            .saturating_add(terms.silence_window_ms())
            .saturating_add(skew_ms);
        if at_ms >= silent_from {
            return Some(LapseWindow::Silent);
        }
        None
    }
}

/// Which boundary a lease fell off. [`crate::projection`] names these for
/// callers; this is the raw comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LapseWindow {
    /// Past the signed window.
    Expired,
    /// Inside the window, but out of contact.
    Silent,
}

/// Every admissible lease fact for one job, in epoch order.
///
/// Empty for a job with no lease terms, which is why it is skipped on the wire:
/// an M1/M2 snapshot must still hash to the bytes its checkpoint committed to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRecord {
    epochs: Vec<LeaseEpoch>,
}

impl LeaseRecord {
    pub fn is_empty(&self) -> bool {
        self.epochs.is_empty()
    }

    pub fn epochs(&self) -> &[LeaseEpoch] {
        &self.epochs
    }

    /// The highest granted epoch.
    pub fn current(&self) -> Option<&LeaseEpoch> {
        self.epochs.last()
    }

    pub fn epoch(&self, epoch: u32) -> Option<&LeaseEpoch> {
        self.epochs.iter().find(|e| e.epoch == epoch)
    }

    pub fn by_lease(&self, lease: LeaseId) -> Option<&LeaseEpoch> {
        self.epochs.iter().find(|e| e.lease == lease)
    }

    /// The signed instant from which a claim may target the next epoch, and the
    /// epoch number it would target. `None` once the job has completed.
    pub fn next_epoch(&self) -> Option<(u32, u64)> {
        match self.epochs.last() {
            None => Some((0, 0)),
            Some(last) if last.end.is_some_and(|end| end.is_final()) => None,
            Some(last) => Some((last.epoch + 1, last.boundary_ms())),
        }
    }

    /// Whether a grant for `epoch` at `granted_at_ms` may follow what is already
    /// retained. Pure: signed value against signed value.
    pub fn accepts_grant(&self, epoch: u32, granted_at_ms: u64) -> bool {
        match self.next_epoch() {
            Some((next, boundary)) => epoch == next && granted_at_ms >= boundary,
            None => false,
        }
    }

    pub(crate) fn push(&mut self, epoch: LeaseEpoch) {
        self.epochs.push(epoch);
    }

    pub(crate) fn last_mut(&mut self) -> Option<&mut LeaseEpoch> {
        self.epochs.last_mut()
    }
}

/// A grant gathered from the log, before the chain rules are applied.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GrantFact {
    pub operation: [u8; 32],
    pub author: [u8; 32],
    pub epoch: u32,
    pub granted_at_ms: u64,
    pub expires_at_ms: u64,
}

/// A fact authored under an existing lease.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LeaseFact {
    pub operation: [u8; 32],
    pub author: [u8; 32],
    pub lease: LeaseId,
    pub at_ms: u64,
    pub body: LeaseFactBody,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum LeaseFactBody {
    Heartbeat(LeaseProgress),
    End(LeaseEnd),
}

/// Everything the fold gathered about one job's leases, keyed so that ties
/// resolve by operation hash on every peer.
#[derive(Default)]
pub(crate) struct GatheredLeases {
    pub grants: BTreeMap<[u8; 32], GrantFact>,
    pub facts: BTreeMap<[u8; 32], LeaseFact>,
}

impl GatheredLeases {
    /// Build the epoch chain. `winner_for(from_ms, until_ms)` supplies the
    /// deterministic claim winner among claims proposed in that signed window;
    /// the chain refuses any grant whose author is not that winner.
    ///
    /// The window is closed at both ends on purpose. Bounded below by the
    /// previous epoch's end, a stale claim cannot ride into a new epoch;
    /// bounded above by the grant's own `granted_at_ms`, a claim authored
    /// *later* cannot retroactively unseat a lease that is already running. A
    /// holder can only ever narrow the field against itself.
    ///
    /// `record` is whatever a retention checkpoint already settled; pass
    /// `LeaseRecord::default()` to build the chain from nothing.
    pub fn resolve(
        &self,
        mut record: LeaseRecord,
        terms: &LeaseTerms,
        winner_for: impl Fn(u64, u64) -> Option<[u8; 32]>,
    ) -> LeaseRecord {
        // Facts that arrived after the checkpoint still belong to its epoch.
        self.apply_facts(&mut record);
        // Grants are keyed by operation hash, so a peer that saw them in a
        // different order still walks them in the same order.
        let mut pending: Vec<&GrantFact> = self.grants.values().collect();
        pending.sort_by_key(|grant| (grant.epoch, grant.granted_at_ms, grant.operation));

        for grant in pending {
            if !record.accepts_grant(grant.epoch, grant.granted_at_ms) {
                continue;
            }
            if grant.expires_at_ms <= grant.granted_at_ms
                || grant.expires_at_ms - grant.granted_at_ms > terms.max_duration_ms
            {
                continue;
            }
            let boundary = record
                .next_epoch()
                .map(|(_, boundary)| boundary)
                .unwrap_or_default();
            if winner_for(boundary, grant.granted_at_ms) != Some(grant.author) {
                continue;
            }
            record.push(LeaseEpoch {
                epoch: grant.epoch,
                lease: LeaseId(grant.operation),
                holder: grant.author,
                granted_at_ms: grant.granted_at_ms,
                expires_at_ms: grant.expires_at_ms,
                last_seen_ms: grant.granted_at_ms,
                progress: LeaseProgress::default(),
                end: None,
            });
            self.apply_facts(&mut record);
        }
        record
    }

    /// Fold the heartbeats and end facts belonging to the newest epoch into it.
    fn apply_facts(&self, record: &mut LeaseRecord) {
        let Some(current) = record.last_mut() else {
            return;
        };
        let mut newest_heartbeat: Option<(u64, [u8; 32], LeaseProgress)> = None;
        let mut earliest_end: Option<(u64, [u8; 32], LeaseEnd)> = None;

        for fact in self.facts.values() {
            if fact.lease != current.lease
                || fact.author != current.holder
                || !current.covers(fact.at_ms)
            {
                continue;
            }
            match fact.body {
                LeaseFactBody::Heartbeat(progress) => {
                    let key = (fact.at_ms, fact.operation);
                    if newest_heartbeat.is_none_or(|(at, op, _)| key > (at, op)) {
                        newest_heartbeat = Some((fact.at_ms, fact.operation, progress));
                    }
                }
                LeaseFactBody::End(end) => {
                    let key = (fact.at_ms, fact.operation);
                    if earliest_end.is_none_or(|(at, op, _)| key < (at, op)) {
                        earliest_end = Some((fact.at_ms, fact.operation, end));
                    }
                }
            }
        }

        if let Some((at_ms, _, progress)) = newest_heartbeat {
            current.last_seen_ms = current.last_seen_ms.max(at_ms);
            current.progress = progress;
        }
        // The earliest end is the moment the lease actually stopped, whether it
        // came from this batch or from a checkpoint.
        if let Some((_, _, end)) = earliest_end
            && current.end.is_none_or(|held| end.at_ms() < held.at_ms())
        {
            current.end = Some(end);
        }
        if let Some(end) = current.end {
            // A heartbeat authored after the lease ended is not evidence of
            // life under it.
            current.last_seen_ms = current.last_seen_ms.min(end.at_ms());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms() -> LeaseTerms {
        LeaseTerms::new(60_000, 10_000)
    }

    #[test]
    fn terms_bound_duration_heartbeat_and_misses() {
        assert!(terms().validate().is_ok());
        assert_eq!(terms().silence_window_ms(), 30_000);

        let mut bad = terms();
        bad.max_duration_ms = 0;
        assert_eq!(bad.validate(), Err(LeaseTermsError::Duration(0)));
        bad.max_duration_ms = MAX_LEASE_DURATION_MS + 1;
        assert!(bad.validate().is_err());

        let mut fast = terms();
        fast.heartbeat_ms = MIN_HEARTBEAT_MS - 1;
        assert!(fast.validate().is_err());
        let mut slow = terms();
        slow.heartbeat_ms = slow.max_duration_ms + 1;
        assert!(slow.validate().is_err());

        let mut misses = terms();
        misses.miss_allowance = 0;
        assert_eq!(misses.validate(), Err(LeaseTermsError::MissAllowance(0)));
    }

    fn grant(op: u8, author: u8, epoch: u32, from: u64, to: u64) -> GrantFact {
        GrantFact {
            operation: [op; 32],
            author: [author; 32],
            epoch,
            granted_at_ms: from,
            expires_at_ms: to,
        }
    }

    fn gathered(grants: Vec<GrantFact>, facts: Vec<LeaseFact>) -> GatheredLeases {
        let mut out = GatheredLeases::default();
        for grant in grants {
            out.grants.insert(grant.operation, grant);
        }
        for fact in facts {
            out.facts.insert(fact.operation, fact);
        }
        out
    }

    #[test]
    fn the_chain_accepts_one_grant_per_epoch_from_the_claim_winner() {
        let record = gathered(vec![grant(1, 9, 0, 100, 1_000)], vec![]).resolve(
            LeaseRecord::default(),
            &terms(),
            |_, _| Some([9; 32]),
        );
        assert_eq!(record.epochs().len(), 1);
        assert_eq!(record.current().unwrap().holder, [9; 32]);
        assert_eq!(record.current().unwrap().lease, LeaseId([1; 32]));
        assert_eq!(record.next_epoch(), Some((1, 1_000)));
    }

    #[test]
    fn a_grant_from_a_non_winner_is_refused() {
        let record = gathered(vec![grant(1, 8, 0, 100, 1_000)], vec![]).resolve(
            LeaseRecord::default(),
            &terms(),
            |_, _| Some([9; 32]),
        );
        assert!(record.is_empty(), "only the claim winner may grant itself");
    }

    #[test]
    fn a_grant_wider_than_the_signed_envelope_is_refused() {
        let over = gathered(vec![grant(1, 9, 0, 0, 60_001)], vec![]);
        assert!(
            over.resolve(LeaseRecord::default(), &terms(), |_, _| Some([9; 32]))
                .is_empty()
        );
        let inverted = gathered(vec![grant(1, 9, 0, 500, 500)], vec![]);
        assert!(
            inverted
                .resolve(LeaseRecord::default(), &terms(), |_, _| Some([9; 32]))
                .is_empty()
        );
    }

    #[test]
    fn the_next_epoch_may_not_start_before_the_previous_one_ended() {
        let terms = terms();
        let early = gathered(
            vec![grant(1, 9, 0, 100, 1_000), grant(2, 8, 1, 999, 2_000)],
            vec![],
        );
        let record = early.resolve(LeaseRecord::default(), &terms, |from, _| {
            Some(if from == 0 { [9; 32] } else { [8; 32] })
        });
        assert_eq!(record.epochs().len(), 1, "epoch 1 overlapped epoch 0");

        let after = gathered(
            vec![grant(1, 9, 0, 100, 1_000), grant(2, 8, 1, 1_000, 2_000)],
            vec![],
        );
        let record = after.resolve(LeaseRecord::default(), &terms, |from, _| {
            Some(if from == 0 { [9; 32] } else { [8; 32] })
        });
        assert_eq!(record.epochs().len(), 2);
        assert_eq!(record.current().unwrap().holder, [8; 32]);
    }

    fn fact(op: u8, author: u8, lease: u8, at_ms: u64, body: LeaseFactBody) -> LeaseFact {
        LeaseFact {
            operation: [op; 32],
            author: [author; 32],
            lease: LeaseId([lease; 32]),
            at_ms,
            body,
        }
    }

    #[test]
    fn heartbeats_advance_last_seen_and_carry_progress() {
        let progress = LeaseProgress {
            done: 3,
            total: 10,
            checkpoint_held: true,
        };
        let record = gathered(
            vec![grant(1, 9, 0, 100, 1_000)],
            vec![
                fact(
                    2,
                    9,
                    1,
                    200,
                    LeaseFactBody::Heartbeat(LeaseProgress::default()),
                ),
                fact(3, 9, 1, 400, LeaseFactBody::Heartbeat(progress)),
            ],
        )
        .resolve(LeaseRecord::default(), &terms(), |_, _| Some([9; 32]));
        let epoch = record.current().unwrap();
        assert_eq!(epoch.last_seen_ms, 400);
        assert_eq!(epoch.progress, progress);
    }

    #[test]
    fn facts_from_the_wrong_author_or_outside_the_window_are_ignored() {
        let record = gathered(
            vec![grant(1, 9, 0, 100, 1_000)],
            vec![
                // Another device's heartbeat for this lease.
                fact(
                    2,
                    8,
                    1,
                    500,
                    LeaseFactBody::Heartbeat(LeaseProgress {
                        done: 99,
                        total: 99,
                        checkpoint_held: false,
                    }),
                ),
                // The holder's own heartbeat, after the window closed.
                fact(
                    3,
                    9,
                    1,
                    5_000,
                    LeaseFactBody::Heartbeat(LeaseProgress {
                        done: 5,
                        total: 10,
                        checkpoint_held: false,
                    }),
                ),
            ],
        )
        .resolve(LeaseRecord::default(), &terms(), |_, _| Some([9; 32]));
        let epoch = record.current().unwrap();
        assert_eq!(epoch.last_seen_ms, 100, "neither fact counted");
        assert_eq!(epoch.progress, LeaseProgress::default());
    }

    #[test]
    fn the_earliest_end_wins_and_a_completion_closes_the_job() {
        let reclaimed = LeaseEnd::Reclaimed {
            reason: ReclaimReason::Battery,
            at_ms: 300,
        };
        let record = gathered(
            vec![grant(1, 9, 0, 100, 1_000)],
            vec![
                fact(
                    2,
                    9,
                    1,
                    600,
                    LeaseFactBody::End(LeaseEnd::Released {
                        reason: ReleaseReason::Yielded,
                        at_ms: 600,
                    }),
                ),
                fact(3, 9, 1, 300, LeaseFactBody::End(reclaimed)),
            ],
        )
        .resolve(LeaseRecord::default(), &terms(), |_, _| Some([9; 32]));
        assert_eq!(record.current().unwrap().end, Some(reclaimed));
        assert_eq!(record.next_epoch(), Some((1, 300)));

        let done = gathered(
            vec![grant(1, 9, 0, 100, 1_000)],
            vec![fact(
                2,
                9,
                1,
                400,
                LeaseFactBody::End(LeaseEnd::Completed { at_ms: 400 }),
            )],
        )
        .resolve(LeaseRecord::default(), &terms(), |_, _| Some([9; 32]));
        assert_eq!(
            done.next_epoch(),
            None,
            "a completed job takes no new lease"
        );
    }

    #[test]
    fn resolution_does_not_depend_on_gather_order() {
        let grants = vec![
            grant(3, 8, 1, 1_000, 2_000),
            grant(1, 9, 0, 100, 1_000),
            grant(9, 7, 1, 1_500, 2_500),
        ];
        let winner = |from: u64, _until: u64| Some(if from == 0 { [9; 32] } else { [8; 32] });
        let forward =
            gathered(grants.clone(), vec![]).resolve(LeaseRecord::default(), &terms(), winner);
        let mut reversed = grants;
        reversed.reverse();
        let backward = gathered(reversed, vec![]).resolve(LeaseRecord::default(), &terms(), winner);
        assert_eq!(forward, backward);
        assert_eq!(forward.epochs().len(), 2);
    }
}
