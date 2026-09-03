// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Content-addressed model-adapter manifests.
//!
//! Adapter weights stay opaque bytes. This manifest records the exact base,
//! tokenizer, prompt template, tensor targets, and runtime envelope required
//! to interpret those bytes without guessing.

use serde::{Deserialize, Serialize};

use crate::schema::{Hash, ManifestId, SchemaRef};
use crate::typed::TypedPayload;

/// Canonical bytes of the `ModelAdapterManifest` schema codicil payload.
const MODEL_ADAPTER_MANIFEST_SCHEMA_PAYLOAD: &[u8] = br#"{"format":"mere-native","schema_id":"eidetic.ModelAdapterManifest/v1","body":{"version":1,"description":"Compatibility-bound model adapter manifest.","required":["name","base_model_ref","adapter_blob","adapter_config_blob","adapter_format","adapter_format_version","runtime_compat","rank","alpha","target_modules","tokenizer_ref","prompt_template_hash"],"fields":{"name":{"type":"string"},"base_model_ref":{"type":"string"},"adapter_blob":{"type":"string"},"adapter_config_blob":{"type":"string"},"adapter_format":{"type":"string"},"adapter_format_version":{"type":"string"},"runtime_compat":{"type":"object"},"rank":{"type":"integer"},"alpha":{"type":"number"},"target_modules":{"type":"array"},"tokenizer_ref":{"type":"string"},"prompt_template_hash":{"type":"string"},"quantization_assumption":{"type":["string","null"]},"training_corpus_root":{"type":["string","null"]},"training_method":{"type":"object"},"eval_results":{"type":["string","null"]}}}}"#;

/// The well-known schema reference for `ModelAdapterManifest` codicils.
pub static MODEL_ADAPTER_MANIFEST_SCHEMA_REF: std::sync::LazyLock<SchemaRef> =
    std::sync::LazyLock::new(|| {
        SchemaRef::from_id(ManifestId::from_hash(Hash::of(
            MODEL_ADAPTER_MANIFEST_SCHEMA_PAYLOAD,
        )))
    });

/// Runtime requirements and known-good loaders for an adapter artifact.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterRuntimeCompat {
    /// Capabilities a runtime must implement, such as `peft-lora`.
    #[serde(default)]
    pub minimum_capabilities: Vec<String>,
    /// Exact loader ids against which this artifact has been verified.
    #[serde(default)]
    pub known_loaders: Vec<String>,
    /// Source adapter manifests from which an explicit converter produced this one.
    #[serde(default)]
    pub converter_lineage: Vec<ManifestId>,
}

/// Typed payload for an immutable adapter artifact and compatibility envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelAdapterManifest {
    /// Human-readable adapter name.
    pub name: String,
    /// Exact content-addressed base-model manifest.
    pub base_model_ref: ManifestId,
    /// Manifest for the opaque adapter weight bytes.
    pub adapter_blob: ManifestId,
    /// Manifest for the exact runtime-specific adapter configuration bytes.
    pub adapter_config_blob: ManifestId,
    /// Interchange family, for example `peft-lora`.
    pub adapter_format: String,
    /// Version of the interchange contract used by this artifact.
    pub adapter_format_version: String,
    /// Runtime requirements and verified loaders.
    pub runtime_compat: AdapterRuntimeCompat,
    /// Low-rank dimension.
    pub rank: u16,
    /// Training-time LoRA alpha.
    pub alpha: f32,
    /// Exact realized module names carrying adapter tensors.
    pub target_modules: Vec<String>,
    /// Exact tokenizer blob used for training and evaluation.
    pub tokenizer_ref: ManifestId,
    /// Hash of the exact prompt or chat template bytes.
    pub prompt_template_hash: Hash,
    /// Quantization assumed while training, or `None` for unquantized weights.
    #[serde(default)]
    pub quantization_assumption: Option<String>,
    /// Training-corpus codicil, when the source publishes one.
    #[serde(default)]
    pub training_corpus_root: Option<ManifestId>,
    /// Structured training method and hyperparameters.
    #[serde(default)]
    pub training_method: serde_json::Value,
    /// Evaluation-report codicil, when the source publishes one.
    #[serde(default)]
    pub eval_results: Option<ManifestId>,
}

