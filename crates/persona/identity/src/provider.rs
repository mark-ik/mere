//! [`IdentityProvider`] trait and reference implementations.

use rand_core::OsRng;

use crate::{Ed25519Keypair, Ed25519PublicKey, IdentityError};

/// A source of identity for the Mere browser.
///
/// Implementors hold the user's master Ed25519 keypair and derive
/// per-protocol keypairs deterministically from a master secret + a
/// protocol-specific salt. **The master secret never leaves the provider.**
///
/// The canonical production implementation is keychain-backed (Windows
/// DPAPI, macOS Keychain Services, Linux Secret Service, etc.) and is
/// provided by host-platform code in `graphshell`. The
/// [`InMemoryProvider`] in this crate is intended for tests, ephemeral
/// runtimes, and standalone use.
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
    /// Derivation is BLAKE2b-256 of `master_seed || salt`, the result
    /// becoming the seed of a new Ed25519 keypair (see
    /// [`Ed25519Keypair::derive_child`]). Per the inherited Cable spec §2.2.
    ///
    /// Salts are protocol- or use-case-specific. Typical salts:
    /// - Cable: the cabal key (32 bytes)
    /// - MLS: the MLS group identifier
    /// - Co-op: the session identifier
    fn derive_keypair(&self, salt: &[u8]) -> Result<Ed25519Keypair, IdentityError>;
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
}
