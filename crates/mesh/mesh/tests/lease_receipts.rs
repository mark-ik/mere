// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! M3's owner-reclaim and lapse receipts.
//!
//! Two workers, one job, a real registry running a real adapter with real
//! cooperative cancellation — and an explicit clock. Every lease timestamp here
//! is an *authored* value, so these receipts are deterministic: nothing sleeps,
//! nothing races, and re-running produces the same board. That is the whole
//! point of keeping "now" out of the fold.
//!
//! An integration test on purpose: it may only touch the crate's public API, so
//! it also checks that a host can actually drive a lease without reaching into
//! the crate's internals.

use std::sync::Arc;

use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};
use mesh::registry::run_job;
use mesh::resources::DelayedResource;
use mesh::spec::DeterminismClass;
use mesh::{
    DeviceConditions, DevicePolicy, HostFacts, HostOffer, JobBoard, JobControl, JobId, JobSpec,
    JobState, LapseReason, LeasePhase, LeasePolicy, LeaseTerms, MemoryBlobSpace, MeshEvent,
    MeshExt, ReclaimReason, ResourceId, ResourceRegistry, WorkerAction,
};
use p2panda_core::Operation;
use proofs::BlobRef;

const MESH: [u8; 32] = [0x4d; 32];
/// Long enough that expiry never fires accidentally in the reclaim receipt.
const LEASE_MS: u64 = 60_000;
const HEARTBEAT_MS: u64 = 10_000;

/// One device's chained log, so a receipt reads as a sequence of events.
struct Device {
    keypair: Ed25519Keypair,
    seq: u32,
    backlink: Option<[u8; 32]>,
}

impl Device {
    fn new(seed: u8) -> Self {
        Self {
            keypair: InMemoryProvider::from_seed([seed; 32])
                .derive_keypair(b"mesh-lease")
                .unwrap(),
            seq: 0,
            backlink: None,
        }
    }

    fn me(&self) -> [u8; 32] {
        self.keypair.public_key().to_bytes()
    }

    fn author(&mut self, event: &MeshEvent) -> Operation<MeshExt> {
        let op = mesh::to_operation(&self.keypair, MESH, event, self.seq, self.backlink);
        self.seq += 1;
        self.backlink = Some(*op.hash.as_bytes());
        op
    }
}

fn leased_spec() -> JobSpec {
    JobSpec::simple(
        ResourceId::parse("mesh.delayed/v1").unwrap(),
        "payload",
        BlobRef::blake3(b"seed"),
        "result",
        64,
        DeterminismClass::Exact,
    )
    .leased(LeaseTerms::new(LEASE_MS, HEARTBEAT_MS))
}

fn exact_policy() -> LeasePolicy {
    LeasePolicy { max_skew_ms: 0 }
}

fn offer<'a>(
    registry: &'a ResourceRegistry,
    policy: &'a DevicePolicy,
    now_ms: u64,
) -> HostOffer<'a> {
    HostOffer::new(registry, HostFacts::cpu(4096), policy)
        .at(now_ms)
        .with_lease_policy(exact_policy())
}

