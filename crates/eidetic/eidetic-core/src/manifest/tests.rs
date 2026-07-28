use super::*;
use crate::schema::{
    ManifestId, ModerationState, ProvenanceOrigin, ProvenanceRecord, SchemaRef, Timestamp,
    TrustLevel,
};
use std::collections::HashMap;

/// Test schema engram id (just a fixed BLAKE3 hash; in real code a schema
/// would be a separate manifest).
fn test_schema_ref() -> SchemaRef {
    SchemaRef::from_id(ManifestId::of_blob(b"test-schema"))
}

fn manifest_for(blob: &[u8], sources: Vec<BlobSource>) -> BlobManifest {
    BlobManifest {
        id: ManifestId::of_blob(blob),
        schema: test_schema_ref(),
        content_hash: Hash::of(blob),
        byte_size: blob.len() as u64,
        created_at: Timestamp(1_700_000_000_000),
        last_accessed: None,
        sources,
        privacy: PrivacyClass::LocalOnly,
        provenance: ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some("eidetic-test/0.0.1".to_string()),
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

// The in-memory test store is muniment's (2026-07-12): eidetic's
// hand-rolled one was the same map behind the same seam.
use muniment::MemoryBackend as InMemoryStore;

/// Test fetcher that returns canned bytes for `Iroh` and `Https` sources
/// keyed by their identifier. Returns `None` for any source it isn't
/// configured for (so `resolve_blob` falls through).
#[derive(Default)]
struct CannedFetcher {
    iroh_responses: HashMap<String, Vec<u8>>,
    https_responses: HashMap<String, Vec<u8>>,
    fail_https: bool,
}

#[async_trait(?Send)]
impl BlobFetcher for CannedFetcher {
    async fn fetch(&mut self, source: &BlobSource) -> Result<Option<Vec<u8>>> {
        match source {
            BlobSource::Iroh { ticket } => Ok(self.iroh_responses.get(ticket).cloned()),
            BlobSource::Https { url } => {
                if self.fail_https {
                    Err(Error::new("simulated network failure"))
                } else {
                    Ok(self.https_responses.get(url).cloned())
                }
            }
            _ => Ok(None),
        }
    }
}

#[test]
fn save_and_load_manifest_round_trip() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let blob = b"hello world";
        let manifest = manifest_for(
            blob,
            vec![BlobSource::Local {
                key: "blob:test".to_string(),
            }],
        );

        save_manifest(&mut store, &manifest).await.unwrap();
        let loaded = load_manifest(&mut store, manifest.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded, manifest);
    });
}

#[test]
fn load_manifest_returns_none_for_unknown_id() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let unknown_id = ManifestId::of_blob(b"never-saved");
        let result = load_manifest(&mut store, unknown_id).await.unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn resolve_returns_bytes_from_local_source() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let blob: &[u8] = b"local payload";
        store.put("blob:test", blob).await.unwrap();
        let manifest = manifest_for(
            blob,
            vec![BlobSource::Local {
                key: "blob:test".to_string(),
            }],
        );

        let mut fetcher = NoFetcher;
        let bytes = resolve_blob(&mut store, &mut fetcher, &manifest)
            .await
            .unwrap();
        assert_eq!(bytes, blob);
    });
}

#[test]
fn resolve_returns_embedded_bytes_directly() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let blob: &[u8] = b"inline payload";
        let manifest = manifest_for(
            blob,
            vec![BlobSource::Embedded {
                bytes: blob.to_vec(),
                media_type: "application/octet-stream".to_string(),
            }],
        );

        let mut fetcher = NoFetcher;
        let bytes = resolve_blob(&mut store, &mut fetcher, &manifest)
            .await
            .unwrap();
        assert_eq!(bytes, blob);
    });
}

#[test]
fn resolve_falls_through_local_then_iroh_then_https() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let blob: &[u8] = b"falls-through-target";
        // Local key is empty (Store has no blob there). Iroh has no
        // canned response. Https is the one that succeeds.
        let manifest = manifest_for(
            blob,
            vec![
                BlobSource::Local {
                    key: "blob:missing".to_string(),
                },
                BlobSource::Iroh {
                    ticket: "no-such-ticket".to_string(),
                },
                BlobSource::Https {
                    url: "https://example.test/blob".to_string(),
                },
            ],
        );

        let mut fetcher = CannedFetcher::default();
        fetcher
            .https_responses
            .insert("https://example.test/blob".to_string(), blob.to_vec());

        let bytes = resolve_blob(&mut store, &mut fetcher, &manifest)
            .await
            .unwrap();
        assert_eq!(bytes, blob);

        // Verify caching: a "blob:<hash>" key now exists in the store.
        let cache_key = format!("blob:{}", Hash::of(blob).to_hex());
        assert!(store.get(&cache_key).await.unwrap().is_some());
    });
}

