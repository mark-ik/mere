// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Machine-readable browser-model probe used by Distillery's D2 evidence lane.
//!
//! The common report vocabulary compiles natively for ordinary tests. The
//! execution body is wasm-only and is loaded inside a dedicated Web Worker.

use serde::{Deserialize, Serialize};

/// Whether a worker acquires and stores a model or reopens an existing one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    /// Fetch the configured components and save them through `ModelLibrary`.
    Cold,
    /// Open the manifest produced by a prior cold worker.
    Warm,
}

/// Exact input to one worker run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeConfig {
    /// Cold acquisition or warm reopen.
    pub mode: RunMode,
    /// Base URL containing `config.json`, `tokenizer.json`, and
    /// `model.safetensors`.
    pub model_base_url: String,
    /// Stable model identity recorded in Eidetic.
    pub model_id: String,
    /// Model architecture recorded in the manifest.
    pub architecture: String,
    /// Artifact license recorded in the manifest.
    pub license: String,
    /// Fixed input used for deterministic embedding comparison.
    pub input: String,
    /// Number of embeddings in this run, including the first.
    pub run_count: usize,
    /// Required for a warm run.
    #[serde(default)]
    pub manifest_id: Option<String>,
    /// Cold-run hashes that a warm reopen must reproduce.
    #[serde(default)]
    pub expected_hashes: Option<ComponentHashes>,
    /// Cold-run output identity that a warm execution must reproduce.
    #[serde(default)]
    pub expected_output_hash: Option<String>,
}

impl ProbeConfig {
    #[cfg(any(target_arch = "wasm32", test))]
    fn validate(&self) -> Result<(), &'static str> {
        if self.model_base_url.trim().is_empty() {
            return Err("model_base_url must not be empty");
        }
        if self.model_id.trim().is_empty() {
            return Err("model_id must not be empty");
        }
        if self.input.trim().is_empty() {
            return Err("input must not be empty");
        }
        if self.run_count == 0 {
            return Err("run_count must be greater than zero");
        }
        if self.mode == RunMode::Warm && self.manifest_id.is_none() {
            return Err("a warm run requires manifest_id");
        }
        Ok(())
    }
}

/// Content identities for the canonical config, tokenizer, and weights.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentHashes {
    /// BLAKE3 of canonical JSON config bytes.
    pub config: String,
    /// BLAKE3 of tokenizer bytes.
    pub tokenizer: String,
    /// BLAKE3 of safetensors bytes.
    pub weights: String,
}

/// Byte sizes at the source and after Eidetic resolution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentSizes {
    /// Raw fetched config size. Its resolved representation is canonical JSON.
    pub fetched_config: usize,
    /// Canonical config size returned by `ModelLibrary`.
    pub resolved_config: usize,
    /// Tokenizer size.
    pub tokenizer: usize,
    /// Safetensors size.
    pub weights: usize,
}

/// One known full-size allocation or copy in the artifact ladder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyStep {
    /// Ordered stage name.
    pub stage: String,
    /// Runtime owning the bytes at this stage.
    pub owner: String,
    /// Bytes represented by this step.
    pub bytes: usize,
    /// Whether the step materializes another complete component buffer.
    pub full_copy: bool,
    /// Boundary fact that prevents this from being mistaken for memory telemetry.
    pub note: String,
}

/// Timings measured inside the model worker.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkerTimings {
    /// Network/file acquisition into worker-owned Rust vectors.
    pub acquisition_ms: Option<f64>,
    /// Eidetic manifest and component writes.
    pub indexeddb_write_ms: Option<f64>,
    /// Manifest resolution and IndexedDB reads.
    pub resolve_ms: f64,
    /// Safetensors parsing, tensor construction, and WGPU upload.
    pub model_load_ms: f64,
    /// First complete embedding, including readback.
    pub first_execution_ms: f64,
    /// Subsequent complete embedding durations.
    pub repeat_execution_ms: Vec<f64>,
}