/// The plan's receipt, steps 1-5: two workers race, one wins the lease and makes
/// observable progress, its owner takes the device back mid-run, and the other
/// worker picks the job up under a fresh epoch and finishes it.
#[tokio::test]
async fn owner_reclaim_hands_the_job_to_a_second_worker() {
    let mut asker = Device::new(1);
    let mut first = Device::new(2);
    let mut second = Device::new(3);
    let registry = ResourceRegistry::builtin();
    let permissive = DevicePolicy::permissive();

    // The job is posted with a lease envelope; both devices propose themselves.
    let post = asker.author(&MeshEvent::JobPostedV2 {
        spec: Box::new(leased_spec()),
        nonce: 0,
        at_ms: 0,
    });
    let id = JobId(*post.hash.as_bytes());
    let claim_a = first.author(&MeshEvent::JobClaimed {
        job: id.0,
        at_ms: 1_000,
    });
    let claim_b = second.author(&MeshEvent::JobClaimed {
        job: id.0,
        at_ms: 1_000,
    });
    let mut log: Vec<Operation<MeshExt>> = vec![post, claim_a.clone(), claim_b.clone()];

    // 1. Exactly one of them is the winner, and every device computes the same
    //    one from the claim set alone.
    let board = JobBoard::fold(MESH, log.iter());
    let (winner, loser) = if claim_a.hash.as_bytes() < claim_b.hash.as_bytes() {
        (&mut first, &mut second)
    } else {
        (&mut second, &mut first)
    };
    let winner_me = winner.me();
    let loser_me = loser.me();
    assert!(matches!(
        mesh::next_action(&board, &winner_me, &offer(&registry, &permissive, 2_000)),
        WorkerAction::Grant { epoch: 0, .. }
    ));
    assert_eq!(
        mesh::next_action(&board, &loser_me, &offer(&registry, &permissive, 2_000)),
        WorkerAction::Idle,
        "the loser waits rather than racing a second grant in"
    );

    log.push(winner.author(&MeshEvent::LeaseGranted {
        job: id.0,
        epoch: 0,
        granted_at_ms: 2_000,
        expires_at_ms: 2_000 + LEASE_MS,
    }));
    let board = JobBoard::fold(MESH, log.iter());
    let lease = board
        .job(id)
        .unwrap()
        .lease_at(3_000, &exact_policy())
        .lease()
        .expect("epoch 0 is granted");

    // 2. The winner starts work and heartbeats observable progress.
    let space = MemoryBlobSpace::in_memory();
    space.put(b"seed").await.unwrap();
    let spec = board.job(id).unwrap().spec.clone().unwrap();
    let (control_handle, control) = JobControl::new();
    let run = {
        let registry = ResourceRegistry::builtin();
        let space = Arc::new(space);
        let running_space = space.clone();
        let spec = spec.clone();
        let control = control.clone();
        tokio::spawn(async move {
            run_job(&registry, &spec, &*running_space, &*running_space, &control).await
        })
    };
    // Let it get somewhere, then heartbeat whatever it actually reported.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let progress = control_handle.progress();
    log.push(winner.author(&MeshEvent::LeaseHeartbeat {
        job: id.0,
        lease: lease.0,
        progress,
        at_ms: 5_000,
    }));
    let board = JobBoard::fold(MESH, log.iter());
    assert!(
        board
            .job(id)
            .unwrap()
            .lease_at(6_000, &exact_policy())
            .held_by(&winner_me),
        "the lease is live and fed"
    );

    // 3. The owner comes back to the keyboard. The scheduler, not the job, says
    //    what happens next.
    let conservative = DevicePolicy::conservative();
    let running = [id];
    let reclaiming = offer(&registry, &conservative, 6_000)
        .running(&running)
        .observing(DeviceConditions::spare().in_use());
    assert_eq!(
        mesh::next_action(&board, &winner_me, &reclaiming),
        WorkerAction::Reclaim {
            job: id,
            lease,
            reason: ReclaimReason::ForegroundActivity
        },
        "owner reclaim outranks the heartbeat that is also due"
    );

    // 4. Execution stops on request, and the revoke is a clean, signed fact.
    control_handle.cancel();
    assert!(
        run.await.unwrap().is_err(),
        "the run stopped rather than committing"
    );
    log.push(winner.author(&MeshEvent::LeaseRevokedByOwner {
        job: id.0,
        lease: lease.0,
        reason: ReclaimReason::ForegroundActivity,
        at_ms: 6_000,
    }));
    let board = JobBoard::fold(MESH, log.iter());
    assert!(
        matches!(
            board.job(id).unwrap().lease_at(7_000, &exact_policy()),
            LeasePhase::Reclaimed {
                reason: ReclaimReason::ForegroundActivity,
                ..
            }
        ),
        "the board says the device was reclaimed, not that the worker failed"
    );

    // 5. The other device re-claims after the revoke and finishes the job.
    log.push(loser.author(&MeshEvent::JobClaimed {
        job: id.0,
        at_ms: 7_000,
    }));
    let board = JobBoard::fold(MESH, log.iter());
    assert_eq!(
        mesh::next_action(&board, &loser_me, &offer(&registry, &permissive, 7_500)),
        WorkerAction::Grant {
            job: id,
            epoch: 1,
            granted_at_ms: 7_500,
            expires_at_ms: 7_500 + LEASE_MS
        }
    );
    log.push(loser.author(&MeshEvent::LeaseGranted {
        job: id.0,
        epoch: 1,
        granted_at_ms: 7_500,
        expires_at_ms: 7_500 + LEASE_MS,
    }));
    let board = JobBoard::fold(MESH, log.iter());
    let second_lease = board
        .job(id)
        .unwrap()
        .lease_at(8_000, &exact_policy())
        .lease()
        .expect("epoch 1 is granted");
    assert_ne!(second_lease, lease, "a new epoch is a new lease");

    let space = MemoryBlobSpace::in_memory();
    space.put(b"seed").await.unwrap();
    let (_second_handle, second_control) = JobControl::new();
    let output = run_job(&registry, &spec, &space, &space, &second_control)
        .await
        .expect("the second device finishes the job");
    assert_eq!(
        space.get(&output.blob).await.unwrap(),
        Some(DelayedResource::expected(b"seed", 16))
    );

    log.push(loser.author(&MeshEvent::JobCompletedUnderLease {
        job: id.0,
        lease: second_lease.0,
        output: Box::new(output.clone()),
        at_ms: 9_000,
    }));
    let board = JobBoard::fold(MESH, log.iter());
    let job = board.job(id).unwrap();
    assert_eq!(
        job.state,
        JobState::Committed {
            winner: loser_me,
            output: Box::new(output)
        }
    );
    assert!(matches!(
        job.lease_at(10_000, &exact_policy()),
        LeasePhase::Done { epoch: 1, .. }
    ));
    assert_eq!(
        mesh::next_action(&board, &winner_me, &offer(&registry, &permissive, 10_000)),
        WorkerAction::Idle,
        "a finished job is not re-let"
    );
}

