// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The time-indexed lease projection — the one place "now" is allowed in.
//!
//! [`crate::board::JobBoard::fold`] retains lease facts without a clock, so two
//! peers holding the same operations hold the same record. Liveness is a
//! different question, and it is asked here with an explicit observation time:
//! `job.lease_at(now_ms, &policy)`. Same facts, different time, different
//! answer — and that is the only thing allowed to vary.
//!
//! Two lapses, deliberately distinguished. [`LapseReason::Expired`] is a fact:
//! the signed window is over. [`LapseReason::HeartbeatSilence`] is an
//! *observation*: this device has not seen a heartbeat lately, which may mean
//! the holder stalled — or may only mean this device is behind on sync. The
//! projection reports it honestly and refuses to guess; the host decides
//! whether its own sync status makes the silence worth acting on.

use crate::lease::{
    LapseWindow, LeaseEnd, LeaseId, LeaseProgress, LeaseRecord, LeaseTerms, ReclaimReason,
    ReleaseReason,
};

/// How this observer treats lease time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeasePolicy {
    /// Clock disagreement tolerated between devices in the ring, in either
    /// direction. Own devices are usually NTP-synced; this is slack, not a
    /// trust model.
    pub max_skew_ms: u64,
}

impl Default for LeasePolicy {
    fn default() -> Self {
        Self {
            max_skew_ms: 60_000,
        }
    }
}

/// Why a lease is not live at the observation time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LapseReason {
    /// Past the window the holder signed. A fact, not a judgement.
    Expired,
    /// Inside the window, but nothing has been heard for the allowed number of
    /// heartbeat intervals. May be a sync gap at *this* device rather than a
    /// stalled holder — never treat it as worker unreliability.
    HeartbeatSilence,
}

/// Where a job's lease stands at one observation time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeasePhase {
    /// The job carries no lease terms; M2 claim/commit semantics apply.
    Unleased,
    /// No lease is live; a claim may target `epoch`.
    Open { epoch: u32 },
    /// A grant is live right now.
    Held {
        epoch: u32,
        lease: LeaseId,
        holder: [u8; 32],
        expires_at_ms: u64,
        last_seen_ms: u64,
        progress: LeaseProgress,
    },
    /// The holder gave it back.
    Released {
        epoch: u32,
        lease: LeaseId,
        holder: [u8; 32],
        reason: ReleaseReason,
        at_ms: u64,
    },
    /// The holder's own device owner took the hardware back.
    Reclaimed {
        epoch: u32,
        lease: LeaseId,
        holder: [u8; 32],
        reason: ReclaimReason,
        at_ms: u64,
    },
    /// Granted, but not live at this time.
    Lapsed {
        epoch: u32,
        lease: LeaseId,
        holder: [u8; 32],
        reason: LapseReason,
    },
    /// The job completed under this lease.
    Done {
        epoch: u32,
        lease: LeaseId,
        holder: [u8; 32],
    },
}

impl LeasePhase {
    /// The device currently bound to the job, if any.
    pub fn holder(&self) -> Option<[u8; 32]> {
        match self {
            Self::Unleased | Self::Open { .. } => None,
            Self::Held { holder, .. }
            | Self::Released { holder, .. }
            | Self::Reclaimed { holder, .. }
            | Self::Lapsed { holder, .. }
            | Self::Done { holder, .. } => Some(*holder),
        }
    }

    pub fn lease(&self) -> Option<LeaseId> {
        match self {
            Self::Unleased | Self::Open { .. } => None,
            Self::Held { lease, .. }
            | Self::Released { lease, .. }
            | Self::Reclaimed { lease, .. }
            | Self::Lapsed { lease, .. }
            | Self::Done { lease, .. } => Some(*lease),
        }
    }

    /// Whether `me` holds a live lease here.
    pub fn held_by(&self, me: &[u8; 32]) -> bool {
        matches!(self, Self::Held { holder, .. } if holder == me)
    }

    /// The epoch a fresh claim would target, or `None` when the job is not open
    /// to one (live lease, completed, or unleased).
    pub fn open_epoch(&self) -> Option<u32> {
        match self {
            Self::Open { epoch } => Some(*epoch),
            Self::Released { epoch, .. }
            | Self::Reclaimed { epoch, .. }
            | Self::Lapsed { epoch, .. } => Some(epoch + 1),
            Self::Unleased | Self::Held { .. } | Self::Done { .. } => None,
        }
    }
}

