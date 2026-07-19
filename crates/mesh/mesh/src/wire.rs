// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mesh operation-wire bridge — job events as signed p2panda operations.
//!
//! Mirrors tessera's `wire` (and Murm's `Post` ↔ `Operation<CabalExt>`
//! split): a [`MeshEvent`] (the logical form the [board](crate::board) folds)
//! rides the synced event-DAG as a signed `Operation<MeshExt>`. The mesh id is
//! the signed addressing extension, so a job posted into one mesh cannot be
//! replayed into another; the event is the CBOR body, bound into the signature
//! via the header's payload hash; the author signs at its per-author log
//! position (`seq_num` / `backlink`), forming a valid p2panda log LogSync
//! reconciles.

use identity::Ed25519Keypair;
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::prune::PruneFlag;
use p2panda_core::{Body, Hash, Header, Operation, SigningKey};
use serde::{Deserialize, Serialize};

use crate::retention::RetentionCheckpoint;

/// Separate per-author logs keep checkpoint authority available while event
/// prefixes are pruned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MeshLogId {
    Events,
    Checkpoints,
}

/// The signed addressing extension on a mesh operation: which mesh (personal
/// space) the job traffic belongs to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshExt {
    /// The mesh / space this event addresses.
    pub mesh_id: [u8; 32],
    /// Upstream-compatible signal that this operation survives its event-log
    /// prefix. Omitted from legacy wire bytes while false.
    #[serde(
        rename = "p",
        skip_serializing_if = "PruneFlag::is_not_set",
        default = "PruneFlag::default"
    )]
    pub prune_flag: PruneFlag,
}

/// What a job asks for. M1 ships the two pure, deterministic kinds that prove
/// transport + protocol + convergence; real kinds (an embeddings batch, a Burn
/// job) arrive with M2's `MeshResource` adapter seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobKind {
    /// Return the payload unchanged (the round-trip proof).
    Echo,
    /// Return the BLAKE3 hash of the payload (32 bytes).
    Blake3,
}

/// A mesh event: the logical job-board record a peer authors. The board folds
/// these (by their operation hashes) into job state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeshEvent {
    /// Ask the mesh to run a job. The job's identity is this operation's hash.
    /// `payload` is inline-small in M1 (content-addressed blobs are M2);
    /// `nonce` distinguishes otherwise-identical asks.
    JobPosted {
        kind: JobKind,
        payload: Vec<u8>,
        nonce: u64,
        at_ms: u64,
    },
    /// Claim a posted job (by the posting operation's hash). Competing claims
    /// are resolved deterministically by the board, not by ordering.
    JobClaimed { job: [u8; 32], at_ms: u64 },
    /// The claimed job's result, from the claim winner.
    JobDone {
        job: [u8; 32],
        result: Vec<u8>,
        at_ms: u64,
    },
    /// Owner-authorized current state and retained per-log frontier.
    RetentionCheckpoint {
        checkpoint: Box<RetentionCheckpoint>,
    },
    /// An author's event-log prune point. The referenced checkpoint remains in
    /// the separate checkpoint log.
    HistoryPruned { checkpoint: [u8; 32], at_ms: u64 },
}

impl MeshEvent {
    pub(crate) fn log_id(&self) -> MeshLogId {
        match self {
            Self::RetentionCheckpoint { .. } => MeshLogId::Checkpoints,
            _ => MeshLogId::Events,
        }
    }
}

/// A malformed mesh operation.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    /// The operation carries no body (a mesh event is always the body).
    #[error("mesh operation has no body")]
    MissingBody,
    /// The body is not a valid CBOR `MeshEvent`.
    #[error("mesh operation body is not a MeshEvent")]
    Malformed,
}

/// Sign a [`MeshEvent`] into an operation on `mesh_id`'s event-DAG at the
/// author's per-author log position (`seq_num` / `backlink`; `0` / `None` for
/// the author's first mesh event).
pub fn to_operation(
    keypair: &Ed25519Keypair,
    mesh_id: [u8; 32],
    event: &MeshEvent,
    seq_num: u32,
    backlink: Option<[u8; 32]>,
) -> Operation<MeshExt> {
    to_operation_with_prune(keypair, mesh_id, event, seq_num, backlink, false)
}

/// Sign a `HistoryPruned` event with p2panda's prune flag set.
pub fn to_prune_operation(
    keypair: &Ed25519Keypair,
    mesh_id: [u8; 32],
    checkpoint: [u8; 32],
    at_ms: u64,
    seq_num: u32,
    backlink: Option<[u8; 32]>,
) -> Operation<MeshExt> {
    to_operation_with_prune(
        keypair,
        mesh_id,
        &MeshEvent::HistoryPruned { checkpoint, at_ms },
        seq_num,
        backlink,
        true,
    )
}