/// The plan's receipt, steps 6-7: the *same* opening history, continued two
/// ways. One device is reclaimed by its owner; the other simply goes quiet. The
/// board tells them apart, and only one of the two is even an observation.
#[test]
fn owner_revoke_and_heartbeat_silence_are_distinguishable_histories() {
    let mut asker = Device::new(1);
    let mut holder = Device::new(2);

    let post = asker.author(&MeshEvent::JobPostedV2 {
        spec: Box::new(leased_spec()),
        nonce: 0,
        at_ms: 0,
    });
    let id = JobId(*post.hash.as_bytes());
    let claim = holder.author(&MeshEvent::JobClaimed {
        job: id.0,
        at_ms: 1_000,
    });
    let grant = holder.author(&MeshEvent::LeaseGranted {
        job: id.0,
        epoch: 0,
        granted_at_ms: 2_000,
        expires_at_ms: 2_000 + LEASE_MS,
    });
    let opening = [post, claim, grant];
    let lease = JobBoard::fold(MESH, opening.iter())
        .job(id)
        .unwrap()
        .lease_at(3_000, &exact_policy())
        .lease()
        .unwrap();

    // History A: the owner takes the device back, and says so.
    let mut revoked = holder;
    let revoke = revoked.author(&MeshEvent::LeaseRevokedByOwner {
        job: id.0,
        lease: lease.0,
        reason: ReclaimReason::Battery,
        at_ms: 5_000,
    });
    let with_revoke: Vec<_> = opening.iter().chain([&revoke]).collect();
    let board_a = JobBoard::fold(MESH, with_revoke);

    // History B: the same opening, then nothing at all.
    let board_b = JobBoard::fold(MESH, opening.iter());

    // Immediately after the revoke the two boards already disagree, and they
    // disagree about *why*.
    assert!(matches!(
        board_a.job(id).unwrap().lease_at(5_500, &exact_policy()),
        LeasePhase::Reclaimed {
            reason: ReclaimReason::Battery,
            ..
        }
    ));
    assert!(
        board_b
            .job(id)
            .unwrap()
            .lease_at(5_500, &exact_policy())
            .held_by(&revoked.me()),
        "silence is not yet evidence of anything"
    );

    // Only once the allowed silence has run out does history B lapse — and it
    // lapses as an observation about contact, never as a device reclaim.
    let silent_from = 2_000 + HEARTBEAT_MS * 3;
    assert!(matches!(
        board_b
            .job(id)
            .unwrap()
            .lease_at(silent_from, &exact_policy()),
        LeasePhase::Lapsed {
            reason: LapseReason::HeartbeatSilence,
            ..
        }
    ));
    // And past the signed window it lapses for a different, harder reason.
    assert!(matches!(
        board_b
            .job(id)
            .unwrap()
            .lease_at(2_000 + LEASE_MS, &exact_policy()),
        LeasePhase::Lapsed {
            reason: LapseReason::Expired,
            ..
        }
    ));

    // Both histories reopen the job at the next epoch, which is the point: a
    // reclaimed device and a silent one both free the work.
    assert_eq!(
        board_a
            .job(id)
            .unwrap()
            .lease_at(5_500, &exact_policy())
            .open_epoch(),
        Some(1)
    );
    assert_eq!(
        board_b
            .job(id)
            .unwrap()
            .lease_at(silent_from, &exact_policy())
            .open_epoch(),
        Some(1)
    );
}

