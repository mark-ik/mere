// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The job board — a deterministic, order-independent fold of mesh operations.
//!
//! Every peer folds the same op set into the same board, regardless of arrival
//! order (the `Ledger` pattern): the fold *gathers* posts, claims, and results
//! first, then *resolves*. The two resolution rules that make convergence hold:
//!
//! - **Claim races resolve by lowest claim-operation hash.** Two devices may
//!   both claim a job before either sees the other's claim; every peer picks
//!   the same winner because the winner is a pure function of the claim set,
//!   not of arrival order. (Lapse / reassign on a dead winner is milestone 3.)
//! - **Only the winner's result is accepted.** A `JobDone` from any other
//!   author is ignored, so a losing racer's result cannot fork the board.
//!
//! A job's identity is its posting operation's hash (content-addressed, like
//! everything on the DAG).

use std::collections::BTreeMap;

use p2panda_core::Operation;
use serde::{Deserialize, Serialize};

use crate::directory::DeviceDirectory;
use crate::fold::{ClaimFact, CompletionFact, GatheredJobLeases, Posted, ResultRecord};
use crate::lease::{GrantFact, LeaseEnd, LeaseFact, LeaseFactBody, LeaseId, LeaseRecord};
use crate::projection::{LeasePhase, LeasePolicy, phase_at};
use crate::retention::JobBoardSnapshot;
use crate::spec::{JobOutput, JobSpec};
use crate::wire::{JobKind, MeshEvent, MeshExt, from_operation, verify};

/// A job's identity: the hash of its posting operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct JobId(pub [u8; 32]);

/// Where a job is in its lifecycle. The two terminal states are the two wire
/// generations: `Done` carries M1's inline bytes, `Committed` carries V2's
/// content-addressed output record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobState {
    /// Posted, no claims yet.
    Posted,
    /// Claimed; `winner` is the deterministic claim-race winner's author key.
    Claimed { winner: [u8; 32] },
    /// The winner returned an inline result (M1).
    Done { winner: [u8; 32], result: Vec<u8> },
    /// The winner committed an output blob honouring the signed grant (V2).
    Committed {
        winner: [u8; 32],
        output: Box<JobOutput>,
    },
}

impl JobState {
    /// The claim-race winner, once one exists.
    pub fn winner(&self) -> Option<[u8; 32]> {
        match self {
            Self::Posted => None,
            Self::Claimed { winner }
            | Self::Done { winner, .. }
            | Self::Committed { winner, .. } => Some(*winner),
        }
    }

    /// Whether the job has a result, in either generation.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Done { .. } | Self::Committed { .. })
    }
}

/// One job on the board.
///
/// The generation fields are mutually exclusive: an M1 job carries `kind` +
/// `payload`, a V2 job carries `spec`. `spec` is skipped when absent and `kind`
/// encodes identically to the pre-V2 field, so a snapshot of M1 jobs still
/// hashes to the bytes its checkpoint committed to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    /// The M1 kind; `None` for a V2 job.
    pub kind: Option<JobKind>,
    /// The M1 inline input. `None` for a V2 job, and `None` after an accepted
    /// checkpoint erases a terminal M1 job's input.
    pub payload: Option<Vec<u8>>,
    /// The V2 manifest; `None` for an M1 job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<Box<JobSpec>>,
    /// The poster's author (verifying) key bytes.
    pub posted_by: [u8; 32],
    pub state: JobState,
    /// Admissible lease facts, clock-free. Empty unless the spec signed lease
    /// terms, and skipped when empty so a pre-M3 snapshot keeps its bytes.
    #[serde(default, skip_serializing_if = "LeaseRecord::is_empty")]
    pub lease: LeaseRecord,
    /// Devices whose claims are eligible for the *next* lease epoch, in
    /// claim-operation-hash order — so the first still-eligible entry is the
    /// winner every peer computes. Populated only for leased jobs, which is what
    /// keeps an unleased job's snapshot bytes where they were.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_claimants: Vec<Claimant>,
}

