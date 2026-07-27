//! Mesh-owned retention policy, checkpoint, and compact board snapshot.
//!
//! Replication validates and stores operations. This module decides who may
//! checkpoint a mesh, which policy revision applies, what the snapshot means,
//! and whether a new frontier advances the accepted one.

use std::collections::BTreeMap;

use p2panda_core::cbor::encode_cbor;
use proofs::{BlobRef, Commitment, CommitmentDomain, Digest};
use serde::{Deserialize, Serialize};
use stickleback::CheckpointAuthority;

use crate::board::{JobBoard, JobState};
use crate::wire::MeshLogId;

/// A configured duration or logical retention boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeepBound {
    Forever,
    ForAgeMs(u64),
    UntilCheckpoint,
}

/// Availability is a hosting promise, distinct from permission to erase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailabilityPolicy {
    pub promised_floor: KeepBound,
}

/// How terminal job payloads are treated in a checkpoint snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PayloadRule {
    Keep,
    EraseTerminalAtCheckpoint,
}

/// Privacy-driven erasure settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErasurePolicy {
    pub privacy_ceiling: KeepBound,
    pub terminal_job_payload: PayloadRule,
}

/// Identity of the governed settings used to authorize a checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PolicyRevision(pub [u8; 32]);

/// Highest operation represented for one author/log pair.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogFrontier {
    pub author: [u8; 32],
    pub log_id: MeshLogId,
    pub seq_num: u64,
    pub operation: [u8; 32],
}

/// Replayable board state at a retained frontier.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobBoardSnapshot {
    pub jobs: Vec<crate::board::Job>,
}

impl JobBoardSnapshot {
    pub fn from_board(board: &JobBoard) -> Self {
        Self {
            jobs: board.jobs().cloned().collect(),
        }
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        encode_cbor(self).expect("a JobBoardSnapshot always CBOR-encodes")
    }

    /// Remove terminal inputs while retaining job identity and compact result.
    pub fn erase_terminal_payloads(&mut self) -> u64 {
        let mut erased = 0;
        for job in &mut self.jobs {
            if matches!(job.state, JobState::Done { .. }) && job.payload.take().is_some() {
                erased += 1;
            }
        }
        erased
    }
}

/// Mesh-specific retention checkpoint carried in a signed event body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionCheckpoint {
    pub version: u16,
    pub mesh_id: [u8; 32],
    pub policy_revision: PolicyRevision,
    pub authority_revision: Digest,
    pub frontier: Vec<LogFrontier>,
    pub snapshot: JobBoardSnapshot,
    pub snapshot_ref: BlobRef,
    pub snapshot_commitment: Commitment,
    pub at_ms: u64,
}

impl RetentionCheckpoint {
    pub fn new(
        mesh_id: [u8; 32],
        policy_revision: PolicyRevision,
        authority_revision: Digest,
        frontier: Vec<LogFrontier>,
        snapshot: JobBoardSnapshot,
        at_ms: u64,
    ) -> Self {
        let snapshot_bytes = snapshot.canonical_bytes();
        let snapshot_ref = BlobRef::blake3(&snapshot_bytes);
        let snapshot_commitment =
            Commitment::direct(CommitmentDomain::StorageCheckpoints, &snapshot_bytes);
        Self {
            version: 1,
            mesh_id,
            policy_revision,
            authority_revision,
            frontier,
            snapshot,
            snapshot_ref,
            snapshot_commitment,
            at_ms,
        }
    }
}

/// Injected mesh governance for retention checkpoints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeshRetentionPolicy {
    pub revision: PolicyRevision,
    pub checkpoint_authority: [u8; 32],
    pub availability: AvailabilityPolicy,
    pub erasure: ErasurePolicy,
}

impl MeshRetentionPolicy {
    pub fn validate_checkpoint(
        &self,
        expected_mesh: [u8; 32],
        author: [u8; 32],
        current: Option<&RetentionCheckpoint>,
        candidate: &RetentionCheckpoint,
    ) -> Result<(), CheckpointError> {
        if candidate.version != 1 {
            return Err(CheckpointError::UnsupportedVersion(candidate.version));
        }
        if candidate.mesh_id != expected_mesh {
            return Err(CheckpointError::ForeignMesh);
        }
        if !self.permits_checkpoint(author, &candidate.authority_revision) {
            return Err(CheckpointError::UnauthorizedAuthority);
        }
        if candidate.policy_revision != self.revision {
            return Err(CheckpointError::StalePolicy);
        }
        let snapshot_bytes = candidate.snapshot.canonical_bytes();
        if !candidate.snapshot_ref.verifies(&snapshot_bytes) {
            return Err(CheckpointError::FalseSnapshotReference);
        }
        if !candidate
            .snapshot_commitment
            .verifies_direct(CommitmentDomain::StorageCheckpoints, &snapshot_bytes)
        {
            return Err(CheckpointError::FalseSnapshotCommitment);
        }
        validate_frontier(current, &candidate.frontier)
    }
}

impl CheckpointAuthority for MeshRetentionPolicy {
    fn authority_revision(&self) -> Digest {
        let mut bytes = b"mere/mesh-checkpoint-authority/v1\0".to_vec();
        bytes.extend_from_slice(&self.checkpoint_authority);
        Digest::blake3(&bytes)
    }

    fn permits_checkpoint(&self, author: [u8; 32], named_revision: &Digest) -> bool {
        author == self.checkpoint_authority && *named_revision == self.authority_revision()
    }
}

