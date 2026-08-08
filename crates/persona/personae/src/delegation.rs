//! Portable, signed capability delegation.
//!
//! This module owns the identity proof and attenuation grammar shared by
//! applications. It does not own an application's grant ledger or policy:
//! Murm may keep session grants in memory, while Gemot or Kith may fold the
//! same statements through durable replicated stores.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{DerivedKeyAttestation, Ed25519PublicKey, Ed25519Signature, IdentityProvider};

const CERTIFICATE_VERSION: u16 = 1;
const REVOCATION_VERSION: u16 = 1;
const CERTIFICATE_DOMAIN: &[u8] = b"personae/delegation-certificate/v1";
const REVOCATION_DOMAIN: &[u8] = b"personae/delegation-revocation/v1";
const SIGNING_KEY_DOMAIN: &[u8] = b"personae/delegation-signing-key/v1";

/// Stable content identifier for one delegation certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DelegationId(pub [u8; 32]);

/// The authority above a delegation certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DelegationParent {
    /// Application-owned root authority, such as a constitutional grant.
    Root([u8; 32]),
    /// Another independently signed delegation certificate.
    Certificate(DelegationId),
}

/// A capability target shared across application-specific authority ledgers.
///
/// `domain` separates applications (`moot`, `murm.coop`, `mesh`); `resource`
/// identifies one space within that application; `path_prefix` selects a
/// structural subset; and `actions` remains an application-owned vocabulary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityScope {
    /// Application or protocol family which owns the action vocabulary.
    pub domain: String,
    /// Opaque application-owned identity of the governed space.
    pub resource: Vec<u8>,
    /// Inclusive structural path prefix within the resource.
    pub path_prefix: String,
    /// Application-owned actions allowed under this scope.
    pub actions: BTreeSet<String>,
}

impl CapabilityScope {
    /// Whether this scope is a valid narrowing of `parent`.
    pub fn attenuates(&self, parent: &Self) -> bool {
        self.is_well_formed()
            && parent.is_well_formed()
            && self.domain == parent.domain
            && self.resource == parent.resource
            && path_covers(&parent.path_prefix, &self.path_prefix)
            && self.actions.is_subset(&parent.actions)
    }

    fn is_well_formed(&self) -> bool {
        !self.domain.is_empty()
            && !self.resource.is_empty()
            && !self.path_prefix.is_empty()
            && !self.actions.is_empty()
            && self.actions.iter().all(|action| !action.is_empty())
    }
}

/// The signed content of an independently delegated capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationCertificate {
    version: u16,
    /// Constitutional, session, mesh, or preceding-certificate authority.
    pub parent: DelegationParent,
    /// Personae master public key which currently holds the parent authority.
    pub issuer: [u8; 32],
    /// Personae master public key receiving this capability.
    pub subject: [u8; 32],
    /// Application/resource/path/action subset carried by this certificate.
    pub scope: CapabilityScope,
    /// First millisecond at which the certificate may be used.
    pub not_before_ms: u64,
    /// Issuer-asserted creation time committed by the signature.
    pub issued_at_ms: u64,
    /// Last usable millisecond, or unbounded when the parent is also unbounded.
    pub expires_at_ms: Option<u64>,
    /// Further delegation steps available to the subject. Zero forbids it.
    pub remaining_delegation_depth: u16,
    /// Issuer-chosen uniqueness, allowing two otherwise identical grants.
    pub nonce: [u8; 32],
}

