// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Stable Personae writer binding shared by replicated domains.

use identity::DerivedKeyAttestation;

/// A derived operation signer could not be bound to its stable Personae root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WriterBindingError {
    #[error("derived writer attestation does not verify for this domain")]
    InvalidAttestation,
    #[error("derived writer attestation contains an invalid key")]
    InvalidDerivedWriter,
    #[error("derived writer attestation does not bind the operation signer")]
    SignerMismatch,
    #[error("derived writer attestation contains an invalid root key")]
    InvalidWriterRoot,
}

impl WriterBindingError {
    /// Stable refusal code for domain admission receipts.
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidAttestation => "invalid-writer-attestation",
            Self::InvalidDerivedWriter => "invalid-derived-writer",
            Self::SignerMismatch => "writer-attestation-mismatch",
            Self::InvalidWriterRoot => "invalid-writer-root",
        }
    }
}

/// Resolve an operation signer to its stable Personae subject.
///
/// A direct root signer resolves to itself. A derived signer must carry an
/// attestation valid under the caller's domain-separated salt, name that exact
/// signer, and contain a valid root key.
pub fn stable_writer_subject(
    signer: [u8; 32],
    attestation: Option<&DerivedKeyAttestation>,
    salt: &[u8],
) -> Result<[u8; 32], WriterBindingError> {
    let Some(attestation) = attestation else {
        return Ok(signer);
    };
    if !attestation.verify(salt) {
        return Err(WriterBindingError::InvalidAttestation);
    }
    let derived = attestation
        .derived_public_key()
        .map_err(|_| WriterBindingError::InvalidDerivedWriter)?
        .to_bytes();
    if derived != signer {
        return Err(WriterBindingError::SignerMismatch);
    }
    attestation
        .master_public_key()
        .map(|key| key.to_bytes())
        .map_err(|_| WriterBindingError::InvalidWriterRoot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use identity::{IdentityProvider, InMemoryProvider};

    #[test]
    fn direct_and_attested_writers_resolve_to_stable_subjects() {
        let identity = InMemoryProvider::from_seed([0x71; 32]);
        let salt = b"stickleback/writer-test";
        let derived = identity.derive_keypair(salt).unwrap();
        let attestation = identity.attest_derived_key(salt).unwrap();

        assert_eq!(
            stable_writer_subject(derived.public_key().to_bytes(), Some(&attestation), salt),
            Ok(identity.master_public_key().to_bytes())
        );
        assert_eq!(
            stable_writer_subject([0x22; 32], None, salt),
            Ok([0x22; 32])
        );
        assert_eq!(
            stable_writer_subject(
                derived.public_key().to_bytes(),
                Some(&attestation),
                b"other-domain",
            ),
            Err(WriterBindingError::InvalidAttestation)
        );
    }
}
