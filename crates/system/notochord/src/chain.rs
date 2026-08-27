// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The personae adapter: delegation-chain validation and the local
//! revocation ledger.
//!
//! Personae supplies the statement grammar (certificates, attenuation,
//! revocations); this module walks a presented chain against the owner's
//! accepted roots, depth budget, clock, and revocation ledger. It creates no
//! statements of its own.

use std::collections::BTreeMap;

use personae::delegation::{
    DelegationCertificate, DelegationId, DelegationParent, SignedDelegationCertificate,
    SignedDelegationRevocation,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::types::ChainFault;

/// A root authority this node accepts chains from.
///
/// A chain's first certificate must claim `DelegationParent::Root(authority)`
/// and be issued by exactly `issuer`; the pair is what the owner trusts, not
/// the authority id alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedRoot {
    /// Application-owned root authority id.
    pub authority: [u8; 32],
    /// Personae master public key entitled to issue under that authority.
    pub issuer: [u8; 32],
}

/// This node's record of certificates their issuers have withdrawn.
///
/// The ledger is local (plan D5): it folds verified revocation statements
/// and answers membership during chain validation. Revoking a parent
/// cascades to every chain below it, because every chain that relies on the
/// parent must present it and validation checks each presented certificate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RevocationLedger {
    revoked: BTreeMap<DelegationId, [u8; 32]>,
}

#[derive(Serialize, Deserialize)]
struct RevocationRecord {
    certificate: DelegationId,
    issuer: [u8; 32],
}

impl Serialize for RevocationLedger {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.revoked
            .iter()
            .map(|(certificate, issuer)| RevocationRecord {
                certificate: *certificate,
                issuer: *issuer,
            })
            .collect::<Vec<_>>()
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RevocationLedger {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let records = Vec::<RevocationRecord>::deserialize(deserializer)?;
        Ok(Self {
            revoked: records
                .into_iter()
                .map(|record| (record.certificate, record.issuer))
                .collect(),
        })
    }
}

impl RevocationLedger {
    /// An empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Verify one revocation statement and record it. Returns whether the
    /// statement verified; an unverifiable statement changes nothing.
    pub fn fold(&mut self, statement: &SignedDelegationRevocation) -> bool {
        if !statement.verify() {
            return false;
        }
        self.revoked.insert(
            statement.revocation.certificate,
            statement.revocation.issuer,
        );
        true
    }

    /// Whether this ledger revokes the given certificate.
    ///
    /// A recorded statement counts only when its declared issuer matches the
    /// certificate's issuer: nobody withdraws authority they did not grant.
    pub fn revokes(&self, certificate: &DelegationCertificate) -> bool {
        self.revoked.get(&certificate.id()) == Some(&certificate.issuer)
    }

    /// Number of recorded revocations.
    pub fn len(&self) -> usize {
        self.revoked.len()
    }

    /// Whether the ledger is empty.
    pub fn is_empty(&self) -> bool {
        self.revoked.is_empty()
    }
}

/// Validate one presented delegation chain, root grant first, subject last.
///
/// The checks, in order: chain presence and depth budget, every signature
/// and signer attestation, termination at a locally accepted root, link
/// integrity and strict attenuation at every step, revocation of any member,
/// validity window of every member at `now_ms`, and finally that the leaf
/// names `subject`. Whether the leaf covers a concrete path and action is
/// the caller's question, asked afterwards via
/// [`DelegationCertificate::covers`].
pub fn validate_chain(
    chain: &[SignedDelegationCertificate],
    subject: [u8; 32],
    trusted_roots: &[TrustedRoot],
    ledger: &RevocationLedger,
    max_depth: u16,
    now_ms: u64,
) -> Result<(), ChainFault> {
    if chain.is_empty() {
        return Err(ChainFault::Empty);
    }
    if chain.len() > usize::from(max_depth) {
        return Err(ChainFault::DepthExceeded);
    }
    for signed in chain {
        if !signed.verify() {
            return Err(ChainFault::BadSignature);
        }
    }

    let first = &chain[0].certificate;
    let anchored = match first.parent {
        DelegationParent::Root(authority) => trusted_roots
            .iter()
            .any(|root| root.authority == authority && root.issuer == first.issuer),
        DelegationParent::Certificate(_) => false,
    };
    if !anchored {
        return Err(ChainFault::UntrustedRoot);
    }

    for pair in chain.windows(2) {
        let parent = &pair[0].certificate;
        let child = &pair[1].certificate;
        if child.parent != DelegationParent::Certificate(parent.id())
            || child.issuer != parent.subject
        {
            return Err(ChainFault::BrokenLink);
        }
        if !child.attenuates(parent) {
            return Err(ChainFault::NotAttenuated);
        }
    }

    for signed in chain {
        let certificate = &signed.certificate;
        if ledger.revokes(certificate) {
            return Err(ChainFault::Revoked);
        }
        if now_ms < certificate.not_before_ms {
            return Err(ChainFault::NotYetValid);
        }
        if certificate
            .expires_at_ms
            .is_some_and(|expires| now_ms > expires)
        {
            return Err(ChainFault::Expired);
        }
    }

    let leaf = &chain[chain.len() - 1].certificate;
    if leaf.subject != subject {
        return Err(ChainFault::SubjectMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use personae::delegation::{CapabilityScope, DelegationRevocation};
    use personae::{IdentityProvider, InMemoryProvider};

    use super::*;

    fn scope() -> CapabilityScope {
        CapabilityScope {
            domain: "mere.network".into(),
            resource: vec![3; 32],
            path_prefix: "/services/murm".into(),
            actions: ["connect".to_string()].into_iter().collect(),
        }
    }

    #[test]
    fn an_unverifiable_revocation_is_not_folded() {
        let issuer = InMemoryProvider::from_seed([1; 32]);
        let signed = SignedDelegationRevocation::issue(
            &issuer,
            DelegationRevocation::new(
                DelegationId([8; 32]),
                issuer.master_public_key().to_bytes(),
                scope(),
                50,
                [5; 32],
            ),
        )
        .unwrap();

        let mut tampered = signed.clone();
        tampered.revocation.at_ms = 51;
        let mut ledger = RevocationLedger::new();
        assert!(!ledger.fold(&tampered));
        assert!(ledger.is_empty());
        assert!(ledger.fold(&signed));
        assert_eq!(ledger.len(), 1);
    }

    #[test]
    fn a_revocation_by_a_different_issuer_does_not_count() {
        let issuer = InMemoryProvider::from_seed([1; 32]);
        let stranger = InMemoryProvider::from_seed([2; 32]);
        let certificate = DelegationCertificate::new(
            DelegationParent::Root([7; 32]),
            issuer.master_public_key().to_bytes(),
            [9; 32],
            scope(),
            5,
            10,
            Some(100),
            1,
            [1; 32],
        );
        let signed = SignedDelegationRevocation::issue(
            &stranger,
            DelegationRevocation::new(
                certificate.id(),
                stranger.master_public_key().to_bytes(),
                scope(),
                50,
                [5; 32],
            ),
        )
        .unwrap();

        let mut ledger = RevocationLedger::new();
        assert!(ledger.fold(&signed));
        assert!(
            !ledger.revokes(&certificate),
            "a stranger cannot withdraw authority they did not grant"
        );
    }
}
