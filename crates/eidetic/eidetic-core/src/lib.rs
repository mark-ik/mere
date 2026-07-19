// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # Eidetic
//!
//! Private local-memory lane for the [`mere`](https://crates.io/crates/mere)
//! browser. Eidetic owns the vocabulary for owner-scoped local blobs, caches,
//! and accumulated browsing memory — the lane that "keeps the impressions
//! over time" in Mere's printing-press metaphor (engines → inker → platen →
//! eidetic).
//!
//! Eidetic defines typed [`Request`] / [`Response`] enums, an async [`Store`]
//! trait that storage backends implement (fjall, redb, OPFS, …), and a
//! [`dispatch`] helper that routes requests to a store. Eidetic does not pick
//! a storage backend, mount filesystems, or know about graphs — it is the
//! pure boundary between reducer-emitted memory requests and concrete blob
//! storage.
//!
//! ## Why async
//!
//! Browser-side stores (OPFS via `FileSystemSyncAccessHandle` in workers,
//! or other wasm-bindgen-backed implementations) cannot expose a synchronous
//! I/O surface from Rust — there is no `block_on` in wasm32-unknown-unknown.
//! The trait is therefore async, with native fjall/redb implementations
//! returning ready futures (no actual async work) and browser implementations
//! actually awaiting JS Promises. The trait uses `?Send` so it works on both
//! single-threaded wasm and multi-threaded native; consumers `.await`
//! eidetic calls in their existing context rather than spawning them.
//!
//! Eidetic is distinct from:
//!
//! - [`transport`](https://crates.io/crates/transport) (peer
//!   transport state — networked, not local-private),
//! - [`gemot`](https://crates.io/crates/gemot) (community/federation
//!   flora — shared, not private),
//! - host UI state (transient, not durable).
//!
//! ## Temporal-integrity contract (R0 invariant)
//!
//! Adopted from the donor `graphshell` history/memory subsystem (per the
//! [adoption roadmap](../../../../design_docs/mere_docs/implementation_strategy/2026-05-27_adoption_roadmap.md)
//! R0; the same contract binds the navigation side in
//! [`node-lineage`](https://docs.rs/node-lineage)). Three invariants, already
//! embodied by [`engram::Engram`] and the [`Store`] trait, named here so they
//! hold as backends and tiers are added:
//!
//! 1. **Temporal-integrity** — an [`engram::Engram`] is immutable and
//!    content-hashed; edits do not exist. A refresh produces a *new* engram
//!    with a fresh hash; a stored blob is never mutated in place.
//! 2. **Replay-isolation** — reading or replaying stored memory does not mutate
//!    the store. A [`Store`] read leaves history untouched, so re-deriving a
//!    past state is side-effect-free.
//! 3. **Shared-projection** — derived views ("recent", caches, indices) are
//!    projections over the single engram store, never a second authoritative
//!    store.

#![doc(html_root_url = "https://docs.rs/eidetic/0.0.1")]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod browsing;
pub mod bundle;
pub mod deleted;
pub mod engram;
pub mod manifest;
pub mod models;
pub mod schema;
pub mod schema_def;
pub mod seal;
pub mod typed;

pub use browsing::{
    BROWSING_TRACE_SCHEMA_REF, BrowsingMemory, BrowsingTrace, PageRef, TraceEvent, TraceTransition,
    bootstrap_browsing_schema, save_trace,
};
pub use bundle::{
    BUNDLE_SCHEMA_REF, Bundle, BundleMember, bundle_schema_ref, load_bundle, save_bundle,
    verify_required_members,
};
pub use deleted::{DeletedNode, list_deleted, record_deleted};
pub use engram::{Engram, TimeBounds};
pub use manifest::{BlobFetcher, BlobManifest, BlobSource, NoFetcher, delete_manifest};
pub use models::{ModelComponents, ModelLibrary, ModelManifest};
pub use schema::{
    Hash, ManifestId, ModerationState, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, SchemaRef,
    SignatureRef, Timestamp, TrustEnvelope, TrustLevel,
};
pub use schema_def::{
    JsonLdValidator, JsonSchemaValidator, META_SCHEMA_REF, MereNativeFieldSpec,
    MereNativeSchemaBody, MereNativeSchemaBuilder, MereNativeValidator, SchemaDefinition,
    SchemaFormat, SchemaValidator, bootstrap_meta_schema, find_schema_by_id, load_schema,
    meta_schema_engram, meta_schema_ref, save_schema, validate_against_schema, validate_payload,
};
pub use seal::{
    PayloadSealer, SealEpochId, SealedBlobRef, is_private_lane, resolve_sealed_blob, seal_marker,
    seal_payload_for_store,
};
pub use typed::{
    TypedPayload, list_typed, load_typed, load_typed_sealed, save_typed, save_typed_sealed,
};

/// Request emitted by reducers and routed to a [`Store`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    /// Load a blob by key. The store returns `None` if the key is unknown.
    LoadBlob { key: String },
    /// Save a blob under the given key. Overwrites any previous value.
    SaveBlob { key: String, value: Vec<u8> },
}

/// Response returned by [`dispatch`] after a [`Request`] resolves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    BlobLoaded { key: String, value: Option<Vec<u8>> },
    BlobSaved { key: String },
}

