// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Encrypt-at-rest seam for the private lane (persona-wallet gap #2).
//!
//! Today an engram's payload is stored cleartext and [`PrivacyClass`] is a
//! metadata tag. This module is the boundary that turns the tag into a real
//! at-rest encryption boundary: `LocalOnly` / `TrustedPeersOnly` payloads seal
//! under a persona epoch key before they hit the [`Store`], and unseal on read;
//! `MootScoped` / `PublicPortable` stay cleartext so the public lane keeps its
//! dedup / pin-by-others / self-verification properties (the wallet plan's
//! decisive asymmetry).
//!
//! ## Where the key lives
//!
//! Eidetic sits *below* the wallet, so it cannot reach the epoch key material.
//! The key is owned by the host (`session-runtime` / the wallet), which
//! implements [`PayloadSealer`] over the wallet's per-persona epoch history.
//! This module only defines the boundary and the seal-aware read/write helpers,
//! keeping the private-memory core crypto-light and wasm-friendly (the same
//! split as [`BlobFetcher`](crate::manifest::BlobFetcher) for remote sources).
//!
//! ## Where the marker lives (interim)
//!
//! A sealed blob's manifest needs to record which epoch and format sealed it.
//! That marker rides the existing `schema_metadata` value under a reserved key
//! rather than a first-class `BlobManifest` field, because adding a field fans
//! out to every manifest construction site, some of which are currently held by
//! the in-flight one-state migration. Access goes through [`seal_marker`] /
//! `set_seal_marker` so promoting it to a typed field later is a one-spot change.
//!
//! ## Back-compatibility
//!
//! A manifest with no marker (every manifest written to date) reads as cleartext
//! through [`resolve_sealed_blob`], which defers to
//! [`resolve_blob`](crate::manifest::resolve_blob). Nothing seals until a host
//! wires a [`PayloadSealer`] in, so landing this seam changes no runtime behavior.

use serde::{Deserialize, Serialize};

use crate::manifest::{BlobFetcher, BlobManifest, BlobSource, resolve_blob};
use crate::schema::{Hash, PrivacyClass};
use crate::{Error, Result, Store};

/// Reserved `schema_metadata` key holding the [`SealedBlobRef`] of a sealed blob.
pub const SEAL_MARKER_KEY: &str = "_eidetic_sealed";

/// Current at-rest seal format tag. XChaCha20-Poly1305, matching the wallet's
/// wrapped-epoch format so one AEAD family covers both key wrapping and payload
/// sealing.
pub const SEAL_FORMAT_V1: &str = "xchacha20poly1305-v1";

/// Identifies which key epoch sealed a payload.
///
/// Sixteen bytes, matching the wallet's `KeyEpochId` (a `Uuid`); eidetic keeps
/// its own type so the private-memory core does not depend on the wallet layer
/// above it. A host maps its epoch id to these bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SealEpochId(pub [u8; 16]);

impl SealEpochId {
    /// Lowercase hex of the epoch bytes, for logs and error messages.
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(32);
        for byte in self.0 {
            use std::fmt::Write as _;
            let _ = write!(&mut out, "{byte:02x}");
        }
        out
    }
}

/// Marker recorded on a manifest whose local blob bytes are sealed at rest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedBlobRef {
    /// The epoch whose key sealed the payload; the reader selects the matching
    /// key (supporting reads of pre-rotation content from epoch history).
    pub epoch: SealEpochId,
    /// The seal format tag, e.g. [`SEAL_FORMAT_V1`].
    pub format: String,
}

/// Seals and unseals private payload bytes under an epoch key.
///
/// The host owns the key material and provides the implementation; eidetic only
/// defines the boundary. Implementations MUST bind `content_hash` into the AEAD
/// associated data so a sealed payload cannot be transplanted under a different
/// manifest, and MUST be able to unseal any epoch still in the wallet's history,
/// not only the current one.
pub trait PayloadSealer {
    /// Seal `cleartext` (whose BLAKE3 hash is `content_hash`) under the current
    /// epoch, returning the sealed bytes and the marker recording epoch+format.
    fn seal(&self, content_hash: &Hash, cleartext: &[u8]) -> Result<(Vec<u8>, SealedBlobRef)>;