fn validate_frontier(
    current: Option<&RetentionCheckpoint>,
    candidate: &[LogFrontier],
) -> Result<(), CheckpointError> {
    let mut next = BTreeMap::new();
    for entry in candidate {
        let key = (entry.author, entry.log_id);
        if next.insert(key, entry).is_some() {
            return Err(CheckpointError::DuplicateFrontier);
        }
    }
    let Some(current) = current else {
        return Ok(());
    };
    for previous in &current.frontier {
        let Some(candidate) = next.get(&(previous.author, previous.log_id)) else {
            return Err(CheckpointError::FrontierRewind);
        };
        if candidate.seq_num < previous.seq_num
            || (candidate.seq_num == previous.seq_num && candidate.operation != previous.operation)
        {
            return Err(CheckpointError::FrontierRewind);
        }
    }
    Ok(())
}

/// Why a checkpoint was refused before store mutation.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CheckpointError {
    #[error("unsupported checkpoint version {0}")]
    UnsupportedVersion(u16),
    #[error("checkpoint addresses another mesh")]
    ForeignMesh,
    #[error("checkpoint author is not the configured authority")]
    UnauthorizedAuthority,
    #[error("checkpoint names a stale policy revision")]
    StalePolicy,
    #[error("checkpoint snapshot blob reference is false")]
    FalseSnapshotReference,
    #[error("checkpoint snapshot commitment is false")]
    FalseSnapshotCommitment,
    #[error("checkpoint frontier contains a duplicate author/log")]
    DuplicateFrontier,
    #[error("checkpoint frontier would rewind accepted history")]
    FrontierRewind,
}

/// User-facing retention effect categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetentionEffect {
    LiveWithdrawal,
    BodyErased { count: u64 },
    BlobCollected { count: u64 },
    PrefixPruned { count: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{Job, JobId, JobState};
    use crate::wire::JobKind;

    const MESH: [u8; 32] = [0x4d; 32];
    const AUTHORITY: [u8; 32] = [0xa1; 32];

    fn policy() -> MeshRetentionPolicy {
        MeshRetentionPolicy {
            revision: PolicyRevision([7; 32]),
            checkpoint_authority: AUTHORITY,
            availability: AvailabilityPolicy {
                promised_floor: KeepBound::Forever,
            },
            erasure: ErasurePolicy {
                privacy_ceiling: KeepBound::UntilCheckpoint,
                terminal_job_payload: PayloadRule::EraseTerminalAtCheckpoint,
            },
        }
    }

    fn frontier(seq_num: u64, operation: u8) -> LogFrontier {
        LogFrontier {
            author: [3; 32],
            log_id: MeshLogId::Events,
            seq_num,
            operation: [operation; 32],
        }
    }

    fn checkpoint(frontier: Vec<LogFrontier>) -> RetentionCheckpoint {
        RetentionCheckpoint::new(
            MESH,
            policy().revision,
            policy().authority_revision(),
            frontier,
            JobBoardSnapshot::default(),
            10,
        )
    }

    #[test]
    fn rejects_false_proofs_foreign_space_stale_policy_and_wrong_authority() {
        let policy = policy();
        let valid = checkpoint(vec![frontier(2, 2)]);
        assert!(
            policy
                .validate_checkpoint(MESH, AUTHORITY, None, &valid)
                .is_ok()
        );

        let mut false_digest = valid.clone();
        false_digest.snapshot_ref.digest.bytes[0] ^= 1;
        assert_eq!(
            policy.validate_checkpoint(MESH, AUTHORITY, None, &false_digest),
            Err(CheckpointError::FalseSnapshotReference)
        );
        let mut false_commitment = valid.clone();
        false_commitment.snapshot_commitment.digest.bytes[0] ^= 1;
        assert_eq!(
            policy.validate_checkpoint(MESH, AUTHORITY, None, &false_commitment),
            Err(CheckpointError::FalseSnapshotCommitment)
        );

        let mut foreign = valid.clone();
        foreign.mesh_id = [9; 32];
        assert_eq!(
            policy.validate_checkpoint(MESH, AUTHORITY, None, &foreign),
            Err(CheckpointError::ForeignMesh)
        );

        let mut stale = valid.clone();
        stale.policy_revision = PolicyRevision([8; 32]);
        assert_eq!(
            policy.validate_checkpoint(MESH, AUTHORITY, None, &stale),
            Err(CheckpointError::StalePolicy)
        );
        assert_eq!(
            policy.validate_checkpoint(MESH, [4; 32], None, &valid),
            Err(CheckpointError::UnauthorizedAuthority)
        );
    }

    #[test]
    fn accepted_frontier_never_rewinds() {
        let policy = policy();
        let current = checkpoint(vec![frontier(4, 4)]);
        let rewound = checkpoint(vec![frontier(3, 3)]);
        assert_eq!(
            policy.validate_checkpoint(MESH, AUTHORITY, Some(&current), &rewound),
            Err(CheckpointError::FrontierRewind)
        );
        let advanced = checkpoint(vec![frontier(5, 5)]);
        assert!(
            policy
                .validate_checkpoint(MESH, AUTHORITY, Some(&current), &advanced)
                .is_ok()
        );
    }

    #[test]
    fn terminal_payload_erasure_keeps_compact_result() {
        let mut snapshot = JobBoardSnapshot {
            jobs: vec![Job {
                id: JobId([1; 32]),
                kind: JobKind::Echo,
                payload: Some(b"private input".to_vec()),
                posted_by: [2; 32],
                state: JobState::Done {
                    winner: [3; 32],
                    result: b"result".to_vec(),
                },
            }],
        };
        assert_eq!(snapshot.erase_terminal_payloads(), 1);
        assert!(snapshot.jobs[0].payload.is_none());
        assert!(matches!(
            &snapshot.jobs[0].state,
            JobState::Done { result, .. } if result == b"result"
        ));
    }
}
