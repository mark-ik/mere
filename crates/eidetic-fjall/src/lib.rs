/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Fjall LSM [`Store`] implementation for [`eidetic`].
//!
//! Persists eidetic blobs and manifests under a single fjall keyspace
//! partition, opened at a directory path the caller provides. The
//! production-default native backend.
//!
//! ## Async-but-blocking
//!
//! Fjall is sync-blocking. `FjallStore` implements the async `Store`
//! trait by performing the blocking call inline and returning the
//! ready future — this matches the design-pass commitment that native
//! impls return ready futures (`?Send`-friendly, no real async work).
//! For workloads where blocking the executor is unacceptable, wrap the
//! call in `tokio::task::spawn_blocking` at the call site.

#![doc(html_root_url = "https://docs.rs/eidetic-fjall/0.0.1")]

use async_trait::async_trait;
use eidetic::{Error, Result, Store};
use fjall::{Config, Keyspace, PartitionCreateOptions, PartitionHandle};
use std::path::Path;

/// Default partition name used by [`FjallStore::open`]. Callers that want
/// to host multiple eidetic stores in one fjall keyspace use
/// [`FjallStore::open_partition`] with distinct names.
pub const DEFAULT_PARTITION: &str = "eidetic";

/// Fjall-backed [`Store`].
///
/// Holds a fjall [`Keyspace`] (the on-disk database) and a
/// [`PartitionHandle`] (the column-family-shaped sub-store within it).
/// Construct with [`FjallStore::open`] for the simple single-partition
/// case.
pub struct FjallStore {
    _keyspace: Keyspace,
    partition: PartitionHandle,
}

impl FjallStore {
    /// Open (or create) a fjall keyspace at `path` and return a Store
    /// over the default partition.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_partition(path, DEFAULT_PARTITION)
    }

    /// Open (or create) a keyspace at `path` and return a Store over the
    /// named partition. Useful when one fjall keyspace hosts multiple
    /// logical stores (e.g. one per identity).
    pub fn open_partition(path: impl AsRef<Path>, partition: &str) -> Result<Self> {
        let keyspace = Config::new(path)
            .open()
            .map_err(|e| Error::new(format!("fjall open: {e}")))?;
        let partition = keyspace
            .open_partition(partition, PartitionCreateOptions::default())
            .map_err(|e| Error::new(format!("fjall partition: {e}")))?;
        Ok(Self {
            _keyspace: keyspace,
            partition,
        })
    }
}

#[async_trait(?Send)]
impl Store for FjallStore {
    async fn load_blob(&mut self, key: &str) -> Result<Option<Vec<u8>>> {
        match self.partition.get(key) {
            Ok(Some(slice)) => Ok(Some(slice.to_vec())),
            Ok(None) => Ok(None),
            Err(e) => Err(Error::new(format!("fjall get {key}: {e}"))),
        }
    }

    async fn save_blob(&mut self, key: &str, value: &[u8]) -> Result<()> {
        self.partition
            .insert(key, value)
            .map_err(|e| Error::new(format!("fjall insert {key}: {e}")))?;
        Ok(())
    }

