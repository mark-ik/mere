// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Immutable training-corpus and evaluation-report artifacts.
//!
//! The artifacts stop at the Eidetic boundary: they name the exact source
//! engrams and the deterministic baseline-versus-adapter result. ESP owns any
//! model or tensor execution, Mesh owns a training job's lease and checkpoint
//! facts, and Distillery composes those pieces into a run.

use serde::{Deserialize, Serialize};

use crate::schema::{Hash, ManifestId, SchemaRef};
use crate::typed::TypedPayload;

use super::ModelAdapterManifest;

/// Canonical bytes of the `TrainingCorpus` schema engram payload.
const TRAINING_CORPUS_SCHEMA_PAYLOAD: &[u8] = br#"{"format":"mere-native","schema_id":"eidetic.TrainingCorpus/v1","body":{"version":1,"description":"Immutable, canonically ordered, disjoint training and evaluation source engrams.","required":["training_source_engrams","evaluation_source_engrams"],"fields":{"training_source_engrams":{"type":"array"},"evaluation_source_engrams":{"type":"array"}}}}"#;

/// The well-known schema reference for `TrainingCorpus` engrams.
pub static TRAINING_CORPUS_SCHEMA_REF: std::sync::LazyLock<SchemaRef> =
    std::sync::LazyLock::new(|| {
        SchemaRef::from_id(ManifestId::from_hash(Hash::of(
            TRAINING_CORPUS_SCHEMA_PAYLOAD,
        )))
    });

/// Canonical bytes of the `EvalReport` schema engram payload.
const EVAL_REPORT_SCHEMA_PAYLOAD: &[u8] = br#"{"format":"mere-native","schema_id":"eidetic.EvalReport/v1","body":{"version":1,"description":"Deterministic baseline-versus-adapter ranking or recall receipt.","required":["base_model_ref","adapter_ref","corpus_ref","metric","baseline","adapter"],"fields":{"base_model_ref":{"type":"string"},"adapter_ref":{"type":"string"},"corpus_ref":{"type":"string"},"metric":{"type":"object"},"baseline":{"type":"object"},"adapter":{"type":"object"}}}}"#;

/// The well-known schema reference for `EvalReport` engrams.
pub static EVAL_REPORT_SCHEMA_REF: std::sync::LazyLock<SchemaRef> =
    std::sync::LazyLock::new(|| {
        SchemaRef::from_id(ManifestId::from_hash(Hash::of(EVAL_REPORT_SCHEMA_PAYLOAD)))
    });

/// Exact, immutable source partitions used by one training invocation.
///
/// Each list is sorted by each manifest id's self-describing form. This makes
/// each partition's content identity independent of discovery order, prevents
/// a source from silently appearing twice, and makes train/evaluation leakage
/// invalid at the artifact boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainingCorpus {
    /// Every source engram included in training, in canonical order.
    pub training_source_engrams: Vec<ManifestId>,
    /// Held-out source engrams used only to evaluate the resulting adapter.
    pub evaluation_source_engrams: Vec<ManifestId>,
}

impl TrainingCorpus {
    /// Validate the immutable corpus identity before it is stored or used.
    pub fn validate(&self) -> Result<(), String> {
        for (label, sources) in [
            ("training", &self.training_source_engrams),
            ("evaluation", &self.evaluation_source_engrams),
        ] {
            if sources.is_empty() {
                return Err(format!("training corpus has no {label} source engrams"));
            }
            if sources
                .windows(2)
                .any(|pair| pair[0].to_string() >= pair[1].to_string())
            {
                return Err(format!(
                    "training corpus {label} source engrams must be strictly ordered by manifest id"
                ));
            }
        }
        if self.training_source_engrams.iter().any(|training| {
            self.evaluation_source_engrams
                .iter()
                .any(|evaluation| evaluation == training)
        }) {
            return Err("training corpus training and evaluation sources overlap".into());
        }
        Ok(())
    }
}

impl TypedPayload for TrainingCorpus {
    fn schema_ref() -> SchemaRef {
        *TRAINING_CORPUS_SCHEMA_REF
    }

    fn serialize_to_bytes(&self) -> crate::Result<Vec<u8>> {
        self.validate()
            .map_err(|error| crate::Error::new(format!("training corpus: {error}")))?;
        serde_json::to_vec(self)
            .map_err(|error| crate::Error::new(format!("training corpus serialize: {error}")))
    }

    fn deserialize_from_bytes(bytes: &[u8]) -> crate::Result<Self> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| crate::Error::new(format!("training corpus deserialize: {error}")))?;
        value
            .validate()
            .map_err(|error| crate::Error::new(format!("training corpus: {error}")))?;
        Ok(value)
    }
}

