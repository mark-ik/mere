//! Cable bilateral chat protocol — Mere-native realization on p2panda
//! operations, plus the [`crate::BilateralProtocol`] adapter.
//!
//! We keep Cable's *trust model* (a shared-secret cabal; per-cabal Ed25519
//! identities; posts forming a causal DAG; channels grouping posts) but not
//! its literal byte wire. A post is a p2panda-core operation — a signed
//! `Header<CabalExt>` plus an optional body, BLAKE3-hashed and CBOR-encoded —
//! the same event type the rest of the substrate (p2panda-net sync, the
//! cabal-space model) is built on. This is the §12 "Cable on our stack"
//! promotion; there is no cabal.club wire interop.
//!
//! ## Modules
//!
//! - [`wire`] — `Post` ↔ operation encode/decode (CBOR) and the `CabalExt`
//!   header extension
//! - [`sign`] — signing / verification via the operation header
//! - [`hash`] — BLAKE3-256 post id (from the canonical CBOR bytes) and cabal
//!   id (from the cabal key)
//! - [`store`] / [`persistent_store`] — in-memory and redb-backed post stores
//! - [`engine`] — the [`CableEngine`] runtime
//!
//! Not yet wired: causal-DAG head tracking and channel time-range request
//! sync. Multi-parent causality currently rides as application data in
//! `CabalExt::parents` (per Probe 1); p2panda's own per-author `seq_num` /
//! `backlink` log is left unused.

pub mod engine;
pub mod hash;
pub mod log_store;
pub mod persistent_store;
pub mod sign;
pub mod store;
pub mod wire;

pub use crate::cable::engine::CableEngine;
pub use crate::cable::hash::hash_cabal_id;
pub use crate::cable::persistent_store::PersistentCabalStore;
pub use crate::cable::sign::{sign_post, verify_post};
pub use crate::cable::store::InMemoryCabalStore;
pub use crate::cable::wire::{
    CabalExt, decode_post, encode_post, operation_id, operation_to_post, post_to_operation,
};

use crate::Post;
use crate::PostId;

/// Compute a post's content-addressed [`PostId`] — the operation's signed-header
/// hash (`Header::hash()`), which is also its p2panda log/backlink identity.
///
/// Convenience alias for [`operation_id`].
pub fn hash_post(post: &Post) -> PostId {
    operation_id(post)
}
