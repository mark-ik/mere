// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mesh's store of record — muniment behind the p2panda-store adapter.
//!
//! [`MeshStore`] wraps [`stickleback::MunimentStore`]: one operation store that both
//! the [`JobBoard`] fold and the LogSync session read, backed by muniment — an
//! in-memory backend for tests, redb on a real device. [`MeshStore::accept`]
//! applies mesh policy, then persists the operation pointer, log entry, and
//! topic-to-log association in one backend batch. Everything accepted locally
//! or from a session is served to peers on the next round.
//!
//! This is the M1 plan's step-0 seam, now over the shared substrate: where the
//! mesh once carried p2panda-store's own SQLite store, it rides the same
//! muniment-backed adapter murm and tessera converge on, so all three share one
//! store family (redb desktop, IndexedDB + OPFS in the browser).
//!
//! Each author has an event log and a checkpoint log under the mesh topic. An
//! event prefix can therefore disappear while the signed checkpoint operation
//! that authorizes it remains available.

use std::collections::BTreeMap;
use std::path::Path;

use muniment::{Backend, MemoryBackend, RedbBackend, StoreError};
use p2panda_core::{Hash, Operation, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use stickleback::{
    Admission, CheckpointAuthority, MunimentStore, OperationPolicy, OperationProcessor,
    ProcessError, ProcessOutcome, Reject, StoreTarget,
};

use crate::board::JobBoard;
use crate::retention::{
    CheckpointError, JobBoardSnapshot, LogFrontier, MeshRetentionPolicy, PayloadRule,
    RetentionCheckpoint,
};
use crate::wire::{MeshEvent, MeshExt, MeshLogId, from_operation};

/// A mesh store failure (the underlying muniment backend).
#[derive(Debug, thiserror::Error)]
pub enum MeshStoreError {
    #[error("mesh store: {0}")]
    Backend(#[from] StoreError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error("mesh retention: {0}")]
    Retention(String),
}

/// Latest accepted checkpoint and its signed operation identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredCheckpoint {
    pub operation: [u8; 32],
    pub checkpoint: RetentionCheckpoint,
}

/// Mesh addressing and body policy supplied to the shared processor.
#[derive(Clone, Copy)]
struct MeshPolicy<'a> {
    mesh_id: [u8; 32],
    retention: Option<&'a MeshRetentionPolicy>,
    current: Option<&'a StoredCheckpoint>,
}