impl DelegationCertificate {
    /// Construct a versioned certificate payload.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        parent: DelegationParent,
        issuer: [u8; 32],
        subject: [u8; 32],
        scope: CapabilityScope,
        issued_at_ms: u64,
        not_before_ms: u64,
        expires_at_ms: Option<u64>,
        remaining_delegation_depth: u16,
        nonce: [u8; 32],
    ) -> Self {
        Self {
            version: CERTIFICATE_VERSION,
            parent,
            issuer,
            subject,
            scope,
            issued_at_ms,
            not_before_ms,
            expires_at_ms,
            remaining_delegation_depth,
            nonce,
        }
    }

    /// Stable content id of this certificate payload.
    pub fn id(&self) -> DelegationId {
        DelegationId(*blake3::hash(&self.signing_bytes()).as_bytes())
    }

    /// Whether this certificate is structurally valid on its own.
    pub fn is_well_formed(&self) -> bool {
        self.version == CERTIFICATE_VERSION
            && self.issuer != [0; 32]
            && self.subject != [0; 32]
            && self.scope.is_well_formed()
            && self.issued_at_ms <= self.not_before_ms
            && self
                .expires_at_ms
                .is_none_or(|expires| expires >= self.not_before_ms)
    }

    /// Whether this certificate narrows an already-valid parent certificate.
    pub fn attenuates(&self, parent: &Self) -> bool {
        self.is_well_formed()
            && parent.is_well_formed()
            && self.parent == DelegationParent::Certificate(parent.id())
            && self.issuer == parent.subject
            && self.scope.attenuates(&parent.scope)
            && self.not_before_ms >= parent.not_before_ms
            && expiry_within(self.expires_at_ms, parent.expires_at_ms)
            && parent.remaining_delegation_depth > 0
            && self.remaining_delegation_depth < parent.remaining_delegation_depth
    }

    /// Whether the certificate covers one action at the evaluation time.
    pub fn covers(&self, path: &str, action: &str, at_ms: u64) -> bool {
        self.is_well_formed()
            && at_ms >= self.not_before_ms
            && self.expires_at_ms.is_none_or(|expires| at_ms <= expires)
            && path_covers(&self.scope.path_prefix, path)
            && self.scope.actions.contains(action)
    }

    fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        push_bytes(&mut bytes, CERTIFICATE_DOMAIN);
        bytes.extend_from_slice(&self.version.to_le_bytes());
        match self.parent {
            DelegationParent::Root(id) => {
                bytes.push(0);
                bytes.extend_from_slice(&id);
            }
            DelegationParent::Certificate(id) => {
                bytes.push(1);
                bytes.extend_from_slice(&id.0);
            }
        }
        bytes.extend_from_slice(&self.issuer);
        bytes.extend_from_slice(&self.subject);
        push_str(&mut bytes, &self.scope.domain);
        push_bytes(&mut bytes, &self.scope.resource);
        push_str(&mut bytes, &self.scope.path_prefix);
        bytes.extend_from_slice(&(self.scope.actions.len() as u64).to_le_bytes());
        for action in &self.scope.actions {
            push_str(&mut bytes, action);
        }
        bytes.extend_from_slice(&self.issued_at_ms.to_le_bytes());
        bytes.extend_from_slice(&self.not_before_ms.to_le_bytes());
        push_expiry(&mut bytes, self.expires_at_ms);
        bytes.extend_from_slice(&self.remaining_delegation_depth.to_le_bytes());
        bytes.extend_from_slice(&self.nonce);
        bytes
    }
}

/// A certificate plus proof that its signing key belongs to the issuer root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedDelegationCertificate {
    /// Capability statement committed by the signature.
    pub certificate: DelegationCertificate,
    /// Master-signed proof of the resource-scoped derived signing key.
    pub signer: DerivedKeyAttestation,
    signature: Vec<u8>,
}

impl SignedDelegationCertificate {
    /// Sign a certificate with a domain- and resource-scoped derived key.
    pub fn issue<P: IdentityProvider>(
        provider: &P,
        certificate: DelegationCertificate,
    ) -> Result<Self, DelegationError> {
        if !certificate.is_well_formed() {
            return Err(DelegationError::MalformedCertificate);
        }
        if provider.master_public_key().to_bytes() != certificate.issuer {
            return Err(DelegationError::WrongIssuer);
        }
        let salt = delegation_signing_salt(&certificate.scope);
        let signer = provider
            .attest_derived_key(&salt)
            .map_err(|_| DelegationError::Identity)?;
        let keypair = provider
            .derive_keypair(&salt)
            .map_err(|_| DelegationError::Identity)?;
        let signature = keypair
            .sign(&certificate.signing_bytes())
            .to_bytes()
            .to_vec();
        Ok(Self {
            certificate,
            signer,
            signature,
        })
    }

    /// Verify payload, issuer binding, derived-key attestation, and signature.
    pub fn verify(&self) -> bool {
        if !self.certificate.is_well_formed() {
            return false;
        }
        let salt = delegation_signing_salt(&self.certificate.scope);
        if !self.signer.verify(&salt) {
            return false;
        }
        let Ok(master) = self.signer.master_public_key() else {
            return false;
        };
        if master.to_bytes() != self.certificate.issuer {
            return false;
        }
        verify_signature(
            self.signer.derived_public_key(),
            &self.signature,
            &self.certificate.signing_bytes(),
        )
    }
}

