// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! D1 receipt: one resident lifecycle owns persistent mesh and blob storage.

use std::sync::Arc;
use std::time::Duration;

use distillery::{
    ResidentAuthority, ResidentReceipt, ResidentSettings, ResidentStorage, RetentionSettings,
};
use mesh::spec::{DeterminismClass, JobSpec};
use mesh::{
    AvailabilityPolicy, ErasurePolicy, KeepBound, LeasePolicy, MESH_AUTHOR_SALT, MeshEvent,
    MeshRetentionPolicy, MeshStore, PayloadRule, PolicyRevision, ResourceId, SyncedMesh,
};
use mesh_host::{HostConfig, MeshHost, Step};
use personae::{IdentityProvider, InMemoryProvider};
use transport::{BlobHash, BlobStore, P2pandaTransport};


// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! D1 receipt: one resident lifecycle owns persistent mesh and blob storage.
};
};

const MESH: [u8; 32] = [0xd2; 32];

fn retention(authority: [u8; 32]) -> MeshRetentionPolicy {
    MeshRetentionPolicy {
        revision: PolicyRevision([8; 32]),
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
async fn resident_lifecycle_ticks_maintains_persists_and_closes() {
    let directory = tempfile::tempdir().expect("temporary resident root");
    let blob_root = directory.path().join("blobs");
    let mesh_path = directory.path().join("mesh.redb");
    let settings = ResidentSettings {
        tick_every: Duration::from_millis(5),
        maintenance_every: Some(Duration::from_millis(20)),
        blob_gc_every: Duration::from_millis(10),
        retention: RetentionSettings {
            collect_after_checkpoint: true,
        },
    };
    let storage = ResidentStorage::open(&blob_root, MESH, settings.blob_gc_every)
        .await
        .expect("open resident storage");
    assert!(storage.blobs().is_persistent());

    let provider = InMemoryProvider::from_seed([0xd2; 32]);
    let author = provider.derive_keypair(MESH_AUTHOR_SALT).unwrap();
    let policy = retention(author.public_key().to_bytes());
    let blobs = storage.blobs();
    let space = storage.space();
    let transport = Arc::new(
        P2pandaTransport::builder(provider.master_keypair())
            .gossip()
            .blobs(&blobs)
            .bind()
            .await
            .expect("bind resident transport"),
    );
    let (endpoint, gossip) = transport.sync_parts().expect("sync parts");
    let synced = SyncedMesh::join(
        endpoint,
        gossip,
        MeshStore::at_path_with_retention(&mesh_path, policy.clone())
            .expect("open resident mesh store"),
        MESH,
    )
    .await
    .expect("join resident mesh");
    let host = MeshHost::new(
        synced,
        author.clone(),
        HostConfig::supervised(space.clone()),
    );
    let mut resident = ResidentAuthority::new(host, transport, storage, settings)
        .expect("compose resident authority");

    let input = space
        .put(b"persistent distillery mash")
        .await
        .expect("stage input");
    resident
        .authority()
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
                at_ms: 1,
            },
        )
        .await
        .expect("post resident job");

    let mut receipts = Vec::new();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let mut stop_tx = Some(stop_tx);
    let mut maintenance_completed = false;
    resident
        .run_until(
            async move {
                tokio::select! {
                    _ = stop_rx => {}
                    _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                }
            },
            |receipt| {
                match &receipt {
                    ResidentReceipt::MaintenanceCompleted(_) => maintenance_completed = true,
                    ResidentReceipt::MaintenanceIdle if maintenance_completed => {
                        if let Some(stop_tx) = stop_tx.take() {
                            let _ = stop_tx.send(());
                        }
                    }
                    _ => {}
                }
                receipts.push(receipt);
            },
        )
        .await
        .expect("run resident authority");

    assert!(
        receipts.iter().any(|receipt| matches!(
            receipt,
            ResidentReceipt::Tick { steps }
                if steps.iter().any(|step| matches!(step, Step::Completed { .. }))
        )),
        "the resident cadence drove the real job to completion"
    );
    assert!(
        receipts.iter().any(|receipt| matches!(
            receipt,
            ResidentReceipt::MaintenanceCompleted(report)
                if report.candidates == 2 && report.collected == 2
        )),
        "scheduled maintenance released both settled custody tags"
    );
    assert!(
        receipts
            .iter()
            .any(|receipt| matches!(receipt, ResidentReceipt::MaintenanceIdle)),
        "later cadence turns report an unchanged frontier without writing it"
    );
    assert!(matches!(
        receipts.last(),
        Some(ResidentReceipt::StopRequested)
    ));

    for _ in 0..100 {
        if !space.has(&input).await {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        !space.has(&input).await,
        "released resident bytes were collected"
    );
    drop(blobs);
    drop(space);
    resident.shutdown().await.expect("clean resident shutdown");

    let reopened_mesh =
        MeshStore::at_path_with_retention(&mesh_path, policy).expect("reopen resident mesh store");
    let board = reopened_mesh
        .board(MESH)
        .await
        .expect("replay durable board");
    assert!(
        board.jobs().any(|job| job.state.is_terminal()),
        "the completed job survived the authority restart boundary"
    );

    let reopened_blobs = BlobStore::open_collecting(&blob_root, Duration::from_millis(10))
        .await
        .expect("reopen resident blobs");
    assert!(reopened_blobs.is_persistent());
    let input_hash = BlobHash::from_bytes(input.digest.as_32().expect("blake3 input reference"));
    assert!(
        !reopened_blobs
            .has(input_hash)
            .await
            .expect("query reopened blobs"),
        "released input stayed absent after the restart boundary"
    );
    reopened_blobs
        .shutdown()
        .await
        .expect("close reopened blobs");
}

#[test]
fn resident_settings_refuse_zero_cadences() {
    let settings = ResidentSettings {
        tick_every: Duration::ZERO,
        maintenance_every: None,
        blob_gc_every: Duration::from_secs(1),
        retention: RetentionSettings::default(),
    };
    assert!(settings.validate().is_err());
}
