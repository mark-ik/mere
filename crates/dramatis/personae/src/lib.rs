//! # personae
//!
//! The identity and carry layer for the Merely ecosystem. personae owns a
//! person's **faces**: the master Ed25519 keypair, deterministic per-protocol
//! key derivation (BLAKE3 keyed-hash), the passphrase- and OS-store-unlocked
//! vault, and sealed-record storage for secrets at rest. A human has *personae*,
//! plural — a work face, a research face, a burner — so the crate is the
//! register of them and the root of trust they derive from.
//!
//! Promoted from mere's `persona/identity`. The carry layer (device roster,
//! capability grants, private-epoch history — the portable-persona spine) folds
//! in as it lifts out of mere's `session-runtime`; personae is the whole
//! trust-plane root, not only the key primitives.
//!
//! ## Quick start
//!
//! ```
//! use personae::{IdentityProvider, InMemoryProvider};
//!
//! let provider = InMemoryProvider::random();
//! let _master_pubkey = provider.master_public_key();
//!
//! // Derive a per-protocol keypair from a salt (e.g. a cabal key).
//! let cabal_salt = b"my-cabal-key-32-byte-salt-here.";
//! let cabal_keypair = provider.derive_keypair(cabal_salt).unwrap();
//!
//! // Sign and verify.
//! let msg = b"hello, cabal";
//! let sig = cabal_keypair.sign(msg);
//! assert!(cabal_keypair.public_key().verify(msg, &sig));
//! ```
//!
//! ## Design notes
//!
//! - **The master secret never leaves the [`IdentityProvider`].** All key
//!   derivation happens inside the provider; consumers receive only the derived
//!   keypair.
//! - **Derivation is `BLAKE3-keyed(master_seed, salt)` → Ed25519 seed** (the
//!   per-protocol-derivation pattern).
//! - **Pure-Rust crypto** — `ed25519-dalek` + `blake3`, no libsodium.
//! - **At-rest security** — `PassphraseEncryptedStorage` (Argon2id +
//!   ChaCha20-Poly1305) and a sealed-record store, unlocked by passphrase or an
//!   OS store (Windows DPAPI today; other platforms follow).
//!
//! ## Status
//!
//! Pre-1.0. Promoted verbatim; the trait surface stabilizes before 0.2.0.

#![doc(html_root_url = "https://docs.rs/personae/0.1.0")]
#![warn(missing_docs)]

#[cfg(feature = "agent")]
pub mod agent;
pub mod bootstrap;
pub mod carry;
pub mod delegation;
mod error;
mod keypair;
pub mod passphrase_root;
pub mod passphrase_storage;
mod profile_wire;
mod provider;
pub mod roster;
pub mod seal;
pub mod sealed_profile_storage;
pub mod sealed_record_storage;
#[cfg(feature = "agent")]
pub mod signing;
#[cfg(feature = "ssh")]
pub mod ssh_slot;
pub mod startup_unlock;
pub mod vault;

pub use crate::error::IdentityError;
pub use crate::keypair::{Ed25519Keypair, Ed25519PublicKey, Ed25519Signature};
pub use crate::passphrase_root::{
    PassphraseWrappedRoot, change_passphrase, load_passphrase_root, passphrase_root_exists,
    save_passphrase_root, unwrap_vault_root, wrap_vault_root,
};
pub use crate::passphrase_storage::PassphraseEncryptedStorage;
pub use crate::provider::{
    DerivedKeyAttestation, IdentityProvider, InMemoryProvider, SealedIdentityProvider,
};
pub use crate::roster::{OpenedVault, Roster, RosterEntry, open_shared};
pub use crate::seal::{seal_bytes, unseal_bytes};
pub use crate::sealed_profile_storage::SealedProfileStorage;
pub use crate::sealed_record_storage::SealedRecordStorage;
pub use crate::startup_unlock::{
    StartupUnlockMode, auto_unlock_backend_available, load_or_create_auto_unlock_root,
};
pub use crate::vault::{
    CredentialLineage, IdentitySlot, IdentityStorage, IdentityVault, InMemoryStorage, Profile,
    ProfileId, ProfileSummary, ProtocolKey, SecretBytes, UnlockTier,
};

/// Identity of a persona — the user's mode-scoped identity boundary.
///
/// v0 uses [`PersonaId::default_persona`] for every session. v1 promotes
/// this into user-managed persona manifests and per-persona vault roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PersonaId(pub uuid::Uuid);

impl PersonaId {
    /// Mint a fresh persona id.
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Wrap an existing UUID as a persona id.
    pub fn from_uuid(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }

    /// Borrow the underlying UUID.
    pub fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }

    /// The single default persona used by every session in v0.
    ///
    /// Fixed UUID so manifests are stable across app restarts before personas
    /// become user-managed.
    pub fn default_persona() -> Self {
        Self(uuid::Uuid::from_u128(
            0x0000_0000_0000_0000_0000_0000_0000_0001,
        ))
    }
}

impl Default for PersonaId {
    fn default() -> Self {
        Self::default_persona()
    }
}

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_provider_signs_and_verifies() {
        let provider = InMemoryProvider::random();
        let kp = provider.derive_keypair(b"test-salt").unwrap();
        let msg = b"hello, mere";
        let sig = kp.sign(msg);
        assert!(kp.public_key().verify(msg, &sig));
    }

    #[test]
    fn derivation_is_deterministic() {
        let seed = [42u8; 32];
        let p1 = InMemoryProvider::from_seed(seed);
        let p2 = InMemoryProvider::from_seed(seed);

        let kp1 = p1.derive_keypair(b"cabal-1").unwrap();
        let kp2 = p2.derive_keypair(b"cabal-1").unwrap();

        assert_eq!(kp1.public_key().to_bytes(), kp2.public_key().to_bytes());
    }

    #[test]
    fn different_salts_yield_different_keys() {
        let p = InMemoryProvider::from_seed([7u8; 32]);
        let k1 = p.derive_keypair(b"cabal-1").unwrap();
        let k2 = p.derive_keypair(b"cabal-2").unwrap();
        assert_ne!(k1.public_key().to_bytes(), k2.public_key().to_bytes());
    }

    #[test]
    fn master_public_key_is_stable() {
        let p = InMemoryProvider::from_seed([13u8; 32]);
        let pk1 = p.master_public_key();
        let pk2 = p.master_public_key();
        assert_eq!(pk1.to_bytes(), pk2.to_bytes());
    }

    #[test]
    fn signature_round_trips_through_bytes() {
        let p = InMemoryProvider::from_seed([0u8; 32]);
        let kp = p.derive_keypair(b"x").unwrap();
        let sig = kp.sign(b"msg");
        let bytes = sig.to_bytes();
        let recovered = Ed25519Signature::from_bytes(&bytes);
        assert!(kp.public_key().verify(b"msg", &recovered));
    }

    #[test]
    fn public_key_round_trips_through_bytes() {
        let p = InMemoryProvider::from_seed([1u8; 32]);
        let pk = p.master_public_key();
        let bytes = pk.to_bytes();
        let recovered = Ed25519PublicKey::from_bytes(&bytes).unwrap();
        assert_eq!(pk.to_bytes(), recovered.to_bytes());
    }

    // Note: deliberate "rejected bytes" test removed — finding bytes that
    // `ed25519-dalek` rejects at decode time (vs verify time) is version-
    // sensitive and not the right boundary to assert in foundation tests.
    // Add when the Cable wire-protocol layer has known-bad-vector test
    // fixtures to assert against.
}
