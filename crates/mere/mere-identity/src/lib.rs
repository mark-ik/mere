//! # Mere-Identity
//!
//! Identity management for the [`mere`](https://crates.io/crates/mere) browser
//! workspace. Owns the user's master Ed25519 keypair, integrates with OS
//! keychain via a backend trait that platform-specific code in
//! [`graphshell`](https://crates.io/crates/graphshell) implements, and provides
//! deterministic per-protocol identity derivation (BLAKE2b-seeded, per the
//! pattern established in the inherited Cable spec §2.2).
//!
//! ## Quick start
//!
//! ```
//! use mere_identity::{IdentityProvider, InMemoryProvider};
//!
//! let provider = InMemoryProvider::random();
//! let _master_pubkey = provider.master_public_key();
//!
//! // Derive a per-protocol keypair from a salt (e.g. a Cable cabal key).
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
//! - **Master secret never leaves the [`IdentityProvider`].** All key
//!   derivation happens inside the provider; consumers receive only the
//!   derived keypair.
//! - **Derivation is BLAKE2b-256(master_seed || salt) → Ed25519 seed**,
//!   matching the inherited Cable spec §2.2.
//! - **Pure-Rust crypto** — `ed25519-dalek` + `blake2`, no `sodiumoxide` /
//!   libsodium dependency. Trade-off vs cable.rs upstream is documented in
//!   the Cable migration plan.
//!
//! ## Consumers
//!
//! - [`murm`](https://crates.io/crates/murm) — derives per-cabal Ed25519
//!   keypairs for bilateral chat
//! - [`mere-transport`](https://crates.io/crates/mere-transport) — derives
//!   the iroh `NodeId` from the master public key
//! - Future: [`moothold`](https://crates.io/crates/moothold), [`eidetic`](https://crates.io/crates/eidetic)
//!
//! ## Status
//!
//! Pre-1.0. The [`IdentityProvider`] trait surface is intended to stabilize
//! before 0.1.0.

#![doc(html_root_url = "https://docs.rs/mere-identity/0.0.1")]
#![warn(missing_docs)]

mod error;
mod keypair;
pub mod passphrase_storage;
mod provider;
pub mod vault;

pub use crate::error::IdentityError;
pub use crate::keypair::{Ed25519Keypair, Ed25519PublicKey, Ed25519Signature};
pub use crate::passphrase_storage::PassphraseEncryptedStorage;
pub use crate::provider::{IdentityProvider, InMemoryProvider};
pub use crate::vault::{
    CredentialLineage, IdentitySlot, IdentityStorage, IdentityVault, InMemoryStorage, Profile,
    ProfileId, ProfileSummary, ProtocolKey, SecretBytes, UnlockTier,
};

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
