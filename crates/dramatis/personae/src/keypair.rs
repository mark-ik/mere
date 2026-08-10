//! Ed25519 keypair, public key, and signature types.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::IdentityError;

/// An Ed25519 keypair (signing + verifying key).
///
/// The signing key bytes are zeroized on drop. The keypair can be cloned, but
/// each clone holds a separate copy of the signing-key memory and is also
/// zeroized on its drop.
///
/// **Security note**: a master `Ed25519Keypair` in code is sensitive. Prefer
/// holding it inside an [`crate::IdentityProvider`] implementation rather than
/// passing it around directly.
#[derive(Clone, ZeroizeOnDrop)]
pub struct Ed25519Keypair(SigningKey);

impl Ed25519Keypair {
    /// Generate a new random keypair from OS randomness.
    ///
    /// An Ed25519 signing key is a 32-byte uniform random seed; sourcing it
    /// from `getrandom` directly keeps the public API free of rand_core
    /// version coupling. Deterministic keys (tests, derivation) go through
    /// [`Self::from_seed`].
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).expect("OS randomness available");
        let keypair = Self(SigningKey::from_bytes(&seed));
        seed.zeroize();
        keypair
    }

    /// Construct from a 32-byte seed.
    ///
    /// The seed becomes the signing-key bytes directly (Ed25519 from-seed
    /// semantics — the seed expands to a 64-byte signing key internally).
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self(SigningKey::from_bytes(&seed))
    }

    /// 32-byte signing-key seed.
    ///
    /// Exposes the secret seed for cryptographic-handshake use that needs
    /// to construct another library's secret-key type (e.g.,
    /// [`transport`](https://crates.io/crates/transport) building
    /// an iroh `SecretKey` whose ed25519-dalek major version may differ
    /// from this crate's). **Treat the returned array as sensitive**:
    /// avoid logging, persisting, or passing it to untrusted code.
    pub fn to_seed(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// Public key for this keypair.
    pub fn public_key(&self) -> Ed25519PublicKey {
        Ed25519PublicKey(self.0.verifying_key())
    }

    /// Sign a message.
    pub fn sign(&self, message: &[u8]) -> Ed25519Signature {
        Ed25519Signature(self.0.sign(message))
    }

    /// Derive a child keypair from a salt using BLAKE3 keyed-hash.
    ///
    /// `child_seed = BLAKE3-keyed(key = self.signing_key_bytes, data = salt)`
    /// `child_keypair = Ed25519::from_seed(child_seed)`
    ///
    /// BLAKE3 in keyed-hash mode (the master seed is the key) gives
    /// domain-separated, deterministic per-protocol key derivation. The
    /// workspace hashes with BLAKE3 throughout; BLAKE2b is gone.
    ///
    /// The master signing-key bytes never leave this method; the caller
    /// receives only the derived keypair.
    pub fn derive_child(&self, salt: &[u8]) -> Ed25519Keypair {
        let mut master_seed = self.0.to_bytes();

        let digest = blake3::keyed_hash(&master_seed, salt);
        let child_seed = *digest.as_bytes();

        // Scrub the copied master seed; the SigningKey itself zeroizes on drop.
        master_seed.zeroize();

        Ed25519Keypair::from_seed(child_seed)
    }
}

/// An Ed25519 public key.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Ed25519PublicKey(VerifyingKey);

impl Ed25519PublicKey {
    /// 32-byte canonical encoding.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// Decode from a 32-byte canonical encoding.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, IdentityError> {
        VerifyingKey::from_bytes(bytes)
            .map(Self)
            .map_err(|_| IdentityError::InvalidPublicKey)
    }

    /// Verify a signature against a message.
    pub fn verify(&self, message: &[u8], signature: &Ed25519Signature) -> bool {
        self.0.verify(message, &signature.0).is_ok()
    }
}

/// An Ed25519 signature.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ed25519Signature(Signature);

impl Ed25519Signature {
    /// 64-byte canonical encoding.
    pub fn to_bytes(&self) -> [u8; 64] {
        self.0.to_bytes()
    }

    /// Decode from a 64-byte canonical encoding.
    ///
    /// Ed25519 signature byte-decoding is infallible at the byte level (every
    /// 64-byte slice is a syntactically-valid signature); validity is only
    /// determined at verify time.
    pub fn from_bytes(bytes: &[u8; 64]) -> Self {
        Self(Signature::from_bytes(bytes))
    }
}