impl OperationPolicy<MeshExt> for MeshPolicy<'_> {
    type LogId = MeshLogId;

    fn admit(&self, operation: &Operation<MeshExt>) -> Result<Admission<Self::LogId>, Reject> {
        if operation.header.extensions.mesh_id != self.mesh_id {
            return Err(Reject::new(
                "wrong-mesh",
                "operation addresses a different compute mesh",
            ));
        }
        let (_, event) = from_operation(operation)
            .map_err(|err| Reject::new("invalid-mesh-event", err.to_string()))?;
        let target = StoreTarget::new(Topic::from(self.mesh_id), event.log_id());
        match event {
            MeshEvent::RetentionCheckpoint { checkpoint } => {
                if operation.header.extensions.prune_flag.is_set() {
                    return Err(Reject::new(
                        "checkpoint-cannot-prune",
                        "checkpoint operations remain in their dedicated log",
                    ));
                }
                let policy = self.retention.ok_or_else(|| {
                    Reject::new(
                        "retention-unconfigured",
                        "this mesh has no configured retention policy",
                    )
                })?;
                policy
                    .validate_checkpoint(
                        self.mesh_id,
                        *operation.header.verifying_key.as_bytes(),
                        self.current.map(|stored| &stored.checkpoint),
                        &checkpoint,
                    )
                    .map_err(checkpoint_reject)?;
                // Only M1's inline inputs live in an operation body, so only
                // they are erasable this way. A V2 input is a blob; collecting
                // it is a blob-retention job, not a body erasure.
                let erased = checkpoint.snapshot.jobs.iter().filter_map(|job| {
                    (job.spec.is_none() && job.state.is_terminal() && job.payload.is_none())
                        .then_some(Hash::from(job.id.0))
                });
                Ok(Admission::keep(target).erasing_payloads(erased))
            }
            MeshEvent::HistoryPruned { checkpoint, .. } => {
                if !operation.header.extensions.prune_flag.is_set() {
                    return Err(Reject::new(
                        "missing-prune-flag",
                        "history-pruned event requires the p2panda prune flag",
                    ));
                }
                let current = self.current.ok_or_else(|| {
                    Reject::new(
                        "checkpoint-missing",
                        "no accepted checkpoint authorizes pruning",
                    )
                })?;
                if checkpoint != current.operation {
                    return Err(Reject::new(
                        "checkpoint-stale",
                        "history-pruned event does not name the latest accepted checkpoint",
                    ));
                }
                Ok(Admission::prune_before_current(target))
            }
            _ => {
                if operation.header.extensions.prune_flag.is_set() {
                    return Err(Reject::new(
                        "unexpected-prune-flag",
                        "only a history-pruned event may carry the prune flag",
                    ));
                }
                // V2 and M3 bodies carry attacker-supplied structure, so their
                // bounds are re-established here — before the operation is
                // stored, not when the board later folds it. Rules that need
                // the *job* (is this author the epoch's claim winner? does the
                // window fit the envelope?) belong to the fold, which has the
                // spec and the claim set; admission checks what one operation
                // can be judged on alone.
                match &event {
                    MeshEvent::JobPostedV2 { spec, .. } => {
                        spec.validate()
                            .map_err(|err| Reject::new("invalid-job-spec", err.to_string()))?;
                    }
                    MeshEvent::JobDoneV2 { output, .. }
                    | MeshEvent::JobCompletedUnderLease { output, .. } => {
                        output
                            .validate_self()
                            .map_err(|err| Reject::new("invalid-job-result", err.to_string()))?;
                    }
                    MeshEvent::LeaseGranted {
                        granted_at_ms,
                        expires_at_ms,
                        ..
                    } => {
                        if expires_at_ms <= granted_at_ms {
                            return Err(Reject::new(
                                "invalid-lease-window",
                                "a lease must expire after it is granted",
                            ));
                        }
                        if expires_at_ms - granted_at_ms > crate::lease::MAX_LEASE_DURATION_MS {
                            return Err(Reject::new(
                                "invalid-lease-window",
                                "lease window exceeds the longest a job may authorize",
                            ));
                        }
                    }
                    _ => {}
                }
                Ok(Admission::keep(target))
            }
        }
    }
}

fn checkpoint_reject(error: CheckpointError) -> Reject {
    let code = match error {
        CheckpointError::UnsupportedVersion(_) => "checkpoint-version",
        CheckpointError::ForeignMesh => "checkpoint-foreign-mesh",
        CheckpointError::UnauthorizedAuthority => "checkpoint-unauthorized",
        CheckpointError::StalePolicy => "checkpoint-stale-policy",
        CheckpointError::FalseSnapshotReference => "checkpoint-false-snapshot-ref",
        CheckpointError::FalseSnapshotCommitment => "checkpoint-false-commitment",
        CheckpointError::DuplicateFrontier => "checkpoint-duplicate-frontier",
        CheckpointError::FrontierRewind => "checkpoint-frontier-rewind",
    };
    Reject::new(code, error.to_string())
}

/// The mesh's operation store: insert once, serve to both the board fold and the
/// LogSync session. Clone-cheap (the muniment handle is shared). Generic over the
/// backend — [`MemoryBackend`] for tests and rehearsals, [`RedbBackend`] for a
/// durable device.
#[derive(Clone)]
pub struct MeshStore<B> {
    store: MunimentStore<B, MeshExt>,
    retention: Option<MeshRetentionPolicy>,
}

impl MeshStore<MemoryBackend> {
    /// An ephemeral in-memory store (tests, the `mesh-peer` rehearsal).
    pub fn in_memory() -> Self {
        Self {
            store: MunimentStore::new(MemoryBackend::new()),
            retention: None,
        }
    }

    pub fn in_memory_with_retention(retention: MeshRetentionPolicy) -> Self {
        Self {
            store: MunimentStore::new(MemoryBackend::new()),
            retention: Some(retention),
        }
    }
}

impl MeshStore<RedbBackend> {
    /// A durable store backed by a redb database at `path`, created if missing.
    pub fn at_path(path: impl AsRef<Path>) -> Result<Self, MeshStoreError> {
        Ok(Self {
            store: MunimentStore::new(RedbBackend::open(path)?),
            retention: None,
        })
    }