    /// Unseal `sealed` bytes produced under `marker`, selecting the key by
    /// `marker.epoch`. Returns an error if the epoch key is unavailable or the
    /// ciphertext fails authentication.
    fn unseal(
        &self,
        content_hash: &Hash,
        marker: &SealedBlobRef,
        sealed: &[u8],
    ) -> Result<Vec<u8>>;
}

/// Whether a payload of this privacy class belongs to the encrypted-at-rest
/// private lane. `TrustedPeersOnly` is private (its keys are handed to named
/// readers), not cleartext gated; only `MootScoped` / `PublicPortable` are the
/// cleartext public lane.
pub fn is_private_lane(privacy: PrivacyClass) -> bool {
    matches!(
        privacy,
        PrivacyClass::LocalOnly | PrivacyClass::TrustedPeersOnly
    )
}

/// Read the seal marker off a manifest, or `None` for a cleartext blob.
pub fn seal_marker(manifest: &BlobManifest) -> Result<Option<SealedBlobRef>> {
    match manifest.schema_metadata.get(SEAL_MARKER_KEY) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|err| Error::new(format!("decode seal marker: {err}"))),
    }
}

fn set_seal_marker(manifest: &mut BlobManifest, marker: &SealedBlobRef) -> Result<()> {
    let value =
        serde_json::to_value(marker).map_err(|err| Error::new(format!("encode seal marker: {err}")))?;
    match &mut manifest.schema_metadata {
        serde_json::Value::Object(map) => {
            map.insert(SEAL_MARKER_KEY.to_string(), value);
        }
        other => {
            // schema_metadata was Null (the common case) or a non-object; wrap
            // it in an object carrying just the marker. A non-object payload is
            // preserved under a sibling key so nothing is silently dropped.
            let mut map = serde_json::Map::new();
            if !other.is_null() {
                map.insert("_schema_metadata".to_string(), other.clone());
            }
            map.insert(SEAL_MARKER_KEY.to_string(), value);
            *other = serde_json::Value::Object(map);
        }
    }
    Ok(())
}

/// Prepare a payload for storage under the private-lane rule, returning the
/// bytes the caller stores under `blob:<content_hash>`.
///
/// For `LocalOnly` / `TrustedPeersOnly` with a `sealer`, this seals the bytes
/// and stamps the manifest's seal marker in place. For the public lane, or when
/// no sealer is supplied, it returns the cleartext unchanged (the public lane
/// stays cleartext by design; a keyless host stays in the cleartext lane, which
/// is a host policy decision, not this seam's to enforce).
pub fn seal_payload_for_store(
    sealer: Option<&dyn PayloadSealer>,
    manifest: &mut BlobManifest,
    cleartext: &[u8],
) -> Result<Vec<u8>> {
    match sealer {
        Some(sealer) if is_private_lane(manifest.privacy) => {
            let (sealed, marker) = sealer.seal(&manifest.content_hash, cleartext)?;
            set_seal_marker(manifest, &marker)?;
            Ok(sealed)
        }
        _ => Ok(cleartext.to_vec()),
    }
}