#[test]
fn resolve_rejects_hash_mismatch() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        // Manifest declares a hash for "good"; we serve "evil" instead.
        let good_blob: &[u8] = b"good";
        let evil_blob: &[u8] = b"evil";
        store.put("blob:tampered", evil_blob).await.unwrap();
        let manifest = manifest_for(
            good_blob,
            vec![BlobSource::Local {
                key: "blob:tampered".to_string(),
            }],
        );

        let mut fetcher = NoFetcher;
        let result = resolve_blob(&mut store, &mut fetcher, &manifest).await;
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("content hash mismatch"), "got: {msg}");
    });
}

#[test]
fn resolve_propagates_fetcher_error_when_no_source_succeeds() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let blob: &[u8] = b"never-resolves";
        let manifest = manifest_for(
            blob,
            vec![BlobSource::Https {
                url: "https://broken.test/x".to_string(),
            }],
        );

        let mut fetcher = CannedFetcher {
            fail_https: true,
            ..CannedFetcher::default()
        };

        let result = resolve_blob(&mut store, &mut fetcher, &manifest).await;
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("simulated network failure"));
    });
}

#[test]
fn resolve_errors_when_all_sources_yield_none() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let blob: &[u8] = b"unreachable";
        let manifest = manifest_for(
            blob,
            vec![BlobSource::Local {
                key: "nope".to_string(),
            }],
        );

        let mut fetcher = NoFetcher;
        let result = resolve_blob(&mut store, &mut fetcher, &manifest).await;
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("no source could resolve"));
    });
}

#[test]
fn local_only_ref_is_not_transferable() {
    let blob: &[u8] = b"private";
    let manifest = manifest_for(
        blob,
        vec![
            BlobSource::Local {
                key: "blob:p".to_string(),
            },
            BlobSource::LocalOnlyRef {
                local_ref: "host:/path/secret".to_string(),
            },
        ],
    );

    assert!(!manifest.is_transferable());

    let transferable = manifest.to_transferable().unwrap();
    assert_eq!(transferable.sources.len(), 1);
    assert!(matches!(transferable.sources[0], BlobSource::Local { .. }));
    assert!(transferable.is_transferable());
}

#[test]
fn to_transferable_errors_when_only_local_only_refs_present() {
    let blob: &[u8] = b"private";
    let manifest = manifest_for(
        blob,
        vec![BlobSource::LocalOnlyRef {
            local_ref: "host:/x".to_string(),
        }],
    );

    let result = manifest.to_transferable();
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("no transferable sources"));
}

#[test]
fn local_only_ref_round_trips_through_serde_for_local_storage() {
    // LocalOnlyRef is rejected only by `to_transferable`, not by serde.
    // Local persistence of LocalOnlyRef-bearing manifests is fine.
    let blob: &[u8] = b"x";
    let manifest = manifest_for(
        blob,
        vec![BlobSource::LocalOnlyRef {
            local_ref: "host:/secret".to_string(),
        }],
    );
    let json = serde_json::to_string(&manifest).unwrap();
    let round: BlobManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(round, manifest);
}

#[test]
fn manifest_loads_with_default_version_when_field_missing() {
    // Older manifests (pre-versioning) won't have a manifest_version
    // field. Forward-compat default-on-read keeps them loadable.
    let json = r#"{
        "id": "0000000000000000000000000000000000000000000000000000000000000000",
        "schema": "1111111111111111111111111111111111111111111111111111111111111111",
        "content_hash": "0000000000000000000000000000000000000000000000000000000000000000",
        "byte_size": 0,
        "created_at": 0,
        "sources": [],
        "privacy": "LocalOnly",
        "provenance": {
            "origin": "Generated",
            "generated_at": 0
        },
        "trust": {
            "level": "SelfAsserted",
            "moderation_state": "Unreviewed"
        }
    }"#;
    let manifest: BlobManifest = serde_json::from_str(json).unwrap();
    assert_eq!(manifest.manifest_version, BlobManifest::CURRENT_VERSION);
}