impl Job {
    /// Whether this device could still run the job: its input is retained (M1)
    /// or its manifest is present (V2).
    pub fn is_runnable(&self) -> bool {
        self.spec.is_some() || self.payload.is_some()
    }

    /// The lease envelope this job was posted under, if any.
    pub fn lease_terms(&self) -> Option<&crate::lease::LeaseTerms> {
        self.spec.as_ref().and_then(|spec| spec.lease.as_ref())
    }

    /// Where the lease stands at one observation time. The *only* job question
    /// that needs a clock, and it takes the reading as an argument.
    pub fn lease_at(&self, at_ms: u64, policy: &LeasePolicy) -> LeasePhase {
        phase_at(&self.lease, self.lease_terms(), at_ms, policy)
    }

    /// Who would win the next epoch for a grant authored at `at_ms` — the same
    /// answer the fold will reach when that grant arrives.
    pub fn next_holder(&self, at_ms: u64) -> Option<[u8; 32]> {
        self.next_claimants
            .iter()
            .find(|claimant| claimant.at_ms <= at_ms)
            .map(|claimant| claimant.author)
    }

    /// Whether `me` has already proposed itself for the next epoch.
    pub fn has_claimed(&self, me: &[u8; 32]) -> bool {
        self.next_claimants
            .iter()
            .any(|claimant| &claimant.author == me)
    }
}

/// The folded board: every known job, keyed (and so ordered) by id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobBoard {
    jobs: BTreeMap<JobId, Job>,
    devices: DeviceDirectory,
}

/// A device eligible for the next lease epoch, with the instant it proposed
/// itself. The instant matters: a grant is judged against the field that
/// existed when it was authored, so a worker must not try to grant on the
/// strength of a claim signed later than its own clock reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claimant {
    pub author: [u8; 32],
    pub at_ms: u64,
}

impl JobBoard {
    /// Fold a set of operations into a board. Order-independent: gathers
    /// everything, then resolves claims and results. Ops that fail signature
    /// verification, decode, or address a different mesh are skipped (the sync
    /// drain verifies too; this is defence in depth for direct callers).
    pub fn fold<'a, I>(mesh_id: [u8; 32], ops: I) -> Self
    where
        I: IntoIterator<Item = &'a Operation<MeshExt>>,
    {
        Self::fold_from_snapshot(mesh_id, &JobBoardSnapshot::default(), ops)
    }

