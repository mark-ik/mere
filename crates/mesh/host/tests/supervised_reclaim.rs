// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gate H0's receipt: two **supervised** hosts over real p2panda-net peers.
//!
//! M3 proved this story with a test authoring lease events by hand. Here nobody
//! drives the protocol: two `MeshHost`s tick, and the claim, the grant, the
//! heartbeat, the reclaim and the handover all come out of the supervisors'
//! own decisions.
//!
//! The clock is manual, so lease boundaries land where the test says. Execution
//! is not: the delayed resource really sleeps, on a real task, off the holder's
//! decision loop — which is the property under test.
//!
//! Blob delivery is gate H1, so the input is staged on both devices. This
//! receipt is about supervision, not about who moved the bytes.

use std::sync::Arc;
use std::time::Duration;

use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};
use mesh::resources::DelayedResource;
use mesh::spec::{DeterminismClass, JobSpec};
use mesh::{
    DevicePolicy, HostFacts, JobId, JobState, LeasePhase, LeasePolicy, LeaseTerms, MemoryBlobSpace,
    MeshEvent, MeshStore, ReclaimReason, ResourceId, ResourceRegistry, SyncedMesh,
};
use mesh_host::{Clock, HostConfig, ManualClock, MeshHost, ObservedConditions, Step};
use muniment::MemoryBackend;
use proofs::BlobRef;
use std::collections::BTreeSet;
use transport::P2pandaTransport;

const MESH: [u8; 32] = [0x4d; 32];
/// Long enough that the run cannot finish before the owner reclaims it.
const UNITS: u64 = 40;
const LEASE_MS: u64 = 600_000;
const HEARTBEAT_MS: u64 = 60_000;
const EXACT: LeasePolicy = LeasePolicy { max_skew_ms: 0 };

type Host = MeshHost<MemoryBackend>;

fn keypair(seed: u8) -> Ed25519Keypair {
    InMemoryProvider::from_seed([seed; 32])
        .derive_keypair(b"mesh-author")
        .unwrap()
}

