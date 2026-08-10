// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The gather half of [`JobBoard::fold`](crate::board::JobBoard::fold).
//!
//! Every map here is keyed by operation hash, which is what makes the board
//! order-independent: "first eligible" reads the same on every peer regardless
//! of what arrived when. Resolution stays clock-free — it compares signed
//! timestamps with each other and never with a reading of now.

use std::collections::BTreeMap;

use crate::board::JobState;
use crate::lease::{
    GatheredLeases, GrantFact, LeaseEnd, LeaseFact, LeaseFactBody, LeaseId, LeaseRecord,
};
use crate::spec::{JobOutput, JobSpec};
use crate::wire::JobKind;

/// A gathered posting, before claims and results are resolved against it.
pub(crate) struct Posted {
    pub kind: Option<JobKind>,
    pub payload: Option<Vec<u8>>,
    pub spec: Option<Box<JobSpec>>,
    pub by: [u8; 32],
}

/// A gathered result, in whichever generation its author wrote.
pub(crate) enum ResultRecord {
    Inline(Vec<u8>),
    Committed(Box<JobOutput>),
}

/// A gathered claim: who proposed themselves, and when they signed it.
#[derive(Clone, Copy)]
pub(crate) struct ClaimFact {
    pub author: [u8; 32],
    pub at_ms: u64,
}

/// A gathered completion authored under a lease.
pub(crate) struct CompletionFact {
    pub author: [u8; 32],
    pub lease: LeaseId,
    pub at_ms: u64,
    pub output: Box<JobOutput>,
}

impl Posted {
    /// Resolve one job's terminal state on the unleased path. A result only
    /// counts when it answers the generation the job was posted in, and a V2
    /// result must additionally honour the signed grant — so a winner cannot
    /// rename the output slot, overflow its ceiling, or substitute another
    /// resource. A *leased* job resolves through its lease chain instead.
    pub fn resolve(&self, winner: [u8; 32], record: Option<&ResultRecord>) -> JobState {
        match (record, &self.spec) {
            (Some(ResultRecord::Inline(result)), None) => JobState::Done {
                winner,
                result: result.clone(),
            },
            (Some(ResultRecord::Committed(output)), Some(spec))
                if spec.lease.is_none() && output.validate_against(spec).is_ok() =>
            {
                JobState::Committed {
                    winner,
                    output: output.clone(),
                }
            }
            _ => JobState::Claimed { winner },
        }
    }
}

/// Everything gathered about one leased job, assembled once the spec is in hand
/// so completions can be checked against the grant they answer.
#[derive(Default)]
pub(crate) struct GatheredJobLeases {
    pub grants: BTreeMap<[u8; 32], GrantFact>,
    pub facts: BTreeMap<[u8; 32], LeaseFact>,
    pub completions: BTreeMap<[u8; 32], CompletionFact>,
}

impl GatheredJobLeases {
    /// Resolve the epoch chain for `spec`, admitting only completions whose
    /// output honours the signed grant. A completion that breaks the grant is
    /// not a completion, so it neither closes the lease nor closes the job.
    ///
    /// `base` is whatever a retention checkpoint already settled. Note that a
    /// checkpoint which prunes the claim operations a later epoch depends on
    /// will stop that epoch validating — checkpointing a job with a live lease
    /// is not yet safe, and the lease plan carries that forward.
    pub fn resolve(
        &self,
        base: Option<LeaseRecord>,
        spec: &JobSpec,
        claims: &BTreeMap<[u8; 32], ClaimFact>,
    ) -> LeaseRecord {
        let Some(terms) = spec.lease.as_ref() else {
            return LeaseRecord::default();
        };
        let mut gathered = GatheredLeases {
            grants: self.grants.clone(),
            facts: self.facts.clone(),
        };
        for (operation, completion) in &self.completions {
            if completion.output.validate_against(spec).is_err() {
                continue;
            }
            gathered.facts.insert(
                *operation,
                LeaseFact {
                    operation: *operation,
                    author: completion.author,
                    lease: completion.lease,
                    at_ms: completion.at_ms,
                    body: LeaseFactBody::End(LeaseEnd::Completed {
                        at_ms: completion.at_ms,
                    }),
                },
            );
        }
        gathered.resolve(base.unwrap_or_default(), terms, |from_ms, until_ms| {
            claims
                .values()
                .find(|claim| claim.at_ms >= from_ms && claim.at_ms <= until_ms)
                .map(|claim| claim.author)
        })
    }

    /// The output of the completion that closed `record`, if one did.
    pub fn completed_output<'a>(
        &'a self,
        record: &'a LeaseRecord,
    ) -> Option<([u8; 32], &'a JobOutput)> {
        let epoch = record
            .epochs()
            .iter()
            .find(|epoch| matches!(epoch.end, Some(LeaseEnd::Completed { .. })))?;
        let completion = self
            .completions
            .values()
            .find(|c| c.lease == epoch.lease && c.author == epoch.holder)?;
        Some((epoch.holder, &completion.output))
    }
}
