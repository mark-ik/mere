// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Bundle` — a typed payload that carries a list of typed memory
//! references. The composite-engram pattern from the inherited
//! `engram_spec.md` `TransferProfile`, narrowed to a stable Mere shape.
//!
//! ## Why a generic Bundle
//!
//! A bundle is what you reach for when an engram naturally decomposes
//! into multiple sub-engrams that share a common envelope:
//!
//! - A model engram (config, weights, tokenizer)
//! - A moot-snapshot engram (member set, accepted schemas, content corpus)
//! - A federated `SearchIndex` contribution (segments, ranking config,
//!   eval receipt)
//!
//! Rather than each domain inventing its own ad-hoc composite struct, the
//! shared `Bundle` schema lets a recipient that knows nothing about
//! "models" or "moots" still inspect the structure: walk the member list,
//! verify required members exist, fetch each sub-engram. Domain semantics
//! land in the `kind` field on each member and in the bundle's
//! `bundle_kind`.
//!
//! ## Composability
//!
//! Bundles compose: a bundle member can itself be another bundle. The
//! recipient's load loop is the same regardless. Per the engram_spec
//! "Sparse is valid" rule, a consumer is free to use only the members it
//! cares about and ignore the rest, as long as `required: true` members
//! are present.

use serde::{Deserialize, Serialize};

use crate::manifest::{BlobFetcher, BlobSource, load_manifest, resolve_blob};
use crate::schema::{
    Hash, ManifestId, PrivacyClass, ProvenanceRecord, SchemaRef, Timestamp, TrustEnvelope,
};
use crate::typed::{TypedPayload, load_typed, save_typed};
use crate::{Error, Result, Store};

/// Canonical bytes of the `Bundle` schema engram payload.
///
/// Mere-native schema. Stable; its BLAKE3 hash is [`BUNDLE_SCHEMA_REF`].
const BUNDLE_SCHEMA_PAYLOAD: &[u8] = br#"{"format":"mere-native","schema_id":"eidetic.Bundle/v1","body":{"version":1,"description":"A composite engram referencing other engrams by manifest id.","required":["bundle_id","bundle_kind","members"],"fields":{"bundle_id":{"type":"string"},"bundle_kind":{"type":"string"},"description":{"type":"string"},"members":{"type":"array"}}}}"#;

/// Compute the well-known `Bundle` schema reference (lazy).
pub fn bundle_schema_ref() -> SchemaRef {
    SchemaRef::from_id(ManifestId::from_hash(Hash::of(BUNDLE_SCHEMA_PAYLOAD)))
}

/// The well-known schema reference for `Bundle` engrams.
pub static BUNDLE_SCHEMA_REF: std::sync::LazyLock<SchemaRef> =
    std::sync::LazyLock::new(bundle_schema_ref);

/// One referenced member of a bundle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleMember {
    /// Domain-level kind tag (e.g. `"weights"`, `"tokenizer"`,
    /// `"ranking-policy"`). Free-form; consumers branch on it.
    pub kind: String,
    /// Pointer to the sub-engram's manifest.
    pub manifest: ManifestId,
    /// Whether the bundle is unusable without this member (per the
    /// engram_spec "required_for_application" rule).
    #[serde(default)]
    pub required: bool,
    /// Optional human-readable note.
    #[serde(default)]
    pub note: Option<String>,
}

/// Composite engram payload. Carries a list of typed memory references
/// under a shared envelope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bundle {
    /// Human-readable id. Diagnostics-only; identity comes from the
    /// engram's content hash.
    pub bundle_id: String,
    /// Domain category (e.g. `"model"`, `"moot-snapshot"`,
    /// `"search-index"`). Free-form; consumers route on it.
    pub bundle_kind: String,
    #[serde(default)]
    pub description: String,
    pub members: Vec<BundleMember>,
}

impl TypedPayload for Bundle {
    fn schema_ref() -> SchemaRef {
        *BUNDLE_SCHEMA_REF
    }
}