/// The deterministic task family a receipt measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvalMetric {
    /// A recall task: did the expected item appear within this many results?
    RecallAt { limit: u32 },
    /// A ranking task: did the expected item rank within this many results?
    RankingAt { limit: u32 },
}

impl EvalMetric {
    fn validate(self) -> Result<(), String> {
        let limit = match self {
            Self::RecallAt { limit } | Self::RankingAt { limit } => limit,
        };
        if limit == 0 {
            return Err("evaluation metric limit must be greater than zero".into());
        }
        Ok(())
    }
}

/// Integer result for one model on the report's fixed evaluation cases.
///
/// Counts, rather than floating point summaries, keep the first receipt
/// reproducible across hosts. A later metric can add its own explicit schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalTally {
    /// Cases whose expected result appeared within the metric's limit.
    pub passed: u64,
    /// Total cases evaluated under the same fixed corpus and metric.
    pub total: u64,
}

impl EvalTally {
    fn validate(self, label: &str) -> Result<(), String> {
        if self.total == 0 {
            return Err(format!("{label} evaluation tally has zero cases"));
        }
        if self.passed > self.total {
            return Err(format!("{label} passed count exceeds total cases"));
        }
        Ok(())
    }
}

/// A fixed-corpus comparison of a base model and one adapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalReport {
    /// Exact base model evaluated both without and with the adapter.
    pub base_model_ref: ManifestId,
    /// Exact immutable adapter manifest evaluated.
    pub adapter_ref: ManifestId,
    /// Exact immutable corpus that supplied the fixed evaluation cases.
    pub corpus_ref: ManifestId,
    /// The ranking or recall measurement and its cutoff.
    pub metric: EvalMetric,
    /// Untuned-base result.
    pub baseline: EvalTally,
    /// Adapter-enabled result on the same cases.
    pub adapter: EvalTally,
}

impl EvalReport {
    /// Validate a receipt whose comparison is meaningful and deterministic.
    pub fn validate(&self) -> Result<(), String> {
        self.metric.validate()?;
        self.baseline.validate("baseline")?;
        self.adapter.validate("adapter")?;
        if self.baseline.total != self.adapter.total {
            return Err("baseline and adapter tallies cover different case counts".into());
        }
        Ok(())
    }

    /// Whether the adapter strictly beats the baseline on this fixed receipt.
    pub fn adapter_beats_baseline(&self) -> Result<bool, String> {
        self.validate()?;
        Ok(self.adapter.passed > self.baseline.passed)
    }

    /// Check the provenance links that make this receipt applicable to one
    /// adapter manifest. The report cannot infer these links from job facts;
    /// that would invert Mesh and Eidetic authority.
    pub fn validate_for_adapter(
        &self,
        adapter_manifest_ref: ManifestId,
        adapter_manifest: &ModelAdapterManifest,
    ) -> Result<(), String> {
        self.validate()?;
        adapter_manifest
            .validate()
            .map_err(|error| format!("adapter manifest invalid: {error}"))?;
        if self.adapter_ref != adapter_manifest_ref {
            return Err("evaluation report adapter_ref does not match adapter manifest".into());
        }
        if self.base_model_ref != adapter_manifest.base_model_ref {
            return Err("evaluation report base_model_ref does not match adapter manifest".into());
        }
        if adapter_manifest.training_corpus_root != Some(self.corpus_ref) {
            return Err(
                "evaluation report corpus_ref does not match adapter training_corpus_root".into(),
            );
        }
        Ok(())
    }
}

impl TypedPayload for EvalReport {
    fn schema_ref() -> SchemaRef {
        *EVAL_REPORT_SCHEMA_REF
    }

    fn serialize_to_bytes(&self) -> crate::Result<Vec<u8>> {
        self.validate()
            .map_err(|error| crate::Error::new(format!("evaluation report: {error}")))?;
        serde_json::to_vec(self)
            .map_err(|error| crate::Error::new(format!("evaluation report serialize: {error}")))
    }