    /// Replay a retained checkpoint plus the operations after its frontier.
    pub fn fold_from_snapshot<'a, I>(mesh_id: [u8; 32], snapshot: &JobBoardSnapshot, ops: I) -> Self
    where
        I: IntoIterator<Item = &'a Operation<MeshExt>>,
    {
        // Gather phase.
        let mut posted: BTreeMap<JobId, Posted> = BTreeMap::new();
        // job → claim-op-hash → claimant. BTreeMap keys give the deterministic
        // winner (lowest claim-op hash) for free; the signed `at_ms` is what
        // makes a claim eligible for a later lease epoch.
        let mut claims: BTreeMap<JobId, BTreeMap<[u8; 32], ClaimFact>> = BTreeMap::new();
        // job → author → result, in whichever generation the author wrote.
        let mut results: BTreeMap<JobId, BTreeMap<[u8; 32], ResultRecord>> = BTreeMap::new();
        // job → lease facts, assembled per job once its spec is known.
        let mut leases: BTreeMap<JobId, GatheredJobLeases> = BTreeMap::new();
        // Lease records carried in from a checkpoint, topped up by the tail.
        let mut retained: BTreeMap<JobId, LeaseRecord> = BTreeMap::new();
        // Which device is behind each author key.
        let mut devices = DeviceDirectory::default();

        // Seed the gather maps from the accepted checkpoint. The all-zero
        // synthetic claim key sorts before any real operation hash, preserving
        // a winner already resolved at the checkpoint frontier.
        for job in &snapshot.jobs {
            posted.insert(
                job.id,
                Posted {
                    kind: job.kind,
                    payload: job.payload.clone(),
                    spec: job.spec.clone(),
                    by: job.posted_by,
                },
            );
            if !job.lease.is_empty() {
                retained.insert(job.id, job.lease.clone());
            }
            if let Some(winner) = job.state.winner() {
                claims.entry(job.id).or_default().insert(
                    [0; 32],
                    ClaimFact {
                        author: winner,
                        at_ms: 0,
                    },
                );
            }
            match &job.state {
                JobState::Done { winner, result } => {
                    results
                        .entry(job.id)
                        .or_default()
                        .insert(*winner, ResultRecord::Inline(result.clone()));
                }
                JobState::Committed { winner, output } => {
                    results
                        .entry(job.id)
                        .or_default()
                        .insert(*winner, ResultRecord::Committed(output.clone()));
                }
                JobState::Posted | JobState::Claimed { .. } => {}
            }
        }

        for op in ops {
            if !verify(op) {
                continue;
            }
            let Ok((id, event)) = from_operation(op) else {
                continue;
            };
            if id != mesh_id {
                continue;
            }
            let author = *op.header.verifying_key.as_bytes();
            match event {
                MeshEvent::JobPosted { kind, payload, .. } => {
                    posted.insert(
                        JobId(*op.hash.as_bytes()),
                        Posted {
                            kind: Some(kind),
                            payload: Some(payload),
                            spec: None,
                            by: author,
                        },
                    );
                }
                MeshEvent::JobPostedV2 { spec, .. } => {
                    // Defence in depth: the store refuses a malformed spec
                    // before it is ever persisted, so this only fires for a
                    // direct caller folding unvetted operations.
                    if spec.validate().is_err() {
                        continue;
                    }
                    posted.insert(
                        JobId(*op.hash.as_bytes()),
                        Posted {
                            kind: None,
                            payload: None,
                            spec: Some(spec),
                            by: author,
                        },
                    );
                }
                MeshEvent::JobClaimed { job, at_ms } => {
                    claims
                        .entry(JobId(job))
                        .or_default()
                        .insert(*op.hash.as_bytes(), ClaimFact { author, at_ms });
                }
                MeshEvent::JobDone { job, result, .. } => {
                    results
                        .entry(JobId(job))
                        .or_default()
                        .insert(author, ResultRecord::Inline(result));
                }
                MeshEvent::JobDoneV2 { job, output, .. } => {
                    results
                        .entry(JobId(job))
                        .or_default()
                        .insert(author, ResultRecord::Committed(output));
                }
                MeshEvent::LeaseGranted {
                    job,
                    epoch,
                    granted_at_ms,
                    expires_at_ms,
                } => {
                    let operation = *op.hash.as_bytes();
                    leases.entry(JobId(job)).or_default().grants.insert(
                        operation,
                        GrantFact {
                            operation,
                            author,
                            epoch,
                            granted_at_ms,
                            expires_at_ms,
                        },
                    );
                }
                MeshEvent::LeaseHeartbeat {
                    job,
                    lease,
                    progress,
                    at_ms,
                } => {
                    let operation = *op.hash.as_bytes();
                    leases.entry(JobId(job)).or_default().facts.insert(
                        operation,
                        LeaseFact {
                            operation,
                            author,
                            lease: LeaseId(lease),
                            at_ms,
                            body: LeaseFactBody::Heartbeat(progress),
                        },
                    );
                }
                MeshEvent::LeaseReleased {
                    job,
                    lease,
                    reason,
                    at_ms,
                } => {
                    let operation = *op.hash.as_bytes();
                    leases.entry(JobId(job)).or_default().facts.insert(
                        operation,
                        LeaseFact {
                            operation,
                            author,
                            lease: LeaseId(lease),
                            at_ms,
                            body: LeaseFactBody::End(LeaseEnd::Released { reason, at_ms }),
                        },
                    );
                }
                MeshEvent::LeaseRevokedByOwner {
                    job,
                    lease,
                    reason,
                    at_ms,
                } => {
                    let operation = *op.hash.as_bytes();
                    leases.entry(JobId(job)).or_default().facts.insert(
                        operation,
                        LeaseFact {
                            operation,
                            author,
                            lease: LeaseId(lease),
                            at_ms,
                            body: LeaseFactBody::End(LeaseEnd::Reclaimed { reason, at_ms }),
                        },
                    );
                }
                MeshEvent::JobCompletedUnderLease {
                    job,
                    lease,
                    output,
                    at_ms,
                } => {
                    leases.entry(JobId(job)).or_default().completions.insert(
                        *op.hash.as_bytes(),
                        CompletionFact {
                            author,
                            lease: LeaseId(lease),
                            at_ms,
                            output,
                        },
                    );
                }
                MeshEvent::DeviceAttested { attestation } => {
                    // Defence in depth behind the store's admission check.
                    devices.admit(author, &attestation);
                }
                MeshEvent::RetentionCheckpoint { .. } | MeshEvent::HistoryPruned { .. } => {}
            }
        }

        // Resolve phase.
        let mut jobs = BTreeMap::new();
        for (id, post) in posted {
            let winner = claims
                .get(&id)
                .and_then(|c| c.first_key_value())
                .map(|(_, claim)| claim.author);
            let empty_claims = BTreeMap::new();
            let job_claims = claims.get(&id).unwrap_or(&empty_claims);
            let gathered = leases.remove(&id).unwrap_or_default();

            // Leases resolve first: on a leased job the terminal state is
            // whichever epoch completed, not whoever won the first claim.
            let leased = post
                .spec
                .as_deref()
                .is_some_and(|spec| spec.lease.is_some());
            let lease = match post.spec.as_deref() {
                Some(spec) if leased => gathered.resolve(retained.remove(&id), spec, job_claims),
                _ => LeaseRecord::default(),
            };
            // Who could take the next epoch, in the order every peer agrees on.
            let next_claimants = match (leased, lease.next_epoch()) {
                (true, Some((_, from_ms))) => job_claims
                    .values()
                    .filter(|claim| claim.at_ms >= from_ms)
                    .map(|claim| Claimant {
                        author: claim.author,
                        at_ms: claim.at_ms,
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let state = match gathered.completed_output(&lease) {
                Some((holder, output)) => JobState::Committed {
                    winner: holder,
                    output: Box::new(output.clone()),
                },
                None => match (lease.current(), winner) {
                    (Some(epoch), _) => JobState::Claimed {
                        winner: epoch.holder,
                    },
                    (None, None) => JobState::Posted,
                    (None, Some(winner)) => {
                        post.resolve(winner, results.get(&id).and_then(|r| r.get(&winner)))
                    }
                },
            };
            jobs.insert(
                id,
                Job {
                    id,
                    kind: post.kind,
                    payload: post.payload,
                    spec: post.spec,
                    posted_by: post.by,
                    state,
                    lease,
                    next_claimants,
                },
            );
        }
        Self { jobs, devices }
    }

    /// Every job's lease phase at one observation time — the `board.at(t)` the
    /// plan asks for. The fold above never reads a clock; this is where one
    /// enters, as an argument.
    pub fn at<'a>(
        &'a self,
        at_ms: u64,
        policy: &'a LeasePolicy,
    ) -> impl Iterator<Item = (JobId, LeasePhase)> + 'a {
        self.jobs
            .values()
            .map(move |job| (job.id, job.lease_at(at_ms, policy)))
    }

    /// One job's lease phase at an observation time.
    pub fn phase_at(&self, id: JobId, at_ms: u64, policy: &LeasePolicy) -> Option<LeasePhase> {
        self.job(id).map(|job| job.lease_at(at_ms, policy))
    }

    /// All jobs, in id order.
    pub fn jobs(&self) -> impl Iterator<Item = &Job> {
        self.jobs.values()
    }

    /// One job by id.
    pub fn job(&self, id: JobId) -> Option<&Job> {
        self.jobs.get(&id)
    }

    /// Which devices have said who they are. A host turns a master key into a
    /// transport address; the mesh stops at the key so it stays transport-free.
    pub fn devices(&self) -> &DeviceDirectory {
        &self.devices
    }

    /// How many jobs the board knows.
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Whether the board is empty.
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::to_operation;
    use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};
    use serde::Serialize;