/// The fold is clock-free: the same operations produce the same board however
/// they arrive, and only the observation time moves the phase.
#[test]
fn lease_facts_fold_identically_in_every_order() {
    let mut asker = Device::new(1);
    let mut holder = Device::new(2);

    let post = asker.author(&MeshEvent::JobPostedV2 {
        spec: Box::new(leased_spec()),
        nonce: 0,
        at_ms: 0,
    });
    let id = JobId(*post.hash.as_bytes());
    let claim = holder.author(&MeshEvent::JobClaimed {
        job: id.0,
        at_ms: 1_000,
    });
    let grant = holder.author(&MeshEvent::LeaseGranted {
        job: id.0,
        epoch: 0,
        granted_at_ms: 2_000,
        expires_at_ms: 2_000 + LEASE_MS,
    });
    let lease = JobBoard::fold(MESH, [&post, &claim, &grant])
        .job(id)
        .unwrap()
        .lease_at(3_000, &exact_policy())
        .lease()
        .unwrap();
    let beat = holder.author(&MeshEvent::LeaseHeartbeat {
        job: id.0,
        lease: lease.0,
        progress: Default::default(),
        at_ms: 20_000,
    });

    let ops = [&post, &claim, &grant, &beat];
    let expected = JobBoard::fold(MESH, ops);
    for shift in 0..ops.len() {
        let mut rotated = ops;
        rotated.rotate_left(shift);
        assert_eq!(JobBoard::fold(MESH, rotated), expected, "rotation {shift}");
    }

    // Same facts; the phase moves only because the clock did.
    let job = expected.job(id).unwrap();
    assert!(job.lease_at(25_000, &exact_policy()).held_by(&holder.me()));
    assert!(matches!(
        job.lease_at(50_001, &exact_policy()),
        LeasePhase::Lapsed {
            reason: LapseReason::HeartbeatSilence,
            ..
        }
    ));
}
