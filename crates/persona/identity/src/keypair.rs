//! Ed25519 keypair, public key, and signature types.

use blake2::{Blake2b, Digest, digest::consts::U32};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::{CryptoRng, RngCore};
use zeroize::ZeroizeOnDrop;

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
    /// Generate a new random keypair.
    pub fn generate<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        Self(SigningKey::generate(rng))
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

    /// Derive a child keypair from a salt using BLAKE2b.
    ///
    /// `child_seed = BLAKE2b-256(self.signing_key_bytes || salt)`
    /// `child_keypair = Ed25519::from_seed(child_seed)`
    ///
    /// This matches the per-protocol-key derivation pattern from the inherited
    /// Cable spec §2.2.
    ///
    /// The master signing-key bytes never leave this method; the caller
    /// receives only the derived keypair.
    pub fn derive_child(&self, salt: &[u8]) -> Ed25519Keypair {
        let master_seed = self.0.to_bytes();

        let mut hasher = Blake2b::<U32>::new();
        hasher.update(master_seed);
        hasher.update(salt);
        let digest = hasher.finalize();

        let mut child_seed = [0u8; 32];
        child_seed.copy_from_slice(&digest);

        // Note: master_seed is on the stack; it goes out of scope at end of
        // function. SigningKey internally zeroizes on drop.
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
