//! Peer-identifier type for transport operations.

use mere_identity::{Ed25519PublicKey, IdentityError};

/// A peer's identifier on the transport network.
///
/// Derived from the peer's master Ed25519 public key (the same key managed
/// by [`mere_identity`]). For iroh-backed transports, this maps directly to
/// iroh's `NodeId`.
///
/// Carries equality and hashing semantics of the underlying public key, so
/// `NodeId` works as a `HashMap` / `HashSet` key.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct NodeId(Ed25519PublicKey);

impl NodeId {
    /// Construct a `NodeId` from a master Ed25519 public key.
    pub fn from_public_key(pk: Ed25519PublicKey) -> Self {
        Self(pk)
    }

    /// The underlying public key.
    pub fn public_key(&self) -> Ed25519PublicKey {
        self.0
    }

    /// 32-byte canonical encoding of the underlying public key.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// Decode from a 32-byte public-key encoding.
    ///
    /// Returns the same error as [`Ed25519PublicKey::from_bytes`] if the bytes
    /// don't form a valid public key.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, IdentityError> {
        Ed25519PublicKey::from_bytes(bytes).map(Self)
    }
}

impl From<Ed25519PublicKey> for NodeId {
    fn from(pk: Ed25519PublicKey) -> Self {
        Self(pk)
    }
}