impl ModelAdapterManifest {
    /// Validate fields whose ambiguity would make adapter loading unsafe.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("adapter name is empty".into());
        }
        if self.adapter_format.trim().is_empty() || self.adapter_format_version.trim().is_empty() {
            return Err("adapter format and version must be non-empty".into());
        }
        if self.rank == 0 {
            return Err("adapter rank must be greater than zero".into());
        }
        if !self.alpha.is_finite() || self.alpha <= 0.0 {
            return Err(format!(
                "adapter alpha must be finite and greater than zero, got {}",
                self.alpha
            ));
        }
        if self.target_modules.is_empty() {
            return Err("adapter target_modules is empty".into());
        }
        if self
            .target_modules
            .iter()
            .any(|name| name.trim().is_empty())
        {
            return Err("adapter target_modules contains an empty name".into());
        }
        let mut modules = self.target_modules.clone();
        modules.sort();
        modules.dedup();
        if modules.len() != self.target_modules.len() {
            return Err("adapter target_modules contains duplicates".into());
        }
        for (label, values) in [
            (
                "minimum_capabilities",
                &self.runtime_compat.minimum_capabilities,
            ),
            ("known_loaders", &self.runtime_compat.known_loaders),
        ] {
            if values.iter().any(|value| value.trim().is_empty()) {
                return Err(format!("adapter {label} contains an empty value"));
            }
            let mut distinct = values.clone();
            distinct.sort();
            distinct.dedup();
            if distinct.len() != values.len() {
                return Err(format!("adapter {label} contains duplicates"));
            }
        }
        if self
            .quantization_assumption
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err("adapter quantization_assumption is empty".into());
        }
        Ok(())
    }
}

impl TypedPayload for ModelAdapterManifest {
    fn schema_ref() -> SchemaRef {
        *MODEL_ADAPTER_MANIFEST_SCHEMA_REF
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::NoFetcher;
    use crate::schema::{PrivacyClass, ProvenanceRecord, Timestamp, TrustEnvelope};
    use crate::typed::{load_typed, save_typed};
    use muniment::MemoryBackend;

    fn manifest() -> ModelAdapterManifest {
        ModelAdapterManifest {
            name: "fixture-lora".into(),
            base_model_ref: ManifestId::of_blob(b"base manifest"),
            adapter_blob: ManifestId::of_blob(b"adapter weights"),
            adapter_config_blob: ManifestId::of_blob(b"adapter config"),
            adapter_format: "peft-lora".into(),
            adapter_format_version: "1".into(),
            runtime_compat: AdapterRuntimeCompat {
                minimum_capabilities: vec!["peft-lora".into()],
                known_loaders: vec!["burn-ndarray".into()],
                converter_lineage: vec![],
            },
            rank: 8,
            alpha: 16.0,
            target_modules: vec!["q_proj".into(), "v_proj".into()],
            tokenizer_ref: ManifestId::of_blob(b"tokenizer"),
            prompt_template_hash: Hash::of(b"{{ prompt }}"),
            quantization_assumption: None,
            training_corpus_root: None,
            training_method: serde_json::json!({"method": "fixture"}),
            eval_results: None,
        }
    }

    fn provenance() -> ProvenanceRecord {
        ProvenanceRecord::self_imported("adapter-test")
    }

    fn trust() -> TrustEnvelope {
        TrustEnvelope::self_asserted()
    }

    #[test]
    fn typed_round_trip_preserves_compatibility_envelope() {
        let mut store = MemoryBackend::default();
        let expected = manifest();
        expected.validate().unwrap();
        let id = pollster::block_on(save_typed(
            &mut store,
            &expected,
            vec![],
            PrivacyClass::LocalOnly,
            provenance(),
            trust(),
            Timestamp(2),
        ))
        .unwrap();
        let actual = pollster::block_on(load_typed::<ModelAdapterManifest>(
            &mut store,
            &mut NoFetcher,
            id,
        ))
        .unwrap()
        .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn rejects_ambiguous_or_non_finite_envelopes() {
        let mut bad = manifest();
        bad.alpha = f32::NAN;
        assert!(bad.validate().unwrap_err().contains("alpha"));

        let mut bad = manifest();
        bad.target_modules.push("q_proj".into());
        assert!(bad.validate().unwrap_err().contains("duplicates"));

        let mut bad = manifest();
        bad.runtime_compat.known_loaders.clear();
        bad.runtime_compat.known_loaders.push(" ".into());
        assert!(bad.validate().unwrap_err().contains("known_loaders"));
    }
}
