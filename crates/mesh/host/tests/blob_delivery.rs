// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gate H1's receipt: a job runs on a device that never had its inputs.
//!
//! Two supervised hosts with **disjoint** blob stores. Alice posts a job naming
//! bytes only she holds; Bob claims it, takes the lease, works out who to ask,
//! pulls the blob over the transport, and finishes. Nothing is staged on Bob in
//! advance — the assertion that he did not hold it is made before he starts.
//!
//! The part that was actually missing was never the transfer. A mesh operation
//! is signed by a key derived under `mesh-author`, while a transport addresses
//! the persona master key, and neither derives the other. `DeviceAttested`
//! closes that: each device publishes the master-signed statement binding its
//! own authoring key, and the board becomes a directory.

use std::sync::Arc;
use std::time::Duration;

use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};
use mesh::resources::DelayedResource;
use mesh::spec::{DeterminismClass, JobSpec};
use mesh::{
    DevicePolicy, HostFacts, JobId, JobState, LeasePolicy, LeaseTerms, MESH_AUTHOR_SALT, MeshEvent,
    MeshStore, ResourceId, ResourceRegistry, SyncedMesh,
};
use mesh_host::{
    HostConfig, ManualClock, MeshHost, ObservedConditions, Step, TransportBlobSpace,
    TransportCourier,
};
use muniment::MemoryBackend;
use proofs::BlobRef;
use std::collections::BTreeSet;
use transport::{BlobStore, P2pandaTransport};

const MESH: [u8; 32] = [0x4d; 32];
const UNITS: u64 = 8;
const INPUT: &[u8] = b"the bytes only the poster holds";
const EXACT: LeasePolicy = LeasePolicy { max_skew_ms: 0 };

type Host = MeshHost<MemoryBackend>;

struct Device {
    provider: InMemoryProvider,
    keypair: Ed25519Keypair,
    blobs: Arc<BlobStore>,
    transport: Arc<P2pandaTransport>,
}

impl Device {
    async fn bind(seed: u8) -> Self {
        let provider = InMemoryProvider::from_seed([seed; 32]);
        let keypair = provider.derive_keypair(MESH_AUTHOR_SALT).unwrap();
        let blobs = Arc::new(BlobStore::new());
        let transport = P2pandaTransport::builder(provider.master_keypair())
            .gossip()
            .blobs(&blobs)
            .bind()
            .await
            .expect("bind");
        Self {
            provider,
            keypair,
            blobs,
            transport: Arc::new(transport),
        }
    }

    fn peer_id(&self) -> transport::PeerID {
        transport::PeerID::from_public_key(self.provider.master_public_key())
    }

    async fn into_host(
        self,
        clock: Arc<ManualClock>,
        policy: DevicePolicy,
    ) -> (Host, Arc<TransportBlobSpace>) {
        let (endpoint, gossip) = self.transport.sync_parts().expect("sync parts");
        let synced = SyncedMesh::join(endpoint, gossip, MeshStore::in_memory(), MESH)
            .await
            .expect("join");

        // One store: the same iroh-blobs store the transport's router serves is
        // the space the restricted namespace reads from.
        let space = Arc::new(TransportBlobSpace::new(self.blobs.clone()));
        let mut config = HostConfig::supervised(space.clone());
        config.registry = registry();
        config.courier = Arc::new(TransportCourier::new(self.transport, self.blobs));
        config.clock = clock;
        config.conditions = Arc::new(ObservedConditions::spare());
        config.facts = HostFacts::cpu(4096);
        config.policy = policy;
        config.lease = EXACT;

        let host = MeshHost::new(synced, self.keypair, config);
        host.announce(self.provider.attest_derived_key(MESH_AUTHOR_SALT).unwrap())
            .await
            .expect("announce");
        (host, space)
    }
}

fn registry() -> ResourceRegistry {
    let mut registry = ResourceRegistry::new();
    registry
        .register(Arc::new(DelayedResource::new(UNITS, 0)))
        .unwrap();
    registry
}

fn leased_spec(input: BlobRef) -> JobSpec {
    JobSpec::simple(
        ResourceId::parse("mesh.delayed/v1").unwrap(),
        "payload",
        input,
        "result",
        64,
        DeterminismClass::Exact,
    )
    .leased(LeaseTerms::new(600_000, 60_000))
}

/// A device that will not take this job, so the poster does not race the worker
/// it is trying to hand the job to.
fn abstaining() -> DevicePolicy {
    DevicePolicy {
        allowed_resources: BTreeSet::from([ResourceId::parse("mesh.echo/v1").unwrap()]),
        ..DevicePolicy::permissive()
    }
}