    pub fn at_path_with_retention(
        path: impl AsRef<Path>,
        retention: MeshRetentionPolicy,
    ) -> Result<Self, MeshStoreError> {
        Ok(Self {
            store: MunimentStore::new(RedbBackend::open(path)?),
            retention: Some(retention),
        })
    }
}

impl<B: Backend + Clone> MeshStore<B> {
    /// A clone of the underlying store, for `LogSync::builder` (which takes the
    /// store by value and reconciles through its trait surface).
    pub fn sync_store(&self) -> MunimentStore<B, MeshExt> {
        self.store.clone()
    }

    /// Validate, persist, and index one operation. Returns `true` when it is new,
    /// `false` when it was already present (idempotent on the hash — re-delivery
    /// during sync is normal, not an error).
    ///
    /// This convenience uses the operation's addressed mesh. A session receiving
    /// from an expected mesh uses [`accept`](Self::accept) so cross-mesh replay is
    /// rejected before mutation.
    pub async fn insert(&self, op: &Operation<MeshExt>) -> Result<bool, MeshStoreError> {
        self.accept(op.header.extensions.mesh_id, op).await
    }

    /// Run the shared policy-before-insert path for an expected mesh.
    pub async fn accept(
        &self,
        mesh_id: [u8; 32],
        op: &Operation<MeshExt>,
    ) -> Result<bool, MeshStoreError> {
        Ok(self.accept_outcome(mesh_id, op).await?.inserted())
    }

    /// Accept one operation and report each durable retention effect.
    pub async fn accept_outcome(
        &self,
        mesh_id: [u8; 32],
        op: &Operation<MeshExt>,
    ) -> Result<ProcessOutcome, MeshStoreError> {
        let current = self.latest_checkpoint(mesh_id).await?;
        let processor = OperationProcessor::new(
            self.store.clone(),
            MeshPolicy {
                mesh_id,
                retention: self.retention.as_ref(),
                current: current.as_ref(),
            },
        );
        Ok(processor.process(op).await?)
    }

    /// Fetch a retained operation, including whether its payload is present.
    pub async fn operation(&self, id: &Hash) -> Result<Option<Operation<MeshExt>>, MeshStoreError> {
        Ok(self.store.get_operation(id).await?)
    }

    /// The latest operation in `author`'s log on this mesh — the seq/backlink
    /// source for authoring the next one. `None` for a first-ever event.
    pub async fn latest(
        &self,
        author: &VerifyingKey,
        _mesh_id: [u8; 32],
    ) -> Result<Option<Operation<MeshExt>>, MeshStoreError> {
        self.latest_in(author, MeshLogId::Events).await
    }

    pub async fn latest_in(
        &self,
        author: &VerifyingKey,
        log_id: MeshLogId,
    ) -> Result<Option<Operation<MeshExt>>, MeshStoreError> {
        Ok(self.store.get_latest_entry(author, &log_id).await?)
    }

    /// Every operation on `mesh_id`, across all known authors' logs — the board
    /// fold's input.
    pub async fn ops(&self, mesh_id: [u8; 32]) -> Result<Vec<Operation<MeshExt>>, MeshStoreError> {
        let logs: BTreeMap<VerifyingKey, Vec<MeshLogId>> =
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
        let Some(stored) = self.latest_checkpoint_from_ops(mesh_id, &ops)? else {
            return Ok(JobBoard::fold(mesh_id, ops.iter()));
        };
        let frontier: BTreeMap<_, _> = stored
            .checkpoint
            .frontier
            .iter()
            .map(|entry| ((entry.author, entry.log_id), entry.seq_num))
            .collect();
        let tail: Vec<_> = ops
            .iter()
            .filter(|op| {
                let Ok((_, event)) = from_operation(op) else {
                    return false;
                };
                frontier
                    .get(&(*op.header.verifying_key.as_bytes(), event.log_id()))
                    .is_none_or(|seq| u64::from(op.header.seq_num) > *seq)
            })
            .collect();
        Ok(JobBoard::fold_from_snapshot(
            mesh_id,
            &stored.checkpoint.snapshot,
            tail,
        ))
    }

    pub async fn latest_checkpoint(
        &self,
        mesh_id: [u8; 32],
    ) -> Result<Option<StoredCheckpoint>, MeshStoreError> {
        let ops = self.ops(mesh_id).await?;
        self.latest_checkpoint_from_ops(mesh_id, &ops)
    }