/// Classify `record` at `at_ms`. `terms` is the job's signed envelope; a job
/// without one is [`LeasePhase::Unleased`].
pub fn phase_at(
    record: &LeaseRecord,
    terms: Option<&LeaseTerms>,
    at_ms: u64,
    policy: &LeasePolicy,
) -> LeasePhase {
    let Some(terms) = terms else {
        return LeasePhase::Unleased;
    };
    let skew = policy.max_skew_ms;
    // A grant dated beyond the skew window has not happened yet from here, so
    // the job stands wherever the previous epoch left it.
    let Some(epoch) = record
        .epochs()
        .iter()
        .rev()
        .find(|epoch| epoch.granted_at_ms <= at_ms.saturating_add(skew))
    else {
        return LeasePhase::Open { epoch: 0 };
    };

    // An end fact that has not come round yet is likewise not in force.
    if let Some(end) = epoch
        .end
        .filter(|end| end.at_ms() <= at_ms.saturating_add(skew))
    {
        return match end {
            LeaseEnd::Released { reason, at_ms } => LeasePhase::Released {
                epoch: epoch.epoch,
                lease: epoch.lease,
                holder: epoch.holder,
                reason,
                at_ms,
            },
            LeaseEnd::Reclaimed { reason, at_ms } => LeasePhase::Reclaimed {
                epoch: epoch.epoch,
                lease: epoch.lease,
                holder: epoch.holder,
                reason,
                at_ms,
            },
            LeaseEnd::Completed { .. } => LeasePhase::Done {
                epoch: epoch.epoch,
                lease: epoch.lease,
                holder: epoch.holder,
            },
        };
    }

    let lapse = |reason| LeasePhase::Lapsed {
        epoch: epoch.epoch,
        lease: epoch.lease,
        holder: epoch.holder,
        reason,
    };
    match epoch.lapse_at(at_ms, terms, skew) {
        Some(LapseWindow::Expired) => lapse(LapseReason::Expired),
        Some(LapseWindow::Silent) => lapse(LapseReason::HeartbeatSilence),
        None => LeasePhase::Held {
            epoch: epoch.epoch,
            lease: epoch.lease,
            holder: epoch.holder,
            expires_at_ms: epoch.expires_at_ms,
            last_seen_ms: epoch.last_seen_ms,
            progress: epoch.progress,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lease::{LeaseEpoch, LeaseTerms};

    const HOLDER: [u8; 32] = [9; 32];
    const LEASE: LeaseId = LeaseId([1; 32]);

    fn terms() -> LeaseTerms {
        // 60s window, heartbeat every 10s, 3 misses tolerated → 30s of silence.
        LeaseTerms::new(60_000, 10_000)
    }

    fn policy() -> LeasePolicy {
        LeasePolicy { max_skew_ms: 0 }
    }

    fn granted(end: Option<LeaseEnd>, last_seen_ms: u64) -> LeaseRecord {
        let mut record = LeaseRecord::default();
        record.push_epoch(LeaseEpoch {
            epoch: 0,
            lease: LEASE,
            holder: HOLDER,
            granted_at_ms: 1_000,
            expires_at_ms: 61_000,
            last_seen_ms,
            progress: LeaseProgress::default(),
            end,
        });
        record
    }

    #[test]
    fn a_job_without_terms_is_never_leased() {
        assert_eq!(
            phase_at(&LeaseRecord::default(), None, 5_000, &policy()),
            LeasePhase::Unleased
        );
        assert_eq!(LeasePhase::Unleased.open_epoch(), None);
    }

    #[test]
    fn an_ungranted_job_is_open_at_epoch_zero() {
        let phase = phase_at(&LeaseRecord::default(), Some(&terms()), 5_000, &policy());
        assert_eq!(phase, LeasePhase::Open { epoch: 0 });
        assert_eq!(phase.open_epoch(), Some(0));
    }

    #[test]
    fn the_same_facts_change_phase_only_with_the_observation_time() {
        let record = granted(None, 1_000);
        let terms = terms();
        // Inside the window and inside the silence allowance.
        assert!(phase_at(&record, Some(&terms), 20_000, &policy()).held_by(&HOLDER));
        // Still inside the window, but 30s of silence have passed.
        assert_eq!(
            phase_at(&record, Some(&terms), 31_000, &policy()),
            LeasePhase::Lapsed {
                epoch: 0,
                lease: LEASE,
                holder: HOLDER,
                reason: LapseReason::HeartbeatSilence
            }
        );
        // Past the signed window entirely.
        assert_eq!(
            phase_at(&record, Some(&terms), 61_000, &policy()),
            LeasePhase::Lapsed {
                epoch: 0,
                lease: LEASE,
                holder: HOLDER,
                reason: LapseReason::Expired
            }
        );
    }

    #[test]
    fn silence_and_expiry_boundaries_are_exact() {
        let record = granted(None, 1_000);
        let terms = terms();
        // last_seen 1_000 + 3 × 10_000 = 31_000 is the first silent instant.
        assert!(phase_at(&record, Some(&terms), 30_999, &policy()).held_by(&HOLDER));
        assert!(matches!(
            phase_at(&record, Some(&terms), 31_000, &policy()),
            LeasePhase::Lapsed {
                reason: LapseReason::HeartbeatSilence,
                ..
            }
        ));
        // A fresh heartbeat pushes both boundaries out.
        let beating = granted(None, 50_000);
        assert!(phase_at(&beating, Some(&terms), 60_999, &policy()).held_by(&HOLDER));
        assert!(matches!(
            phase_at(&beating, Some(&terms), 61_000, &policy()),
            LeasePhase::Lapsed {
                reason: LapseReason::Expired,
                ..
            }
        ));
    }

    #[test]
    fn skew_widens_both_boundaries_in_the_holders_favour() {
        let record = granted(None, 1_000);
        let terms = terms();
        let lenient = LeasePolicy { max_skew_ms: 5_000 };
        assert!(matches!(
            phase_at(&record, Some(&terms), 33_000, &policy()),
            LeasePhase::Lapsed { .. }
        ));
        assert!(
            phase_at(&record, Some(&terms), 33_000, &lenient).held_by(&HOLDER),
            "within skew the holder keeps the benefit of the doubt"
        );
    }

    #[test]
    fn a_future_dated_grant_is_not_live_yet() {
        let record = granted(None, 1_000);
        // The grant is at 1_000; observing at 0 with no skew sees nothing yet.
        assert_eq!(
            phase_at(&record, Some(&terms()), 0, &policy()),
            LeasePhase::Open { epoch: 0 }
        );
    }

    #[test]
    fn ends_project_distinctly_and_reopen_the_next_epoch() {
        let terms = terms();
        let reclaimed = granted(
            Some(LeaseEnd::Reclaimed {
                reason: ReclaimReason::ForegroundActivity,
                at_ms: 20_000,
            }),
            10_000,
        );
        let phase = phase_at(&reclaimed, Some(&terms), 25_000, &policy());
        assert_eq!(
            phase,
            LeasePhase::Reclaimed {
                epoch: 0,
                lease: LEASE,
                holder: HOLDER,
                reason: ReclaimReason::ForegroundActivity,
                at_ms: 20_000
            }
        );
        assert_eq!(phase.open_epoch(), Some(1));

        let released = granted(
            Some(LeaseEnd::Released {
                reason: ReleaseReason::InputUnavailable,
                at_ms: 20_000,
            }),
            10_000,
        );
        assert!(matches!(
            phase_at(&released, Some(&terms), 25_000, &policy()),
            LeasePhase::Released {
                reason: ReleaseReason::InputUnavailable,
                ..
            }
        ));

        let done = granted(Some(LeaseEnd::Completed { at_ms: 20_000 }), 10_000);
        let phase = phase_at(&done, Some(&terms), 25_000, &policy());
        assert_eq!(
            phase,
            LeasePhase::Done {
                epoch: 0,
                lease: LEASE,
                holder: HOLDER
            }
        );
        assert_eq!(phase.open_epoch(), None, "a finished job is not re-let");
    }

    #[test]
    fn an_end_dated_ahead_of_the_observer_is_not_in_force_yet() {
        let reclaimed = granted(
            Some(LeaseEnd::Reclaimed {
                reason: ReclaimReason::Battery,
                at_ms: 40_000,
            }),
            35_000,
        );
        assert!(
            phase_at(&reclaimed, Some(&terms()), 36_000, &policy()).held_by(&HOLDER),
            "the lease still stands until its end fact comes round"
        );
    }
}