fn to_operation_with_prune(
    keypair: &Ed25519Keypair,
    mesh_id: [u8; 32],
    event: &MeshEvent,
    seq_num: u32,
    backlink: Option<[u8; 32]>,
    prune: bool,
) -> Operation<MeshExt> {
    let signing_key = SigningKey::from_bytes(&keypair.to_seed());
    let body_bytes = encode_cbor(event).expect("a MeshEvent always CBOR-encodes");
    let body = Body::new(&body_bytes);
    let mut header = Header {
        version: 1,
        verifying_key: signing_key.verifying_key(),
        signature: None,
        payload_size: body.size(),
        payload_hash: Some(body.hash()),
        seq_num,
        backlink: backlink.map(Hash::from),
        extensions: MeshExt {
            mesh_id,
            prune_flag: PruneFlag::new(prune),
        },
    };
    header.sign(&signing_key);
    let hash = header.hash();
    Operation {
        hash,
        header,
        body: Some(body),
    }
}

/// Decode the mesh id + event from an operation. Does *not* check the
/// signature — call [`verify`] for that.
pub fn from_operation(op: &Operation<MeshExt>) -> Result<([u8; 32], MeshEvent), WireError> {
    let body = op.body.as_ref().ok_or(WireError::MissingBody)?;
    let event: MeshEvent =
        decode_cbor(body.to_bytes().as_slice()).map_err(|_| WireError::Malformed)?;
    Ok((op.header.extensions.mesh_id, event))
}

/// Whether the operation's signature is valid for its author and content.
pub fn verify(op: &Operation<MeshExt>) -> bool {
    op.header.verify()
}

#[cfg(test)]
mod tests {
    use super::*;
    use identity::{IdentityProvider, InMemoryProvider};
    use serde::{Deserialize, Serialize};

    const MESH: [u8; 32] = [0x4d; 32];

    fn keypair(seed: u8) -> Ed25519Keypair {
        InMemoryProvider::from_seed([seed; 32])
            .derive_keypair(b"mesh-wire")
            .unwrap()
    }

    fn posted() -> MeshEvent {
        MeshEvent::JobPosted {
            kind: JobKind::Blake3,
            payload: b"hello mesh".to_vec(),
            nonce: 1,
            at_ms: 42,
        }
    }

    #[test]
    fn event_round_trips_through_an_operation() {
        let kp = keypair(7);
        let event = posted();
        let op = to_operation(&kp, MESH, &event, 0, None);
        let (mesh_id, decoded) = from_operation(&op).expect("decode");
        assert_eq!(mesh_id, MESH);
        assert_eq!(decoded, event);
    }

    #[test]
    fn a_signed_operation_verifies() {
        let kp = keypair(7);
        let op = to_operation(&kp, MESH, &posted(), 0, None);
        assert!(verify(&op));
    }

    #[test]
    fn tampering_the_mesh_id_breaks_verification() {
        let kp = keypair(7);
        let mut op = to_operation(&kp, MESH, &posted(), 0, None);
        op.header.extensions.mesh_id = [0xff; 32];
        assert!(
            !verify(&op),
            "the mesh id is signed, so a cross-mesh replay fails"
        );
    }

    #[test]
    fn the_per_author_log_validates_under_p2panda() {
        use p2panda_core::operation::{validate_backlink, validate_header};
        let kp = keypair(7);
        let op0 = to_operation(&kp, MESH, &posted(), 0, None);
        let claim = MeshEvent::JobClaimed {
            job: *op0.hash.as_bytes(),
            at_ms: 50,
        };
        let op1 = to_operation(&kp, MESH, &claim, 1, Some(*op0.hash.as_bytes()));

        // The chain satisfies p2panda's own validators, so LogSync accepts it.
        assert!(validate_header(&op0.header).is_ok());
        assert!(validate_header(&op1.header).is_ok());
        assert!(validate_backlink(&op0.header, &op1.header).is_ok());
    }

    #[test]
    fn default_prune_flag_preserves_legacy_mesh_header_identity() {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        struct LegacyMeshExt {
            mesh_id: [u8; 32],
        }

        let kp = keypair(7);
        let signing_key = SigningKey::from_bytes(&kp.to_seed());
        let event = posted();
        let body = Body::new(&encode_cbor(&event).unwrap());
        let mut legacy = Header {
            version: 1,
            verifying_key: signing_key.verifying_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            seq_num: 0,
            backlink: None,
            extensions: LegacyMeshExt { mesh_id: MESH },
        };
        legacy.sign(&signing_key);
        let current = to_operation(&kp, MESH, &event, 0, None);

        assert_eq!(legacy.to_bytes(), current.header.to_bytes());
        assert_eq!(legacy.hash(), current.header.hash());
        assert_eq!(legacy.signature, current.header.signature);
    }

    #[test]
    fn history_pruned_event_sets_the_upstream_flag() {
        let kp = keypair(7);
        let operation = to_prune_operation(&kp, MESH, [9; 32], 42, 3, Some([8; 32]));
        assert!(operation.header.extensions.prune_flag.is_set());
        assert_eq!(
            from_operation(&operation).unwrap().1,
            MeshEvent::HistoryPruned {
                checkpoint: [9; 32],
                at_ms: 42
            }
        );
    }
}