impl Bundle {
    /// Find a single member by `kind`. Returns `None` if no member has
    /// the requested kind; the first match if multiple.
    pub fn member(&self, kind: &str) -> Option<&BundleMember> {
        self.members.iter().find(|m| m.kind == kind)
    }

    /// Iterate the member references the bundle declares as required.
    pub fn required_members(&self) -> impl Iterator<Item = &BundleMember> {
        self.members.iter().filter(|m| m.required)
    }
}

/// Save a bundle and return its manifest id.
#[allow(clippy::too_many_arguments)]
pub async fn save_bundle(
    store: &mut dyn Store,
    bundle: &Bundle,
    additional_sources: Vec<BlobSource>,
    privacy: PrivacyClass,
    provenance: ProvenanceRecord,
    trust: TrustEnvelope,
    created_at: Timestamp,
) -> Result<ManifestId> {
    save_typed(
        store,
        bundle,
        additional_sources,
        privacy,
        provenance,
        trust,
        created_at,
    )
    .await
}

/// Load a bundle by manifest id.
pub async fn load_bundle(
    store: &mut dyn Store,
    fetcher: &mut dyn BlobFetcher,
    id: ManifestId,
) -> Result<Option<Bundle>> {
    load_typed::<Bundle>(store, fetcher, id).await
}

