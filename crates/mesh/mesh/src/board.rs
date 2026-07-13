/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

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
use crate::wire::{JobKind, MeshEvent, MeshExt, from_operation, verify};

/// A job's identity: the hash of its `JobPosted` operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct JobId(pub [u8; 32]);

/// Where a job is in its M1 lifecycle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobState {
    /// Posted, no claims yet.
    Posted,
    /// Claimed; `winner` is the deterministic claim-race winner's author key.
    Claimed { winner: [u8; 32] },
    /// The winner returned a result.
    Done { winner: [u8; 32], result: Vec<u8> },
}

/// One job on the board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub kind: JobKind,
    /// `None` after an accepted checkpoint erases a terminal job's input.
    pub payload: Option<Vec<u8>>,
    /// The poster's author (verifying) key bytes.
    pub posted_by: [u8; 32],
    pub state: JobState,
}

/// The folded board: every known job, keyed (and so ordered) by id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobBoard {
    jobs: BTreeMap<JobId, Job>,
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
        struct Posted {
            kind: JobKind,
            payload: Option<Vec<u8>>,
            by: [u8; 32],
        }
        let mut posted: BTreeMap<JobId, Posted> = BTreeMap::new();
        // job → claim-op-hash → claimant author. BTreeMap keys give the
        // deterministic winner (lowest claim-op hash) for free.
        let mut claims: BTreeMap<JobId, BTreeMap<[u8; 32], [u8; 32]>> = BTreeMap::new();
        // job → author → result.
        let mut results: BTreeMap<JobId, BTreeMap<[u8; 32], Vec<u8>>> = BTreeMap::new();

        // Seed the gather maps from the accepted checkpoint. The all-zero
        // synthetic claim key sorts before any real operation hash, preserving
        // a winner already resolved at the checkpoint frontier.
        for job in &snapshot.jobs {
            posted.insert(
                job.id,
                Posted {
                    kind: job.kind,
                    payload: job.payload.clone(),
                    by: job.posted_by,
                },
            );
            match &job.state {
                JobState::Posted => {}
                JobState::Claimed { winner } => {
                    claims.entry(job.id).or_default().insert([0; 32], *winner);
                }
                JobState::Done { winner, result } => {
                    claims.entry(job.id).or_default().insert([0; 32], *winner);
                    results
                        .entry(job.id)
                        .or_default()
                        .insert(*winner, result.clone());
                }
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
                            kind,
                            payload: Some(payload),
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
                        .insert(author, result);
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
                Some(winner) => match results.get(&id).and_then(|r| r.get(&winner)) {
                    Some(result) => JobState::Done {
                        winner,
                        result: result.clone(),
                    },
                    None => JobState::Claimed { winner },
                },
            };
            jobs.insert(
                id,
                Job {
                    id,
                    kind: post.kind,
                    payload: post.payload,
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

    fn post(kp: &Ed25519Keypair, seq: u64, back: Option<[u8; 32]>) -> Operation<MeshExt> {
        to_operation(
            kp,
            MESH,
            &MeshEvent::JobPosted {
                kind: JobKind::Echo,
                payload: b"job".to_vec(),
                nonce: seq,
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