/// Signed removal of one certificate by the identity which issued it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationRevocation {
    version: u16,
    /// Certificate withdrawn by this statement.
    pub certificate: DelegationId,
    /// Identity root which originally issued the certificate.
    pub issuer: [u8; 32],
    /// Original certificate scope, binding the same derived signing key.
    pub scope: CapabilityScope,
    /// Issuer-chosen uniqueness for this revocation statement.
    pub nonce: [u8; 32],
    /// Issuer-asserted revocation time committed by the signature.
    pub at_ms: u64,
}

impl DelegationRevocation {
    /// Construct a versioned revocation payload.
    pub fn new(
        certificate: DelegationId,
        issuer: [u8; 32],
        scope: CapabilityScope,
        at_ms: u64,
        nonce: [u8; 32],
    ) -> Self {
        Self {
            version: REVOCATION_VERSION,
            certificate,
            issuer,
            scope,
            at_ms,
            nonce,
        }
    }

    fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        push_bytes(&mut bytes, REVOCATION_DOMAIN);
        bytes.extend_from_slice(&self.version.to_le_bytes());
        bytes.extend_from_slice(&self.certificate.0);
        bytes.extend_from_slice(&self.issuer);
        push_str(&mut bytes, &self.scope.domain);
        push_bytes(&mut bytes, &self.scope.resource);
        push_str(&mut bytes, &self.scope.path_prefix);
        bytes.extend_from_slice(&(self.scope.actions.len() as u64).to_le_bytes());
        for action in &self.scope.actions {
            push_str(&mut bytes, action);
        }
        bytes.extend_from_slice(&self.at_ms.to_le_bytes());
        bytes.extend_from_slice(&self.nonce);
        bytes
    }
}

/// A verified revocation can be folded by any application-owned grant ledger.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedDelegationRevocation {
    /// Revocation statement committed by the signature.
    pub revocation: DelegationRevocation,
    /// Master-signed proof of the resource-scoped derived signing key.
    pub signer: DerivedKeyAttestation,
    signature: Vec<u8>,
}

impl SignedDelegationRevocation {
    /// Sign a revocation with the same resource-scoped key as issuance.
    pub fn issue<P: IdentityProvider>(
        provider: &P,
        revocation: DelegationRevocation,
    ) -> Result<Self, DelegationError> {
        if revocation.version != REVOCATION_VERSION || !revocation.scope.is_well_formed() {
            return Err(DelegationError::MalformedRevocation);
        }
        if provider.master_public_key().to_bytes() != revocation.issuer {
            return Err(DelegationError::WrongIssuer);
        }
        let salt = delegation_signing_salt(&revocation.scope);
        let signer = provider
            .attest_derived_key(&salt)
            .map_err(|_| DelegationError::Identity)?;
        let keypair = provider
            .derive_keypair(&salt)
            .map_err(|_| DelegationError::Identity)?;
        let signature = keypair
            .sign(&revocation.signing_bytes())
            .to_bytes()
            .to_vec();
        Ok(Self {
            revocation,
            signer,
            signature,
        })
    }

