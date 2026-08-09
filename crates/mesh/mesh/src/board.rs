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
}

impl Job {
    /// Whether this device could still run the job: its input is retained (M1)
    /// or its manifest is present (V2).
    pub fn is_runnable(&self) -> bool {
        self.spec.is_some() || self.payload.is_some()
    }
}

/// The folded board: every known job, keyed (and so ordered) by id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobBoard {
    jobs: BTreeMap<JobId, Job>,
}

/// A gathered posting, before claims and results are resolved against it.
struct Posted {
    kind: Option<JobKind>,
    payload: Option<Vec<u8>>,
    spec: Option<Box<JobSpec>>,
    by: [u8; 32],
}

/// A gathered result, in whichever generation its author wrote.
enum ResultRecord {
    Inline(Vec<u8>),
    Committed(Box<JobOutput>),
}

impl Posted {
    /// Resolve one job's terminal state. A result only counts when it answers
    /// the generation the job was posted in, and a V2 result must additionally
    /// honour the signed grant — so a winner cannot rename the output slot,
    /// overflow its ceiling, or substitute another resource.
    fn resolve(&self, winner: [u8; 32], record: Option<&ResultRecord>) -> JobState {
        match (record, &self.spec) {
            (Some(ResultRecord::Inline(result)), None) => JobState::Done {
                winner,
                result: result.clone(),
            },
            (Some(ResultRecord::Committed(output)), Some(spec))
                if output.validate_against(spec).is_ok() =>
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
        // job → claim-op-hash → claimant author. BTreeMap keys give the
        // deterministic winner (lowest claim-op hash) for free.
        let mut claims: BTreeMap<JobId, BTreeMap<[u8; 32], [u8; 32]>> = BTreeMap::new();
        // job → author → result, in whichever generation the author wrote.
        let mut results: BTreeMap<JobId, BTreeMap<[u8; 32], ResultRecord>> = BTreeMap::new();

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
            if let Some(winner) = job.state.winner() {
                claims.entry(job.id).or_default().insert([0; 32], winner);
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
                MeshEvent::JobClaimed { job, .. } => {
                    claims
                        .entry(JobId(job))
                        .or_default()
                        .insert(*op.hash.as_bytes(), author);
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
                MeshEvent::RetentionCheckpoint { .. } | MeshEvent::HistoryPruned { .. } => {}
            }
        }

        // Resolve phase.
        let mut jobs = BTreeMap::new();
        for (id, post) in posted {
            let winner = claims
                .get(&id)
                .and_then(|c| c.first_key_value())
                .map(|(_, claimant)| *claimant);
            let state = match winner {
                None => JobState::Posted,
                Some(winner) => post.resolve(winner, results.get(&id).and_then(|r| r.get(&winner))),
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
                },
            );
        }
        Self { jobs }
    }

    /// All jobs, in id order.
    pub fn jobs(&self) -> impl Iterator<Item = &Job> {
        self.jobs.values()
    }

    /// One job by id.
    pub fn job(&self, id: JobId) -> Option<&Job> {
        self.jobs.get(&id)
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
