/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The mesh's store of record — muniment behind the p2panda-store adapter.
//!
//! [`MeshStore`] wraps [`mooting::MunimentStore`]: one operation store that both
//! the [`JobBoard`] fold and the LogSync session read, backed by muniment — an
//! in-memory backend for tests, redb on a real device. [`MeshStore::insert`]
//! persists an operation *and* indexes it for sync (the topic → author/log
//! association, then the op with its log entry), so everything inserted, authored
//! locally or drained from a session, is served to peers on the next round.
//!
//! This is the M1 plan's step-0 seam, now over the shared substrate: where the
//! mesh once carried p2panda-store's own SQLite store, it rides the same
//! muniment-backed adapter murm and tessera converge on, so all three share one
//! store family (redb desktop, IndexedDB + OPFS in the browser).
//!
//! A mesh log is one append-only log per author per mesh: the log id *is* the
//! mesh id (`[u8; 32]`), exactly as the moot id keys tessera's logs. The topic is
//! `Topic::from(mesh_id)`.

use std::collections::BTreeMap;
use std::path::Path;

use mooting::MunimentStore;
use muniment::{Backend, MemoryBackend, RedbBackend, StoreError};
use p2panda_core::{Operation, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;

use crate::board::JobBoard;
use crate::wire::MeshExt;

/// A mesh store failure (the underlying muniment backend).
#[derive(Debug, thiserror::Error)]
pub enum MeshStoreError {
    #[error("mesh store: {0}")]
    Backend(#[from] StoreError),
}

/// The mesh's operation store: insert once, serve to both the board fold and the
/// LogSync session. Clone-cheap (the muniment handle is shared). Generic over the
/// backend — [`MemoryBackend`] for tests and rehearsals, [`RedbBackend`] for a
/// durable device.
#[derive(Clone)]
pub struct MeshStore<B> {
    store: MunimentStore<B, MeshExt>,
}

impl MeshStore<MemoryBackend> {
    /// An ephemeral in-memory store (tests, the `mesh-peer` rehearsal).
    pub fn in_memory() -> Self {
        Self {
            store: MunimentStore::new(MemoryBackend::new()),
        }
    }
}

impl MeshStore<RedbBackend> {
    /// A durable store backed by a redb database at `path`, created if missing.
    pub fn at_path(path: impl AsRef<Path>) -> Result<Self, MeshStoreError> {
        Ok(Self {
            store: MunimentStore::new(RedbBackend::open(path)?),
        })
    }
}

impl<B: Backend> MeshStore<B> {
    /// A clone of the underlying store, for `LogSync::builder` (which takes the
    /// store by value and reconciles through its trait surface).
    pub fn sync_store(&self) -> MunimentStore<B, MeshExt> {
        self.store.clone()
    }

    /// Persist + index one operation. Returns `true` when the operation is new,
    /// `false` when it was already present (idempotent on the hash — re-delivery
    /// during sync is normal, not an error).
    ///
    /// Associates the topic first, then inserts the op: a crash in between leaves
    /// at worst a topic pointing at an empty log (harmless — `ops` finds nothing
    /// there, and re-inserting completes it), never an op the topic can't reach.
    ///
    /// The caller verifies signatures first ([`crate::wire::verify`]); the store
    /// is the dumb of-record layer.
    pub async fn insert(&self, op: &Operation<MeshExt>) -> Result<bool, MeshStoreError> {
        let mesh_id = op.header.extensions.mesh_id;
        self.store
            .associate(&Topic::from(mesh_id), &op.header.verifying_key, &mesh_id)
            .await?;
        Ok(self.store.insert_operation(&op.hash, op, &mesh_id).await?)
    }

    /// The latest operation in `author`'s log on this mesh — the seq/backlink
    /// source for authoring the next one. `None` for a first-ever event.
    pub async fn latest(
        &self,
        author: &VerifyingKey,
        mesh_id: [u8; 32],
    ) -> Result<Option<Operation<MeshExt>>, MeshStoreError> {
        Ok(self.store.get_latest_entry(author, &mesh_id).await?)
    }

    /// Every operation on `mesh_id`, across all known authors' logs — the board
    /// fold's input.
    pub async fn ops(&self, mesh_id: [u8; 32]) -> Result<Vec<Operation<MeshExt>>, MeshStoreError> {
        let logs: BTreeMap<VerifyingKey, Vec<[u8; 32]>> =
            self.store.resolve(&Topic::from(mesh_id)).await?;
        let mut out = Vec::new();
        for (author, log_ids) in logs {
            for log_id in log_ids {
                if let Some(entries) = self
                    .store
                    .get_log_entries(&author, &log_id, None, None)
                    .await?
                {
                    out.extend(entries.into_iter().map(|(op, _raw_header)| op));
                }
            }
        }
        Ok(out)
    }

    /// Fold the store's view of `mesh_id` into its [`JobBoard`].
    pub async fn board(&self, mesh_id: [u8; 32]) -> Result<JobBoard, MeshStoreError> {
        let ops = self.ops(mesh_id).await?;
        Ok(JobBoard::fold(mesh_id, ops.iter()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::JobState;
    use crate::wire::{JobKind, MeshEvent, to_operation};
    use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};

    const MESH: [u8; 32] = [0x4d; 32];

    fn keypair(seed: u8) -> Ed25519Keypair {
        InMemoryProvider::from_seed([seed; 32])
            .derive_keypair(b"mesh-store")
            .unwrap()
    }

    fn posted(kp: &Ed25519Keypair) -> Operation<MeshExt> {
        to_operation(
            kp,
            MESH,
            &MeshEvent::JobPosted {
                kind: JobKind::Echo,
                payload: b"store".to_vec(),
                nonce: 0,
                at_ms: 1,
            },
            0,
            None,
        )
    }

    #[tokio::test]
    async fn insert_is_idempotent_and_feeds_the_board() {
        let store = MeshStore::in_memory();
        let kp = keypair(1);
        let op = posted(&kp);

        assert!(store.insert(&op).await.unwrap(), "first insert is new");
        assert!(!store.insert(&op).await.unwrap(), "re-insert is a no-op");

        let ops = store.ops(MESH).await.unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0], op);

        let board = store.board(MESH).await.unwrap();
        let job = board.jobs().next().expect("the posted job folds in");
        assert_eq!(job.state, JobState::Posted);
    }

    #[tokio::test]
    async fn latest_walks_the_author_log() {
        let store = MeshStore::in_memory();
        let kp = keypair(1);
        let author = {
            use p2panda_core::SigningKey;
            SigningKey::from_bytes(&kp.to_seed()).verifying_key()
        };

        assert!(store.latest(&author, MESH).await.unwrap().is_none());

        let op0 = posted(&kp);
        store.insert(&op0).await.unwrap();
        let op1 = to_operation(
            &kp,
            MESH,
            &MeshEvent::JobClaimed {
                job: *op0.hash.as_bytes(),
                at_ms: 2,
            },
            1,
            Some(*op0.hash.as_bytes()),
        );
        store.insert(&op1).await.unwrap();

        let latest = store.latest(&author, MESH).await.unwrap().unwrap();
        assert_eq!(latest.header.seq_num, 1);
        assert_eq!(latest.hash, op1.hash);
    }

    #[tokio::test]
    async fn two_in_memory_stores_do_not_share_state() {
        let a = MeshStore::in_memory();
        let b = MeshStore::in_memory();
        a.insert(&posted(&keypair(1))).await.unwrap();
        assert_eq!(a.ops(MESH).await.unwrap().len(), 1);
        assert!(b.ops(MESH).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_file_store_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mesh.redb");
        let op = posted(&keypair(1));
        {
            let store = MeshStore::at_path(&path).unwrap();
            store.insert(&op).await.unwrap();
        }
        let reopened = MeshStore::at_path(&path).unwrap();
        let ops = reopened.ops(MESH).await.unwrap();
        assert_eq!(ops.len(), 1, "the op survives a close + reopen");
        assert_eq!(ops[0], op);
    }
}