    /// Verify payload, issuer binding, derived-key attestation, and signature.
    pub fn verify(&self) -> bool {
        if self.revocation.version != REVOCATION_VERSION || !self.revocation.scope.is_well_formed()
        {
            return false;
        }
        let salt = delegation_signing_salt(&self.revocation.scope);
        if !self.signer.verify(&salt) {
            return false;
        }
        let Ok(master) = self.signer.master_public_key() else {
            return false;
        };
        if master.to_bytes() != self.revocation.issuer {
            return false;
        }
        verify_signature(
            self.signer.derived_public_key(),
            &self.signature,
            &self.revocation.signing_bytes(),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
/// Failure while issuing a signed delegation statement.
pub enum DelegationError {
    /// Required certificate fields or time bounds are invalid.
    #[error("delegation certificate is malformed")]
    MalformedCertificate,
    /// Required revocation fields are invalid.
    #[error("delegation revocation is malformed")]
    MalformedRevocation,
    /// The supplied identity is not the statement's declared issuer.
    #[error("signing identity does not match delegation issuer")]
    WrongIssuer,
    /// The identity provider could not produce its scoped signing proof.
    #[error("identity provider could not derive delegation signer")]
    Identity,
}

/// Deterministic salt for the signing key assigned to one authority resource.
///
/// Hosts use this when the same derived key signs an outer transport envelope.
pub fn delegation_signing_salt(scope: &CapabilityScope) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_bytes(&mut bytes, SIGNING_KEY_DOMAIN);
    push_str(&mut bytes, &scope.domain);
    push_bytes(&mut bytes, &scope.resource);
    bytes
}

/// Whether `prefix` selects `path` at a structural slash boundary.
///
/// A scope for `/publications/a` reaches `/publications/a/version`, but not
/// `/publications/a-private`. Application service routing uses the same rule
/// when a base service owns resource-specific children.
pub fn path_covers(prefix: &str, path: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn expiry_within(child: Option<u64>, parent: Option<u64>) -> bool {
    match (child, parent) {
        (_, None) => true,
        (Some(child), Some(parent)) => child <= parent,
        (None, Some(_)) => false,
    }
}

fn verify_signature(
    public_key: Result<Ed25519PublicKey, crate::IdentityError>,
    signature: &[u8],
    message: &[u8],
) -> bool {
    let Ok(public_key) = public_key else {
        return false;
    };
    let Ok(signature) = <[u8; 64]>::try_from(signature) else {
        return false;
    };
    public_key.verify(message, &Ed25519Signature::from_bytes(&signature))
}

fn push_str(bytes: &mut Vec<u8>, value: &str) {
    push_bytes(bytes, value.as_bytes());
}

fn push_bytes(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
    bytes.extend_from_slice(value);
}

fn push_expiry(bytes: &mut Vec<u8>, value: Option<u64>) {
    match value {
        Some(value) => {
            bytes.push(1);
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        None => bytes.push(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryProvider;

    fn scope(path: &str, actions: &[&str]) -> CapabilityScope {
        CapabilityScope {
            domain: "moot".into(),
            resource: vec![9; 32],
            path_prefix: path.into(),
            actions: actions.iter().map(|action| (*action).into()).collect(),
        }
    }

    fn certificate(
        provider: &InMemoryProvider,
        parent: DelegationParent,
        subject: [u8; 32],
        path: &str,
        actions: &[&str],
        depth: u16,
    ) -> DelegationCertificate {
        DelegationCertificate::new(
            parent,
            provider.master_public_key().to_bytes(),
            subject,
            scope(path, actions),
            5,
            10,
            Some(100),
            depth,
            [depth as u8 + 1; 32],
        )
    }

    #[test]
    fn certificate_verifies_and_binds_every_signed_field() {
        let issuer = InMemoryProvider::from_seed([1; 32]);
        let subject = InMemoryProvider::from_seed([2; 32]);
        let signed = SignedDelegationCertificate::issue(
            &issuer,
            certificate(
                &issuer,
                DelegationParent::Root([7; 32]),
                subject.master_public_key().to_bytes(),
                "moot/fauna",
                &["read", "write"],
                2,
            ),
        )
        .unwrap();
        assert!(signed.verify());

        let mut tampered = signed.clone();
        tampered.certificate.scope.path_prefix = "moot/secret".into();
        assert!(!tampered.verify());
    }

    #[test]
    fn child_must_narrow_scope_time_and_delegation_depth() {
        let root_holder = InMemoryProvider::from_seed([1; 32]);
        let child_holder = InMemoryProvider::from_seed([2; 32]);
        let leaf = InMemoryProvider::from_seed([3; 32]);
        let parent = certificate(
            &root_holder,
            DelegationParent::Root([7; 32]),
            child_holder.master_public_key().to_bytes(),
            "moot/fauna",
            &["read", "write"],
            2,
        );
        let child = certificate(
            &child_holder,
            DelegationParent::Certificate(parent.id()),
            leaf.master_public_key().to_bytes(),
            "moot/fauna/research",
            &["read"],
            1,
        );
        assert!(child.attenuates(&parent));

        let mut widened = child.clone();
        widened.scope.path_prefix = "moot".into();
        assert!(!widened.attenuates(&parent));
        let mut endless = child.clone();
        endless.expires_at_ms = None;
        assert!(!endless.attenuates(&parent));
        let mut deep = child;
        deep.remaining_delegation_depth = 2;
        assert!(!deep.attenuates(&parent));
    }

    #[test]
    fn revocation_is_bound_to_issuer_scope_and_target() {
        let issuer = InMemoryProvider::from_seed([1; 32]);
        let signed = SignedDelegationRevocation::issue(
            &issuer,
            DelegationRevocation::new(
                DelegationId([8; 32]),
                issuer.master_public_key().to_bytes(),
                scope("moot/fauna", &["write"]),
                50,
                [5; 32],
            ),
        )
        .unwrap();
        assert!(signed.verify());

        let mut tampered = signed;
        tampered.revocation.certificate = DelegationId([9; 32]);
        assert!(!tampered.verify());
    }
}