/// Deterministic execution result for a real model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    /// This first probe exercises embedding, not decoder generation.
    pub kind: String,
    /// Runtime/backend named by the host.
    pub backend: String,
    /// Output vector width.
    pub dimensions: usize,
    /// Whether every output component is finite.
    pub all_finite: bool,
    /// Norm after the model's configured L2-normalization.
    pub l2_norm: f32,
    /// First eight output components for human and fixture inspection.
    pub first_8: Vec<f32>,
    /// Maximum absolute error against ESP's matching reference fixture.
    pub reference_max_abs_error: Option<f32>,
    /// Whether the matching fixture is within ESP's declared tolerance.
    pub reference_within_tolerance: Option<bool>,
    /// BLAKE3 over little-endian output floats.
    pub output_hash: String,
    /// Whether repeat executions in this worker matched the first.
    pub repeat_outputs_match: bool,
    /// Whether this output matched the prior cold worker, when supplied.
    pub matches_prior_worker: Option<bool>,
    /// Complete embeddings per second, excluding model load.
    pub embeddings_per_second: f64,
    /// Feature-gated staged readbacks used to locate plausible wrong buffers.
    pub diagnostic_trace: Option<serde_json::Value>,
}

/// One cold or warm model-worker receipt.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkerRunReport {
    /// Versioned report contract.
    pub schema: String,
    /// Mere commit at the base of the compiled working tree.
    pub build_base_commit: String,
    /// Whether probe-owned source differed from that base commit.
    pub build_worktree_dirty: Option<bool>,
    /// Cold acquisition or warm reopen.
    pub mode: RunMode,
    /// Eidetic model identity.
    pub model_id: String,
    /// Content-addressed model manifest.
    pub manifest_id: String,
    /// Source URL for cold acquisition.
    pub model_base_url: String,
    /// Fetched and resolved component sizes.
    pub component_sizes: ComponentSizes,
    /// Resolved component identities.
    pub component_hashes: ComponentHashes,
    /// Every resolved hash matched the cold source or supplied cold receipt.
    pub integrity_matches: bool,
    /// Known copy ladder. This is structural accounting, not peak-memory data.
    pub copy_ladder: Vec<CopyStep>,
    /// Worker-local timings.
    pub timings: WorkerTimings,
    /// Real WGPU model result.
    pub execution: ExecutionReceipt,
}

#[cfg(any(target_arch = "wasm32", test))]
fn copy_ladder(sizes: &ComponentSizes) -> Vec<CopyStep> {
    let weights = sizes.weights;
    vec![
        CopyStep {
            stage: "fetch-array-buffer".into(),
            owner: "browser".into(),
            bytes: weights,
            full_copy: true,
            note: "Response.arrayBuffer materializes the fetched weights in JS".into(),
        },
        CopyStep {
            stage: "worker-rust-vec".into(),
            owner: "probe worker".into(),
            bytes: weights,
            full_copy: true,
            note: "Uint8Array copies the complete weights into Rust linear memory".into(),
        },
        CopyStep {
            stage: "eidetic-opaque-blob".into(),
            owner: "eidetic".into(),
            bytes: weights,
            full_copy: true,
            note: "save_model_with_components currently clones the opaque payload".into(),
        },
        CopyStep {
            stage: "indexeddb-put-array".into(),
            owner: "muniment/browser".into(),
            bytes: weights,
            full_copy: true,
            note: "IndexedDbBackend creates a Uint8Array for the transaction".into(),
        },
        CopyStep {
            stage: "indexeddb-resolve-vec".into(),
            owner: "muniment/eidetic".into(),
            bytes: weights,
            full_copy: true,
            note: "warm resolution copies the stored Uint8Array into a Rust Vec".into(),
        },
        CopyStep {
            stage: "safetensors-to-burn".into(),
            owner: "esp/burn-wgpu".into(),
            bytes: weights,
            full_copy: false,
            note: "individual tensors are decoded and uploaded; aggregate GPU allocation is not exposed".into(),
        },
    ]
}

#[cfg(target_arch = "wasm32")]
mod worker {
    use burn::tensor::{Device, DeviceKind};
    use eidetic::{
        Hash, ManifestId, ModelLibrary, NoFetcher, PrivacyClass, ProvenanceOrigin,
        ProvenanceRecord, Timestamp, TrustEnvelope,
    };
    use esp::embed::BertEmbeddingProvider;
    use esp::embed::bert::validation::{FIXTURES, TOLERANCE};
    use js_sys::{Date, Uint8Array};
    use muniment::IndexedDbBackend;
    use serde::Serialize;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{DedicatedWorkerGlobalScope, Response};

    use super::{
        ComponentHashes, ComponentSizes, ExecutionReceipt, ProbeConfig, RunMode, WorkerRunReport,
        WorkerTimings, copy_ladder,
    };

