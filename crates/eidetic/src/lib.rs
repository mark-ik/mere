/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # Eidetic
//!
//! Private local-memory lane for the [`mere`](https://crates.io/crates/mere)
//! browser. Eidetic owns the vocabulary for owner-scoped local blobs, caches,
//! and accumulated browsing memory — the lane that "keeps the impressions
//! over time" in Mere's printing-press metaphor (engines → inker → platen →
//! verso-tile → eidetic).
//!
//! Eidetic defines typed [`Request`] / [`Response`] enums, a [`Store`] trait
//! that storage backends implement (fjall, redb, IndexedDB, …), and a
//! [`dispatch`] helper that routes requests to a store. Eidetic does not pick
//! a storage backend, mount filesystems, or know about graphs — it is the
//! pure boundary between reducer-emitted memory requests and concrete blob
//! storage.
//!
//! Eidetic is distinct from:
//!
//! - [`mere-transport`](https://crates.io/crates/mere-transport) (peer
//!   transport state — networked, not local-private),
//! - [`moothold`](https://crates.io/crates/moothold) (community/federation
//!   flora — shared, not private),
//! - host UI state (transient, not durable).

#![doc(html_root_url = "https://docs.rs/eidetic/0.0.1")]

use serde::{Deserialize, Serialize};

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
/// Implementations decide where blobs live (fjall, redb, IndexedDB, in-memory,
/// …). The trait surface is intentionally narrow: load by key, save by key.
/// Index/snapshot/journal concerns belong to higher-level seams that may
/// build on top of `Store`.
pub trait Store {
    fn load_blob(&mut self, key: &str) -> Result<Option<Vec<u8>>>;

    fn save_blob(&mut self, key: &str, value: &[u8]) -> Result<()>;
}

/// Route a [`Request`] to a [`Store`] and produce the matching [`Response`].
pub fn dispatch(store: &mut dyn Store, request: &Request) -> Result<Response> {
    match request {
        Request::LoadBlob { key } => Ok(Response::BlobLoaded {
            key: key.clone(),
            value: store.load_blob(key)?,
        }),
        Request::SaveBlob { key, value } => {
            store.save_blob(key, value)?;
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

    impl Store for InMemoryStore {
        fn load_blob(&mut self, key: &str) -> Result<Option<Vec<u8>>> {
            Ok(self.blobs.get(key).cloned())
        }

        fn save_blob(&mut self, key: &str, value: &[u8]) -> Result<()> {
            self.blobs.insert(key.to_string(), value.to_vec());
            Ok(())
        }
    }

    #[test]
    fn dispatch_round_trips_save_then_load() {
        let mut store = InMemoryStore::default();
        let saved = dispatch(
            &mut store,
            &Request::SaveBlob {
                key: "k".into(),
                value: b"hello".to_vec(),
            },
        )
        .unwrap();
        assert_eq!(
            saved,
            Response::BlobSaved {
                key: "k".to_string()
            }
        );

        let loaded = dispatch(&mut store, &Request::LoadBlob { key: "k".into() }).unwrap();
        assert_eq!(
            loaded,
            Response::BlobLoaded {
                key: "k".to_string(),
                value: Some(b"hello".to_vec()),
            }
        );
    }

    #[test]
    fn dispatch_load_returns_none_for_unknown_key() {
        let mut store = InMemoryStore::default();
        let loaded = dispatch(
            &mut store,
            &Request::LoadBlob {
                key: "missing".into(),
            },
        )
        .unwrap();
        assert_eq!(
            loaded,
            Response::BlobLoaded {
                key: "missing".to_string(),
                value: None,
            }
        );
    }

    #[test]
    fn dispatch_propagates_store_errors() {
        struct FailingStore;
        impl Store for FailingStore {
            fn load_blob(&mut self, _key: &str) -> Result<Option<Vec<u8>>> {
                Err(Error::new("disk on fire"))
            }
            fn save_blob(&mut self, _key: &str, _value: &[u8]) -> Result<()> {
                Err(Error::new("disk on fire"))
            }
        }

        let err = dispatch(&mut FailingStore, &Request::LoadBlob { key: "k".into() }).unwrap_err();
        assert_eq!(err.message, "disk on fire");
    }
}
