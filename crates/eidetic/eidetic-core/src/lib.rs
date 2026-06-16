/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

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
//! - [`moothold`](https://crates.io/crates/moothold) (community/federation
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
pub mod typed;

pub use browsing::{
    BROWSING_TRACE_SCHEMA_REF, BrowsingMemory, BrowsingTrace, PageRef, TraceEvent,
    TraceTransition, bootstrap_browsing_schema, save_trace,
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
pub use typed::{TypedPayload, list_typed, load_typed, save_typed};

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

/// Owner-scoped private blob store.
///
/// Implementations decide where blobs live (fjall, redb, OPFS, in-memory,
/// …). The trait surface is intentionally narrow: load by key, save by key,
/// enumerate by key prefix. Index/snapshot/journal concerns belong to
/// higher-level seams that may build on top of `Store`.
///
/// The trait is `?Send` so single-threaded wasm impls (browser OPFS) can
/// satisfy it without contortions; native multi-threaded consumers that need
/// `Send` futures can add the bound at the call site.
///
/// ## `iter_keys`
///
/// Backends that don't support enumeration return an error (the default
/// impl). Higher layers that need listing (`list_manifests`, `list_typed`)
/// will surface that error to the consumer. Most real backends — fjall,
/// redb, OPFS, and the test in-memory stores — can answer enumeration
/// cheaply.
#[async_trait(?Send)]
pub trait Store {
    async fn load_blob(&mut self, key: &str) -> Result<Option<Vec<u8>>>;

    async fn save_blob(&mut self, key: &str, value: &[u8]) -> Result<()>;

    /// Return all keys that begin with `prefix`. Order is implementation-
    /// defined; consumers that need a stable order should sort the
    /// returned vector.
    async fn iter_keys(&mut self, _prefix: &str) -> Result<Vec<String>> {
        Err(Error::new(
            "Store implementation does not support iter_keys",
        ))
    }

    /// Delete the value under `key`, returning whether something was
    /// deleted. Backends that don't support deletion return an error (the
    /// default impl).
    ///
    /// Layer-4 quota policies (e.g. browsing-memory age-out) delete
    /// *manifests* via [`manifest::delete_manifest`]; blob bytes stay until
    /// an explicit GC pass walks reachability (design pass §8 — a blob may
    /// be referenced by multiple manifests).
    async fn delete_blob(&mut self, _key: &str) -> Result<bool> {
        Err(Error::new(
            "Store implementation does not support delete_blob",
        ))
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
            value: store.load_blob(key).await?,
        }),
        Request::SaveBlob { key, value } => {
            store.save_blob(key, value).await?;
            Ok(Response::BlobSaved { key: key.clone() })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct InMemoryStore {
        blobs: HashMap<String, Vec<u8>>,
    }

    #[async_trait(?Send)]
    impl Store for InMemoryStore {
        async fn load_blob(&mut self, key: &str) -> Result<Option<Vec<u8>>> {
            Ok(self.blobs.get(key).cloned())
        }

        async fn save_blob(&mut self, key: &str, value: &[u8]) -> Result<()> {
            self.blobs.insert(key.to_string(), value.to_vec());
            Ok(())
        }

        async fn iter_keys(&mut self, prefix: &str) -> Result<Vec<String>> {
            Ok(self
                .blobs
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect())
        }
    }

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
        struct FailingStore;
        #[async_trait(?Send)]
        impl Store for FailingStore {
            async fn load_blob(&mut self, _key: &str) -> Result<Option<Vec<u8>>> {
                Err(Error::new("disk on fire"))
            }
            async fn save_blob(&mut self, _key: &str, _value: &[u8]) -> Result<()> {
                Err(Error::new("disk on fire"))
            }
        }

        pollster::block_on(async {
            let err = dispatch(&mut FailingStore, &Request::LoadBlob { key: "k".into() })
                .await
                .unwrap_err();
            assert_eq!(err.message, "disk on fire");
        });
    }
}