/// A registry offering only the rehearsal resource. `unit_delay_ms` changes how
/// long the run takes and nothing about what it produces — timing is not part
/// of the resource's contract, which is why both hosts can use different values
/// and still be the same `mesh.delayed/v1`.
fn registry(unit_delay_ms: u64) -> ResourceRegistry {
    let mut registry = ResourceRegistry::new();
    registry
        .register(Arc::new(DelayedResource::new(UNITS, unit_delay_ms)))
        .unwrap();
    registry
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

/// A device that will not take this job at all, so the claim race has one
/// entrant and the receipt is deterministic.
fn abstaining() -> DevicePolicy {
    DevicePolicy {
        allowed_resources: BTreeSet::from([ResourceId::parse("mesh.echo/v1").unwrap()]),
        ..DevicePolicy::permissive()
    }
}

async fn two_peers() -> (P2pandaTransport, P2pandaTransport) {
    let alice_provider = Arc::new(InMemoryProvider::from_seed([70; 32]));
    let bob_provider = Arc::new(InMemoryProvider::from_seed([71; 32]));
    let alice_id = transport::PeerID::from_public_key(alice_provider.master_public_key());
    let bob_id = transport::PeerID::from_public_key(bob_provider.master_public_key());

    let alice = P2pandaTransport::builder(alice_provider.master_keypair())
        .gossip()
        .bind()
        .await
        .expect("bind alice");
    let bob = P2pandaTransport::builder(bob_provider.master_keypair())
        .gossip()
        .bind()
        .await
        .expect("bind bob");

    let overlay = transport::sync_overlay_topic(MESH);
    alice
        .add_peer(bob.endpoint_addr().await.unwrap())
        .await
        .unwrap();
    alice.set_topics(bob_id, &[overlay]).await.unwrap();
    bob.add_peer(alice.endpoint_addr().await.unwrap())
        .await
        .unwrap();
    bob.set_topics(alice_id, &[overlay]).await.unwrap();
    (alice, bob)
}

/// One supervised device: its own store, sync session, blob space and
/// conditions, sharing the ring's clock.
async fn spawn_host(
    transport: &P2pandaTransport,
    keypair: Ed25519Keypair,
    clock: Arc<ManualClock>,
    policy: DevicePolicy,
    unit_delay_ms: u64,
) -> (Host, Arc<ObservedConditions>) {
    let (endpoint, gossip) = transport.sync_parts().expect("sync parts");
    let synced = SyncedMesh::join(endpoint, gossip, MeshStore::in_memory(), MESH)
        .await
        .expect("join");
    let blobs = Arc::new(MemoryBlobSpace::in_memory());
    blobs.put(b"seed").await.expect("stage the input");
    let conditions = Arc::new(ObservedConditions::spare());

    let mut config = HostConfig::supervised(blobs);
    config.registry = registry(unit_delay_ms);
    config.clock = clock;
    config.conditions = conditions.clone();
    config.facts = HostFacts::cpu(4096);
    config.policy = policy;
    config.lease = EXACT;

    (MeshHost::new(synced, keypair, config), conditions)
}

/// Tick both hosts until `done` holds over everything either has done. Ticking
/// both each round is what lets sync make progress.
async fn pump(
    alice: &mut Host,
    bob: &mut Host,
    what: &str,
    mut done: impl FnMut(&[Step], &[Step]) -> bool,
) -> (Vec<Step>, Vec<Step>) {
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for _ in 0..200 {
        a.extend(alice.tick().await.expect("alice ticks"));
        b.extend(bob.tick().await.expect("bob ticks"));
        if done(&a, &b) {
            return (a, b);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    panic!("timed out waiting for: {what}\nalice: {a:#?}\nbob: {b:#?}");
}

async fn phase(host: &Host, id: JobId, at_ms: u64) -> LeasePhase {
    host.synced()
        .board()
        .await
        .unwrap()
        .job(id)
        .expect("the job is known")
        .lease_at(at_ms, &EXACT)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reclaimed_host_stops_before_it_says_so_and_the_other_finishes() {
    let clock = Arc::new(ManualClock::at(1_000));
    let (alice_t, bob_t) = two_peers().await;
    let alice_kp = keypair(72);
    let bob_kp = keypair(73);
    let alice_me = alice_kp.public_key().to_bytes();
    let bob_me = bob_kp.public_key().to_bytes();

    // Alice posts but abstains from working, so Bob is the only entrant. Her
    // zero-delay registry is the same resource: it will produce the same bytes.
    let (mut alice, _alice_conditions) =
        spawn_host(&alice_t, alice_kp.clone(), clock.clone(), abstaining(), 0).await;
    // Bob lends only while genuinely spare — `permissive()` would lend
    // unconditionally, and a device that never withholds never reclaims.
    let (mut bob, bob_conditions) = spawn_host(
        &bob_t,
        bob_kp,
        clock.clone(),
        DevicePolicy::conservative(),
        40,
    )
    .await;

    let posted = alice
        .synced()
        .author(
            &alice_kp,
            &MeshEvent::JobPostedV2 {
                spec: Box::new(leased_spec()),
                nonce: 1,
                at_ms: 1_000,
            },
        )
        .await
        .expect("alice posts a lendable job");
    let id = JobId(*posted.hash.as_bytes());

    // 1. Bob claims, grants himself the lease, and starts — nobody authored a
    //    lease event by hand.
    let (_, b) = pump(&mut alice, &mut bob, "bob starts the job", |_, b| {
        b.iter().any(|s| matches!(s, Step::Started { .. }))
    })
    .await;
    assert!(
        b.iter().any(|s| matches!(s, Step::Claimed { .. }))
            && b.iter()
                .any(|s| matches!(s, Step::Granted { epoch: 0, .. })),
        "the supervisor drove claim then grant: {b:#?}"
    );
    assert!(
        matches!(
            phase(&bob, id, clock.now_ms()).await,
            LeasePhase::Held { .. }
        ),
        "the lease is live"
    );

    // 2. The run is off the decision loop: Bob keeps ticking while it advances.
    let mut moved = false;
    for _ in 0..60 {
        bob.tick().await.expect("bob ticks while working");
        if bob.progress(id).is_some_and(|p| p.done > 0) {
            moved = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert!(moved, "the supervisor kept ticking while the job ran");

    // 3. A heartbeat carries what the run actually reported, not a constant.
    clock.advance(HEARTBEAT_MS + 1_000);
    let mut beat = None;
    for _ in 0..20 {
        for step in bob.tick().await.expect("bob ticks") {
            if let Step::Heartbeat { progress, .. } = step {
                beat = Some(progress);
            }
        }
        if beat.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    let beat = beat.expect("a heartbeat came due and went out");
    assert!(
        beat.done > 0 && beat.total == UNITS,
        "the heartbeat reported real progress: {beat:?}"
    );

    // 4. The human comes back to Bob's keyboard.
    bob_conditions.in_use();
    let steps = bob.tick().await.expect("bob ticks");
    assert!(
        steps.iter().any(|s| matches!(s, Step::AwaitingStop { .. })),
        "the demand is recorded before anything is authored: {steps:#?}"
    );
    assert!(
        phase(&bob, id, clock.now_ms()).await.held_by(&bob_me),
        "no revoke while the run is still stopping — the board still shows it held"
    );

    // 5. Only once the run lets go does the revoke go out, and it says how far
    //    the work had got.
    let mut stopped_at = None;
    for _ in 0..60 {
        for step in bob.tick().await.expect("bob ticks") {
            if let Step::Reclaimed {
                reason,
                stopped_at: p,
                ..
            } = step
            {
                assert_eq!(reason, ReclaimReason::ForegroundActivity);
                stopped_at = Some(p);
            }
        }
        if stopped_at.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    let stopped_at = stopped_at.expect("the reclaim completed");
    assert!(
        stopped_at.done < UNITS,
        "the run stopped mid-flight rather than finishing ({} of {UNITS})",
        stopped_at.done
    );
    assert_eq!(bob.progress(id), None, "nothing is in flight any more");
    assert!(
        matches!(
            phase(&bob, id, clock.now_ms()).await,
            LeasePhase::Reclaimed {
                reason: ReclaimReason::ForegroundActivity,
                ..
            }
        ),
        "a device fact, not a failed worker"
    );

    // 6. Alice's owner allows the resource. She is only let in once her *own*
    //    board has the reclaim: a device that grants on a board which has not
    //    caught up wins locally, loses to the real winner later, and throws the
    //    work away — correct, but not what this receipt is measuring.
    clock.advance(1_000);
    let now = clock.now_ms();
    let mut caught_up = false;
    for _ in 0..200 {
        alice.tick().await.expect("alice ticks");
        bob.tick().await.expect("bob ticks");
        if matches!(phase(&alice, id, now).await, LeasePhase::Reclaimed { .. }) {
            caught_up = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert!(caught_up, "the reclaim converged to alice");
    alice.set_policy(DevicePolicy::permissive());
    let (a, _) = pump(&mut alice, &mut bob, "alice finishes the job", |a, _| {
        a.iter()
            .any(|s| matches!(s, Step::Completed { lease: Some(_), .. }))
    })
    .await;
    assert!(
        a.iter()
            .any(|s| matches!(s, Step::Granted { epoch: 1, .. })),
        "alice took a fresh epoch rather than inheriting bob's lease: {a:#?}"
    );

    // 7. Both boards converge on the same terminal state, under the new lease.
    let mut converged = false;
    for _ in 0..200 {
        alice.tick().await.expect("alice ticks");
        bob.tick().await.expect("bob ticks");
        if matches!(
            phase(&bob, id, now).await,
            LeasePhase::Done { epoch: 1, .. }
        ) {
            converged = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert!(converged, "the completion reached the reclaimed device too");

    for (host, who) in [(&alice, "alice"), (&bob, "bob")] {
        let board = host.synced().board().await.unwrap();
        let job = board.job(id).expect("the job is known");
        assert!(
            matches!(job.state, JobState::Committed { winner, .. } if winner == alice_me),
            "{who} attributes the result to the device that finished it: {:?}",
            job.state
        );
        assert!(
            matches!(job.lease_at(now, &EXACT), LeasePhase::Done { epoch: 1, .. }),
            "{who} sees the job done under epoch 1"
        );
    }
}
