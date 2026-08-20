// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Distillery v0's projection proof: the port drives a real mesh host and
//! releases the blob custody that an accepted checkpoint settles.

use std::sync::Arc;
use std::time::Duration;

use distillery::{Distillery, RetentionSettings};
use identity::{IdentityProvider, InMemoryProvider};
use mesh::spec::{DeterminismClass, JobSpec};
use mesh::{
    AvailabilityPolicy, ErasurePolicy, KeepBound, LeasePolicy, MESH_AUTHOR_SALT, MeshEvent,
    MeshRetentionPolicy, MeshStore, PayloadRule, PolicyRevision, ResourceId, SyncedMesh,
};
use mesh_host::{HostConfig, ManualClock, MeshHost, Step, TransportBlobSpace};
use transport::{BlobStore, P2pandaTransport};

const MESH: [u8; 32] = [0xd1; 32];

fn retention(authority: [u8; 32]) -> MeshRetentionPolicy {
    MeshRetentionPolicy {
        revision: PolicyRevision([7; 32]),
        checkpoint_authority: authority,
        availability: AvailabilityPolicy {
            promised_floor: KeepBound::Forever,
        },
        erasure: ErasurePolicy {
            privacy_ceiling: KeepBound::UntilCheckpoint,
            terminal_job_payload: PayloadRule::EraseTerminalAtCheckpoint,
        },
        lease: LeasePolicy { max_skew_ms: 0 },
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_port_drives_the_host_and_collects_only_after_checkpoint() {
    let provider = InMemoryProvider::from_seed([0xd1; 32]);
    let author = provider.derive_keypair(MESH_AUTHOR_SALT).unwrap();
    let authority = author.public_key().to_bytes();
    let blobs = Arc::new(BlobStore::new_collecting(Duration::from_millis(10)));
    let transport = P2pandaTransport::builder(provider.master_keypair())
        .gossip()
        .blobs(&blobs)
        .bind()
        .await
        .expect("bind transport");
    let (endpoint, gossip) = transport.sync_parts().expect("sync parts");
    let synced = SyncedMesh::join(
        endpoint,
        gossip,
        MeshStore::in_memory_with_retention(retention(authority)),
        MESH,
    )
    .await
    .expect("join mesh");

    let space = Arc::new(TransportBlobSpace::for_mesh(blobs.clone(), MESH));
    let clock = Arc::new(ManualClock::at(1_000));
    let mut config = HostConfig::supervised(space.clone());
    config.clock = clock;
    let host = MeshHost::new(synced, author.clone(), config);
    let mut distillery = Distillery::new(
        host,
        space.clone(),
        RetentionSettings {
            collect_after_checkpoint: true,
        },
    );

    let input = space.put(b"distillery mash").await.expect("stage input");
    distillery
        .host()
        .synced()
        .author(
            &author,
            &MeshEvent::JobPostedV2 {
                spec: Box::new(JobSpec::simple(
                    ResourceId::parse("mesh.blake3/v1").unwrap(),
                    "payload",
                    input.clone(),
                    "result",
                    32,
                    DeterminismClass::Exact,
                )),
                nonce: 1,
                at_ms: 1_000,
            },
        )
        .await
        .expect("post job");

    let mut completed = false;
    for _ in 0..100 {
        let steps = distillery.tick().await.expect("authority tick");
        if steps
            .iter()
            .any(|step| matches!(step, Step::Completed { .. }))
        {
            completed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        completed,
        "Distillery drove the substrate job to completion"
    );
    assert!(
        space.has(&input).await,
        "completion alone retains the input"
    );

    let report = distillery.maintain().await.expect("checkpoint and collect");
    assert_eq!(report.candidates, 2, "input and distinct hash output");
    assert_eq!(report.collected, 2);
    assert_eq!(
        report.effects,
        [mesh::RetentionEffect::BlobCollected { count: 2 }]
    );
    assert!(
        distillery
            .maintain_if_advanced()
            .await
            .expect("idle maintenance")
            .is_none(),
        "the resident cadence must not author the same frontier twice"
    );

    for _ in 0..100 {
        if !space.has(&input).await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the collecting store kept an untagged settled input");
}