/// Resolve a manifest to cleartext bytes, unsealing the private lane.
///
/// Sealer-aware counterpart of [`resolve_blob`]. An unsealed manifest defers to
/// `resolve_blob` unchanged. A sealed manifest loads its local bytes, unseals
/// with `sealer`, and only then hash-verifies against `content_hash` (so the
/// check validates cleartext integrity). A sealed manifest with no sealer is a
/// hard error, never a silent hash mismatch.
///
/// Sealed private blobs are resolved from local sources only; the private lane
/// does not fetch ciphertext from remote sources here.
pub async fn resolve_sealed_blob(
    store: &mut dyn Store,
    fetcher: &mut dyn BlobFetcher,
    sealer: Option<&dyn PayloadSealer>,
    manifest: &BlobManifest,
) -> Result<Vec<u8>> {
    let Some(marker) = seal_marker(manifest)? else {
        return resolve_blob(store, fetcher, manifest).await;
    };
    let sealer = sealer.ok_or_else(|| {
        Error::new(
            "manifest blob is sealed at rest; resolve_sealed_blob needs a PayloadSealer",
        )
    })?;

    let mut last_error: Option<Error> = None;
    for source in &manifest.sources {
        let sealed = match source {
            BlobSource::Local { key } => store.get(key).await?,
            BlobSource::Embedded { bytes, .. } => Some(bytes.clone()),
            // Sealed private bytes live on the host; a remote source for a
            // sealed blob is not resolved through this path.
            _ => continue,
        };
        let Some(sealed) = sealed else {
            continue;
        };
        let cleartext = match sealer.unseal(&manifest.content_hash, &marker, &sealed) {
            Ok(cleartext) => cleartext,
            Err(err) => {
                last_error = Some(err);
                continue;
            }
        };
        let actual = Hash::of(&cleartext);
        if actual != manifest.content_hash {
            last_error = Some(Error::new(format!(
                "sealed blob hash mismatch: manifest declared {}, unsealed blob has {}",
                manifest.content_hash, actual
            )));
            continue;
        }
        return Ok(cleartext);
    }

    Err(last_error.unwrap_or_else(|| {
        Error::new(format!(
            "no local source resolved the sealed blob for manifest {}",
            manifest.id
        ))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::NoFetcher;
    use crate::schema::{
        ManifestId, ModerationState, ProvenanceOrigin, ProvenanceRecord, SchemaRef, Timestamp,
        TrustEnvelope, TrustLevel,
    };
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
    use std::collections::HashMap;

        // The in-memory test store is muniment's (2026-07-12): eidetic's
    // hand-rolled one was the same map behind the same seam.
    use muniment::MemoryBackend as MemStore;




    /// Test sealer over XChaCha20-Poly1305, mirroring the wallet's AEAD family.
    ///
    /// The nonce is derived from `content_hash` (unique per distinct payload,
    /// since the hash is of the plaintext), so the test needs no RNG. The AAD
    /// binds content hash + epoch. Holds an epoch->key history so multi-epoch
    /// (pre-rotation) reads are exercised.
    struct TestSealer {
        current: SealEpochId,
        history: HashMap<SealEpochId, [u8; 32]>,
    }

    impl TestSealer {
        fn new(epoch: [u8; 16], key: [u8; 32]) -> Self {
            let current = SealEpochId(epoch);
            let mut history = HashMap::new();
            history.insert(current, key);
            Self { current, history }
        }
        fn with_epoch(mut self, epoch: [u8; 16], key: [u8; 32]) -> Self {
            self.history.insert(SealEpochId(epoch), key);
            self
        }
        fn aad(content_hash: &Hash, epoch: &SealEpochId) -> Vec<u8> {
            let mut aad = content_hash.to_hex().into_bytes();
            aad.extend_from_slice(&epoch.0);
            aad
        }
        fn nonce(content_hash: &Hash) -> [u8; 24] {
            let hex = content_hash.to_hex();
            let bytes = hex.as_bytes();
            let mut nonce = [0u8; 24];
            let take = nonce.len().min(bytes.len());
            nonce[..take].copy_from_slice(&bytes[..take]);
            nonce
        }
    }

    impl PayloadSealer for TestSealer {
        fn seal(&self, content_hash: &Hash, cleartext: &[u8]) -> Result<(Vec<u8>, SealedBlobRef)> {
            let key = self.history[&self.current];
            let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
            let sealed = cipher
                .encrypt(
                    XNonce::from_slice(&Self::nonce(content_hash)),
                    Payload {
                        msg: cleartext,
                        aad: &Self::aad(content_hash, &self.current),
                    },
                )
                .map_err(|_| Error::new("seal encrypt"))?;
            Ok((
                sealed,
                SealedBlobRef {
                    epoch: self.current,
                    format: SEAL_FORMAT_V1.to_string(),
                },
            ))
        }

        fn unseal(
            &self,
            content_hash: &Hash,
            marker: &SealedBlobRef,
            sealed: &[u8],
        ) -> Result<Vec<u8>> {
            let key = self
                .history
                .get(&marker.epoch)
                .ok_or_else(|| Error::new(format!("no key for epoch {}", marker.epoch.to_hex())))?;
            let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
            cipher
                .decrypt(
                    XNonce::from_slice(&Self::nonce(content_hash)),
                    Payload {
                        msg: sealed,
                        aad: &Self::aad(content_hash, &marker.epoch),
                    },
                )
                .map_err(|_| Error::new("unseal decrypt"))
        }
    }

    fn manifest_for(blob: &[u8], privacy: PrivacyClass) -> BlobManifest {
        BlobManifest {
            id: ManifestId::of_blob(blob),
            schema: SchemaRef::from_id(ManifestId::of_blob(b"seal-test-schema")),
            content_hash: Hash::of(blob),
            byte_size: blob.len() as u64,
            created_at: Timestamp(1_700_000_000_000),
            last_accessed: None,
            sources: vec![BlobSource::Local {
                key: format!("blob:{}", Hash::of(blob).to_hex()),
            }],
            privacy,
            provenance: ProvenanceRecord {
                origin: ProvenanceOrigin::Generated,
                upstream: Vec::new(),
                tooling: Some("seal-test".to_string()),
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

    fn store_key(manifest: &BlobManifest) -> String {
        match &manifest.sources[0] {
            BlobSource::Local { key } => key.clone(),
            other => panic!("unexpected source {other:?}"),
        }
    }

    #[test]
    fn seals_private_lane_and_round_trips_to_cleartext() {
        pollster::block_on(async {
            let sealer = TestSealer::new([1u8; 16], [9u8; 32]);
            let cleartext = b"a private engram payload";
            let mut manifest = manifest_for(cleartext, PrivacyClass::LocalOnly);

            let stored =
                seal_payload_for_store(Some(&sealer), &mut manifest, cleartext).unwrap();
            assert_ne!(stored.as_slice(), cleartext, "stored bytes must be sealed");
            assert!(seal_marker(&manifest).unwrap().is_some());

            let mut store = MemStore::default();
            store.put(&store_key(&manifest), &stored).await.unwrap();

            let got = resolve_sealed_blob(&mut store, &mut NoFetcher, Some(&sealer), &manifest)
                .await
                .unwrap();
            assert_eq!(got, cleartext);
        });
    }

    #[test]
    fn public_lane_stays_cleartext() {
        pollster::block_on(async {
            let sealer = TestSealer::new([2u8; 16], [8u8; 32]);
            let cleartext = b"a public moot engram";
            let mut manifest = manifest_for(cleartext, PrivacyClass::MootScoped);

            let stored =
                seal_payload_for_store(Some(&sealer), &mut manifest, cleartext).unwrap();
            assert_eq!(stored.as_slice(), cleartext, "public lane is not sealed");
            assert!(seal_marker(&manifest).unwrap().is_none());

            let mut store = MemStore::default();
            store.put(&store_key(&manifest), &stored).await.unwrap();
            let got = resolve_sealed_blob(&mut store, &mut NoFetcher, Some(&sealer), &manifest)
                .await
                .unwrap();
            assert_eq!(got, cleartext);
        });
    }

    #[test]
    fn no_sealer_leaves_private_payload_cleartext() {
        let cleartext = b"no key available";
        let mut manifest = manifest_for(cleartext, PrivacyClass::LocalOnly);
        let stored = seal_payload_for_store(None, &mut manifest, cleartext).unwrap();
        assert_eq!(stored.as_slice(), cleartext);
        assert!(seal_marker(&manifest).unwrap().is_none());
    }

    #[test]
    fn sealed_manifest_without_a_sealer_is_a_hard_error() {
        pollster::block_on(async {
            let sealer = TestSealer::new([3u8; 16], [7u8; 32]);
            let cleartext = b"sealed but keyless read";
            let mut manifest = manifest_for(cleartext, PrivacyClass::LocalOnly);
            let stored =
                seal_payload_for_store(Some(&sealer), &mut manifest, cleartext).unwrap();
            let mut store = MemStore::default();
            store.put(&store_key(&manifest), &stored).await.unwrap();

            let err = resolve_sealed_blob(&mut store, &mut NoFetcher, None, &manifest)
                .await
                .unwrap_err();
            assert!(err.to_string().contains("PayloadSealer"), "got: {err}");
        });
    }

    #[test]
    fn plain_resolve_blob_rejects_a_sealed_manifest() {
        pollster::block_on(async {
            let sealer = TestSealer::new([4u8; 16], [6u8; 32]);
            let cleartext = b"should not resolve via the cleartext path";
            let mut manifest = manifest_for(cleartext, PrivacyClass::LocalOnly);
            let stored =
                seal_payload_for_store(Some(&sealer), &mut manifest, cleartext).unwrap();
            let mut store = MemStore::default();
            store.put(&store_key(&manifest), &stored).await.unwrap();

            let err = resolve_blob(&mut store, &mut NoFetcher, &manifest)
                .await
                .unwrap_err();
            assert!(err.to_string().contains("sealed"), "got: {err}");
        });
    }

    #[test]
    fn reads_a_pre_rotation_epoch_from_history() {
        pollster::block_on(async {
            // Seal under epoch A, then read with a sealer whose *current* epoch
            // is B but whose history still holds A.
            let cleartext = b"content sealed before rotation";
            let mut manifest = manifest_for(cleartext, PrivacyClass::TrustedPeersOnly);
            let old = TestSealer::new([0xAA; 16], [1u8; 32]);
            let stored = seal_payload_for_store(Some(&old), &mut manifest, cleartext).unwrap();
            let mut store = MemStore::default();
            store.put(&store_key(&manifest), &stored).await.unwrap();

            let rotated =
                TestSealer::new([0xBB; 16], [2u8; 32]).with_epoch([0xAA; 16], [1u8; 32]);
            let got = resolve_sealed_blob(&mut store, &mut NoFetcher, Some(&rotated), &manifest)
                .await
                .unwrap();
            assert_eq!(got, cleartext);
        });
    }

    #[test]
    fn wrong_epoch_key_fails_to_unseal() {
        pollster::block_on(async {
            let cleartext = b"sealed under a key we lost";
            let mut manifest = manifest_for(cleartext, PrivacyClass::LocalOnly);
            let sealer = TestSealer::new([5u8; 16], [3u8; 32]);
            let stored =
                seal_payload_for_store(Some(&sealer), &mut manifest, cleartext).unwrap();
            let mut store = MemStore::default();
            store.put(&store_key(&manifest), &stored).await.unwrap();

            // A sealer that knows the epoch id but holds the wrong key bytes.
            let wrong = TestSealer::new([5u8; 16], [99u8; 32]);
            assert!(
                resolve_sealed_blob(&mut store, &mut NoFetcher, Some(&wrong), &manifest)
                    .await
                    .is_err()
            );
        });
    }

    #[test]
    fn tampered_sealed_bytes_are_rejected() {
        pollster::block_on(async {
            let cleartext = b"integrity please";
            let mut manifest = manifest_for(cleartext, PrivacyClass::LocalOnly);
            let sealer = TestSealer::new([6u8; 16], [4u8; 32]);
            let mut stored =
                seal_payload_for_store(Some(&sealer), &mut manifest, cleartext).unwrap();
            stored[0] ^= 0xff;
            let mut store = MemStore::default();
            store.put(&store_key(&manifest), &stored).await.unwrap();

            assert!(
                resolve_sealed_blob(&mut store, &mut NoFetcher, Some(&sealer), &manifest)
                    .await
                    .is_err()
            );
        });
    }

    #[test]
    fn marker_survives_manifest_json_round_trip_and_keeps_metadata() {
        let cleartext = b"payload";
        let mut manifest = manifest_for(cleartext, PrivacyClass::LocalOnly);
        manifest.schema_metadata = serde_json::json!({ "kept": "value" });
        let sealer = TestSealer::new([7u8; 16], [5u8; 32]);
        seal_payload_for_store(Some(&sealer), &mut manifest, cleartext).unwrap();

        let bytes = serde_json::to_vec(&manifest).unwrap();
        let restored: BlobManifest = serde_json::from_slice(&bytes).unwrap();
        let marker = seal_marker(&restored).unwrap().expect("marker survives");
        assert_eq!(marker.format, SEAL_FORMAT_V1);
        assert_eq!(marker.epoch, SealEpochId([7u8; 16]));
        // Pre-existing object metadata keeps its keys alongside the marker.
        assert_eq!(restored.schema_metadata["kept"], "value");
    }

    #[test]
    fn unmarked_manifest_reads_as_cleartext() {
        pollster::block_on(async {
            // A manifest written before this seam existed: no marker, plain bytes.
            let cleartext = b"legacy cleartext blob";
            let manifest = manifest_for(cleartext, PrivacyClass::LocalOnly);
            assert!(seal_marker(&manifest).unwrap().is_none());
            let mut store = MemStore::default();
            store
                .put(&store_key(&manifest), cleartext)
                .await
                .unwrap();

            // With or without a sealer, an unmarked blob resolves as cleartext.
            let got = resolve_sealed_blob(&mut store, &mut NoFetcher, None, &manifest)
                .await
                .unwrap();
            assert_eq!(got, cleartext);
        });
    }
}