    async fn iter_keys(&mut self, prefix: &str) -> Result<Vec<String>> {
        let mut keys = Vec::new();
        for entry in self.partition.prefix(prefix) {
            let (key, _value) = entry.map_err(|e| Error::new(format!("fjall prefix scan: {e}")))?;
            let key_str = std::str::from_utf8(&key)
                .map_err(|e| Error::new(format!("fjall non-utf8 key: {e}")))?;
            keys.push(key_str.to_string());
        }
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eidetic::manifest::{load_manifest, save_manifest};
    use eidetic::{
        BlobManifest, BlobSource, Hash, ManifestId, ModerationState, PrivacyClass,
        ProvenanceOrigin, ProvenanceRecord, SchemaRef, Timestamp, TrustEnvelope, TrustLevel,
    };
    use tempfile::TempDir;

    fn open_store() -> (FjallStore, TempDir) {
        let dir = TempDir::new().expect("tempdir");
        let store = FjallStore::open(dir.path()).expect("fjall open");
        (store, dir)
    }

    fn test_manifest(blob: &[u8]) -> BlobManifest {
        BlobManifest {
            id: ManifestId::of_blob(blob),
            schema: SchemaRef::from_id(ManifestId::of_blob(b"test-schema")),
            content_hash: Hash::of(blob),
            byte_size: blob.len() as u64,
            created_at: Timestamp(1_700_000_000_000),
            last_accessed: None,
            sources: vec![BlobSource::Local {
                key: format!("blob:{}", Hash::of(blob).to_hex()),
            }],
            privacy: PrivacyClass::LocalOnly,
            provenance: ProvenanceRecord {
                origin: ProvenanceOrigin::Generated,
                upstream: Vec::new(),
                tooling: Some("eidetic-fjall-test".to_string()),
                generated_at: Timestamp(1_700_000_000_000),
            },
            trust: TrustEnvelope {
                level: TrustLevel::SelfAsserted,
                signatures: Vec::new(),
                moderation_state: ModerationState::Unreviewed,
            },
            schema_metadata: serde_json::Value::Null,
            manifest_version: BlobManifest::CURRENT_VERSION,
        }
    }

    #[test]
    fn save_then_load_returns_same_bytes() {
        pollster::block_on(async {
            let (mut store, _dir) = open_store();
            store.save_blob("k", b"hello").await.unwrap();
            let got = store.load_blob("k").await.unwrap();
            assert_eq!(got, Some(b"hello".to_vec()));
        });
    }

    #[test]
    fn load_missing_returns_none() {
        pollster::block_on(async {
            let (mut store, _dir) = open_store();
            let got = store.load_blob("never-saved").await.unwrap();
            assert!(got.is_none());
        });
    }

    #[test]
    fn save_overwrites_existing() {
        pollster::block_on(async {
            let (mut store, _dir) = open_store();
            store.save_blob("k", b"first").await.unwrap();
            store.save_blob("k", b"second").await.unwrap();
            let got = store.load_blob("k").await.unwrap();
            assert_eq!(got, Some(b"second".to_vec()));
        });
    }

    #[test]
    fn manifest_round_trips_through_fjall_store() {
        pollster::block_on(async {
            let (mut store, _dir) = open_store();
            let blob = b"test-payload";
            let manifest = test_manifest(blob);

            save_manifest(&mut store, &manifest).await.unwrap();
            let loaded = load_manifest(&mut store, manifest.id)
                .await
                .unwrap()
                .expect("manifest present after save");
            assert_eq!(loaded, manifest);
        });
    }

    #[test]
    fn data_persists_across_store_reopen() {
        pollster::block_on(async {
            let dir = TempDir::new().expect("tempdir");
            {
                let mut store = FjallStore::open(dir.path()).expect("open 1");
                store.save_blob("persistent", b"survive").await.unwrap();
            }
            {
                let mut store = FjallStore::open(dir.path()).expect("open 2");
                let got = store.load_blob("persistent").await.unwrap();
                assert_eq!(got, Some(b"survive".to_vec()));
            }
        });
    }

    #[test]
    fn iter_keys_returns_prefix_matches() {
        pollster::block_on(async {
            let (mut store, _dir) = open_store();
            store.save_blob("manifest:a", b"1").await.unwrap();
            store.save_blob("manifest:b", b"2").await.unwrap();
            store.save_blob("blob:c", b"3").await.unwrap();

            let mut manifests = store.iter_keys("manifest:").await.unwrap();
            manifests.sort();
            assert_eq!(manifests, vec!["manifest:a", "manifest:b"]);

            let blobs = store.iter_keys("blob:").await.unwrap();
            assert_eq!(blobs, vec!["blob:c"]);

            let nothing = store.iter_keys("missing:").await.unwrap();
            assert!(nothing.is_empty());
        });
    }

    #[test]
    fn separate_partitions_isolate_keys() {
        pollster::block_on(async {
            let dir = TempDir::new().expect("tempdir");
            let mut alice = FjallStore::open_partition(dir.path(), "alice").unwrap();
            let mut bob = FjallStore::open_partition(dir.path(), "bob").unwrap();

            alice.save_blob("k", b"alice-value").await.unwrap();
            bob.save_blob("k", b"bob-value").await.unwrap();

            assert_eq!(
                alice.load_blob("k").await.unwrap(),
                Some(b"alice-value".to_vec())
            );
            assert_eq!(
                bob.load_blob("k").await.unwrap(),
                Some(b"bob-value".to_vec())
            );
        });
    }
}
