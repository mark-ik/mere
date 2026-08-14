// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Withdrawing a device grant, as statements that travel.
//!
//! The wallet's old `revoked` list was local: it answered "is this device
//! revoked?" for the machine holding it and for nobody else. These statements
//! are the portable form, verifiable by anyone holding the issuer's public
//! key, which is what revoking a stolen station has to mean.

use crate::delegation::{
    DelegationError, DelegationRevocation, SignedDelegationCertificate, SignedDelegationRevocation,
};
use crate::{IdentityProvider, InMemoryProvider};

use super::DeviceGrantSet;

/// Withdraw every certificate in a grant set, as portable signed statements.
///
/// Each statement is signed by the authority that issued the certificate it
/// withdraws: the master for the device-scoped certificate, and each persona's
/// own chain root for its own. `RevocationLedger::revokes` refuses a statement
/// whose declared issuer does not match the certificate's, so signing them any
/// other way would produce statements nobody honours.
///
/// This is what a local `revoked` list could never be. A radio, a peer, or a
/// second device can verify one of these against the issuer's public key
/// alone, which is what makes revoking a stolen station mean something beyond
/// the wallet that issued it.
pub fn revoke_device_grant_set(
    master_seed: [u8; 32],
    set: &DeviceGrantSet,
    at_ms: u64,
) -> Result<Vec<SignedDelegationRevocation>, DelegationError> {
    let mut statements = Vec::new();
    if let Some(certificate) = &set.device {
        let master = InMemoryProvider::from_seed(master_seed);
        statements.push(revoke_one(&master, certificate, at_ms)?);
    }
    for (&persona, certificate) in &set.personas {
        let master = InMemoryProvider::from_seed(master_seed);
        let persona_keypair = master
            .derive_keypair(&crate::carry::persona_wallet_salt(persona))
            .map_err(|_| DelegationError::Identity)?;
        let persona_provider = InMemoryProvider::from_seed(persona_keypair.to_seed());
        statements.push(revoke_one(&persona_provider, certificate, at_ms)?);
    }
    Ok(statements)
}

fn revoke_one<P: IdentityProvider>(
    provider: &P,
    certificate: &SignedDelegationCertificate,
    at_ms: u64,
) -> Result<SignedDelegationRevocation, DelegationError> {
    let id = certificate.certificate.id();
    let nonce = *blake3::hash(&[id.0.as_slice(), &at_ms.to_le_bytes()].concat()).as_bytes();
    SignedDelegationRevocation::issue(
        provider,
        DelegationRevocation::new(
            id,
            certificate.certificate.issuer,
            certificate.certificate.scope.clone(),
            at_ms,
            nonce,
        ),
    )
}

#[cfg(test)]
mod tests {
    use crate::PersonaId;
    use crate::carry::{ACTION_TRANSPORT_EGRESS, DevicePublicKey, issue_device_grant_set};

    use super::*;

    const MASTER_SEED: [u8; 32] = [0x4d; 32];
    const NOW_MS: u64 = 1_770_000_000_000;

    fn persona(n: u128) -> PersonaId {
        PersonaId::from_uuid(uuid::Uuid::from_u128(n))
    }

    fn set(actions: &[&str], personas: &[PersonaId]) -> DeviceGrantSet {
        issue_device_grant_set(
            MASTER_SEED,
            crate::carry::DeviceId::from_uuid(uuid::Uuid::from_u128(0x2026_0812_0002)),
            DevicePublicKey([0x5a; 32]),
            actions,
            personas,
            60_000,
            NOW_MS,
        )
        .expect("issuing the grant set")
    }

    #[test]
    fn every_certificate_in_a_set_gets_its_own_revocation() {
        let set = set(
            &[ACTION_TRANSPORT_EGRESS, crate::carry::ACTION_IDENTITY_ACT],
            &[persona(1), persona(2)],
        );
        let statements = revoke_device_grant_set(MASTER_SEED, &set, NOW_MS + 1).unwrap();

        assert_eq!(statements.len(), 3, "one device certificate and two personas");
        assert!(statements.iter().all(|statement| statement.verify()));
    }

    /// The ledger refuses a statement whose declared issuer does not match the
    /// certificate's, so each statement has to be signed by the authority that
    /// granted it. This asserts the pairing rather than trusting it.
    #[test]
    fn a_revocation_is_signed_by_the_authority_that_granted_it() {
        let set = set(
            &[ACTION_TRANSPORT_EGRESS, crate::carry::ACTION_IDENTITY_ACT],
            &[persona(1)],
        );
        let statements = revoke_device_grant_set(MASTER_SEED, &set, NOW_MS + 1).unwrap();

        for statement in &statements {
            let target = statement.revocation.certificate;
            let certificate = set
                .certificates()
                .find(|candidate| candidate.certificate.id() == target)
                .expect("every statement withdraws a certificate in the set");
            assert_eq!(statement.revocation.issuer, certificate.certificate.issuer);
        }
    }

    #[test]
    fn a_station_set_revokes_with_one_statement() {
        let set = set(&[ACTION_TRANSPORT_EGRESS], &[]);
        let statements = revoke_device_grant_set(MASTER_SEED, &set, NOW_MS + 1).unwrap();

        assert_eq!(statements.len(), 1);
        assert!(statements[0].verify());
    }
}