async fn pump(
    alice: &mut Host,
    bob: &mut Host,
    what: &str,
    mut done: impl FnMut(&[Step], &[Step]) -> bool,
) -> (Vec<Step>, Vec<Step>) {
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for _ in 0..250 {
        a.extend(alice.tick().await.expect("alice ticks"));
        b.extend(bob.tick().await.expect("bob ticks"));
        if done(&a, &b) {
            return (a, b);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    panic!("timed out waiting for: {what}\nalice: {a:#?}\nbob: {b:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_worker_runs_a_job_whose_inputs_it_never_held() {
    let clock = Arc::new(ManualClock::at(1_000));
    let alice_device = Device::bind(80).await;
    let bob_device = Device::bind(81).await;

    // Cross-register so each transport can dial the other.
    let (alice_peer, bob_peer) = (alice_device.peer_id(), bob_device.peer_id());
    let overlay = transport::sync_overlay_topic(MESH);
    alice_device
        .transport
        .add_peer(bob_device.transport.endpoint_addr().await.unwrap())
        .await
        .unwrap();
    alice_device
        .transport
        .set_topics(bob_peer, &[overlay])
        .await
        .unwrap();
    bob_device
        .transport
        .add_peer(alice_device.transport.endpoint_addr().await.unwrap())
        .await
        .unwrap();
    bob_device
        .transport
        .set_topics(alice_peer, &[overlay])
        .await
        .unwrap();

    let alice_kp = alice_device.keypair.clone();
    let alice_me = alice_kp.public_key().to_bytes();
    let bob_me = bob_device.keypair.public_key().to_bytes();

    let (mut alice, alice_blobs) = alice_device.into_host(clock.clone(), abstaining()).await;
    let (mut bob, bob_blobs) = bob_device
        .into_host(clock.clone(), DevicePolicy::permissive())
        .await;

    // Only Alice holds the input. This is the whole premise, so assert it.
    let input = alice_blobs
        .put(INPUT)
        .await
        .expect("alice stages the input");
    assert_eq!(input, BlobRef::blake3(INPUT));
    assert!(alice_blobs.has(&input).await);
    assert!(
        !bob_blobs.has(&input).await,
        "bob has never seen these bytes"
    );

    let posted = alice
        .synced()
        .author(
            &alice_kp,
            &MeshEvent::JobPostedV2 {
                spec: Box::new(leased_spec(input.clone())),
                nonce: 1,
                at_ms: 1_000,
            },
        )
        .await
        .expect("alice posts");
    let id = JobId(*posted.hash.as_bytes());

    // The directory has to resolve before a fetch can be aimed anywhere.
    let mut resolved = None;
    for _ in 0..250 {
        alice.tick().await.expect("alice ticks");
        bob.tick().await.expect("bob ticks");
        resolved = bob
            .synced()
            .board()
            .await
            .unwrap()
            .devices()
            .master_of(&alice_me);
        if resolved.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert_eq!(
        resolved,
        Some(alice_peer.to_bytes()),
        "bob resolves alice's mesh author key to the master key his transport addresses \
         — the two are different keys, which is the gap this closes"
    );

    let (_, b) = pump(&mut alice, &mut bob, "bob completes the job", |_, b| {
        b.iter()
            .any(|s| matches!(s, Step::Completed { lease: Some(_), .. }))
    })
    .await;
    assert!(
        b.iter()
            .any(|s| matches!(s, Step::Granted { epoch: 0, .. })),
        "bob took the lease before fetching: {b:#?}"
    );

    // He has the bytes now, and they are the right bytes.
    assert!(
        bob_blobs.has(&input).await,
        "the input arrived over the transport"
    );
    assert_eq!(
        mesh::BlobSource::fetch(&*bob_blobs, &input).await.unwrap(),
        Some(INPUT.to_vec())
    );

    // And the result converges back to the poster.
    let mut converged = false;
    for _ in 0..250 {
        alice.tick().await.expect("alice ticks");
        bob.tick().await.expect("bob ticks");
        if alice
            .synced()
            .board()
            .await
            .unwrap()
            .job(id)
            .is_some_and(|job| job.state.is_terminal())
        {
            converged = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert!(converged, "the completion reached alice");

    for (host, who) in [(&alice, "alice"), (&bob, "bob")] {
        let board = host.synced().board().await.unwrap();
        let job = board.job(id).expect("the job is known");
        assert!(
            matches!(job.state, JobState::Committed { winner, .. } if winner == bob_me),
            "{who} attributes the result to bob: {:?}",
            job.state
        );
    }
}