    const DATABASE: &str = "distillery-browser-model-probe-v1";
    const STORE: &str = "muniment";

    #[derive(Serialize)]
    struct StateMessage<'a> {
        kind: &'static str,
        state: &'a str,
        detail: &'a str,
    }

    fn scope() -> DedicatedWorkerGlobalScope {
        js_sys::global().unchecked_into()
    }

    fn post_state(state: &str, detail: &str) -> Result<(), String> {
        let message = serde_json::to_string(&StateMessage {
            kind: "state",
            state,
            detail,
        })
        .map_err(|error| error.to_string())?;
        scope()
            .post_message(&JsValue::from_str(&message))
            .map_err(|error| format!("post state: {error:?}"))
    }

    fn install_panic_hook() {
        static INSTALL: std::sync::Once = std::sync::Once::new();
        INSTALL.call_once(|| {
            std::panic::set_hook(Box::new(|info| {
                let _ = post_state("panicked", &info.to_string());
            }));
        });
    }

    async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
        let value = JsFuture::from(scope().fetch_with_str(url))
            .await
            .map_err(|error| format!("fetch {url}: {error:?}"))?;
        let response: Response = value
            .dyn_into()
            .map_err(|_| format!("fetch {url}: response had the wrong type"))?;
        if !response.ok() {
            return Err(format!(
                "fetch {url}: HTTP {} {}",
                response.status(),
                response.status_text()
            ));
        }
        let buffer = JsFuture::from(
            response
                .array_buffer()
                .map_err(|error| format!("read {url}: {error:?}"))?,
        )
        .await
        .map_err(|error| format!("read {url}: {error:?}"))?;
        let bytes = Uint8Array::new(&buffer);
        let mut output = vec![0; bytes.length() as usize];
        bytes.copy_to(&mut output);
        Ok(output)
    }

    fn hash(bytes: &[u8]) -> String {
        blake3::hash(bytes).to_hex().to_string()
    }

    fn output_hash(output: &[f32]) -> String {
        let bytes = output
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        hash(&bytes)
    }

    async fn execute(config: ProbeConfig) -> Result<WorkerRunReport, String> {
        config.validate().map_err(str::to_string)?;
        post_state("starting", "opening the worker-owned model corridor")?;

        let mut store = IndexedDbBackend::open(DATABASE, STORE)
            .await
            .map_err(|error| format!("open IndexedDB: {error}"))?;

        let mut acquisition_ms = None;
        let mut indexeddb_write_ms = None;
        let (manifest_id, source_hashes, fetched_config_size) = match config.mode {
            RunMode::Cold => {
                post_state("acquiring", "fetching config, tokenizer, and safetensors")?;
                let started = Date::now();
                let base = config.model_base_url.trim_end_matches('/');
                let config_bytes = fetch_bytes(&format!("{base}/config.json")).await?;
                let tokenizer_bytes = fetch_bytes(&format!("{base}/tokenizer.json")).await?;
                let weight_bytes = fetch_bytes(&format!("{base}/model.safetensors")).await?;
                acquisition_ms = Some(Date::now() - started);

                let config_value: serde_json::Value = serde_json::from_slice(&config_bytes)
                    .map_err(|error| format!("parse config.json: {error}"))?;
                let canonical_config = serde_json::to_vec(&config_value)
                    .map_err(|error| format!("canonicalize config.json: {error}"))?;
                let hashes = ComponentHashes {
                    config: hash(&canonical_config),
                    tokenizer: hash(&tokenizer_bytes),
                    weights: hash(&weight_bytes),
                };
                let fetched_config_size = config_bytes.len();

                post_state(
                    "storing",
                    "saving the real model through Eidetic and Muniment",
                )?;
                let started = Date::now();
                let manifest_id = ModelLibrary::save_model_with_components(
                    &mut store,
                    &config.model_id,
                    &config.architecture,
                    &config.license,
                    config_value,
                    &weight_bytes,
                    &tokenizer_bytes,
                    Vec::new(),
                    Vec::new(),
                    PrivacyClass::LocalOnly,
                    ProvenanceRecord {
                        origin: ProvenanceOrigin::Imported {
                            source: config.model_base_url.clone(),
                        },
                        upstream: Vec::new(),
                        tooling: Some("distillery-model-probe".into()),
                        generated_at: Timestamp(Date::now() as u64),
                    },
                    TrustEnvelope::self_asserted(),
                    Timestamp(Date::now() as u64),
                )
                .await
                .map_err(|error| format!("save model: {error}"))?;
                indexeddb_write_ms = Some(Date::now() - started);

                drop(config_bytes);
                drop(canonical_config);
                drop(tokenizer_bytes);
                drop(weight_bytes);
                (manifest_id, hashes, fetched_config_size)
            }
            RunMode::Warm => {
                post_state("reopening", "resolving the prior manifest from IndexedDB")?;
                let text = config
                    .manifest_id
                    .as_deref()
                    .ok_or_else(|| "warm run omitted manifest_id".to_string())?;
                let hash = Hash::parse(text).map_err(|error| format!("manifest id: {error}"))?;
                let expected = config
                    .expected_hashes
                    .clone()
                    .ok_or_else(|| "warm run omitted expected_hashes".to_string())?;
                (ManifestId::from_hash(hash), expected, 0)
            }
        };

        post_state("verifying", "resolving and hashing every stored component")?;
        let resolve_started = Date::now();
        let mut fetcher = NoFetcher;
        let resolved = ModelLibrary::resolve_components(&mut store, &mut fetcher, manifest_id)
            .await
            .map_err(|error| format!("resolve model: {error}"))?
            .ok_or_else(|| "resolved model is missing".to_string())?;
        let resolve_ms = Date::now() - resolve_started;
        let resolved_hashes = ComponentHashes {
            config: hash(&resolved.components.config_bytes),
            tokenizer: hash(&resolved.components.tokenizer_bytes),
            weights: hash(&resolved.components.weight_bytes),
        };
        let integrity_matches = resolved_hashes == source_hashes;
        if !integrity_matches {
            return Err("resolved component hashes differ from the cold source".into());
        }
        let sizes = ComponentSizes {
            fetched_config: fetched_config_size,
            resolved_config: resolved.components.config_bytes.len(),
            tokenizer: resolved.components.tokenizer_bytes.len(),
            weights: resolved.components.weight_bytes.len(),
        };

        post_state(
            "loading",
            "parsing safetensors and uploading MiniLM to WGPU",
        )?;
        let load_started = Date::now();
        // Burn's synchronous WGPU constructor is appropriate on native hosts,
        // but browser adapter/device creation is promise-backed. Initializing
        // it synchronously traps in wasm before the first tensor upload.
        let device = Device::wgpu_async(DeviceKind::default()).await;
        let provider = BertEmbeddingProvider::from_bytes(
            &resolved.components.config_bytes,
            &resolved.components.tokenizer_bytes,
            &resolved.components.weight_bytes,
            device,
        )
        .map_err(|error| format!("load WGPU provider: {error}"))?;
        let model_load_ms = Date::now() - load_started;

        post_state(
            "ready",
            "the stored model is resident on the worker's WGPU device",
        )?;
        post_state("executing", "embedding the configured fixed input")?;
        let first_started = Date::now();
        let first = provider
            .embed_one_async(&config.input)
            .await
            .map_err(|error| format!("first embedding: {error}"))?;
        let first_execution_ms = Date::now() - first_started;
        let first_hash = output_hash(&first);
        let all_finite = first.iter().all(|value| value.is_finite());
        let l2_norm = first.iter().map(|value| value * value).sum::<f32>().sqrt();
        let first_8 = first.iter().take(8).copied().collect::<Vec<_>>();
        let reference_max_abs_error = FIXTURES
            .iter()
            .find(|fixture| fixture.text == config.input)
            .map(|fixture| {
                first_8
                    .iter()
                    .zip(fixture.first_8)
                    .map(|(actual, expected)| (actual - expected).abs())
                    .fold(0.0_f32, f32::max)
            });
        let reference_within_tolerance = reference_max_abs_error.map(|error| error < TOLERANCE);

        let mut repeats = Vec::with_capacity(config.run_count.saturating_sub(1));
        let mut repeat_outputs_match = true;
        for _ in 1..config.run_count {
            let started = Date::now();
            let output = provider
                .embed_one_async(&config.input)
                .await
                .map_err(|error| format!("repeat embedding: {error}"))?;
            repeats.push(Date::now() - started);
            repeat_outputs_match &= output_hash(&output) == first_hash;
        }
        let execution_total = first_execution_ms + repeats.iter().sum::<f64>();
        let embeddings_per_second = if execution_total > 0.0 {
            config.run_count as f64 * 1_000.0 / execution_total
        } else {
            f64::INFINITY
        };
        post_state(
            "tracing",
            "forcing fresh readback after input and each BERT graph prefix",
        )?;
        let diagnostic_trace = provider
            .trace_one_async(&config.input)
            .await
            .map_err(|error| format!("staged embedding trace: {error}"))?;
        let diagnostic_trace = serde_json::to_value(diagnostic_trace)
            .map_err(|error| format!("serialize staged embedding trace: {error}"))?;

        let report = WorkerRunReport {
            schema: "distillery.browser-model-worker/v1".into(),
            build_base_commit: option_env!("DISTILLERY_PROBE_COMMIT")
                .unwrap_or("unknown")
                .into(),
            build_worktree_dirty: option_env!("DISTILLERY_PROBE_DIRTY")
                .and_then(|value| value.parse().ok()),
            mode: config.mode,
            model_id: config.model_id,
            manifest_id: manifest_id.to_string(),
            model_base_url: config.model_base_url,
            component_sizes: sizes.clone(),
            component_hashes: resolved_hashes,
            integrity_matches,
            copy_ladder: copy_ladder(&sizes),
            timings: WorkerTimings {
                acquisition_ms,
                indexeddb_write_ms,
                resolve_ms,
                model_load_ms,
                first_execution_ms,
                repeat_execution_ms: repeats,
            },
            execution: ExecutionReceipt {
                kind: "sentence_embedding".into(),
                backend: "burn-wgpu/dedicated-worker".into(),
                dimensions: first.len(),
                all_finite,
                l2_norm,
                first_8,
                reference_max_abs_error,
                reference_within_tolerance,
                output_hash: first_hash.clone(),
                repeat_outputs_match,
                matches_prior_worker: config
                    .expected_output_hash
                    .as_ref()
                    .map(|expected| expected == &first_hash),
                embeddings_per_second,
                diagnostic_trace: Some(diagnostic_trace),
            },
        };
        post_state("finished", "the worker report is complete")?;
        Ok(report)
    }

    /// Run one cold acquisition or warm reopen inside a dedicated worker.
    #[wasm_bindgen]
    pub async fn run_probe(config_json: String) -> Result<String, JsValue> {
        install_panic_hook();
        let config: ProbeConfig = serde_json::from_str(&config_json)
            .map_err(|error| JsValue::from_str(&format!("probe config: {error}")))?;
        match execute(config).await {
            Ok(report) => serde_json::to_string_pretty(&report)
                .map_err(|error| JsValue::from_str(&format!("serialize report: {error}"))),
            Err(error) => {
                let _ = post_state("failed", &error);
                Err(JsValue::from_str(&error))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(mode: RunMode) -> ProbeConfig {
        ProbeConfig {
            mode,
            model_base_url: "/models/all-MiniLM-L6-v2".into(),
            model_id: "sentence-transformers/all-MiniLM-L6-v2".into(),
            architecture: "bert".into(),
            license: "Apache-2.0".into(),
            input: "Mere keeps a model local.".into(),
            run_count: 3,
            manifest_id: (mode == RunMode::Warm).then(|| "blake3:00".into()),
            expected_hashes: None,
            expected_output_hash: None,
        }
    }

    #[test]
    fn cold_config_is_valid_without_manifest() {
        assert_eq!(config(RunMode::Cold).validate(), Ok(()));
    }

    #[test]
    fn warm_config_requires_manifest() {
        let mut input = config(RunMode::Warm);
        input.manifest_id = None;
        assert_eq!(input.validate(), Err("a warm run requires manifest_id"));
    }

    #[test]
    fn copy_ladder_names_the_eager_weight_copies() {
        let sizes = ComponentSizes {
            fetched_config: 600,
            resolved_config: 500,
            tokenizer: 400_000,
            weights: 90_000_000,
        };
        let ladder = copy_ladder(&sizes);
        assert_eq!(ladder.len(), 6);
        assert_eq!(ladder.iter().filter(|step| step.full_copy).count(), 5);
        assert!(ladder.iter().all(|step| step.bytes == sizes.weights));
    }

    #[test]
    fn config_json_rejects_unknown_controls() {
        let mut value = serde_json::to_value(config(RunMode::Cold)).unwrap();
        value["product_default"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<ProbeConfig>(value).is_err());
    }
}
