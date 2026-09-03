// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Typed proof vocabulary shared by replicated Mere domains.
//!
//! P2panda operation hashes remain operation identities. These types name
//! application commitments and content references explicitly so callers cannot
//! silently exchange one meaning for another.

use serde::{Deserialize, Serialize};

/// Digest family used by a typed identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DigestAlg {
    Blake3,
    P2pandaOperation,
    Sha256,
    William3,
}

/// A digest paired with its algorithm.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Digest {
    pub alg: DigestAlg,
    pub bytes: Vec<u8>,
}

impl Digest {
    /// BLAKE3 digest of `bytes`.
    pub fn blake3(bytes: &[u8]) -> Self {
        Self {
            alg: DigestAlg::Blake3,
            bytes: blake3::hash(bytes).as_bytes().to_vec(),
        }
    }

    /// Construct a typed p2panda operation identity.
    pub fn p2panda_operation(bytes: [u8; 32]) -> Self {
        Self {
            alg: DigestAlg::P2pandaOperation,
            bytes: bytes.to_vec(),
        }
    }

    /// Read a 32-byte digest without discarding its algorithm.
    pub fn as_32(&self) -> Result<[u8; 32], ProofTypeError> {
        self.bytes
            .as_slice()
            .try_into()
            .map_err(|_| ProofTypeError::WrongDigestLength(self.bytes.len()))
    }
}

/// How an application commitment should be interpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommitmentScheme {
    /// Direct digest over canonical bytes, without a tree claim.
    DigestV1,
    MerkleV1,
    SparseMerkleV1,
    MmrV1,
    AccumulatorV1,
    VectorCommitmentV1,
    PsiCardinalityV1,
}

/// Meaning of a commitment root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommitmentDomain {
    MootAdmins,
    MootMods,
    MootStakeholders,
    MootMembers,
    #[serde(alias = "TesseraReceipts")]
    StandingReceipts,
    StorageChunks,
    StorageCheckpoints,
    MeshJobResults,
    DelegationSet,
}

/// Typed application-level commitment.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Commitment {
    pub scheme: CommitmentScheme,
    pub domain: CommitmentDomain,
    pub version: u16,
    pub digest: Digest,
}

impl Commitment {
    /// Direct version-1 commitment to canonical bytes.
    pub fn direct(domain: CommitmentDomain, bytes: &[u8]) -> Self {
        Self {
            scheme: CommitmentScheme::DigestV1,
            domain,
            version: 1,
            digest: Digest::blake3(bytes),
        }
    }

    /// Whether this direct commitment matches canonical bytes and domain.
    pub fn verifies_direct(&self, domain: CommitmentDomain, bytes: &[u8]) -> bool {
        self.scheme == CommitmentScheme::DigestV1
            && self.domain == domain
            && self.version == 1
            && self.digest == Digest::blake3(bytes)
    }
}

/// Content-addressed blob bytes. This is deliberately not a commitment root.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlobRef {
    pub digest: Digest,
    pub byte_len: u64,
}

impl BlobRef {
    pub fn blake3(bytes: &[u8]) -> Self {
        Self {
            digest: Digest::blake3(bytes),
            byte_len: bytes.len() as u64,
        }
    }

    pub fn verifies(&self, bytes: &[u8]) -> bool {
        self.byte_len == bytes.len() as u64 && self.digest == Digest::blake3(bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProofTypeError {
    #[error("expected a 32-byte digest, found {0} bytes")]
    WrongDigestLength(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_reference_and_application_commitment_keep_distinct_meanings() {
        let bytes = b"checkpoint";
        let blob = BlobRef::blake3(bytes);
        let commitment = Commitment::direct(CommitmentDomain::StorageCheckpoints, bytes);
        assert!(blob.verifies(bytes));
        assert!(commitment.verifies_direct(CommitmentDomain::StorageCheckpoints, bytes));
        assert_eq!(blob.digest, commitment.digest);
        assert_ne!(blob.digest.alg, DigestAlg::P2pandaOperation);
    }

    #[test]
    fn p2panda_identity_is_typed_separately() {
        let operation = Digest::p2panda_operation([7; 32]);
        assert_eq!(operation.alg, DigestAlg::P2pandaOperation);
        assert_eq!(operation.as_32().unwrap(), [7; 32]);
    }
}