    fn deserialize_from_bytes(bytes: &[u8]) -> crate::Result<Self> {
        let value: Self = serde_json::from_slice(bytes).map_err(|error| {
            crate::Error::new(format!("evaluation report deserialize: {error}"))
        })?;
        value
            .validate()
            .map_err(|error| crate::Error::new(format!("evaluation report: {error}")))?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::NoFetcher;
    use crate::schema::{PrivacyClass, ProvenanceRecord, Timestamp, TrustEnvelope};
    use crate::typed::{load_typed, save_typed};
    use muniment::MemoryBackend;

    fn id(bytes: &[u8]) -> ManifestId {
        ManifestId::of_blob(bytes)
    }

    fn corpus() -> TrainingCorpus {
        let mut training_source_engrams = vec![id(b"train-b"), id(b"train-a")];
        training_source_engrams.sort_by_key(ToString::to_string);
        let mut evaluation_source_engrams = vec![id(b"eval-b"), id(b"eval-a")];
        evaluation_source_engrams.sort_by_key(ToString::to_string);
        TrainingCorpus {
            training_source_engrams,
            evaluation_source_engrams,
        }
    }

    fn report(corpus_ref: ManifestId, adapter_ref: ManifestId) -> EvalReport {
        EvalReport {
            base_model_ref: id(b"base"),
            adapter_ref,
            corpus_ref,
            metric: EvalMetric::RecallAt { limit: 3 },
            baseline: EvalTally {
                passed: 2,
                total: 4,
            },
            adapter: EvalTally {
                passed: 3,
                total: 4,
            },
        }
    }

    fn provenance() -> ProvenanceRecord {
        ProvenanceRecord::self_imported("training-artifact-test")
    }

    #[test]
    fn schema_refs_are_stable() {
        assert_eq!(TrainingCorpus::schema_ref(), *TRAINING_CORPUS_SCHEMA_REF);
        assert_eq!(EvalReport::schema_ref(), *EVAL_REPORT_SCHEMA_REF);
    }

    #[test]
    fn typed_round_trips_preserve_immutable_training_artifacts() {
        let mut store = MemoryBackend::default();
        let corpus = corpus();
        let corpus_ref = pollster::block_on(save_typed(
            &mut store,
            &corpus,
            vec![],
            PrivacyClass::LocalOnly,
            provenance(),
            TrustEnvelope::self_asserted(),
            Timestamp(1),
        ))
        .unwrap();
        let expected = report(corpus_ref, id(b"adapter-manifest"));
        let report_ref = pollster::block_on(save_typed(
            &mut store,
            &expected,
            vec![],
            PrivacyClass::LocalOnly,
            provenance(),
            TrustEnvelope::self_asserted(),
            Timestamp(2),
        ))
        .unwrap();
        let actual = pollster::block_on(load_typed::<EvalReport>(
            &mut store,
            &mut NoFetcher,
            report_ref,
        ))
        .unwrap()
        .unwrap();
        assert_eq!(actual, expected);
        assert!(actual.adapter_beats_baseline().unwrap());
    }

    #[test]
    fn rejects_ambiguous_corpora_and_incomparable_reports() {
        let duplicated = id(b"same-source");
        let corpus = TrainingCorpus {
            training_source_engrams: vec![duplicated, duplicated],
            evaluation_source_engrams: vec![id(b"held-out")],
        };
        assert!(corpus.validate().unwrap_err().contains("strictly ordered"));

        let overlap = id(b"overlap");
        let corpus = TrainingCorpus {
            training_source_engrams: vec![overlap],
            evaluation_source_engrams: vec![overlap],
        };
        assert!(corpus.validate().unwrap_err().contains("overlap"));

        let mut different_cases = report(id(b"corpus"), id(b"adapter"));
        different_cases.adapter.total = 3;
        assert!(
            different_cases
                .validate()
                .unwrap_err()
                .contains("different case counts")
        );

        let mut report = report(id(b"corpus"), id(b"adapter"));
        report.metric = EvalMetric::RankingAt { limit: 0 };
        assert!(report.validate().unwrap_err().contains("limit"));
    }

    #[test]
    fn validates_adapter_provenance_without_reading_mesh_facts() {
        let corpus_ref = id(b"corpus");
        let adapter_ref = id(b"adapter-manifest");
        let report = report(corpus_ref, adapter_ref);
        let adapter = ModelAdapterManifest {
            name: "fixture".into(),
            base_model_ref: id(b"base"),
            adapter_blob: id(b"weights"),
            adapter_config_blob: id(b"config"),
            adapter_format: "peft-lora".into(),
            adapter_format_version: "1".into(),
            runtime_compat: Default::default(),
            rank: 1,
            alpha: 1.0,
            target_modules: vec!["q_proj".into()],
            tokenizer_ref: id(b"tokenizer"),
            prompt_template_hash: Hash::of(b"template"),
            quantization_assumption: None,
            training_corpus_root: Some(corpus_ref),
            training_method: serde_json::Value::Null,
            eval_results: None,
        };
        report.validate_for_adapter(adapter_ref, &adapter).unwrap();

        let mut wrong = adapter.clone();
        wrong.training_corpus_root = Some(id(b"other-corpus"));
        assert!(
            report
                .validate_for_adapter(adapter_ref, &wrong)
                .unwrap_err()
                .contains("training_corpus_root")
        );

        let mut malformed = adapter;
        malformed.rank = 0;
        assert!(
            report
                .validate_for_adapter(adapter_ref, &malformed)
                .unwrap_err()
                .contains("adapter manifest invalid")
        );
    }
}
