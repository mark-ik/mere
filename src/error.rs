//! Error types for `identity`.

use thiserror::Error;

/// Errors raised by `identity` operations.
#[derive(Debug, Error)]
pub enum IdentityError {
    /// The provided byte sequence does not encode a valid Ed25519 public key.
    #[error("invalid Ed25519 public key bytes")]
    InvalidPublicKey,

    /// The provided byte sequence does not encode a valid Ed25519 signature.
    #[error("invalid Ed25519 signature bytes")]
    InvalidSignature,

    /// Per-protocol key derivation failed.
    #[error("key derivation failed: {0}")]
    DerivationFailed(String),

    /// The keychain (or other identity backend) reported an error.
    #[error("identity backend error: {0}")]
    Backend(String),
}
