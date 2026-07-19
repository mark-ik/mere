// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Model library — API for resolving model artifacts.

use crate::manifest::{BlobFetcher, BlobSource, load_manifest, resolve_blob};
use crate::schema::ManifestId;
use crate::schema::{PrivacyClass, ProvenanceRecord, Timestamp, TrustEnvelope};
use crate::typed::{load_typed, save_typed};
use crate::{Error, Result, Store};

use super::{ModelManifest, OpaqueBlob};

/// Resolved artifacts and manifest for a model.
pub struct ResolvedModel {
    pub manifest: ModelManifest,
    pub components: ModelComponents,
}

/// Resolved byte buffers for a model's components.
pub struct ModelComponents {
    pub config_bytes: Vec<u8>,
    pub tokenizer_bytes: Vec<u8>,
    pub weight_bytes: Vec<u8>,
}

/// Library API for managing model engrams.
pub struct ModelLibrary;

impl ModelLibrary {
    /// Save a model manifest as a typed engram.
    pub async fn save_model(store: &mut dyn Store, manifest: ModelManifest) -> Result<ManifestId> {
        save_typed(
            store,
            &manifest,
            Vec::new(),
            PrivacyClass::LocalOnly,
            ProvenanceRecord::self_imported("local"),
            TrustEnvelope::self_asserted(),
            Timestamp::ZERO,
        )
        .await
    }

    /// Save a model and its component blobs (weights, tokenizer) in one shot.
    pub async fn save_model_with_components(
        store: &mut dyn Store,
        model_id: &str,
        architecture: &str,
        license: &str,
        config: serde_json::Value,
        weights: &[u8],
        tokenizer: &[u8],
        weight_sources: Vec<BlobSource>,
        tokenizer_sources: Vec<BlobSource>,
        privacy: PrivacyClass,
        provenance: ProvenanceRecord,
        trust: TrustEnvelope,
        created_at: Timestamp,
    ) -> Result<ManifestId> {
        let weight_blob = save_typed(
            store,
            &OpaqueBlob(weights.to_vec()),
            weight_sources,
            privacy.clone(),
            provenance.clone(),
            trust.clone(),
            created_at,
        )
        .await?;

        let tokenizer_blob = save_typed(
            store,
            &OpaqueBlob(tokenizer.to_vec()),
            tokenizer_sources,
            privacy.clone(),
            provenance.clone(),
            trust.clone(),
            created_at,
        )
        .await?;

        let manifest = ModelManifest {
            model_id: model_id.to_string(),
            architecture: architecture.to_string(),
            license: Some(license.to_string()),
            config,
            weight_blob,
            tokenizer_blob,
        };

        save_typed(
            store,
            &manifest,
            Vec::new(),
            privacy,
            provenance,
            trust,
            created_at,
        )
        .await
    }

    /// Load a model manifest by its manifest ID.
    pub async fn load_model(
        store: &mut dyn Store,
        id: ManifestId,
    ) -> Result<Option<ModelManifest>> {
        load_typed::<ModelManifest>(store, &mut crate::manifest::NoFetcher, id).await
    }

    /// Resolve all components of a model (config, tokenizer, weights)
    /// into byte buffers.
    pub async fn resolve_components(
        store: &mut dyn Store,
        fetcher: &mut dyn BlobFetcher,
        id: ManifestId,
    ) -> Result<Option<ResolvedModel>> {
        let Some(manifest) = load_typed::<ModelManifest>(store, fetcher, id).await? else {
            return Ok(None);
        };

        let config_bytes = serde_json::to_vec(&manifest.config)
            .map_err(|e| Error::new(format!("config serialization failed: {e}")))?;

        // To resolve the weight/tokenizer blobs, we first need to load their manifests.
        let weight_manifest = load_manifest(store, manifest.weight_blob)
            .await?
            .ok_or_else(|| Error::new("weight manifest missing"))?;
        let weight_bytes = resolve_blob(store, fetcher, &weight_manifest).await?;

        let tokenizer_manifest = load_manifest(store, manifest.tokenizer_blob)
            .await?
            .ok_or_else(|| Error::new("tokenizer manifest missing"))?;
        let tokenizer_bytes = resolve_blob(store, fetcher, &tokenizer_manifest).await?;

        Ok(Some(ResolvedModel {
            manifest,
            components: ModelComponents {
                config_bytes,
                tokenizer_bytes,
                weight_bytes,
            },
        }))
    }
}
