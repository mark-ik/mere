/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The worker's decision function and the M1 job executors.
//!
//! [`next_action`] is pure (board + identity in, action out) so the loop is
//! testable without networking; the *host* drives it — the `mesh-peer` bin
//! now, meerkat's P6 compute actor later. Owner-priority / preemption /
//! checkpoint classes are the resource brief's scheduler layer (later
//! milestones); an M1 worker simply works when asked.

use p2panda_core::Hash;

use crate::board::{JobBoard, JobId, JobState};
use crate::wire::JobKind;

/// What the worker loop should do next, in priority order: finish what you
/// won before claiming more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerAction {
    /// You are the claim winner of this job and no result exists: execute it
    /// and post `JobDone`.
    Execute(JobId),
    /// This job has no claim from you yet (and no result): post `JobClaimed`.
    /// Racing another device's claim is fine — the board resolves the winner
    /// deterministically.
    Claim(JobId),
    /// Nothing to do.
    Idle,
}

/// The next thing `me` (an author/verifying key) should do against `board`.
pub fn next_action(board: &JobBoard, me: &[u8; 32]) -> WorkerAction {
    // 1) A job I won and haven't finished.
    for job in board.jobs() {
        if let JobState::Claimed { winner } = &job.state
            && winner == me
            && job.payload.is_some()
        {
            return WorkerAction::Execute(job.id);
        }
    }
    // 2) An unclaimed job (don't double-claim my own posts? M1 allows
    //    self-execution — a one-device mesh still round-trips, which keeps the
    //    smallest setup useful and the demo honest).
    for job in board.jobs() {
        if matches!(job.state, JobState::Posted) {
            return WorkerAction::Claim(job.id);
        }
    }
    WorkerAction::Idle
}

/// Run an M1 job kind over its payload. Pure and deterministic by
/// construction — the property the brief's T0 verification tier is built on.
pub fn execute(kind: JobKind, payload: &[u8]) -> Vec<u8> {
    match kind {
        JobKind::Echo => payload.to_vec(),
        JobKind::Blake3 => Hash::digest(payload).as_bytes().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{MeshEvent, to_operation};
    use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};

    const MESH: [u8; 32] = [0x4d; 32];

    fn keypair(seed: u8) -> Ed25519Keypair {
        InMemoryProvider::from_seed([seed; 32])
            .derive_keypair(b"mesh-worker")
            .unwrap()
    }

    #[test]
    fn execute_kinds_are_deterministic() {
        assert_eq!(execute(JobKind::Echo, b"abc"), b"abc".to_vec());
        let h1 = execute(JobKind::Blake3, b"abc");
        let h2 = execute(JobKind::Blake3, b"abc");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 32);
        assert_eq!(h1, Hash::digest(b"abc").as_bytes().to_vec());
    }

    #[test]
    fn worker_claims_then_executes_then_idles() {
        let asker = keypair(1);
        let worker = keypair(2);
        let me = worker.public_key().to_bytes();

        let posted = to_operation(
            &asker,
            MESH,
            &MeshEvent::JobPosted {
                kind: JobKind::Echo,
                payload: b"x".to_vec(),
                nonce: 0,
                at_ms: 1,
            },
            0,
            None,
        );
        let id = crate::board::JobId(*posted.hash.as_bytes());

        let board = JobBoard::fold(MESH, [&posted]);
        assert_eq!(next_action(&board, &me), WorkerAction::Claim(id));

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
        assert_eq!(next_action(&board, &me), WorkerAction::Execute(id));

        let done = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobDone {
                job: id.0,
                result: execute(JobKind::Echo, b"x"),
                at_ms: 3,
            },
            1,
            Some(*claim.hash.as_bytes()),
        );
        let board = JobBoard::fold(MESH, [&posted, &claim, &done]);
        assert_eq!(next_action(&board, &me), WorkerAction::Idle);
    }

    #[test]
    fn a_claim_loser_does_not_execute() {
        let asker = keypair(1);
        let w1 = keypair(2);
        let w2 = keypair(3);
        let posted = to_operation(
            &asker,
            MESH,
            &MeshEvent::JobPosted {
                kind: JobKind::Echo,
                payload: b"x".to_vec(),
                nonce: 0,
                at_ms: 1,
            },
            0,
            None,
        );
        let id = crate::board::JobId(*posted.hash.as_bytes());
        let c1 = to_operation(
            &w1,
            MESH,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 2,
            },
            0,
            None,
        );
        let c2 = to_operation(
            &w2,
            MESH,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 2,
            },
            0,
            None,
        );
        let board = JobBoard::fold(MESH, [&posted, &c1, &c2]);
        let winner_is_w1 = c1.hash.as_bytes() < c2.hash.as_bytes();
        let (winner_me, loser_me) = if winner_is_w1 {
            (w1.public_key().to_bytes(), w2.public_key().to_bytes())
        } else {
            (w2.public_key().to_bytes(), w1.public_key().to_bytes())
        };
        assert_eq!(next_action(&board, &winner_me), WorkerAction::Execute(id));
        assert_eq!(
            next_action(&board, &loser_me),
            WorkerAction::Idle,
            "the loser neither executes nor re-claims a claimed job"
        );
    }
}