    fn latest_checkpoint_from_ops(
        &self,
        mesh_id: [u8; 32],
        ops: &[Operation<MeshExt>],
    ) -> Result<Option<StoredCheckpoint>, MeshStoreError> {
        let Some(policy) = self.retention.as_ref() else {
            return Ok(None);
        };
        let mut checkpoints: Vec<_> = ops
            .iter()
            .filter_map(|op| match from_operation(op).ok()?.1 {
                MeshEvent::RetentionCheckpoint { checkpoint } => Some((op, *checkpoint)),
                _ => None,
            })
            .collect();
        checkpoints.sort_by_key(|(op, _)| op.header.seq_num);
        let mut current: Option<StoredCheckpoint> = None;
        for (op, checkpoint) in checkpoints {
            policy
                .validate_checkpoint(
                    mesh_id,
                    *op.header.verifying_key.as_bytes(),
                    current.as_ref().map(|stored| &stored.checkpoint),
                    &checkpoint,
                )
                .map_err(|error| MeshStoreError::Retention(error.to_string()))?;
            current = Some(StoredCheckpoint {
                operation: *op.hash.as_bytes(),
                checkpoint,
            });
        }
        Ok(current)
    }

    pub async fn build_checkpoint(
        &self,
        mesh_id: [u8; 32],
        at_ms: u64,
    ) -> Result<RetentionCheckpoint, MeshStoreError> {
        let policy = self.retention.as_ref().ok_or_else(|| {
            MeshStoreError::Retention("this mesh has no configured retention policy".into())
        })?;
        let ops = self.ops(mesh_id).await?;
        let current = self.latest_checkpoint_from_ops(mesh_id, &ops)?;
        let board = self.board(mesh_id).await?;
        let mut snapshot = JobBoardSnapshot::from_board(&board);
        if policy.erasure.terminal_job_payload == PayloadRule::EraseTerminalAtCheckpoint {
            snapshot.erase_terminal_payloads();
        }
        let mut frontier: BTreeMap<_, _> = current
            .iter()
            .flat_map(|stored| stored.checkpoint.frontier.iter().cloned())
            .map(|entry| ((entry.author, entry.log_id), entry))
            .collect();
        for op in &ops {
            let Ok((_, event)) = from_operation(op) else {
                continue;
            };
            let log_id = event.log_id();
            if log_id != MeshLogId::Events {
                continue;
            }
            let key = (*op.header.verifying_key.as_bytes(), log_id);
            let entry = LogFrontier {
                author: key.0,
                log_id,
                seq_num: u64::from(op.header.seq_num),
                operation: *op.hash.as_bytes(),
            };
            frontier.insert(key, entry);
        }
        Ok(RetentionCheckpoint::new(
            mesh_id,
            policy.revision,
            policy.authority_revision(),
            frontier.into_values().collect(),
            snapshot,
            at_ms,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::JobState;
    use crate::retention::{
        AvailabilityPolicy, ErasurePolicy, KeepBound, MeshRetentionPolicy, PayloadRule,
        PolicyRevision,
    };
    use crate::wire::{JobKind, MeshEvent, to_operation, to_prune_operation};
    use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};

    const MESH: [u8; 32] = [0x4d; 32];

    fn keypair(seed: u8) -> Ed25519Keypair {
        InMemoryProvider::from_seed([seed; 32])
            .derive_keypair(b"mesh-store")
            .unwrap()
    }

    fn retention(authority: &Ed25519Keypair) -> MeshRetentionPolicy {
        MeshRetentionPolicy {
            revision: PolicyRevision([7; 32]),
            checkpoint_authority: authority.public_key().to_bytes(),
            availability: AvailabilityPolicy {
                promised_floor: KeepBound::Forever,
            },
            erasure: ErasurePolicy {
                privacy_ceiling: KeepBound::UntilCheckpoint,
                terminal_job_payload: PayloadRule::EraseTerminalAtCheckpoint,
            },
        }
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
    async fn rejection_precedes_every_store_mutation() {
        let store = MeshStore::in_memory();
        let mut wrong_mesh = posted(&keypair(1));
        wrong_mesh.header.extensions.mesh_id = [0xff; 32];

        assert!(store.accept(MESH, &wrong_mesh).await.is_err());
        assert!(store.ops(MESH).await.unwrap().is_empty());
        assert!(store.ops([0xff; 32]).await.unwrap().is_empty());

        let other = [0xaa; 32];
        let valid_other = to_operation(
            &keypair(2),
            other,
            &MeshEvent::JobPosted {
                kind: JobKind::Echo,
                payload: b"other".to_vec(),
                nonce: 1,
                at_ms: 1,
            },
            0,
            None,
        );
        assert!(store.accept(MESH, &valid_other).await.is_err());
        assert!(store.ops(MESH).await.unwrap().is_empty());
        assert!(store.ops(other).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_malformed_v2_spec_or_result_is_refused_before_mutation() {
        use crate::ident::{ImplementationId, ResourceId};
        use crate::spec::{DeterminismClass, JobInput, JobOutput, JobSpec, VerificationClass};
        use proofs::BlobRef;

        let store = MeshStore::in_memory();
        let kp = keypair(1);
        let good = JobSpec::simple(
            ResourceId::parse("esp.embed.lexical/v1").unwrap(),
            "texts",
            BlobRef::blake3(b"batch"),
            "vectors",
            4096,
            DeterminismClass::Exact,
        );

        let duplicate_names = {
            let mut spec = good.clone();
            spec.inputs.push(JobInput {
                name: "texts".to_string(),
                blob: BlobRef::blake3(b"other"),
            });
            spec
        };
        let oversized_grant = {
            let mut spec = good.clone();
            spec.output.max_bytes = crate::spec::MAX_OUTPUT_BYTES + 1;
            spec
        };
        let bad_slot_name = {
            let mut spec = good.clone();
            spec.output.name = "Vectors!".to_string();
            spec
        };

        for (n, spec) in [duplicate_names, oversized_grant, bad_slot_name]
            .into_iter()
            .enumerate()
        {
            let op = to_operation(
                &kp,
                MESH,
                &MeshEvent::JobPostedV2 {
                    spec: Box::new(spec),
                    nonce: n as u64,
                    at_ms: 1,
                },
                0,
                None,
            );
            assert!(store.accept(MESH, &op).await.is_err(), "spec case {n}");
        }

        // A result whose identities do not parse is refused on its own, before
        // any board is consulted.
        let forged: ResourceId = p2panda_core::cbor::decode_cbor(
            p2panda_core::cbor::encode_cbor(&"not an id")
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        let bad_result = to_operation(
            &kp,
            MESH,
            &MeshEvent::JobDoneV2 {
                job: [9; 32],
                output: Box::new(JobOutput {
                    name: "vectors".to_string(),
                    blob: BlobRef::blake3(b"x"),
                    resource: forged,
                    implementation: ImplementationId::parse("mesh.lexical.fnv1a/v1").unwrap(),
                    verification: VerificationClass::ExactBytes,
                }),
                at_ms: 2,
            },
            0,
            None,
        );
        assert!(store.accept(MESH, &bad_result).await.is_err());

        assert!(
            store.ops(MESH).await.unwrap().is_empty(),
            "nothing malformed reached the store"
        );

        // The well-formed spec goes through the same door and lands.
        let accepted = to_operation(
            &kp,
            MESH,
            &MeshEvent::JobPostedV2 {
                spec: Box::new(good),
                nonce: 9,
                at_ms: 1,
            },
            0,
            None,
        );
        assert!(store.accept(MESH, &accepted).await.unwrap());
        assert_eq!(store.board(MESH).await.unwrap().len(), 1);
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

    #[tokio::test]
    async fn checkpoint_survives_event_prefix_prune_and_replays_the_board() {
        let authority = keypair(1);
        let store = MeshStore::in_memory_with_retention(retention(&authority));
        let posted = posted(&authority);
        store.insert(&posted).await.unwrap();

        let checkpoint = store.build_checkpoint(MESH, 10).await.unwrap();
        let checkpoint_op = to_operation(
            &authority,
            MESH,
            &MeshEvent::RetentionCheckpoint {
                checkpoint: Box::new(checkpoint.clone()),
            },
            0,
            None,
        );
        assert!(store.accept(MESH, &checkpoint_op).await.unwrap());

        let prune = to_prune_operation(
            &authority,
            MESH,
            *checkpoint_op.hash.as_bytes(),
            11,
            1,
            Some(*posted.hash.as_bytes()),
        );
        assert!(store.accept(MESH, &prune).await.unwrap());

        let ops = store.ops(MESH).await.unwrap();
        assert_eq!(
            ops.len(),
            2,
            "checkpoint log and surviving prune point remain"
        );
        assert!(!ops.iter().any(|op| op.hash == posted.hash));
        assert!(ops.iter().any(|op| op.hash == checkpoint_op.hash));
        assert!(ops.iter().any(|op| op.hash == prune.hash));

        let retained = store.board(MESH).await.unwrap();
        assert_eq!(retained.len(), 1);
        assert_eq!(retained.jobs().next().unwrap().state, JobState::Posted);
        assert_eq!(
            store
                .latest_in(
                    &p2panda_core::SigningKey::from_bytes(&authority.to_seed()).verifying_key(),
                    MeshLogId::Checkpoints,
                )
                .await
                .unwrap()
                .unwrap()
                .hash,
            checkpoint_op.hash
        );

        let next_checkpoint = store.build_checkpoint(MESH, 12).await.unwrap();
        assert_eq!(next_checkpoint.frontier.len(), 1);
        assert_eq!(next_checkpoint.frontier[0].seq_num, 1);
        let next_checkpoint_op = to_operation(
            &authority,
            MESH,
            &MeshEvent::RetentionCheckpoint {
                checkpoint: Box::new(next_checkpoint),
            },
            1,
            Some(*checkpoint_op.hash.as_bytes()),
        );
        assert!(store.accept(MESH, &next_checkpoint_op).await.unwrap());
    }

    #[tokio::test]
    async fn checkpoint_atomically_erases_terminal_input_and_keeps_result() {
        let authority = keypair(1);
        let store = MeshStore::in_memory_with_retention(retention(&authority));
        let post = posted(&authority);
        store.insert(&post).await.unwrap();
        let claim = to_operation(
            &authority,
            MESH,
            &MeshEvent::JobClaimed {
                job: *post.hash.as_bytes(),
                at_ms: 2,
            },
            1,
            Some(*post.hash.as_bytes()),
        );
        store.insert(&claim).await.unwrap();
        let done = to_operation(
            &authority,
            MESH,
            &MeshEvent::JobDone {
                job: *post.hash.as_bytes(),
                result: b"kept result".to_vec(),
                at_ms: 3,
            },
            2,
            Some(*claim.hash.as_bytes()),
        );
        store.insert(&done).await.unwrap();

        let checkpoint = store.build_checkpoint(MESH, 10).await.unwrap();
        let checkpoint_op = to_operation(
            &authority,
            MESH,
            &MeshEvent::RetentionCheckpoint {
                checkpoint: Box::new(checkpoint),
            },
            0,
            None,
        );
        let outcome = store.accept_outcome(MESH, &checkpoint_op).await.unwrap();
        assert_eq!(outcome.erased_payloads(), 1);
        assert!(
            store
                .operation(&post.hash)
                .await
                .unwrap()
                .unwrap()
                .body
                .is_none(),
            "the signed post header remains while its input body is erased"
        );

        let board = store.board(MESH).await.unwrap();
        let job = board.jobs().next().unwrap();
        assert!(job.payload.is_none());
        assert!(matches!(
            &job.state,
            JobState::Done { result, .. } if result == b"kept result"
        ));
    }

    #[tokio::test]
    async fn unauthorized_or_false_checkpoint_leaves_store_unchanged() {
        let authority = keypair(1);
        let intruder = keypair(2);
        let store = MeshStore::in_memory_with_retention(retention(&authority));
        let checkpoint = store.build_checkpoint(MESH, 10).await.unwrap();

        let unauthorized = to_operation(
            &intruder,
            MESH,
            &MeshEvent::RetentionCheckpoint {
                checkpoint: Box::new(checkpoint.clone()),
            },
            0,
            None,
        );
        assert!(store.accept(MESH, &unauthorized).await.is_err());

        let mut false_checkpoint = checkpoint;
        false_checkpoint.snapshot_commitment.digest.bytes[0] ^= 1;
        let false_digest = to_operation(
            &authority,
            MESH,
            &MeshEvent::RetentionCheckpoint {
                checkpoint: Box::new(false_checkpoint),
            },
            0,
            None,
        );
        assert!(store.accept(MESH, &false_digest).await.is_err());
        assert!(store.ops(MESH).await.unwrap().is_empty());
    }
}
