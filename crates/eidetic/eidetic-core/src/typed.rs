/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Layer 3 — Rust-side bindings for schema-conformant typed payloads.
//!
//! Layer 3 is **not** the schema. The schema lives one level up as a
//! content-addressed engram (Phase 4 work). Layer 3 is the language-specific
//! binding to a schema — Rust structs that serialize/deserialize bytes that
//! conform to a known schema, given a [`SchemaRef`] declared by the type.
//! Other implementations in other languages would bind the same schemas to
//! their own native types.
//!
//! ## Save / load semantics
//!
//! [`save_typed`] serializes a typed value to bytes, computes the BLAKE3
//! hash, builds a [`BlobManifest`] with the type's [`SchemaRef`], saves the
//! blob bytes under `"blob:<hash>"`, saves the manifest under
//! `"manifest:<hash>"`, and returns the manifest id.
//!
//! [`load_typed`] looks up the manifest, verifies its `schema` field matches
//! `T::schema_ref()`, resolves the blob via [`resolve_blob`], hash-verifies
//! the bytes, then deserializes them into `T`.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::manifest::{
    BlobFetcher, BlobManifest, BlobSource, list_manifests, load_manifest, save_manifest,
};
use crate::schema::{
    Hash, ManifestId, PrivacyClass, ProvenanceRecord, SchemaRef, Timestamp, TrustEnvelope,
};
use crate::seal::{PayloadSealer, resolve_sealed_blob, seal_payload_for_store};
use crate::{Error, Result, Store};

/// A Rust type that binds to a known schema.
///
/// Implementors declare which schema they conform to via [`schema_ref`]. The
/// per-schema serializer is chosen by the impl — JSON is the default offered
/// by the trait's default methods, but implementations are free to override
/// (rkyv for [`VectorIndex`], raw bytes for `ModelWeights`, JSON-LD for
/// schema.org-shaped payloads, etc.).
///
/// [`schema_ref`]: TypedPayload::schema_ref
/// [`VectorIndex`]: https://crates.io/crates/embed
pub trait TypedPayload: Serialize + DeserializeOwned + Sized {
    /// Content-addressed reference to the schema this type conforms to.
    /// In practice this is typically a `const fn`-computed `SchemaRef` or a
    /// lazily-computed one keyed by the schema engram's known content hash.
    fn schema_ref() -> SchemaRef;

    /// Serialize the value to bytes. Default implementation uses
    /// `serde_json`; impls override to pick a per-schema serializer.
    fn serialize_to_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(|e| Error::new(format!("typed serialize: {e}")))
    }

    /// Deserialize bytes into a value. Default uses `serde_json`.
    fn deserialize_from_bytes(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| Error::new(format!("typed deserialize: {e}")))
    }
}

/// Save a typed payload to the Store, producing a manifest id.
///
/// The blob bytes are saved under `"blob:<hash>"`; the manifest is saved
/// under `"manifest:<hash>"`. The returned id is the BLAKE3 hash of the
/// serialized bytes.
///
/// `additional_sources` lets callers attach Iroh tickets / HTTPS URLs / etc.
/// alongside the implicit `Local` source. Pass an empty `Vec` for purely
/// local-cached artifacts.
pub async fn save_typed<T: TypedPayload>(
    store: &mut dyn Store,
    value: &T,
    additional_sources: Vec<BlobSource>,
    privacy: PrivacyClass,
    provenance: ProvenanceRecord,
    trust: TrustEnvelope,
    created_at: Timestamp,
) -> Result<ManifestId> {
    save_typed_sealed(
        store,
        None,
        value,
        additional_sources,
        privacy,
        provenance,
        trust,
        created_at,
    )
    .await
}