    const MESH: [u8; 32] = [0x4d; 32];

    fn keypair(seed: u8) -> Ed25519Keypair {
        InMemoryProvider::from_seed([seed; 32])
            .derive_keypair(b"mesh-board")
            .unwrap()
    }

    fn author(kp: &Ed25519Keypair) -> [u8; 32] {
        kp.public_key().to_bytes()
    }

    fn post(kp: &Ed25519Keypair, seq: u32, back: Option<[u8; 32]>) -> Operation<MeshExt> {
        to_operation(
            kp,
            MESH,
            &MeshEvent::JobPosted {
                kind: JobKind::Echo,
                payload: b"job".to_vec(),
                nonce: u64::from(seq),
                at_ms: 1,
            },
            seq,
            back,
        )
    }

    #[test]
    fn lifecycle_posted_claimed_done() {
        let asker = keypair(1);
        let worker = keypair(2);
        let posted = post(&asker, 0, None);
        let id = JobId(*posted.hash.as_bytes());

        let board = JobBoard::fold(MESH, [&posted]);
        assert_eq!(board.job(id).unwrap().state, JobState::Posted);

        let claim = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 2,
            },
            0,
            None,
        );
        let board = JobBoard::fold(MESH, [&posted, &claim]);
        assert_eq!(
            board.job(id).unwrap().state,
            JobState::Claimed {
                winner: author(&worker)
            }
        );

        let done = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobDone {
                job: id.0,
                result: b"job".to_vec(),
                at_ms: 3,
            },
            1,
            Some(*claim.hash.as_bytes()),
        );
        let board = JobBoard::fold(MESH, [&posted, &claim, &done]);
        assert_eq!(
            board.job(id).unwrap().state,
            JobState::Done {
                winner: author(&worker),
                result: b"job".to_vec()
            }
        );
    }

    #[test]
    fn claim_race_resolves_identically_in_both_fold_orders() {
        let asker = keypair(1);
        let w1 = keypair(2);
        let w2 = keypair(3);
        let posted = post(&asker, 0, None);
        let id = JobId(*posted.hash.as_bytes());
        let claim1 = to_operation(
            &w1,
            MESH,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 2,
            },
            0,
            None,
        );
        let claim2 = to_operation(
            &w2,
            MESH,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 2,
            },
            0,
            None,
        );

        let a = JobBoard::fold(MESH, [&posted, &claim1, &claim2]);
        let b = JobBoard::fold(MESH, [&claim2, &posted, &claim1]);
        assert_eq!(a, b, "fold is order-independent");

        // The winner is the lowest claim-op hash, computed not assumed.
        let expected = if claim1.hash.as_bytes() < claim2.hash.as_bytes() {
            author(&w1)
        } else {
            author(&w2)
        };
        assert_eq!(
            a.job(id).unwrap().state,
            JobState::Claimed { winner: expected }
        );
    }

    #[test]
    fn a_non_winner_result_is_ignored() {
        let asker = keypair(1);
        let w1 = keypair(2);
        let w2 = keypair(3);
        let posted = post(&asker, 0, None);
        let id = JobId(*posted.hash.as_bytes());
        let claim1 = to_operation(
            &w1,
            MESH,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 2,
            },
            0,
            None,
        );
        let claim2 = to_operation(
            &w2,
            MESH,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 2,
            },
            0,
            None,
        );
        let winner_kp = if claim1.hash.as_bytes() < claim2.hash.as_bytes() {
            &w1
        } else {
            &w2
        };
        let loser_kp = if std::ptr::eq(winner_kp, &w1) {
            &w2
        } else {
            &w1
        };

        // The loser races a result in; the board must not accept it.
        let loser_done = to_operation(
            loser_kp,
            MESH,
            &MeshEvent::JobDone {
                job: id.0,
                result: b"forged".to_vec(),
                at_ms: 3,
            },
            1,
            Some(*if std::ptr::eq(loser_kp, &w1) {
                claim1.hash.as_bytes()
            } else {
                claim2.hash.as_bytes()
            }),
        );
        let board = JobBoard::fold(MESH, [&posted, &claim1, &claim2, &loser_done]);
        assert_eq!(
            board.job(id).unwrap().state,
            JobState::Claimed {
                winner: author(winner_kp)
            },
            "a non-winner JobDone leaves the job claimed, not done"
        );
    }

    #[test]
    fn foreign_mesh_ops_are_skipped() {
        let asker = keypair(1);
        let posted_here = post(&asker, 0, None);
        let posted_elsewhere = to_operation(
            &asker,
            [0xee; 32],
            &MeshEvent::JobPosted {
                kind: JobKind::Echo,
                payload: b"other".to_vec(),
                nonce: 9,
                at_ms: 1,
            },
            1,
            Some(*posted_here.hash.as_bytes()),
        );
        let board = JobBoard::fold(MESH, [&posted_here, &posted_elsewhere]);
        assert_eq!(board.len(), 1, "only this mesh's jobs fold in");
    }

    fn v2_spec() -> JobSpec {
        JobSpec::simple(
            crate::ident::ResourceId::parse("esp.embed.lexical/v1").unwrap(),
            "texts",
            proofs::BlobRef::blake3(b"a batch"),
            "vectors",
            4096,
            crate::spec::DeterminismClass::Exact,
        )
    }

    fn v2_output(bytes: &[u8]) -> JobOutput {
        JobOutput {
            name: "vectors".to_string(),
            blob: proofs::BlobRef::blake3(bytes),
            resource: v2_spec().resource,
            implementation: crate::ident::ImplementationId::parse("mesh.lexical.fnv1a/v1").unwrap(),
            verification: crate::spec::VerificationClass::ExactBytes,
        }
    }

    fn post_v2(kp: &Ed25519Keypair, spec: JobSpec) -> Operation<MeshExt> {
        to_operation(
            kp,
            MESH,
            &MeshEvent::JobPostedV2 {
                spec: Box::new(spec),
                nonce: 0,
                at_ms: 1,
            },
            0,
            None,
        )
    }

    #[test]
    fn a_v2_job_folds_posted_claimed_committed() {
        let asker = keypair(1);
        let worker = keypair(2);
        let posted = post_v2(&asker, v2_spec());
        let id = JobId(*posted.hash.as_bytes());

        let board = JobBoard::fold(MESH, [&posted]);
        let job = board.job(id).unwrap();
        assert_eq!(job.state, JobState::Posted);
        assert_eq!(job.kind, None);
        assert_eq!(job.spec.as_deref(), Some(&v2_spec()));

        let claim = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 2,
            },
            0,
            None,
        );
        let output = v2_output(b"the vectors");
        let done = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobDoneV2 {
                job: id.0,
                output: Box::new(output.clone()),
                at_ms: 3,
            },
            1,
            Some(*claim.hash.as_bytes()),
        );
        let board = JobBoard::fold(MESH, [&posted, &claim, &done]);
        assert_eq!(
            board.job(id).unwrap().state,
            JobState::Committed {
                winner: author(&worker),
                output: Box::new(output)
            }
        );
    }

    #[test]
    fn a_result_that_breaks_the_signed_grant_is_not_a_result() {
        let asker = keypair(1);
        let worker = keypair(2);
        let posted = post_v2(&asker, v2_spec());
        let id = JobId(*posted.hash.as_bytes());
        let claim = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 2,
            },
            0,
            None,
        );

        // Each of these is signed by the claim winner and still refused: the
        // grant, not the signature, is what bounds a result.
        let renamed = {
            let mut o = v2_output(b"x");
            o.name = "somewhere-else".to_string();
            o
        };
        let oversize = {
            let mut o = v2_output(b"x");
            o.blob.byte_len = 4097;
            o
        };
        let swapped = {
            let mut o = v2_output(b"x");
            o.resource = crate::ident::ResourceId::parse("mesh.echo/v1").unwrap();
            o
        };
        for (n, output) in [renamed, oversize, swapped].into_iter().enumerate() {
            let done = to_operation(
                &worker,
                MESH,
                &MeshEvent::JobDoneV2 {
                    job: id.0,
                    output: Box::new(output),
                    at_ms: 3,
                },
                1,
                Some(*claim.hash.as_bytes()),
            );
            let board = JobBoard::fold(MESH, [&posted, &claim, &done]);
            assert_eq!(
                board.job(id).unwrap().state,
                JobState::Claimed {
                    winner: author(&worker)
                },
                "case {n} left the job done"
            );
        }
    }

    #[test]
    fn a_result_from_the_other_generation_is_ignored() {
        let asker = keypair(1);
        let worker = keypair(2);
        // An inline result cannot close a V2 job...
        let v2 = post_v2(&asker, v2_spec());
        let v2_id = JobId(*v2.hash.as_bytes());
        let claim = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobClaimed {
                job: v2_id.0,
                at_ms: 2,
            },
            0,
            None,
        );
        let inline = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobDone {
                job: v2_id.0,
                result: b"inline".to_vec(),
                at_ms: 3,
            },
            1,
            Some(*claim.hash.as_bytes()),
        );
        let board = JobBoard::fold(MESH, [&v2, &claim, &inline]);
        assert_eq!(
            board.job(v2_id).unwrap().state,
            JobState::Claimed {
                winner: author(&worker)
            }
        );

        // ...and a committed output cannot close an M1 job.
        let m1 = post(&asker, 0, None);
        let m1_id = JobId(*m1.hash.as_bytes());
        let m1_claim = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobClaimed {
                job: m1_id.0,
                at_ms: 2,
            },
            0,
            None,
        );
        let committed = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobDoneV2 {
                job: m1_id.0,
                output: Box::new(v2_output(b"x")),
                at_ms: 3,
            },
            1,
            Some(*m1_claim.hash.as_bytes()),
        );
        let board = JobBoard::fold(MESH, [&m1, &m1_claim, &committed]);
        assert_eq!(
            board.job(m1_id).unwrap().state,
            JobState::Claimed {
                winner: author(&worker)
            }
        );
    }

    #[test]
    fn a_malformed_spec_never_reaches_the_board() {
        let asker = keypair(1);
        let mut spec = v2_spec();
        spec.output.max_bytes = 0;
        let posted = post_v2(&asker, spec);
        assert!(
            JobBoard::fold(MESH, [&posted]).is_empty(),
            "the fold is defence in depth behind the store's admission check"
        );
    }

    #[test]
    fn mixed_generation_replicas_converge_in_every_fold_order() {
        let asker = keypair(1);
        let worker = keypair(2);
        let m1_post = post(&asker, 0, None);
        let m1_id = JobId(*m1_post.hash.as_bytes());
        let v2_post = to_operation(
            &asker,
            MESH,
            &MeshEvent::JobPostedV2 {
                spec: Box::new(v2_spec()),
                nonce: 7,
                at_ms: 1,
            },
            1,
            Some(*m1_post.hash.as_bytes()),
        );
        let v2_id = JobId(*v2_post.hash.as_bytes());
        let m1_claim = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobClaimed {
                job: m1_id.0,
                at_ms: 2,
            },
            0,
            None,
        );
        let m1_done = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobDone {
                job: m1_id.0,
                result: b"job".to_vec(),
                at_ms: 3,
            },
            1,
            Some(*m1_claim.hash.as_bytes()),
        );
        let v2_claim = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobClaimed {
                job: v2_id.0,
                at_ms: 4,
            },
            2,
            Some(*m1_done.hash.as_bytes()),
        );
        let v2_done = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobDoneV2 {
                job: v2_id.0,
                output: Box::new(v2_output(b"the vectors")),
                at_ms: 5,
            },
            3,
            Some(*v2_claim.hash.as_bytes()),
        );

        let ops = [&m1_post, &v2_post, &m1_claim, &m1_done, &v2_claim, &v2_done];
        let expected = JobBoard::fold(MESH, ops);
        assert_eq!(expected.len(), 2);
        assert!(expected.jobs().all(|job| job.state.is_terminal()));

        // Every rotation, plus the full reversal: arrival order is irrelevant
        // to a mixed-generation replica just as it is to a single-generation one.
        for shift in 0..ops.len() {
            let mut rotated = ops;
            rotated.rotate_left(shift);
            assert_eq!(JobBoard::fold(MESH, rotated), expected, "rotation {shift}");
        }
        let mut reversed = ops;
        reversed.reverse();
        assert_eq!(JobBoard::fold(MESH, reversed), expected);
    }

    #[test]
    fn an_m1_only_snapshot_still_hashes_to_its_pre_v2_bytes() {
        // A stored checkpoint commits to `canonical_bytes()`. If the V2 fields
        // changed that encoding for legacy jobs, every M1 checkpoint on disk
        // would fail its own snapshot-reference check on the next validate.
        #[derive(Serialize)]
        enum LegacyJobState {
            #[allow(dead_code)]
            Posted,
            Claimed {
                winner: [u8; 32],
            },
            #[allow(dead_code)]
            Done {
                winner: [u8; 32],
                result: Vec<u8>,
            },
        }
        #[derive(Serialize)]
        struct LegacyJob {
            id: JobId,
            kind: JobKind,
            payload: Option<Vec<u8>>,
            posted_by: [u8; 32],
            state: LegacyJobState,
        }
        #[derive(Serialize)]
        struct LegacySnapshot {
            jobs: Vec<LegacyJob>,
        }

        let asker = keypair(1);
        let worker = keypair(2);
        let posted = post(&asker, 0, None);
        let id = JobId(*posted.hash.as_bytes());
        let claim = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 2,
            },
            0,
            None,
        );
        let board = JobBoard::fold(MESH, [&posted, &claim]);
        let snapshot = JobBoardSnapshot::from_board(&board);

        let legacy = LegacySnapshot {
            jobs: vec![LegacyJob {
                id,
                kind: JobKind::Echo,
                payload: Some(b"job".to_vec()),
                posted_by: author(&asker),
                state: LegacyJobState::Claimed {
                    winner: author(&worker),
                },
            }],
        };
        assert_eq!(
            snapshot.canonical_bytes(),
            p2panda_core::cbor::encode_cbor(&legacy).unwrap(),
            "V2 fields must be invisible in an M1-only snapshot's bytes"
        );
    }

    #[test]
    fn checkpoint_plus_tail_equals_full_replay() {
        let asker = keypair(1);
        let worker = keypair(2);
        let posted = post(&asker, 0, None);
        let id = JobId(*posted.hash.as_bytes());
        let claim = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 2,
            },
            0,
            None,
        );
        let at_checkpoint = JobBoard::fold(MESH, [&posted, &claim]);
        let snapshot = JobBoardSnapshot::from_board(&at_checkpoint);
        let done = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobDone {
                job: id.0,
                result: b"job".to_vec(),
                at_ms: 3,
            },
            1,
            Some(*claim.hash.as_bytes()),
        );

        let full = JobBoard::fold(MESH, [&posted, &claim, &done]);
        let retained = JobBoard::fold_from_snapshot(MESH, &snapshot, [&done]);
        assert_eq!(full, retained);
    }
}
