//! Domain-injected authority for retention trust-root transitions.

use proofs::Digest;

/// Authorization seam for accepting a retention checkpoint.
///
/// Mesh implements this with its owner key. Moot supplies a signer set and
/// revision derived from an accepted constitution. Replication does not infer
/// authority from transport access or visible membership.
pub trait CheckpointAuthority {
    fn authority_revision(&self) -> Digest;

    fn permits_checkpoint(&self, author: [u8; 32], named_revision: &Digest) -> bool;
}