/// Save a typed payload, sealing the private lane at rest.
///
/// As [`save_typed`], but with a [`PayloadSealer`]: when `sealer` is `Some` and
/// the payload's `privacy` is a private-lane class (`LocalOnly` /
/// `TrustedPeersOnly`), the stored blob bytes are sealed under the sealer's epoch
/// and the manifest is stamped with the seal marker. The public lane and the
/// no-sealer case store cleartext. The blob key and `content_hash` stay the
/// cleartext hash, so addressing and integrity are unchanged; only the bytes on
/// disk differ. Read back with [`load_typed_sealed`].
#[allow(clippy::too_many_arguments)] // mirrors save_typed + the sealer.
pub async fn save_typed_sealed<T: TypedPayload>(
    store: &mut dyn Store,
    sealer: Option<&dyn PayloadSealer>,
    value: &T,
    additional_sources: Vec<BlobSource>,
    privacy: PrivacyClass,
    provenance: ProvenanceRecord,
    trust: TrustEnvelope,
    created_at: Timestamp,
) -> Result<ManifestId> {
    let bytes = value.serialize_to_bytes()?;
    let hash = Hash::of(&bytes);
    let id = ManifestId::from_hash(hash);
    let local_key = format!("blob:{}", hash.to_hex());

    let mut sources = Vec::with_capacity(1 + additional_sources.len());
    sources.push(BlobSource::Local {
        key: local_key.clone(),
    });
    sources.extend(additional_sources);

    let mut manifest = BlobManifest {
        id,
        schema: T::schema_ref(),
        content_hash: hash,
        byte_size: bytes.len() as u64,
        created_at,
        last_accessed: None,
        sources,
        privacy,
        provenance,
        trust,
        schema_metadata: serde_json::Value::Null,
        manifest_version: BlobManifest::CURRENT_VERSION,
    };

    // Seals + stamps the manifest marker for the private lane; passthrough
    // otherwise. The stored bytes may be sealed; the manifest's content_hash
    // stays the cleartext hash for the post-unseal integrity check.
    let stored = seal_payload_for_store(sealer, &mut manifest, &bytes)?;
    store.save_blob(&local_key, &stored).await?;
    save_manifest(store, &manifest).await?;
    Ok(id)
}

/// Enumerate all manifests in the Store whose schema matches
/// `T::schema_ref()`. Returns the manifest list (without resolving the
/// payload bytes); use [`load_typed`] on each id to fetch.
pub async fn list_typed<T: TypedPayload>(store: &mut dyn Store) -> Result<Vec<BlobManifest>> {
    list_manifests(store, Some(T::schema_ref())).await
}

/// Load a typed payload by manifest id.
///
/// Verifies the manifest's `schema` field matches `T::schema_ref()` before
/// deserializing — a manifest with a different schema is a load error
/// (returns `Err`, not `None`) because mistyped reads are bugs.
///
/// Returns `Ok(None)` if no manifest is stored under `id`.
pub async fn load_typed<T: TypedPayload>(
    store: &mut dyn Store,
    fetcher: &mut dyn BlobFetcher,
    id: ManifestId,
) -> Result<Option<T>> {
    load_typed_sealed(store, fetcher, None, id).await
}

