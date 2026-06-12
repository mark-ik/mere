/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Mesh sync over LogSync — the personal space's job log, replicated.
//!
//! [`SyncedMesh`] runs the offline-catch-up + live lane for one mesh's job
//! log, mirroring tessera's `SyncedMoot`: a `LogSync` (RBSR) session over the
//! [`MeshStore`], reconciling operations a device missed while away and
//! draining each received operation into the store (verified + idempotent).
//! Where the tessera session only *receives* (scores are read-side), a mesh
//! peer also *speaks*: [`author`](SyncedMesh::author) signs an event at the
//! device's next log position, persists it, and pushes it onto the live
//! gossip lane so connected peers see it now — RBSR covers whoever was away.
//!
//! It is **decoupled from the host transport**: `join` takes the raw
//! p2panda-net [`Endpoint`] + [`Gossip`] (the host pulls them from its
//! `P2pandaTransport` via `sync_parts`). The mesh id is the LogSync topic
//! (and the operation's signed addressing extension), so a session syncs
//! exactly that mesh's log.
//!
//! [`board`](SyncedMesh::board) folds the synced store into the
//! [`JobBoard`] every peer agrees on; [`sync_status`](SyncedMesh::sync_status)
//! is real (non-placebo) feedback: rounds finished, operations caught up,
//! last activity.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use identity::Ed25519Keypair;
use p2panda_core::{Operation, SigningKey, Topic};
use p2panda_net::sync::SyncHandle;
use p2panda_net::{Endpoint, Gossip, LogSync};
use p2panda_store::SqliteStore;
use p2panda_sync::protocols::TopicLogSyncEvent;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

use crate::board::JobBoard;
use crate::store::{MeshStore, MeshStoreError};
use crate::wire::{to_operation, verify, MeshEvent, MeshExt};

/// The mesh's LogSync session type: the SQLite store, one log per author per
/// mesh (the log id is the mesh id), mesh extensions on every operation.
type MeshLogSync = LogSync<SqliteStore, [u8; 32], MeshExt>;
type MeshSyncHandle = SyncHandle<Operation<MeshExt>, TopicLogSyncEvent<MeshExt>>;

/// Unix-epoch milliseconds, for stamping the last sync activity.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A mesh sync failure (LogSync session setup, publish, or the store).
#[derive(Debug, thiserror::Error)]
pub enum MeshSyncError {
    #[error("mesh sync: {0}")]
    Backend(String),
    #[error(transparent)]
    Store(#[from] MeshStoreError),
}

/// A snapshot of a mesh's sync activity, for a real sync indicator.
///
/// Counters accumulate over the mesh's subscription; read a snapshot with
/// [`SyncedMesh::sync_status`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncStatus {
    /// A LogSync reconciliation round is currently in progress.
    pub syncing: bool,
    /// LogSync rounds that have finished (a peer's catch-up completing).
    pub sync_rounds: u64,
    /// Mesh operations received over the session (catch-up and live alike).
    pub ops_received: u64,
    /// Unix-epoch milliseconds of the most recent sync activity, or `None`
    /// if nothing has arrived yet.
    pub last_activity_ms: Option<u64>,
}

/// The result of a manual [`SyncedMesh::resync`] checkpoint.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncRound {
    /// New operations that landed during the checkpoint window. `0` means
    /// already up to date.
    pub items_received: u64,
}

/// A device joined to a mesh's LogSync session.
///
/// Holds the store, the live publish lane, and a background task draining the
/// session into the store. Dropping it ends the session and stops the task.
pub struct SyncedMesh {
    store: MeshStore,
    mesh_id: [u8; 32],
    /// Keeps the session actor alive; dropped with the struct.
    _log_sync: MeshLogSync,
    /// The live lane: newly authored operations are pushed here so connected
    /// peers receive them without waiting for a reconciliation round.
    handle: MeshSyncHandle,
    /// Drains operations the session reconciles (or gossips) into the store.
    logsync_task: JoinHandle<()>,
    /// Live sync activity, written by the drain task; read via
    /// [`sync_status`](Self::sync_status).
    status: Arc<Mutex<SyncStatus>>,
}

