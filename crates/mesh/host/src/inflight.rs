// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! One running job, and the state the supervisor keeps beside it.

use mesh::{
    JobBoard, JobControlHandle, JobId, JobOutput, LeaseId, LeasePhase, LeasePolicy, NamespaceError,
    ReclaimReason, ReleaseReason, RunError,
};
use tokio::task::JoinHandle;

/// What a finished run produced, in whichever generation it answers.
pub enum RunOutcome {
    /// A V2 job's committed output.
    Committed(Box<JobOutput>),
    /// An M1 job's inline result.
    Inline(Vec<u8>),
}

/// A reclaim in progress: the owner has asked for the device back and the run
/// has not stopped yet.
///
/// The revoke is authored *after* the run stops, never before, so a peer that
/// sees the fact knows the hardware is genuinely free. That is why this is a
/// state rather than a single action.
pub struct Reclaiming {
    pub reason: ReclaimReason,
    /// When the run gets cancelled outright. Equal to the request time for an
    /// interruptible job; a grace window later for a
    /// [`CheckpointClass::NonInterruptible`](mesh::CheckpointClass) one.
    pub hard_cancel_at_ms: u64,
    /// Whether the cancel signal has been sent yet.
    pub signalled: bool,
}

/// A job this device is running right now.
pub struct InFlight {
    pub handle: JobControlHandle,
    pub task: JoinHandle<Result<RunOutcome, RunError>>,
    /// The lease this run is bound to, if the job was posted with terms.
    pub lease: Option<LeaseId>,
    pub reclaim: Option<Reclaiming>,
}

impl InFlight {
    /// Whether the owner has already asked for this one back.
    pub fn is_reclaiming(&self) -> bool {
        self.reclaim.is_some()
    }
}

/// Whether `me` still holds `lease` on `job` at `at_ms`.
///
/// The question a supervisor must keep asking about work it has already
/// started. A device can grant itself a lease on a board that has not caught
/// up, win locally, and begin — and then lose to a peer's earlier grant once
/// the facts arrive. That is the fold doing its job, but the loser has to
/// notice: every peer would drop a completion authored under a lease that did
/// not survive, so the run is wasted and reporting it would be a lie.
///
/// Also covers the ordinary endings: expiry, and a revoke authored elsewhere.
pub fn still_held(
    board: &JobBoard,
    job: JobId,
    lease: LeaseId,
    me: &[u8; 32],
    at_ms: u64,
    policy: &LeasePolicy,
) -> bool {
    board.job(job).is_some_and(|record| {
        matches!(
            record.lease_at(at_ms, policy),
            LeasePhase::Held { lease: held, holder, .. } if held == lease && &holder == me
        )
    })
}