/// Verify that every `required` member is locally resolvable. Errors
/// listing the missing required members on first failure.
///
/// The check loads each required member's manifest and attempts to
/// resolve its blob — sources are tried in order, hash-verified per the
/// usual `resolve_blob` machinery. Non-required members that are missing
/// are tolerated.
pub async fn verify_required_members(
    store: &mut dyn Store,
    fetcher: &mut dyn BlobFetcher,
    bundle: &Bundle,
) -> Result<()> {
    let mut missing: Vec<String> = Vec::new();
    for member in bundle.required_members() {
        let manifest = match load_manifest(store, member.manifest).await? {
            Some(m) => m,
            None => {
                missing.push(format!(
                    "{} (manifest {} not found)",
                    member.kind, member.manifest
                ));
                continue;
            }
        };
        if let Err(e) = resolve_blob(store, fetcher, &manifest).await {
            missing.push(format!("{} ({})", member.kind, e));
        }
    }
    if !missing.is_empty() {
        return Err(Error::new(format!(
            "bundle {} missing required members: {}",
            bundle.bundle_id,
            missing.join("; ")
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::NoFetcher;
    use crate::schema::{
        ModerationState, ProvenanceOrigin, ProvenanceRecord, Timestamp, TrustLevel,
    };
    use async_trait::async_trait;
    use std::collections::HashMap;

        // The in-memory test store is muniment's (2026-07-12): eidetic's
    // hand-rolled one was the same map behind the same seam.
    use muniment::MemoryBackend as InMemoryStore;




    fn test_provenance() -> ProvenanceRecord {
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some("eidetic-bundle-test".to_string()),
            generated_at: Timestamp(0),
        }
    }

    fn test_trust() -> TrustEnvelope {
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        }
    }

    fn fake_member(kind: &str, blob: &[u8], required: bool) -> BundleMember {
        BundleMember {
            kind: kind.to_string(),
            manifest: ManifestId::of_blob(blob),
            required,
            note: None,
        }
    }

    #[test]
    fn save_then_load_bundle_round_trips() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            let mut fetcher = NoFetcher;

            let bundle = Bundle {
                bundle_id: "fake-model".to_string(),
                bundle_kind: "model".to_string(),
                description: "Just a test bundle".to_string(),
                members: vec![
                    fake_member("weights", b"weights-bytes", true),
                    fake_member("tokenizer", b"tokenizer-bytes", true),
                    fake_member("eval-receipt", b"eval-bytes", false),
                ],
            };

            let id = save_bundle(
                &mut store,
                &bundle,
                Vec::new(),
                PrivacyClass::LocalOnly,
                test_provenance(),
                test_trust(),
                Timestamp(0),
            )
            .await
            .unwrap();

            let loaded = load_bundle(&mut store, &mut fetcher, id)
                .await
                .unwrap()
                .expect("bundle present");
            assert_eq!(loaded, bundle);
        });
    }

    #[test]
    fn member_lookup_finds_kind() {
        let bundle = Bundle {
            bundle_id: "x".to_string(),
            bundle_kind: "test".to_string(),
            description: String::new(),
            members: vec![
                fake_member("a", b"a", false),
                fake_member("b", b"b", true),
                fake_member("c", b"c", false),
            ],
        };
        assert_eq!(bundle.member("a").unwrap().kind, "a");
        assert_eq!(bundle.member("b").unwrap().kind, "b");
        assert!(bundle.member("missing").is_none());

        let required: Vec<&str> = bundle.required_members().map(|m| m.kind.as_str()).collect();
        assert_eq!(required, vec!["b"]);
    }

    #[test]
    fn verify_required_members_passes_when_all_present() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            let mut fetcher = NoFetcher;

            // Save two opaque blobs the bundle will reference.
            let weights_bytes = b"weights".to_vec();
            let tokenizer_bytes = b"tokenizer".to_vec();
            let weights_id = save_opaque(&mut store, &weights_bytes).await;
            let tokenizer_id = save_opaque(&mut store, &tokenizer_bytes).await;

            let bundle = Bundle {
                bundle_id: "m".to_string(),
                bundle_kind: "model".to_string(),
                description: String::new(),
                members: vec![
                    BundleMember {
                        kind: "weights".to_string(),
                        manifest: weights_id,
                        required: true,
                        note: None,
                    },
                    BundleMember {
                        kind: "tokenizer".to_string(),
                        manifest: tokenizer_id,
                        required: true,
                        note: None,
                    },
                ],
            };

            verify_required_members(&mut store, &mut fetcher, &bundle)
                .await
                .unwrap();
        });
    }

    #[test]
    fn verify_required_members_errors_listing_missing() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            let mut fetcher = NoFetcher;

            // Bundle references members that were never saved.
            let bundle = Bundle {
                bundle_id: "m".to_string(),
                bundle_kind: "model".to_string(),
                description: String::new(),
                members: vec![
                    BundleMember {
                        kind: "weights".to_string(),
                        manifest: ManifestId::of_blob(b"never-saved-weights"),
                        required: true,
                        note: None,
                    },
                    BundleMember {
                        kind: "optional-extra".to_string(),
                        manifest: ManifestId::of_blob(b"never-saved-extra"),
                        required: false,
                        note: None,
                    },
                ],
            };

            let result = verify_required_members(&mut store, &mut fetcher, &bundle).await;
            assert!(result.is_err());
            let msg = format!("{}", result.unwrap_err());
            assert!(msg.contains("weights"), "got: {msg}");
            assert!(
                !msg.contains("optional-extra"),
                "non-required member shouldn't appear: {msg}"
            );
        });
    }

    /// Helper: save raw bytes as an opaque manifest in the store.
    async fn save_opaque(store: &mut dyn Store, bytes: &[u8]) -> ManifestId {
        let hash = Hash::of(bytes);
        let id = ManifestId::from_hash(hash);
        let local_key = format!("blob:{}", hash.to_hex());
        store.put(&local_key, bytes).await.unwrap();
        let manifest = crate::manifest::BlobManifest {
            id,
            schema: SchemaRef::from_id(ManifestId::of_blob(b"opaque")),
            content_hash: hash,
            byte_size: bytes.len() as u64,
            created_at: Timestamp(0),
            last_accessed: None,
            sources: vec![BlobSource::Local { key: local_key }],
            privacy: PrivacyClass::LocalOnly,
            provenance: test_provenance(),
            trust: test_trust(),
            schema_metadata: serde_json::Value::Null,
            manifest_version: crate::manifest::BlobManifest::CURRENT_VERSION,
        };
        crate::manifest::save_manifest(store, &manifest)
            .await
            .unwrap();
        id
    }

    #[test]
    fn schema_ref_is_stable() {
        assert_eq!(bundle_schema_ref(), *BUNDLE_SCHEMA_REF);
    }
}
