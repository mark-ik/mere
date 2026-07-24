//! [`IdentityProvider`] trait and reference implementations.

use std::path::Path;

use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::sealed_record_storage::SealedRecordStorage;
use crate::{Ed25519Keypair, Ed25519PublicKey, Ed25519Signature, IdentityError};

const SEALED_IDENTITY_FORMAT_VERSION: u8 = 1;
const DERIVED_KEY_ATTESTATION_VERSION: u16 = 1;
const DERIVED_KEY_ATTESTATION_DOMAIN: &[u8] = b"personae/derived-key-attestation/v1";

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct SealedIdentityRecord {
    format_version: u8,
    master_seed: [u8; 32],
}

/// A master-signed statement binding one deterministically derived key to its
/// identity root and derivation salt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedKeyAttestation {
    format_version: u16,
    master: [u8; 32],
    derived: [u8; 32],
    signature: Vec<u8>,
}

impl DerivedKeyAttestation {
    /// The durable master public key that authorized the derived key.
    pub fn master_public_key(&self) -> Result<Ed25519PublicKey, IdentityError> {
        Ed25519PublicKey::from_bytes(&self.master)
    }

    /// The derived public key authorized by the master.
    pub fn derived_public_key(&self) -> Result<Ed25519PublicKey, IdentityError> {
        Ed25519PublicKey::from_bytes(&self.derived)
    }

    /// Verify the master signature and the supplied derivation salt.
    pub fn verify(&self, salt: &[u8]) -> bool {
        if self.format_version != DERIVED_KEY_ATTESTATION_VERSION {
            return false;
        }
        let Ok(master) = self.master_public_key() else {
            return false;
        };
        let Ok(signature_bytes) = <[u8; 64]>::try_from(self.signature.as_slice()) else {
            return false;
        };
        let signature = Ed25519Signature::from_bytes(&signature_bytes);
        master.verify(&derived_key_attestation_message(self, salt), &signature)
    }
}

/// A source of identity for the Mere browser.
///
/// Implementors hold the user's master Ed25519 keypair and derive
/// per-protocol keypairs deterministically from a master secret + a
/// protocol-specific salt. **The master secret never leaves the provider.**
///
/// [`SealedIdentityProvider`] is the production implementation for hosts that
/// already have an unlocked [`SealedRecordStorage`]. [`InMemoryProvider`] is
/// intended for tests, ephemeral runtimes, and standalone use.
///
/// ## Why a trait?
///
/// Identity has two stable concerns (master public key + per-protocol
/// derivation) and one platform-specific concern (where the master secret
/// is stored: keychain, in-memory, hardware token, etc.). The trait
/// separates them.
pub trait IdentityProvider: Send + Sync {
    /// The master public key.
    ///
    /// In Mere, the iroh `NodeId` is derived from this (per
    /// [`transport`](https://crates.io/crates/transport)).
    fn master_public_key(&self) -> Ed25519PublicKey;

    /// Derive a per-protocol keypair from a salt.
    ///
    /// Derivation is `BLAKE3-keyed(master_seed, salt)`, the result
    /// becoming the seed of a new Ed25519 keypair (see
    /// [`Ed25519Keypair::derive_child`]).
    ///
    /// Salts are protocol- or use-case-specific. Typical salts:
    /// - Cable: the cabal key (32 bytes)
    /// - MLS: the MLS group identifier
    /// - Co-op: the session identifier
    fn derive_keypair(&self, salt: &[u8]) -> Result<Ed25519Keypair, IdentityError>;

    /// Certify the key derived from `salt` under this identity's master key.
    fn attest_derived_key(&self, salt: &[u8]) -> Result<DerivedKeyAttestation, IdentityError>;
}

pub(crate) fn attest_derived_key(master: &Ed25519Keypair, salt: &[u8]) -> DerivedKeyAttestation {
    let derived = master.derive_child(salt).public_key().to_bytes();
    let mut attestation = DerivedKeyAttestation {
        format_version: DERIVED_KEY_ATTESTATION_VERSION,
        master: master.public_key().to_bytes(),
        derived,
        signature: Vec::new(),
    };
    attestation.signature = master
        .sign(&derived_key_attestation_message(&attestation, salt))
        .to_bytes()
        .to_vec();
    attestation
}

fn derived_key_attestation_message(attestation: &DerivedKeyAttestation, salt: &[u8]) -> Vec<u8> {
    let mut message =
        Vec::with_capacity(DERIVED_KEY_ATTESTATION_DOMAIN.len() + 2 + 8 + salt.len() + 32 + 32);
    message.extend_from_slice(DERIVED_KEY_ATTESTATION_DOMAIN);
    message.extend_from_slice(&attestation.format_version.to_le_bytes());
    message.extend_from_slice(&(salt.len() as u64).to_le_bytes());
    message.extend_from_slice(salt);
    message.extend_from_slice(&attestation.master);
    message.extend_from_slice(&attestation.derived);
    message
}

/// An identity provider loaded from a versioned sealed record.
///
/// The caller owns unlock policy and path selection. This provider owns key
/// generation, the seed record format, and immediate scrubbing of temporary
/// seed copies. Its master key remains in memory only while the provider lives.
pub struct SealedIdentityProvider {
    master: Ed25519Keypair,
}