/// How a failed run should be reported to the ring.
///
/// A blob this device never received is not unreliability — it is a lane the
/// mesh does not yet provide (host lanes plan, gate H1). Saying so is the
/// difference between "this worker is flaky" and "nobody delivered the bytes".
pub fn release_reason(error: &RunError) -> ReleaseReason {
    match error {
        RunError::Namespace(NamespaceError::MissingBlob(_)) => ReleaseReason::InputUnavailable,
        _ => ReleaseReason::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};
    use mesh::spec::{DeterminismClass, JobSpec};
    use mesh::{LeaseTerms, MeshEvent, MeshExt, ResourceId, to_operation};
    use p2panda_core::Operation;

    const MESH: [u8; 32] = [0x4d; 32];
    const EXACT: LeasePolicy = LeasePolicy { max_skew_ms: 0 };

    struct Device {
        keypair: Ed25519Keypair,
        seq: u32,
        backlink: Option<[u8; 32]>,
    }

    impl Device {
        fn new(seed: u8) -> Self {
            Self {
                keypair: InMemoryProvider::from_seed([seed; 32])
                    .derive_keypair(b"mesh-host")
                    .unwrap(),
                seq: 0,
                backlink: None,
            }
        }

        fn me(&self) -> [u8; 32] {
            self.keypair.public_key().to_bytes()
        }

        fn author(&mut self, event: &MeshEvent) -> Operation<MeshExt> {
            let op = to_operation(&self.keypair, MESH, event, self.seq, self.backlink);
            self.seq += 1;
            self.backlink = Some(*op.hash.as_bytes());
            op
        }
    }

    /// Two devices both claim and both grant themselves epoch 0, each on a
    /// board that has not seen the other. Exactly one survives the fold.
    #[test]
    fn a_grant_that_lost_the_race_is_no_longer_held() {
        let mut asker = Device::new(1);
        let mut a = Device::new(2);
        let mut b = Device::new(3);

        let spec = JobSpec::simple(
            ResourceId::parse("mesh.delayed/v1").unwrap(),
            "payload",
            proofs::BlobRef::blake3(b"seed"),
            "result",
            64,
            DeterminismClass::Exact,
        )
        .leased(LeaseTerms::new(600_000, 60_000));
        let post = asker.author(&MeshEvent::JobPostedV2 {
            spec: Box::new(spec),
            nonce: 0,
            at_ms: 0,
        });
        let job = JobId(*post.hash.as_bytes());

        let mut grants = Vec::new();
        for device in [&mut a, &mut b] {
            grants.push(device.author(&MeshEvent::JobClaimed {
                job: job.0,
                at_ms: 1_000,
            }));
            grants.push(device.author(&MeshEvent::LeaseGranted {
                job: job.0,
                epoch: 0,
                granted_at_ms: 2_000,
                expires_at_ms: 602_000,
            }));
        }
        let (a_lease, b_lease) = (
            LeaseId(*grants[1].hash.as_bytes()),
            LeaseId(*grants[3].hash.as_bytes()),
        );

        // Each device alone: its own grant stands, because it is the only
        // claimant it knows about.
        let alone_a = JobBoard::fold(MESH, [&post, &grants[0], &grants[1]]);
        assert!(still_held(&alone_a, job, a_lease, &a.me(), 3_000, &EXACT));
        let alone_b = JobBoard::fold(MESH, [&post, &grants[2], &grants[3]]);
        assert!(still_held(&alone_b, job, b_lease, &b.me(), 3_000, &EXACT));

        // Once the facts meet, exactly one lease survives — and the loser's
        // supervisor can see that it must abandon its run.
        let full = JobBoard::fold(
            MESH,
            [&post, &grants[0], &grants[1], &grants[2], &grants[3]],
        );
        let a_survives = still_held(&full, job, a_lease, &a.me(), 3_000, &EXACT);
        let b_survives = still_held(&full, job, b_lease, &b.me(), 3_000, &EXACT);
        assert!(
            a_survives ^ b_survives,
            "exactly one of two racing grants survives the fold"
        );
    }

    #[test]
    fn an_expired_lease_is_no_longer_held_and_an_unknown_job_never_was() {
        let mut asker = Device::new(1);
        let mut holder = Device::new(2);
        let spec = JobSpec::simple(
            ResourceId::parse("mesh.delayed/v1").unwrap(),
            "payload",
            proofs::BlobRef::blake3(b"seed"),
            "result",
            64,
            DeterminismClass::Exact,
        )
        .leased(LeaseTerms::new(60_000, 10_000));
        let post = asker.author(&MeshEvent::JobPostedV2 {
            spec: Box::new(spec),
            nonce: 0,
            at_ms: 0,
        });
        let job = JobId(*post.hash.as_bytes());
        let claim = holder.author(&MeshEvent::JobClaimed {
            job: job.0,
            at_ms: 1_000,
        });
        let grant = holder.author(&MeshEvent::LeaseGranted {
            job: job.0,
            epoch: 0,
            granted_at_ms: 2_000,
            expires_at_ms: 62_000,
        });
        let lease = LeaseId(*grant.hash.as_bytes());
        let board = JobBoard::fold(MESH, [&post, &claim, &grant]);

        assert!(still_held(&board, job, lease, &holder.me(), 3_000, &EXACT));
        assert!(
            !still_held(&board, job, lease, &holder.me(), 62_000, &EXACT),
            "the signed window closed"
        );
        assert!(
            !still_held(&board, job, lease, &Device::new(9).me(), 3_000, &EXACT),
            "someone else's lease is not this device's to run under"
        );
        assert!(
            !still_held(&board, JobId([7; 32]), lease, &holder.me(), 3_000, &EXACT),
            "a job this device has never heard of is not held"
        );
    }

    #[test]
    fn a_missing_blob_is_not_worker_unreliability() {
        let missing = RunError::Namespace(NamespaceError::MissingBlob("texts".to_string()));
        assert_eq!(release_reason(&missing), ReleaseReason::InputUnavailable);

        let broken = RunError::Namespace(NamespaceError::DigestMismatch("texts".to_string()));
        assert_eq!(release_reason(&broken), ReleaseReason::Failed);
    }
}
