//! Cable bilateral chat protocol — wire encoding, post hashing, and
//! [`crate::BilateralProtocol`] adapter.
//!
//! Cable is a binary, varint-framed, BLAKE2b-content-addressed append-only
//! log protocol. Posts form a causal DAG via 32-byte BLAKE2b post hashes;
//! channels group posts within a cabal; sync is pull-based via channel
//! time-range requests.
//!
//! ## Phase 2B status
//!
//! Currently implemented:
//!
//! - [`varint`] — LEB128 varint encoding / decoding (foundation for all
//!   length-prefixed wire fields)
//! - [`hash`] — BLAKE2b-256 post hashing (`PostId` derivation from canonical
//!   wire bytes)
//!
//! Not yet implemented (later 2B phases):
//!
//! - Post wire encoding/decoding (one variant per `PostKind`)
//! - Post signing + signature verification
//! - Causal-DAG `links` resolution and head tracking
//! - Channel time-range request sync messages
//! - Concrete [`crate::BilateralProtocol`] impl
//!
//! ## Wire-spec verification
//!
//! The exact byte-layout of Cable posts is defined by the upstream
//! [Cable spec](https://github.com/cabal-club/cable). Our implementation
//! tracks that spec; any wire-level decisions in this module that aren't
//! yet directly cross-referenced against a specific spec section are marked
//! `// TODO(cable-spec):` for follow-up verification.

pub mod engine;
pub mod hash;
pub mod persistent_store;
pub mod sign;
pub mod store;
pub mod varint;
pub mod wire;

pub use crate::cable::engine::CableEngine;
pub use crate::cable::hash::{hash_cabal_id, hash_post_bytes};
pub use crate::cable::persistent_store::PersistentCabalStore;
pub use crate::cable::sign::{sign_post, verify_post};
pub use crate::cable::store::InMemoryCabalStore;
pub use crate::cable::wire::{decode_post, encode_post};

use crate::{Post, PostId};

/// Compute a post's content-addressed [`PostId`] (BLAKE2b hash of the
/// canonical wire encoding).
///
/// Convenience wrapper for `hash_post_bytes(&encode_post(post))`.
pub fn hash_post(post: &Post) -> PostId {
    hash_post_bytes(&encode_post(post))
}