impl SyncedMesh {
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
        store: MeshStore,
        mesh_id: [u8; 32],
    ) -> Result<Self, MeshSyncError> {
        let status = Arc::new(Mutex::new(SyncStatus::default()));

        let log_sync = MeshLogSync::builder(store.sqlite(), endpoint, gossip)
            .spawn()
            .await
            .map_err(|e| MeshSyncError::Backend(format!("logsync spawn: {e}")))?;
        let handle = log_sync
            .stream(Topic::from(mesh_id), true)
            .await
            .map_err(|e| MeshSyncError::Backend(format!("logsync stream: {e}")))?;
        let mut sub = handle
            .subscribe()
            .await
            .map_err(|e| MeshSyncError::Backend(format!("logsync subscribe: {e}")))?;

        let task_store = store.clone();
        let task_status = Arc::clone(&status);
        let logsync_task = tokio::spawn(async move {
            while let Some(item) = sub.next().await {
                let Ok(from_sync) = item else { continue };
                match from_sync.event {
                    TopicLogSyncEvent::SyncStarted { .. } => {
                        let mut s = task_status.lock().unwrap();
                        s.syncing = true;
                        s.last_activity_ms = Some(now_ms());
                    }
                    TopicLogSyncEvent::OperationReceived { operation, .. } => {
                        let op = *operation;
                        // Verify the signature and the addressed mesh
                        // (defence in depth behind the protocol's header
                        // validation) before folding it in; count only ops
                        // that verify and are new.
                        if verify(&op)
                            && op.header.extensions.mesh_id == mesh_id
                            && matches!(task_store.insert(&op).await, Ok(true))
                        {
                            let mut s = task_status.lock().unwrap();
                            s.ops_received += 1;
                            s.last_activity_ms = Some(now_ms());
                        }
                    }
                    TopicLogSyncEvent::SyncFinished { .. } => {
                        let mut s = task_status.lock().unwrap();
                        s.syncing = false;
                        s.sync_rounds += 1;
                        s.last_activity_ms = Some(now_ms());
                    }
                    _ => {}
                }
            }
        });

        Ok(Self {
            store,
            mesh_id,
            _log_sync: log_sync,
            handle,
            logsync_task,
            status,
        })
    }

    /// The mesh's operation store.
    pub fn store(&self) -> &MeshStore {
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
        let author = SigningKey::from_bytes(&keypair.to_seed()).verifying_key();
        let latest = self.store.latest(&author, self.mesh_id).await?;
        let (seq_num, backlink) = match latest {
            Some(prev) => (prev.header.seq_num + 1, Some(*prev.hash.as_bytes())),
            None => (0, None),
        };
        let op = to_operation(keypair, self.mesh_id, event, seq_num, backlink);
        self.store.insert(&op).await?;
        self.handle
            .publish(op.clone())
            .await
            .map_err(|e| MeshSyncError::Backend(format!("publish: {e}")))?;
        Ok(op)
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
        self.status.lock().unwrap().clone()
    }

    /// Run a manual "sync now" checkpoint and report what arrived.
    ///
    /// LogSync already reconciles **continuously**, so this is not a fake
    /// spinner and not a forced re-fetch (p2panda 0.6.1 exposes no public
    /// "re-initiate" hook): it watches the live session until it goes quiet
    /// and returns the real count of operations that landed during the
    /// window. A quiet, already-synced mesh reports `0` quickly.
    pub async fn resync(&self) -> Result<SyncRound, MeshSyncError> {
        let received = || self.status.lock().unwrap().ops_received;
        let start = received();
        let mut last = start;
        let mut quiet = 0u8;
        // Poll up to ~3s; stop after ~300ms with nothing new (settled).
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let now = received();
            if now == last {
                quiet += 1;
                if quiet >= 3 {
                    break;
                }
            } else {
                last = now;
                quiet = 0;
            }
        }
        Ok(SyncRound {
            items_received: last.saturating_sub(start),
        })
    }
}

impl Drop for SyncedMesh {
    fn drop(&mut self) {
        self.logsync_task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{JobId, JobState};
    use crate::wire::JobKind;
    use crate::worker::{execute, next_action, WorkerAction};
    use identity::{IdentityProvider, InMemoryProvider};
    use std::sync::Arc as StdArc;
    use transport::P2pandaTransport;

    const MESH: [u8; 32] = [0x77; 32];

    /// Two bound transports tagged with each other on the mesh's overlay
    /// topic (the tessera two-peer bootstrap, verbatim).
    async fn two_peers() -> (P2pandaTransport, P2pandaTransport) {
        let alice_provider = StdArc::new(InMemoryProvider::from_seed([60; 32]));
        let bob_provider = StdArc::new(InMemoryProvider::from_seed([61; 32]));
        let alice_id =
            transport::PeerID::from_public_key(alice_provider.master_public_key());
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

    async fn join(t: &P2pandaTransport) -> SyncedMesh {
        let (ep, gossip) = t.sync_parts().expect("sync parts");
        let store = MeshStore::in_memory().await.expect("store");
        SyncedMesh::join(ep, gossip, store, MESH).await.expect("join")
    }

    /// Poll `mesh`'s board until `pred` holds (or the timeout trips).
    async fn wait_for_board(
        mesh: &SyncedMesh,
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
        bob.author(&bob_kp, &MeshEvent::JobClaimed { job: id.0, at_ms: 20 })
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
        let result = execute(job.kind, &job.payload);
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
        println!("mesh resync checkpoint: items_received={}", round.items_received);
    }

    /// The offline-catch-up lane: A holds a posted job *before* B connects;
    /// B's empty store converges over RBSR (no live publish involved).
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_late_joiner_catches_up_on_an_existing_log() {
        let (alice_t, bob_t) = two_peers().await;

        let alice_kp = InMemoryProvider::from_seed([60; 32])
            .derive_keypair(b"mesh-author")
            .unwrap();
        let alice_store = MeshStore::in_memory().await.unwrap();
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