/// Error type returned by [`Store`] implementations and [`dispatch`].
///
/// Eidetic uses a small, owned error vocabulary so downstream crates can
/// `From`-convert into their own error types without taking a dependency on
/// any particular error library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub message: String,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

/// Crate-level result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// The storage seam, **muniment's** (2026-07-12).
///
/// Eidetic used to declare its own `Store` trait — get/put/delete/list over
/// `&str -> Vec<u8>`, async, one method per operation. That was
/// [`muniment::Backend`] written twice: eidetic poured its own floor before
/// muniment existed. It now consumes the real one, and `Store` remains as an
/// alias so every layer above (manifests, schemas, sealing, engrams) reads
/// unchanged.
///
/// What the swap buys beyond deleting a duplicate: backends take `&self`,
/// `delete`/`list` are required rather than `Err`-by-default, muniment's
/// `scan` (ordered range) and `apply` (atomic batch) come along, and every
/// muniment backend (memory, redb) is now an eidetic store — while
/// eidetic-fjall becomes a backend the whole family can reuse.
pub use muniment::{Backend, Backend as Store, MemoryBackend, WriteOp};

/// muniment's store errors flow into eidetic's owned error vocabulary, so the
/// layers above keep returning [`Result`] and `?` still works at every call.
impl From<muniment::error::StoreError> for Error {
    fn from(err: muniment::error::StoreError) -> Self {
        Error::new(err.to_string())
    }
}


/// Idempotent first-init seeding for any [`Store`].
///
/// Currently equivalent to [`bootstrap_meta_schema`]; in the future will
/// also seed any other well-known schemas eidetic itself ships with
/// (e.g. the `OpaqueBlob` schema). Higher-layer consumers that ship their
/// own schemas (model storage, vector indices, browsing memory) should
/// follow this pattern with their own `bootstrap_*` helpers.
pub async fn bootstrap(store: &mut dyn Store) -> Result<()> {
    bootstrap_meta_schema(store).await
}

/// Route a [`Request`] to a [`Store`] and produce the matching [`Response`].
pub async fn dispatch(store: &mut dyn Store, request: &Request) -> Result<Response> {
    match request {
        Request::LoadBlob { key } => Ok(Response::BlobLoaded {
            key: key.clone(),
            value: store.get(key).await?,
        }),
        Request::SaveBlob { key, value } => {
            store.put(key, value).await?;
            Ok(Response::BlobSaved { key: key.clone() })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muniment::error::StoreError;
    use std::collections::HashMap;

        // The in-memory test store is muniment's (2026-07-12): eidetic's
    // hand-rolled one was the same map behind the same seam.
    use muniment::MemoryBackend as InMemoryStore;




    #[test]
    fn dispatch_round_trips_save_then_load() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            let saved = dispatch(
                &mut store,
                &Request::SaveBlob {
                    key: "k".into(),
                    value: b"hello".to_vec(),
                },
            )
            .await
            .unwrap();
            assert_eq!(
                saved,
                Response::BlobSaved {
                    key: "k".to_string()
                }
            );

            let loaded = dispatch(&mut store, &Request::LoadBlob { key: "k".into() })
                .await
                .unwrap();
            assert_eq!(
                loaded,
                Response::BlobLoaded {
                    key: "k".to_string(),
                    value: Some(b"hello".to_vec()),
                }
            );
        });
    }

    #[test]
    fn dispatch_load_returns_none_for_unknown_key() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            let loaded = dispatch(
                &mut store,
                &Request::LoadBlob {
                    key: "missing".into(),
                },
            )
            .await
            .unwrap();
            assert_eq!(
                loaded,
                Response::BlobLoaded {
                    key: "missing".to_string(),
                    value: None,
                }
            );
        });
    }

    #[test]
    fn dispatch_propagates_store_errors() {
        // A backend whose disk is on fire, in muniment's vocabulary.
        struct FailingStore;
        type R<T> = std::result::Result<T, StoreError>;
        #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
        #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
        impl Backend for FailingStore {
            async fn get(&self, _key: &str) -> R<Option<Vec<u8>>> {
                Err(StoreError::Backend("disk on fire".into()))
            }
            async fn put(&self, _key: &str, _bytes: &[u8]) -> R<()> {
                Err(StoreError::Backend("disk on fire".into()))
            }
            async fn delete(&self, _key: &str) -> R<()> {
                Err(StoreError::Backend("disk on fire".into()))
            }
            async fn list(&self, _prefix: &str) -> R<Vec<String>> {
                Err(StoreError::Backend("disk on fire".into()))
            }
            async fn scan(&self, _start: &str, _end: &str) -> R<Vec<String>> {
                Err(StoreError::Backend("disk on fire".into()))
            }
            async fn apply(&self, _ops: &[WriteOp]) -> R<()> {
                Err(StoreError::Backend("disk on fire".into()))
            }
        }

        pollster::block_on(async {
            let err = dispatch(&mut FailingStore, &Request::LoadBlob { key: "k".into() })
                .await
                .unwrap_err();
            // muniment's StoreError Display prefixes its variant ("backend: …"),
            // and eidetic's From carries that through — the error vocabulary is
            // now the seam's, which is the point of the swap.
            assert_eq!(err.message, "backend: disk on fire");
        });
    }
}