/// Load a typed payload, unsealing the private lane at rest.
///
/// As [`load_typed`], but with a [`PayloadSealer`]: a manifest whose blob is
/// sealed (written via [`save_typed_sealed`]) is unsealed with `sealer` before
/// the hash check. A sealed blob loaded with `sealer = None` is a hard error
/// (never a silent hash mismatch); an unsealed blob reads cleartext either way.
pub async fn load_typed_sealed<T: TypedPayload>(
    store: &mut dyn Store,
    fetcher: &mut dyn BlobFetcher,
    sealer: Option<&dyn PayloadSealer>,
    id: ManifestId,
) -> Result<Option<T>> {
    let Some(manifest) = load_manifest(store, id).await? else {
        return Ok(None);
    };

    if manifest.schema != T::schema_ref() {
        return Err(Error::new(format!(
            "schema mismatch loading manifest {}: declared {}, requested {}",
            id,
            manifest.schema,
            T::schema_ref()
        )));
    }

    let bytes = resolve_sealed_blob(store, fetcher, sealer, &manifest).await?;
    let value = T::deserialize_from_bytes(&bytes)?;
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::NoFetcher;
    use crate::schema::{
        ManifestId, ModerationState, ProvenanceOrigin, ProvenanceRecord, SchemaRef, Timestamp,
        TrustLevel,
    };
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};
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

    /// First test schema: a simple greeting record.
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Greeting {
        salutation: String,
        recipient: String,
    }

    impl TypedPayload for Greeting {
        fn schema_ref() -> SchemaRef {
            SchemaRef::from_id(ManifestId::of_blob(b"schema:test-greeting/v1"))
        }
    }

    /// Second test schema: a numeric record. Ensures the schema-mismatch
    /// guard rejects mistyped loads.
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Counter {
        ticks: u64,
    }

    impl TypedPayload for Counter {
        fn schema_ref() -> SchemaRef {
            SchemaRef::from_id(ManifestId::of_blob(b"schema:test-counter/v1"))
        }
    }

    fn test_provenance() -> ProvenanceRecord {
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some("eidetic-test/0.0.1".to_string()),
            generated_at: Timestamp(1_700_000_000_000),
        }
    }

    fn test_trust() -> TrustEnvelope {
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        }
    }

    #[test]
    fn save_then_load_round_trips_value() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            let original = Greeting {
                salutation: "Hello".to_string(),
                recipient: "world".to_string(),
            };

            let id = save_typed(
                &mut store,
                &original,
                Vec::new(),
                PrivacyClass::LocalOnly,
                test_provenance(),
                test_trust(),
                Timestamp(1_700_000_000_000),
            )
            .await
            .unwrap();

            let mut fetcher = NoFetcher;
            let loaded: Greeting = load_typed(&mut store, &mut fetcher, id)
                .await
                .unwrap()
                .expect("manifest present after save");
            assert_eq!(loaded, original);
        });
    }

    #[test]
    fn load_typed_returns_none_for_unknown_id() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            let mut fetcher = NoFetcher;
            let unknown = ManifestId::of_blob(b"never-saved");
            let result: Option<Greeting> =
                load_typed(&mut store, &mut fetcher, unknown).await.unwrap();
            assert!(result.is_none());
        });
    }

    #[test]
    fn schema_mismatch_load_returns_error() {
        pollster::block_on(async {
            // Save a Greeting, then try to load it as a Counter. Should
            // error with an explicit schema mismatch message rather than
            // silently producing garbage.
            let mut store = InMemoryStore::default();
            let id = save_typed(
                &mut store,
                &Greeting {
                    salutation: "Hi".to_string(),
                    recipient: "you".to_string(),
                },
                Vec::new(),
                PrivacyClass::LocalOnly,
                test_provenance(),
                test_trust(),
                Timestamp(0),
            )
            .await
            .unwrap();

            let mut fetcher = NoFetcher;
            let result: Result<Option<Counter>> = load_typed(&mut store, &mut fetcher, id).await;
            assert!(result.is_err());
            assert!(format!("{}", result.unwrap_err()).contains("schema mismatch"));
        });
    }

    #[test]
    fn multiple_schemas_coexist_in_one_store() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            let mut fetcher = NoFetcher;

            let greeting_id = save_typed(
                &mut store,
                &Greeting {
                    salutation: "Yo".to_string(),
                    recipient: "everyone".to_string(),
                },
                Vec::new(),
                PrivacyClass::LocalOnly,
                test_provenance(),
                test_trust(),
                Timestamp(0),
            )
            .await
            .unwrap();

            let counter_id = save_typed(
                &mut store,
                &Counter { ticks: 42 },
                Vec::new(),
                PrivacyClass::LocalOnly,
                test_provenance(),
                test_trust(),
                Timestamp(0),
            )
            .await
            .unwrap();

            assert_ne!(
                greeting_id, counter_id,
                "different content -> different ids"
            );

            let g: Greeting = load_typed(&mut store, &mut fetcher, greeting_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(g.salutation, "Yo");

            let c: Counter = load_typed(&mut store, &mut fetcher, counter_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(c.ticks, 42);
        });
    }

    #[test]
    fn list_typed_returns_only_matching_schema_manifests() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();

            // Save two Greetings and one Counter.
            save_typed(
                &mut store,
                &Greeting {
                    salutation: "Yo".to_string(),
                    recipient: "alice".to_string(),
                },
                Vec::new(),
                PrivacyClass::LocalOnly,
                test_provenance(),
                test_trust(),
                Timestamp(0),
            )
            .await
            .unwrap();
            save_typed(
                &mut store,
                &Greeting {
                    salutation: "Hi".to_string(),
                    recipient: "bob".to_string(),
                },
                Vec::new(),
                PrivacyClass::LocalOnly,
                test_provenance(),
                test_trust(),
                Timestamp(0),
            )
            .await
            .unwrap();
            save_typed(
                &mut store,
                &Counter { ticks: 7 },
                Vec::new(),
                PrivacyClass::LocalOnly,
                test_provenance(),
                test_trust(),
                Timestamp(0),
            )
            .await
            .unwrap();

            let greetings = list_typed::<Greeting>(&mut store).await.unwrap();
            assert_eq!(greetings.len(), 2);
            for m in &greetings {
                assert_eq!(m.schema, Greeting::schema_ref());
            }

            let counters = list_typed::<Counter>(&mut store).await.unwrap();
            assert_eq!(counters.len(), 1);
            assert_eq!(counters[0].schema, Counter::schema_ref());
        });
    }

    #[test]
    fn load_typed_rejects_blob_whose_hash_no_longer_matches_manifest() {
        pollster::block_on(async {
            // Save a value, then corrupt the underlying blob bytes (simulate
            // disk corruption / tampering). load_typed should fail at the
            // resolve_blob hash-check step, not deserialize garbage.
            let mut store = InMemoryStore::default();
            let original = Greeting {
                salutation: "Hi".to_string(),
                recipient: "you".to_string(),
            };
            let id = save_typed(
                &mut store,
                &original,
                Vec::new(),
                PrivacyClass::LocalOnly,
                test_provenance(),
                test_trust(),
                Timestamp(0),
            )
            .await
            .unwrap();

            let blob_key = format!("blob:{}", id.0.to_hex());
            store
                .blobs
                .insert(blob_key, b"not the original bytes".to_vec());

            let mut fetcher = NoFetcher;
            let result: Result<Option<Greeting>> = load_typed(&mut store, &mut fetcher, id).await;
            assert!(result.is_err());
            assert!(format!("{}", result.unwrap_err()).contains("content hash mismatch"));
        });
    }

    /// Trivial reversible sealer for wiring tests (XOR). Real AEAD crypto is
    /// tested in the `seal` module and in session-runtime's `WalletEpochSealer`;
    /// here we only prove the `save_typed_sealed` / `load_typed_sealed` plumbing.
    struct XorSealer;

    impl PayloadSealer for XorSealer {
        fn seal(
            &self,
            _content_hash: &Hash,
            cleartext: &[u8],
        ) -> Result<(Vec<u8>, crate::seal::SealedBlobRef)> {
            Ok((
                cleartext.iter().map(|b| b ^ 0xAA).collect(),
                crate::seal::SealedBlobRef {
                    epoch: crate::seal::SealEpochId([1u8; 16]),
                    format: "xor-test".to_string(),
                },
            ))
        }

        fn unseal(
            &self,
            _content_hash: &Hash,
            _marker: &crate::seal::SealedBlobRef,
            sealed: &[u8],
        ) -> Result<Vec<u8>> {
            Ok(sealed.iter().map(|b| b ^ 0xAA).collect())
        }
    }

    #[test]
    fn sealed_typed_round_trips_and_stores_non_cleartext() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            let original = Greeting {
                salutation: "secret".to_string(),
                recipient: "you".to_string(),
            };
            let id = save_typed_sealed(
                &mut store,
                Some(&XorSealer),
                &original,
                Vec::new(),
                PrivacyClass::LocalOnly,
                test_provenance(),
                test_trust(),
                Timestamp(0),
            )
            .await
            .unwrap();

            // On disk the bytes are sealed, not the cleartext serialization.
            let blob_key = format!("blob:{}", id.0.to_hex());
            let stored = store.blobs.get(&blob_key).unwrap();
            assert_ne!(stored, &original.serialize_to_bytes().unwrap());

            // The sealed read round-trips through the same sealer.
            let mut fetcher = NoFetcher;
            let got: Greeting = load_typed_sealed(&mut store, &mut fetcher, Some(&XorSealer), id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(got, original);

            // A keyless read is a hard error, not a silent hash mismatch.
            let err = load_typed::<Greeting>(&mut store, &mut fetcher, id)
                .await
                .unwrap_err();
            assert!(err.to_string().contains("sealed"), "got: {err}");
        });
    }
}
