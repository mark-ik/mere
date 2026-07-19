// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

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
//! The drain half (the loop, the [`SyncStatus`] counters, `resync`, the
//! task lifetime) is [`murm_replication::SyncedSpace`], shared with murm's cabal
//! sync. This module keeps only the mesh-specific parts: building the session
//! over the [`MeshStore`], the domain admission policy and addressed-mesh
//! guard, the authoring path, and the [`JobBoard`] fold.
//!
//! It is **endpoint-decoupled**: `join` takes the raw p2panda-net [`Endpoint`]
//! + [`Gossip`] (the host pulls them from its `P2pandaTransport` via
//! `sync_parts`), so the lib never builds a transport. The mesh id is the
//! LogSync topic (and the operation's signed addressing extension), so a
//! session syncs exactly that mesh's log.

use identity::Ed25519Keypair;
use muniment::Backend;
use murm_replication::{MunimentStore, SyncedSpace};
use p2panda_core::{Operation, SigningKey, Topic};
use p2panda_net::sync::SyncHandle;
use p2panda_net::{Endpoint, Gossip, LogSync};
use p2panda_sync::protocols::TopicLogSyncEvent;

use crate::board::JobBoard;
use crate::retention::RetentionCheckpoint;
use crate::store::{MeshStore, MeshStoreError};
use crate::wire::{MeshEvent, MeshExt, MeshLogId, to_operation, to_prune_operation};

// The shared drain's status + checkpoint types are re-exported so the mesh
// public surface (`mesh::SyncStatus` / `mesh::SyncRound`) stays stable.
pub use murm_replication::{SyncRound, SyncStatus};

/// The mesh's LogSync session type: the muniment-backed store, one log per
/// author per mesh (the log id is the mesh id), mesh extensions on every
/// operation.
type MeshLogSync<B> = LogSync<MunimentStore<B, MeshExt>, MeshLogId, MeshExt>;
type MeshSyncHandle = SyncHandle<Operation<MeshExt>, TopicLogSyncEvent<MeshExt>>;

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
/// shared [`SyncedSpace`] draining reconciled operations into the store.
/// Dropping it stops the drain and ends the session.
pub struct SyncedMesh<B: Backend + Clone + Send + 'static> {
    store: MeshStore<B>,
    mesh_id: [u8; 32],
    /// The shared LogSync drain: reconciled operations flow through the
    /// `accept` closure below into the store. Dropped first (aborts the task).
    space: SyncedSpace,
    /// The live lane: newly authored operations are pushed here so connected
    /// peers receive them without waiting for a reconciliation round.
    handle: MeshSyncHandle,
    /// Keeps the session actor alive; dropped with the struct (ends the session).
    _log_sync: MeshLogSync<B>,
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
        let log_sync = MeshLogSync::builder(store.sync_store(), endpoint, gossip)
            .spawn()
            .await
            .map_err(|e| MeshSyncError::Backend(format!("logsync spawn: {e}")))?;
        let handle = log_sync
            .stream(Topic::from(mesh_id), true)
            .await
            .map_err(|e| MeshSyncError::Backend(format!("logsync stream: {e}")))?;
        let sub = handle
            .subscribe()
            .await
            .map_err(|e| MeshSyncError::Backend(format!("logsync subscribe: {e}")))?;

        // Remote receipt and local authoring use the same shared processor.
        // `accept` counts an operation only when policy admits it and the atomic
        // indexed write reports it as new.
        let accept_store = store.clone();
        let space = SyncedSpace::drive(sub, move |op: Operation<MeshExt>| {
            let store = accept_store.clone();
            async move { matches!(store.accept(mesh_id, &op).await, Ok(true)) }
        });

        Ok(Self {
            store,
            mesh_id,
            space,
            handle,
            _log_sync: log_sync,
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
        self.handle
            .publish(op.clone())
            .map_err(|e| MeshSyncError::Backend(format!("publish: {e}")))?;
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
        self.space.sync_status()
    }

    /// Run a manual "sync now" checkpoint and report what arrived. Delegates to
    /// the shared drain's settle-based checkpoint: not a fake spinner and not a
    /// forced re-fetch (p2panda 0.6.1 exposes no "re-initiate" hook), it watches
    /// the live session until it goes quiet and returns the real count of
    /// operations that landed during the window. A quiet mesh reports `0` fast.
    pub async fn resync(&self) -> Result<SyncRound, MeshSyncError> {
        Ok(self.space.resync().await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{JobId, JobState};
    use crate::wire::JobKind;
    use crate::worker::{WorkerAction, execute, next_action};
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
        wait_for_board(&bob, |b| b.job(id).is_some(), "bob sees the posted job").await;
        let board = bob.board().await.unwrap();
        assert_eq!(next_action(&board, &bob_me), WorkerAction::Claim(id));
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
        assert_eq!(next_action(&board, &bob_me), WorkerAction::Execute(id));
        let job = board.job(id).unwrap();
        let result = execute(
            job.kind,
            job.payload.as_deref().expect("claimed job has payload"),
        );
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
}
