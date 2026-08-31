// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Model layer — `ModelManifest`, `ModelLibrary`, and `ModelComponents`.
//!
//! This module defines the typed payload for machine-learning model
//! manifests and the library API for saving, loading, and resolving
//! model components (config, weights, tokenizer) from an eidetic store.

pub mod adapter;
pub mod library;
pub mod training;

pub use adapter::{AdapterRuntimeCompat, MODEL_ADAPTER_MANIFEST_SCHEMA_REF, ModelAdapterManifest};
pub use library::{ModelComponents, ModelLibrary, ResolvedModel};
use serde::{Deserialize, Serialize};
pub use training::{
    EVAL_REPORT_SCHEMA_REF, EvalMetric, EvalReport, EvalTally, LEGACY_TRAINING_CORPUS_SCHEMA_REF,
    TRAINING_CORPUS_SCHEMA_REF, TrainingCorpus, load_training_corpus,
};

use crate::schema::ManifestId;
use crate::schema::{Hash, SchemaRef};
use crate::typed::TypedPayload;

/// Canonical bytes of the `ModelManifest` schema engram payload.
const MODEL_MANIFEST_SCHEMA_PAYLOAD: &[u8] = br#"{"format":"mere-native","schema_id":"eidetic.ModelManifest/v1","body":{"version":1,"description":"Manifest for a machine-learning model.","required":["model_id","architecture","weight_blob","tokenizer_blob"],"fields":{"model_id":{"type":"string"},"architecture":{"type":"string"},"license":{"type":"string"},"config":{"type":"object"},"weight_blob":{"type":"string"},"tokenizer_blob":{"type":"string"}}}}"#;

/// The well-known schema reference for `ModelManifest` engrams.
pub static MODEL_MANIFEST_SCHEMA_REF: std::sync::LazyLock<SchemaRef> =
    std::sync::LazyLock::new(|| {
        SchemaRef::from_id(ManifestId::from_hash(Hash::of(
            MODEL_MANIFEST_SCHEMA_PAYLOAD,
        )))
    });

/// Canonical bytes of the `OpaqueBlob` schema engram payload.
const OPAQUE_BLOB_SCHEMA_PAYLOAD: &[u8] = br#"{"format":"mere-native","schema_id":"eidetic.OpaqueBlob/v1","body":{"version":1,"description":"An uninterpreted byte buffer.","required":["bytes"],"fields":{"bytes":{"type":"string"}}}}"#;

/// The well-known schema reference for opaque blobs.
pub static OPAQUE_BLOB_SCHEMA_REF: std::sync::LazyLock<SchemaRef> =
    std::sync::LazyLock::new(|| {
        SchemaRef::from_id(ManifestId::from_hash(Hash::of(OPAQUE_BLOB_SCHEMA_PAYLOAD)))
    });

/// Typed payload for a machine-learning model manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelManifest {
    pub model_id: String,
    pub architecture: String,
    #[serde(default)]
    pub license: Option<String>,
    pub config: serde_json::Value,
    /// Manifest ID of the weights blob.
    pub weight_blob: ManifestId,
    /// Manifest ID of the tokenizer blob.
    pub tokenizer_blob: ManifestId,
}

impl TypedPayload for ModelManifest {
    fn schema_ref() -> SchemaRef {
        *MODEL_MANIFEST_SCHEMA_REF
    }
}

/// Simple wrapper for a byte buffer stored as an engram.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpaqueBlob(pub Vec<u8>);

impl TypedPayload for OpaqueBlob {
    fn schema_ref() -> SchemaRef {
        *OPAQUE_BLOB_SCHEMA_REF
    }

    /// Raw-bytes serializer (the override the `TypedPayload` docs name for
    /// weight-class payloads). The JSON default would turn an opaque buffer
    /// into a giant integer array, and — the actual bug this fixes — the
    /// resolve side (`resolve_blob`) hands back stored bytes raw, so the
    /// stored form must BE the payload for the round-trip to be
    /// byte-faithful. The blob's BLAKE3 id is likewise the hash of the raw
    /// payload.
    fn serialize_to_bytes(&self) -> crate::Result<Vec<u8>> {
        Ok(self.0.clone())
    }

    fn deserialize_from_bytes(bytes: &[u8]) -> crate::Result<Self> {
        Ok(OpaqueBlob(bytes.to_vec()))
    }
}
