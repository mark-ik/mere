// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Mesh sync over LogSync — the personal space's job log, replicated.
//!
//! [`SyncedMesh`] runs the offline-catch-up + live lane for one mesh's job
//! log: a `LogSync` (RBSR) session over the [`MeshStore`], reconciling
//! operations a device missed while away and passing each received operation
//! through the shared policy-before-insert processor. Where a receive-only
//! consumer just folds, a mesh peer also *speaks*:
//! [`author`](SyncedMesh::author) signs an event at the device's next log
//! position, passes it through that same processor, and pushes a newly inserted
//! operation onto the live lane so connected peers see it now. RBSR covers
//! whoever was away.
//!
//! The join ceremony and drain (session + live lane + loop, the
//! [`SyncStatus`] counters, `resync`, the task lifetimes) are
//! [`stickleback::JoinedSpace`], shared with murm's cabal sync. This
//! module keeps only the mesh-specific parts: the domain admission policy and
//! addressed-mesh guard, the authoring path, and the [`JobBoard`] fold.
//!
//! It is **endpoint-decoupled**: `join` takes the raw p2panda-net
//! [`Endpoint`] and [`Gossip`] (the host pulls them from its
//! `P2pandaTransport` via `sync_parts`), so the lib never builds a transport.
//! The mesh id is the LogSync topic (and the operation's signed addressing
//! extension), so a session syncs exactly that mesh's log.

use identity::Ed25519Keypair;
use muniment::Backend;
use p2panda_core::{Operation, SigningKey};
use p2panda_net::{Endpoint, Gossip};
use stickleback::JoinedSpace;

use crate::board::JobBoard;
use crate::retention::RetentionCheckpoint;
use crate::store::{MeshStore, MeshStoreError};
use crate::wire::{MeshEvent, MeshExt, MeshLogId, to_operation, to_prune_operation};

// The shared drain's status + checkpoint types are re-exported so the mesh
// public surface (`mesh::SyncStatus` / `mesh::SyncRound`) stays stable.
pub use stickleback::{SyncRound, SyncStatus};

/// A mesh sync failure (LogSync session setup, publish, or the store).
#[derive(Debug, thiserror::Error)]
pub enum MeshSyncError {
    #[error("mesh sync: {0}")]
    Backend(String),
    #[error(transparent)]
    Store(#[from] MeshStoreError),
}

/// A device joined to a mesh's LogSync session.
///
/// Holds the store, the live publish lane, the session (kept alive), and the
/// shared [`JoinedSpace`] draining reconciled operations into the store.
/// Dropping it stops the drain and ends the session.
pub struct SyncedMesh<B: Backend + Clone + Send + 'static> {
    store: MeshStore<B>,
    mesh_id: [u8; 32],
    /// The joined LogSync session (session + live lane + drain, drop-ordered):
    /// reconciled operations flow through the `accept` closure below into the
    /// store, and authored operations publish onto the live lane.
    joined: JoinedSpace<MeshExt>,
}