impl SealedIdentityProvider {
    /// Load an existing identity from `record_path`, or create and seal one.
    pub fn load_or_create(
        storage: &SealedRecordStorage,
        record_path: impl AsRef<Path>,
    ) -> Result<Self, IdentityError> {
        let record_path = record_path.as_ref();
        let mut record = match storage.load_record::<SealedIdentityRecord>(record_path)? {
            Some(record) => {
                if record.format_version != SEALED_IDENTITY_FORMAT_VERSION {
                    return Err(IdentityError::Backend(format!(
                        "unsupported sealed identity version {} at {:?}",
                        record.format_version, record_path
                    )));
                }
                record
            }
            None => {
                let master = Ed25519Keypair::generate(&mut OsRng);
                let record = SealedIdentityRecord {
                    format_version: SEALED_IDENTITY_FORMAT_VERSION,
                    master_seed: master.to_seed(),
                };
                storage.save_record(record_path, &record)?;
                record
            }
        };
        let master = Ed25519Keypair::from_seed(record.master_seed);
        record.master_seed.zeroize();
        Ok(Self { master })
    }

    /// Borrow the master keypair for transport implementations that require it.
    pub fn master_keypair(&self) -> &Ed25519Keypair {
        &self.master
    }
}

impl IdentityProvider for SealedIdentityProvider {
    fn master_public_key(&self) -> Ed25519PublicKey {
        self.master.public_key()
    }

    fn derive_keypair(&self, salt: &[u8]) -> Result<Ed25519Keypair, IdentityError> {
        Ok(self.master.derive_child(salt))
    }

    fn attest_derived_key(&self, salt: &[u8]) -> Result<DerivedKeyAttestation, IdentityError> {
        Ok(attest_derived_key(&self.master, salt))
    }
}

/// In-memory identity provider for tests, standalone runtimes, and ephemeral
/// uses.
///
/// Holds the master keypair in memory; never persisted. Production code
/// should use a keychain-backed provider implemented by the host (e.g.
/// `graphshell`'s desktop keychain backend).
///
/// **Not intended for production**. The master keypair is lost when the
/// provider drops; if you need persistent identity across runs you need a
/// persistent backend.
pub struct InMemoryProvider {
    master: Ed25519Keypair,
}

impl InMemoryProvider {
    /// Create a new provider with a freshly-generated random master keypair.
    pub fn random() -> Self {
        Self {
            master: Ed25519Keypair::generate(&mut OsRng),
        }
    }

    /// Create from a 32-byte master seed.
    ///
    /// Useful for test reproducibility and for scenarios where the master
    /// secret is provisioned out-of-band.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            master: Ed25519Keypair::from_seed(seed),
        }
    }

    /// Borrow the master keypair.
    ///
    /// Exposed for transport-layer use — the iroh QUIC handshake needs the
    /// master signing key to authenticate the local node. Most consumers
    /// should use [`derive_keypair`](IdentityProvider::derive_keypair)
    /// instead; this escape hatch is for transport identity specifically.
    pub fn master_keypair(&self) -> &Ed25519Keypair {
        &self.master
    }
}

impl IdentityProvider for InMemoryProvider {
    fn master_public_key(&self) -> Ed25519PublicKey {
        self.master.public_key()
    }

    fn derive_keypair(&self, salt: &[u8]) -> Result<Ed25519Keypair, IdentityError> {
        Ok(self.master.derive_child(salt))
    }

    fn attest_derived_key(&self, salt: &[u8]) -> Result<DerivedKeyAttestation, IdentityError> {
        Ok(attest_derived_key(&self.master, salt))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_provider_is_stable_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let storage = SealedRecordStorage::open_with_key(dir.path(), [0x45; 32]);
        let first =
            SealedIdentityProvider::load_or_create(&storage, "identity/default.json").unwrap();
        let first_public = first.master_public_key();
        drop(first);

        let second =
            SealedIdentityProvider::load_or_create(&storage, "identity/default.json").unwrap();
        assert_eq!(second.master_public_key(), first_public);
    }

    #[test]
    fn sealed_provider_rejects_the_wrong_record_root() {
        let dir = tempfile::tempdir().unwrap();
        let storage = SealedRecordStorage::open_with_key(dir.path(), [0x45; 32]);
        SealedIdentityProvider::load_or_create(&storage, "identity/default.json").unwrap();

        let wrong_storage = SealedRecordStorage::open_with_key(dir.path(), [0x46; 32]);
        let error = SealedIdentityProvider::load_or_create(&wrong_storage, "identity/default.json")
            .err()
            .expect("wrong root should fail");
        assert!(error.to_string().contains("decrypt sealed record"));
    }

    #[test]
    fn derived_key_attestation_binds_master_key_and_salt() {
        let provider = InMemoryProvider::from_seed([0x31; 32]);
        let attestation = provider.attest_derived_key(b"strophe/session/one").unwrap();

        assert!(attestation.verify(b"strophe/session/one"));
        assert!(!attestation.verify(b"strophe/session/two"));
        assert_eq!(
            attestation.master_public_key().unwrap(),
            provider.master_public_key()
        );
        assert_eq!(
            attestation.derived_public_key().unwrap(),
            provider
                .derive_keypair(b"strophe/session/one")
                .unwrap()
                .public_key()
        );
    }

    #[test]
    fn derived_key_attestation_rejects_tampering() {
        let provider = InMemoryProvider::from_seed([0x31; 32]);
        let mut attestation = provider.attest_derived_key(b"strophe/session/one").unwrap();
        attestation.derived[0] ^= 1;

        assert!(!attestation.verify(b"strophe/session/one"));
    }
}