impl<B: Backend + Clone + Send + Sync + 'static> SyncedMesh<B> {
    /// Join a mesh's LogSync session over `store`, driven by the host's
    /// `endpoint` + `gossip` (from its transport's `sync_parts`).
    ///
    /// Spawns a background task that reconciles the mesh's log and drains
    /// each received operation into the store (verifying the signature and
    /// the addressed mesh, idempotent on the hash). The session syncs the
    /// topic `Topic::from(mesh_id)` in live mode.
    pub async fn join(
        endpoint: Endpoint,
        gossip: Gossip,
        store: MeshStore<B>,
        mesh_id: [u8; 32],
    ) -> Result<Self, MeshSyncError> {
        // Remote receipt and local authoring use the same shared processor.
        // `accept` counts an operation only when policy admits it and the atomic
        // indexed write reports it as new.
        let accept_store = store.clone();
        let joined = JoinedSpace::join::<_, MeshLogId, _, _>(
            stickleback::lane_id("mesh/v1", mesh_id),
            store.sync_store(),
            endpoint,
            gossip,
            mesh_id,
            move |op: Operation<MeshExt>| {
                let store = accept_store.clone();
                async move { matches!(store.accept(mesh_id, &op).await, Ok(true)) }
            },
        )
        .await
        .map_err(|e| MeshSyncError::Backend(e.to_string()))?;

        Ok(Self {
            store,
            mesh_id,
            joined,
        })
    }

    /// The mesh's operation store.
    pub fn store(&self) -> &MeshStore<B> {
        &self.store
    }

    /// The mesh this session syncs.
    pub fn mesh_id(&self) -> [u8; 32] {
        self.mesh_id
    }

    /// Sign `event` at `keypair`'s next log position, persist it, and push it
    /// onto the live lane. Returns the operation (its hash is the job id when
    /// the event is a `JobPosted`).
    ///
    /// One authoring path per device key: concurrent `author` calls with the
    /// same keypair would race the log position. M1 hosts (the peer bin, the
    /// P6 actor) drive one job loop per device, which satisfies this by
    /// construction.
    pub async fn author(
        &self,
        keypair: &Ed25519Keypair,
        event: &MeshEvent,
    ) -> Result<Operation<MeshExt>, MeshSyncError> {
        self.author_event(keypair, event, false).await
    }

    async fn author_event(
        &self,
        keypair: &Ed25519Keypair,
        event: &MeshEvent,
        prune: bool,
    ) -> Result<Operation<MeshExt>, MeshSyncError> {
        let author = SigningKey::from_bytes(&keypair.to_seed()).verifying_key();
        let latest = self.store.latest_in(&author, event.log_id()).await?;
        let (seq_num, backlink) = match latest {
            Some(prev) => (prev.header.seq_num + 1, Some(*prev.hash.as_bytes())),
            None => (0, None),
        };
        let op = if prune {
            let MeshEvent::HistoryPruned { checkpoint, at_ms } = event else {
                return Err(MeshSyncError::Backend(
                    "only a history-pruned event can request prefix pruning".into(),
                ));
            };
            to_prune_operation(
                keypair,
                self.mesh_id,
                *checkpoint,
                *at_ms,
                seq_num,
                backlink,
            )
        } else {
            to_operation(keypair, self.mesh_id, event, seq_num, backlink)
        };
        if !self.store.accept(self.mesh_id, &op).await? {
            return Err(MeshSyncError::Backend(
                "authored operation was already present".into(),
            ));
        }
        self.joined
            .publish(op.clone())
            .map_err(|e| MeshSyncError::Backend(e.to_string()))?;
        Ok(op)
    }

    /// Author an owner checkpoint from the current board and event-log frontier.
    pub async fn checkpoint(
        &self,
        keypair: &Ed25519Keypair,
        at_ms: u64,
    ) -> Result<(Operation<MeshExt>, RetentionCheckpoint), MeshSyncError> {
        let checkpoint = self.store.build_checkpoint(self.mesh_id, at_ms).await?;
        let operation = self
            .author_event(
                keypair,
                &MeshEvent::RetentionCheckpoint {
                    checkpoint: Box::new(checkpoint.clone()),
                },
                false,
            )
            .await?;
        Ok((operation, checkpoint))
    }

    /// Prune this author's event-log prefix beneath the latest checkpoint.
    pub async fn prune_history(
        &self,
        keypair: &Ed25519Keypair,
        at_ms: u64,
    ) -> Result<Operation<MeshExt>, MeshSyncError> {
        let checkpoint = self
            .store
            .latest_checkpoint(self.mesh_id)
            .await?
            .ok_or_else(|| MeshSyncError::Backend("no accepted retention checkpoint".into()))?;
        self.author_event(
            keypair,
            &MeshEvent::HistoryPruned {
                checkpoint: checkpoint.operation,
                at_ms,
            },
            true,
        )
        .await
    }

    /// Fold the synced store into the mesh's [`JobBoard`] — the same board on
    /// every converged peer.
    pub async fn board(&self) -> Result<JobBoard, MeshStoreError> {
        self.store.board(self.mesh_id).await
    }

    /// A snapshot of this mesh's sync activity, for a real sync indicator:
    /// whether a round is in progress, rounds finished, operations received,
    /// and when activity last happened.
    pub fn sync_status(&self) -> SyncStatus {
        self.joined.sync_status()
    }

    /// Run a manual "sync now" checkpoint and report what arrived. Delegates to
    /// the shared drain's settle-based checkpoint: not a fake spinner and not a
    /// forced re-fetch (p2panda 0.6.1 exposes no "re-initiate" hook), it watches
    /// the live session until it goes quiet and returns the real count of
    /// operations that landed during the window. A quiet mesh reports `0` fast.
    pub async fn resync(&self) -> Result<SyncRound, MeshSyncError> {
        Ok(self.joined.resync().await)
    }

    /// Leave the sync lane and wait until its durable store handles are free.
    pub async fn leave(self) -> Result<(), MeshSyncError> {
        self.joined
            .leave_and_wait()
            .await
            .map_err(|error| MeshSyncError::Backend(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{JobId, JobState};
    use crate::ident::ResourceId;
    use crate::namespace::MemoryBlobSpace;
    use crate::policy::DevicePolicy;
    use crate::registry::{ResourceRegistry, Verdict, run_job, run_legacy, verify_output};
    use crate::resource::JobControl;
    use crate::spec::{DeterminismClass, HostFacts, JobSpec};
    use crate::wire::JobKind;
    use crate::worker::{HostOffer, WorkerAction, next_action};
    use identity::{IdentityProvider, InMemoryProvider};
    use muniment::MemoryBackend;
    use std::sync::Arc as StdArc;
    use std::time::Duration;
    use transport::P2pandaTransport;

    const MESH: [u8; 32] = [0x77; 32];

    /// Two bound transports tagged with each other on the mesh's overlay
    /// topic (the tessera two-peer bootstrap, verbatim).
    async fn two_peers() -> (P2pandaTransport, P2pandaTransport) {
        let alice_provider = StdArc::new(InMemoryProvider::from_seed([60; 32]));
        let bob_provider = StdArc::new(InMemoryProvider::from_seed([61; 32]));
        let alice_id = transport::PeerID::from_public_key(alice_provider.master_public_key());
        let bob_id = transport::PeerID::from_public_key(bob_provider.master_public_key());

        let alice_t = P2pandaTransport::builder(alice_provider.master_keypair())
            .gossip()
            .bind()
            .await
            .expect("bind alice");
        let bob_t = P2pandaTransport::builder(bob_provider.master_keypair())
            .gossip()
            .bind()
            .await
            .expect("bind bob");

        let overlay = transport::sync_overlay_topic(MESH);
        alice_t
            .add_peer(bob_t.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        alice_t.set_topics(bob_id, &[overlay]).await.unwrap();
        bob_t
            .add_peer(alice_t.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        bob_t.set_topics(alice_id, &[overlay]).await.unwrap();
        (alice_t, bob_t)
    }

    async fn join(t: &P2pandaTransport) -> SyncedMesh<MemoryBackend> {
        let (ep, gossip) = t.sync_parts().expect("sync parts");
        let store = MeshStore::in_memory();
        SyncedMesh::join(ep, gossip, store, MESH)
            .await
            .expect("join")
    }

    /// Poll `mesh`'s board until `pred` holds (or the timeout trips).
    async fn wait_for_board(
        mesh: &SyncedMesh<MemoryBackend>,
        pred: impl Fn(&JobBoard) -> bool,
        what: &str,
    ) {
        let outcome = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if pred(&mesh.board().await.unwrap()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        })
        .await;
        assert!(outcome.is_ok(), "timed out waiting for: {what}");
    }

    /// The milestone-1 round-trip, in process: A posts a job, B claims and
    /// executes it over the live lane, and the result lands back on A.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_job_posted_on_one_peer_is_worked_by_the_other() {
        let (alice_t, bob_t) = two_peers().await;
        let alice = join(&alice_t).await;
        let bob = join(&bob_t).await;

        let alice_kp = InMemoryProvider::from_seed([60; 32])
            .derive_keypair(b"mesh-author")
            .unwrap();
        let bob_kp = InMemoryProvider::from_seed([61; 32])
            .derive_keypair(b"mesh-author")
            .unwrap();
        let bob_me = bob_kp.public_key().to_bytes();

        // A posts.
        let posted = alice
            .author(
                &alice_kp,
                &MeshEvent::JobPosted {
                    kind: JobKind::Echo,
                    payload: b"ping mesh".to_vec(),
                    nonce: 1,
                    at_ms: 10,
                },
            )
            .await
            .expect("alice posts");
        let id = JobId(*posted.hash.as_bytes());

        // B sees the job and its worker loop runs it: claim, execute, return.
        let registry = ResourceRegistry::builtin();
        let policy = DevicePolicy::permissive();
        let offer = HostOffer::new(&registry, HostFacts::cpu(4096), &policy);
        wait_for_board(&bob, |b| b.job(id).is_some(), "bob sees the posted job").await;
        let board = bob.board().await.unwrap();
        assert_eq!(
            next_action(&board, &bob_me, &offer),
            WorkerAction::Claim(id)
        );
        bob.author(
            &bob_kp,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 20,
            },
        )
        .await
        .expect("bob claims");

        wait_for_board(
            &bob,
            |b| matches!(b.job(id).map(|j| &j.state), Some(JobState::Claimed { winner }) if *winner == bob_me),
            "bob's claim wins on his own board",
        )
        .await;
        let board = bob.board().await.unwrap();
        assert_eq!(
            next_action(&board, &bob_me, &offer),
            WorkerAction::Execute(id)
        );
        let job = board.job(id).unwrap();
        let (_cancel, control) = JobControl::new();
        let result = run_legacy(
            &registry,
            job.kind.expect("an M1 job carries its kind"),
            job.payload.as_deref().expect("claimed job has payload"),
            &control,
        )
        .await
        .expect("bob runs the legacy job through the V2 route");
        bob.author(
            &bob_kp,
            &MeshEvent::JobDone {
                job: id.0,
                result: result.clone(),
                at_ms: 30,
            },
        )
        .await
        .expect("bob returns the result");

        // The result lands back on A: the milestone's literal shape.
        wait_for_board(
            &alice,
            |b| {
                matches!(
                    b.job(id).map(|j| &j.state),
                    Some(JobState::Done { winner, result: r }) if *winner == bob_me && r == b"ping mesh"
                )
            },
            "alice receives bob's result",
        )
        .await;

        // Real (non-placebo) sync feedback on both sides.
        assert!(
            alice.sync_status().ops_received >= 2,
            "alice received bob's claim + done (got {})",
            alice.sync_status().ops_received
        );
        assert!(
            bob.sync_status().ops_received >= 1,
            "bob received alice's post (got {})",
            bob.sync_status().ops_received
        );
        assert!(alice.sync_status().last_activity_ms.is_some());

        // The manual checkpoint settles quickly on a synced mesh.
        let round = alice.resync().await.expect("resync runs");
        println!(
            "mesh resync checkpoint: items_received={}",
            round.items_received
        );
    }

    /// The offline-catch-up lane: A holds a posted job *before* B connects;
    /// B's empty store converges over RBSR (no live publish involved).
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_late_joiner_catches_up_on_an_existing_log() {
        let (alice_t, bob_t) = two_peers().await;

        let alice_kp = InMemoryProvider::from_seed([60; 32])
            .derive_keypair(b"mesh-author")
            .unwrap();
        let alice_store = MeshStore::in_memory();
        let posted = to_operation(
            &alice_kp,
            MESH,
            &MeshEvent::JobPosted {
                kind: JobKind::Blake3,
                payload: b"catch me up".to_vec(),
                nonce: 2,
                at_ms: 5,
            },
            0,
            None,
        );
        alice_store.insert(&posted).await.unwrap();
        let id = JobId(*posted.hash.as_bytes());

        let (a_ep, a_gossip) = alice_t.sync_parts().expect("alice sync parts");
        let _alice = SyncedMesh::join(a_ep, a_gossip, alice_store, MESH)
            .await
            .expect("alice join");
        let bob = join(&bob_t).await;

        wait_for_board(&bob, |b| b.job(id).is_some(), "bob catches up on the job").await;
        let status = bob.sync_status();
        assert!(
            status.ops_received >= 1,
            "sync status recorded the caught-up op (got {})",
            status.ops_received
        );
        assert!(status.last_activity_ms.is_some());
    }

    /// The M2 receipt: a blob-backed lexical embedding job crosses two peers,
    /// runs inside a restricted namespace, converges as a content-addressed
    /// result, and verifies on a local re-run — while a blob the job did not
    /// name stays unreadable on the very device holding it.
    ///
    /// Blob *transport* is deliberately not proven here: moving bytes between
    /// devices is the host's job (drop export, a fetch lane), and M2's claim is
    /// about the namespace, not the courier. The test stages the granted input
    /// on both devices to stand in for whatever courier a host uses.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_blob_backed_lexical_job_crosses_two_peers_and_verifies() {
        use crate::namespace::{JobNamespaceView, NamespaceError};
        use crate::resources::LexicalBatch;

        let (alice_t, bob_t) = two_peers().await;
        let alice = join(&alice_t).await;
        let bob = join(&bob_t).await;

        let alice_kp = InMemoryProvider::from_seed([62; 32])
            .derive_keypair(b"mesh-author")
            .unwrap();
        let bob_kp = InMemoryProvider::from_seed([63; 32])
            .derive_keypair(b"mesh-author")
            .unwrap();
        let bob_me = bob_kp.public_key().to_bytes();

        // 1. Alice stores a batch and posts its address — not its bytes.
        let batch = LexicalBatch::new(
            64,
            vec![
                "async rust programming".to_string(),
                "rust runtime internals".to_string(),
                "italian dinner recipes".to_string(),
            ],
        )
        .encode();
        let alice_blobs = MemoryBlobSpace::in_memory();
        let input = alice_blobs.put(&batch).await.unwrap();
        let spec = JobSpec::simple(
            ResourceId::parse("esp.embed.lexical/v1").unwrap(),
            "texts",
            input,
            "vectors",
            64 * 1024,
            DeterminismClass::Exact,
        );
        let posted = alice
            .author(
                &alice_kp,
                &MeshEvent::JobPostedV2 {
                    spec: Box::new(spec.clone()),
                    nonce: 1,
                    at_ms: 10,
                },
            )
            .await
            .expect("alice posts the V2 job");
        let id = JobId(*posted.hash.as_bytes());

        // 2. Bob advertises the resource and claims the job.
        let registry = ResourceRegistry::builtin();
        let policy = DevicePolicy::permissive();
        let offer = HostOffer::new(&registry, HostFacts::cpu(4096), &policy);
        let bob_blobs = MemoryBlobSpace::in_memory();
        bob_blobs.put(&batch).await.unwrap();
        let private = bob_blobs.put(b"bob's private notes").await.unwrap();

        wait_for_board(&bob, |b| b.job(id).is_some(), "bob sees the V2 job").await;
        let board = bob.board().await.unwrap();
        assert_eq!(
            next_action(&board, &bob_me, &offer),
            WorkerAction::Claim(id)
        );
        bob.author(
            &bob_kp,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 20,
            },
        )
        .await
        .expect("bob claims");
        wait_for_board(
            &bob,
            |b| matches!(b.job(id).map(|j| &j.state), Some(JobState::Claimed { winner }) if *winner == bob_me),
            "bob's claim wins on his own board",
        )
        .await;

        // 3. Bob's host grants a namespace for exactly this job and runs it.
        let board = bob.board().await.unwrap();
        assert_eq!(
            next_action(&board, &bob_me, &offer),
            WorkerAction::Execute(id)
        );
        let granted = board
            .job(id)
            .unwrap()
            .spec
            .clone()
            .expect("a V2 job carries its spec");
        let (_cancel, control) = JobControl::new();
        let output = run_job(&registry, &granted, &bob_blobs, &bob_blobs, &control)
            .await
            .expect("bob runs the lexical job");

        // 6. The private blob is on this device and still out of reach.
        assert!(bob_blobs.has(&private).await.unwrap(), "bob holds the blob");
        let view = JobNamespaceView::grant(&granted, &bob_blobs, &bob_blobs);
        assert_eq!(
            view.read("notes").await,
            Err(NamespaceError::UngrantedInput("notes".to_string())),
            "holding bytes locally does not make them reachable from a job"
        );
        assert_eq!(view.input_names().collect::<Vec<_>>(), ["texts"]);

        // 4. The result converges to Alice as a committed output.
        bob.author(
            &bob_kp,
            &MeshEvent::JobDoneV2 {
                job: id.0,
                output: Box::new(output.clone()),
                at_ms: 30,
            },
        )
        .await
        .expect("bob commits the result");
        let expected = output.blob.clone();
        wait_for_board(
            &alice,
            move |b| {
                matches!(
                    b.job(id).map(|j| &j.state),
                    Some(JobState::Committed { winner, output: o })
                        if *winner == bob_me && o.blob == expected
                )
            },
            "alice receives bob's committed output",
        )
        .await;

        // 5. Alice re-runs it locally and the declared class holds.
        assert_eq!(
            verify_output(
                &registry,
                &spec,
                &output,
                &alice_blobs,
                &alice_blobs,
                &control
            )
            .await
            .unwrap(),
            Verdict::Reproduced {
                by: crate::ident::ImplementationId::parse("mesh.lexical.fnv1a/v1").unwrap()
            }
        );
    }

    /// The M3 convergence receipt: an owner reclaim authored on one device
    /// arrives at the other as a *reclaim*, the job reopens at the next epoch,
    /// and the second device finishes it. Every lease timestamp is authored, so
    /// only sync — not the clock — is what the test waits on.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn an_owner_reclaim_converges_and_the_other_peer_finishes_the_job() {
        use crate::lease::{LeaseTerms, ReclaimReason};
        use crate::projection::{LeasePhase, LeasePolicy};
        use crate::resources::DelayedResource;
        use proofs::BlobRef;

        const LEASE_MS: u64 = 60_000;
        let exact = LeasePolicy { max_skew_ms: 0 };

        let (alice_t, bob_t) = two_peers().await;
        let alice = join(&alice_t).await;
        let bob = join(&bob_t).await;

        let alice_kp = InMemoryProvider::from_seed([64; 32])
            .derive_keypair(b"mesh-author")
            .unwrap();
        let bob_kp = InMemoryProvider::from_seed([65; 32])
            .derive_keypair(b"mesh-author")
            .unwrap();
        let alice_me = alice_kp.public_key().to_bytes();
        let bob_me = bob_kp.public_key().to_bytes();

        // Alice posts a lendable job. Both devices hold the input.
        let spec = JobSpec::simple(
            ResourceId::parse("mesh.delayed/v1").unwrap(),
            "payload",
            BlobRef::blake3(b"seed"),
            "result",
            64,
            DeterminismClass::Exact,
        )
        .leased(LeaseTerms::new(LEASE_MS, 10_000));
        let posted = alice
            .author(
                &alice_kp,
                &MeshEvent::JobPostedV2 {
                    spec: Box::new(spec.clone()),
                    nonce: 1,
                    at_ms: 0,
                },
            )
            .await
            .expect("alice posts a leased job");
        let id = JobId(*posted.hash.as_bytes());

        // Bob claims and grants himself a lease inside Alice's envelope.
        wait_for_board(&bob, |b| b.job(id).is_some(), "bob sees the leased job").await;
        bob.author(
            &bob_kp,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 1_000,
            },
        )
        .await
        .expect("bob claims");
        wait_for_board(
            &bob,
            |b| b.job(id).is_some_and(|job| job.has_claimed(&bob_me)),
            "bob's claim lands on his own board",
        )
        .await;
        let grant = bob
            .author(
                &bob_kp,
                &MeshEvent::LeaseGranted {
                    job: id.0,
                    epoch: 0,
                    granted_at_ms: 2_000,
                    expires_at_ms: 2_000 + LEASE_MS,
                },
            )
            .await
            .expect("bob grants himself the lease");
        let lease = crate::lease::LeaseId(*grant.hash.as_bytes());

        // Alice sees the lease as held by Bob, not as her own to take.
        wait_for_board(
            &alice,
            move |b| {
                b.job(id)
                    .is_some_and(|job| job.lease_at(3_000, &exact).held_by(&bob_me))
            },
            "alice sees bob holding the lease",
        )
        .await;

        // Bob's owner takes the device back mid-lease.
        bob.author(
            &bob_kp,
            &MeshEvent::LeaseRevokedByOwner {
                job: id.0,
                lease: lease.0,
                reason: ReclaimReason::ForegroundActivity,
                at_ms: 6_000,
            },
        )
        .await
        .expect("bob's owner reclaims the device");
        wait_for_board(
            &alice,
            move |b| {
                matches!(
                    b.job(id).map(|job| job.lease_at(7_000, &exact)),
                    Some(LeasePhase::Reclaimed {
                        reason: ReclaimReason::ForegroundActivity,
                        ..
                    })
                )
            },
            "the reclaim converges to alice as a reclaim, not a failure",
        )
        .await;

        // Alice takes the next epoch and finishes the job.
        alice
            .author(
                &alice_kp,
                &MeshEvent::JobClaimed {
                    job: id.0,
                    at_ms: 7_000,
                },
            )
            .await
            .expect("alice re-claims");
        wait_for_board(
            &alice,
            move |b| {
                b.job(id)
                    .is_some_and(|job| job.next_holder(7_500) == Some(alice_me))
            },
            "alice is the eligible winner for epoch 1",
        )
        .await;
        let second_grant = alice
            .author(
                &alice_kp,
                &MeshEvent::LeaseGranted {
                    job: id.0,
                    epoch: 1,
                    granted_at_ms: 7_500,
                    expires_at_ms: 7_500 + LEASE_MS,
                },
            )
            .await
            .expect("alice grants epoch 1");

        let registry = ResourceRegistry::builtin();
        let blobs = MemoryBlobSpace::in_memory();
        blobs.put(b"seed").await.unwrap();
        let (_cancel, control) = JobControl::new();
        let output = run_job(&registry, &spec, &blobs, &blobs, &control)
            .await
            .expect("alice runs the delayed job");
        assert_eq!(
            blobs.get(&output.blob).await.unwrap(),
            Some(DelayedResource::expected(b"seed", 16))
        );
        alice
            .author(
                &alice_kp,
                &MeshEvent::JobCompletedUnderLease {
                    job: id.0,
                    lease: *second_grant.hash.as_bytes(),
                    output: Box::new(output.clone()),
                    at_ms: 9_000,
                },
            )
            .await
            .expect("alice commits the result under her lease");

        // Both peers converge on the same terminal state, attributed to Alice.
        let expected = output.blob.clone();
        for (peer, who) in [(&alice, "alice"), (&bob, "bob")] {
            let expected = expected.clone();
            wait_for_board(
                peer,
                move |b| {
                    matches!(
                        b.job(id).map(|job| &job.state),
                        Some(JobState::Committed { winner, output: o })
                            if *winner == alice_me && o.blob == expected
                    ) && matches!(
                        b.job(id).map(|job| job.lease_at(10_000, &exact)),
                        Some(LeasePhase::Done { epoch: 1, .. })
                    )
                },
                who,
            )
            .await;
        }
    }
}
