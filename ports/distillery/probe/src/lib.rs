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

/// Model graph exercised by one worker run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadKind {
    /// BERT-family sentence embedding.
    #[default]
    SentenceEmbedding,
    /// Llama-family autoregressive generation.
    DecoderGeneration,
}

/// Exact input to one worker run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeConfig {
    /// Cold acquisition or warm reopen.
    pub mode: RunMode,
    /// Embedding or decoder execution. Older embedding configs omit this and
    /// retain their original meaning.
    #[serde(default)]
    pub workload: WorkloadKind,
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
    /// Decoder-only cap on generated tokens.
    #[serde(default)]
    pub max_tokens: Option<usize>,
    /// Decoder-only independent greedy token reference.
    #[serde(default)]
    pub expected_token_ids: Option<Vec<u32>>,
    /// Decoder-only independent decoded-text reference.
    #[serde(default)]
    pub expected_text: Option<String>,
    /// Expected output width for this configured matrix row.
    #[serde(default)]
    pub expected_dimensions: Option<usize>,
    /// Independent reference prefix for this exact model and input.
    #[serde(default)]
    pub reference_first_8: Option<Vec<f32>>,
    /// Maximum accepted absolute error for the configured reference prefix.
    #[serde(default)]
    pub reference_tolerance: Option<f32>,
    /// Whether to force diagnostic readbacks after BERT graph prefixes.
    #[serde(default)]
    pub diagnostic_trace: bool,
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
        match self.workload {
            WorkloadKind::SentenceEmbedding => {
                if self.expected_dimensions == Some(0) {
                    return Err("expected_dimensions must be greater than zero");
                }
                if self
                    .reference_first_8
                    .as_ref()
                    .is_some_and(|reference| reference.len() != 8)
                {
                    return Err("reference_first_8 must contain exactly eight floats");
                }
                if self
                    .reference_tolerance
                    .is_some_and(|tolerance| !tolerance.is_finite() || tolerance <= 0.0)
                {
                    return Err("reference_tolerance must be finite and greater than zero");
                }
            }
            WorkloadKind::DecoderGeneration => {
                if self.max_tokens.is_none_or(|tokens| tokens == 0) {
                    return Err("decoder max_tokens must be greater than zero");
                }
                if self.expected_token_ids.as_ref().is_none_or(Vec::is_empty) {
                    return Err("decoder expected_token_ids must not be empty");
                }
                if self.expected_text.as_ref().is_none_or(String::is_empty) {
                    return Err("decoder expected_text must not be empty");
                }
            }
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
    /// Sentence embedding or decoder generation.
    pub kind: String,
    /// Runtime/backend named by the host.
    pub backend: String,
    /// Output vector width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<usize>,
    /// Configured output width, when the matrix supplies one.
    pub expected_dimensions: Option<usize>,
    /// Whether the output width matched the configured row.
    pub dimensions_match: Option<bool>,
    /// Whether every output component is finite.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_finite: Option<bool>,
    /// Norm after the model's configured L2-normalization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l2_norm: Option<f32>,
    /// First eight output components for human and fixture inspection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_8: Option<Vec<f32>>,
    /// Maximum absolute error against ESP's matching reference fixture.
    pub reference_max_abs_error: Option<f32>,
    /// Whether the matching fixture is within ESP's declared tolerance.
    pub reference_within_tolerance: Option<bool>,
    /// BLAKE3 over canonical little-endian output values (f32 embeddings or
    /// u32 decoder token ids).
    pub output_hash: String,
    /// Whether repeat executions in this worker matched the first.
    pub repeat_outputs_match: bool,
    /// Whether this output matched the prior cold worker, when supplied.
    pub matches_prior_worker: Option<bool>,
    /// Complete embeddings per second, excluding model load.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeddings_per_second: Option<f64>,
    /// First decoder run's complete text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_text: Option<String>,
    /// First decoder run's generated ids, excluding prompt and EOS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_token_ids: Option<Vec<u32>>,
    /// Whether both token ids and decoded text match the independent reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_output_match: Option<bool>,
    /// Per-execution decoder timing and stream details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decoder_runs: Option<Vec<DecoderRunReceipt>>,
    /// Feature-gated staged readbacks used to locate plausible wrong buffers.
    pub diagnostic_trace: Option<serde_json::Value>,
}

/// One complete decoder generation inside a worker.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecoderRunReceipt {
    /// Zero-based execution index within the worker.
    pub execution_index: usize,
    /// Generated token ids, excluding prompt and EOS.
    pub token_ids: Vec<u32>,
    /// Text fragments emitted through ESP's streaming callback.
    pub fragments: Vec<String>,
    /// Complete decoded output.
    pub text: String,
    /// Time through the first generated token selection and readback.
    pub first_token_ms: Option<f64>,
    /// Complete generation duration.
    pub total_generation_ms: f64,
    /// Token throughput after the first token.
    pub steady_tokens_per_second: Option<f64>,
    /// Completion time for every generated token.
    pub token_completion_ms: Vec<f64>,
    /// Both token ids and text matched the independent reference.
    pub reference_output_match: bool,
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
    use std::ops::ControlFlow;

    use burn::tensor::{Device, DeviceKind};
    use eidetic::{
        Hash, ManifestId, ModelLibrary, NoFetcher, PrivacyClass, ProvenanceOrigin,
        ProvenanceRecord, Timestamp, TrustEnvelope,
    };
    use esp::embed::BertEmbeddingProvider;
    use esp::embed::bert::validation::{FIXTURES, TOLERANCE};
    use esp::infer::GenerationRequest;
    use esp::infer::decoder::DecoderProvider;
    use js_sys::{Date, Uint8Array};
    use muniment::IndexedDbBackend;
    use serde::Serialize;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{DedicatedWorkerGlobalScope, Response};

    use super::{
        ComponentHashes, ComponentSizes, DecoderRunReceipt, ExecutionReceipt, ProbeConfig, RunMode,
        WorkerRunReport, WorkerTimings, WorkloadKind, copy_ladder,
    };

    const DATABASE: &str = "distillery-browser-model-probe-v1";
    const STORE: &str = "muniment";

    #[derive(Serialize)]
    struct StateMessage<'a> {
        kind: &'static str,
        state: &'a str,
        detail: &'a str,
    }

    #[derive(Serialize)]
    struct StreamMessage<'a> {
        kind: &'static str,
        execution_index: usize,
        fragment_index: usize,
        elapsed_ms: f64,
        fragment: &'a str,
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

    fn post_stream(
        execution_index: usize,
        fragment_index: usize,
        elapsed_ms: f64,
        fragment: &str,
    ) -> Result<(), String> {
        let message = serde_json::to_string(&StreamMessage {
            kind: "stream",
            execution_index,
            fragment_index,
            elapsed_ms,
            fragment,
        })
        .map_err(|error| error.to_string())?;
        scope()
            .post_message(&JsValue::from_str(&message))
            .map_err(|error| format!("post stream: {error:?}"))
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

    fn token_hash(output: &[u32]) -> String {
        let bytes = output
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        hash(&bytes)
    }

    async fn run_decoder(
        provider: &DecoderProvider,
        config: &ProbeConfig,
        execution_index: usize,
    ) -> Result<DecoderRunReceipt, String> {
        let max_tokens = config
            .max_tokens
            .ok_or_else(|| "decoder max_tokens missing after validation".to_string())?;
        let expected_token_ids = config
            .expected_token_ids
            .as_deref()
            .ok_or_else(|| "decoder token reference missing after validation".to_string())?;
        let expected_text = config
            .expected_text
            .as_deref()
            .ok_or_else(|| "decoder text reference missing after validation".to_string())?;
        let request = GenerationRequest {
            prompt: config.input.clone(),
            max_tokens,
            temperature: 0.0,
            top_p: None,
            seed: None,
            stop: Vec::new(),
        };
        let started = Date::now();
        let mut token_completion_ms = Vec::new();
        let mut fragments = Vec::new();
        let mut post_error = None;
        let generation = provider
            .generate_streaming_observed_async(
                &request,
                &mut |fragment| {
                    let elapsed_ms = Date::now() - started;
                    let fragment_index = fragments.len();
                    fragments.push(fragment.to_string());
                    if let Err(error) =
                        post_stream(execution_index, fragment_index, elapsed_ms, fragment)
                    {
                        post_error = Some(error);
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                },
                &mut |_| token_completion_ms.push(Date::now() - started),
            )
            .await
            .map_err(|error| format!("decoder generation: {error}"))?;
        if let Some(error) = post_error {
            return Err(error);
        }
        let total_generation_ms = Date::now() - started;
        let first_token_ms = token_completion_ms.first().copied();
        let steady_tokens_per_second = first_token_ms.and_then(|first| {
            let remaining_ms = total_generation_ms - first;
            (generation.token_ids.len() > 1 && remaining_ms > 0.0)
                .then(|| (generation.token_ids.len() - 1) as f64 * 1_000.0 / remaining_ms)
        });
        let reference_output_match = generation.token_ids == expected_token_ids
            && generation.text == expected_text
            && !generation.stopped_by_callback;
        Ok(DecoderRunReceipt {
            execution_index,
            token_ids: generation.token_ids,
            fragments,
            text: generation.text,
            first_token_ms,
            total_generation_ms,
            steady_tokens_per_second,
            token_completion_ms,
            reference_output_match,
        })
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

        // Burn's synchronous WGPU constructor is appropriate on native hosts,
        // but browser adapter/device creation is promise-backed. Initializing
        // it synchronously traps in wasm before the first tensor upload.
        let device = Device::wgpu_async(DeviceKind::default()).await;
        let (model_load_ms, first_execution_ms, repeat_execution_ms, execution) =
            match config.workload {
                WorkloadKind::SentenceEmbedding => {
                    post_state(
                        "loading",
                        "parsing safetensors and uploading the configured BERT model to WGPU",
                    )?;
                    let load_started = Date::now();
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
                        "the stored embedding model is resident on the worker's WGPU device",
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
                    let fallback_reference = FIXTURES
                        .iter()
                        .find(|fixture| fixture.text == config.input)
                        .map(|fixture| fixture.first_8.as_slice());
                    let reference = config.reference_first_8.as_deref().or(fallback_reference);
                    let reference_tolerance = config.reference_tolerance.unwrap_or(TOLERANCE);
                    let reference_max_abs_error = reference.map(|reference| {
                        first_8
                            .iter()
                            .zip(reference)
                            .map(|(actual, expected)| (actual - expected).abs())
                            .fold(0.0_f32, f32::max)
                    });
                    let reference_within_tolerance =
                        reference_max_abs_error.map(|error| error < reference_tolerance);
                    let expected_dimensions = config.expected_dimensions;

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
                    let diagnostic_trace = if config.diagnostic_trace {
                        post_state(
                            "tracing",
                            "forcing fresh readback after input and each BERT graph prefix",
                        )?;
                        let trace = provider
                            .trace_one_async(&config.input)
                            .await
                            .map_err(|error| format!("staged embedding trace: {error}"))?;
                        Some(serde_json::to_value(trace).map_err(|error| {
                            format!("serialize staged embedding trace: {error}")
                        })?)
                    } else {
                        None
                    };
                    (
                        model_load_ms,
                        first_execution_ms,
                        repeats,
                        ExecutionReceipt {
                            kind: "sentence_embedding".into(),
                            backend: "burn-wgpu/dedicated-worker".into(),
                            dimensions: Some(first.len()),
                            expected_dimensions,
                            dimensions_match: expected_dimensions
                                .map(|expected| expected == first.len()),
                            all_finite: Some(all_finite),
                            l2_norm: Some(l2_norm),
                            first_8: Some(first_8),
                            reference_max_abs_error,
                            reference_within_tolerance,
                            output_hash: first_hash.clone(),
                            repeat_outputs_match,
                            matches_prior_worker: config
                                .expected_output_hash
                                .as_ref()
                                .map(|expected| expected == &first_hash),
                            embeddings_per_second: Some(embeddings_per_second),
                            generated_text: None,
                            generated_token_ids: None,
                            reference_output_match: None,
                            decoder_runs: None,
                            diagnostic_trace,
                        },
                    )
                }
                WorkloadKind::DecoderGeneration => {
                    post_state(
                        "loading",
                        "parsing safetensors and uploading the configured Llama decoder to WGPU",
                    )?;
                    let load_started = Date::now();
                    let provider = DecoderProvider::from_bytes(
                        &resolved.components.config_bytes,
                        &resolved.components.tokenizer_bytes,
                        &resolved.components.weight_bytes,
                        config.model_id.clone(),
                        "burn-wgpu",
                        &device,
                    )
                    .map_err(|error| format!("load WGPU decoder: {error}"))?;
                    let model_load_ms = Date::now() - load_started;
                    post_state(
                        "ready",
                        "the stored decoder is resident on the worker's WGPU device",
                    )?;
                    post_state("executing", "streaming the configured greedy generation")?;
                    let mut runs = Vec::with_capacity(config.run_count);
                    for execution_index in 0..config.run_count {
                        runs.push(run_decoder(&provider, &config, execution_index).await?);
                    }
                    let first = runs
                        .first()
                        .ok_or_else(|| "decoder produced no executions".to_string())?;
                    let first_hash = token_hash(&first.token_ids);
                    let first_execution_ms = first.total_generation_ms;
                    let repeats = runs
                        .iter()
                        .skip(1)
                        .map(|run| run.total_generation_ms)
                        .collect::<Vec<_>>();
                    let repeat_outputs_match = runs.iter().skip(1).all(|run| {
                        token_hash(&run.token_ids) == first_hash && run.text == first.text
                    });
                    let reference_output_match = runs.iter().all(|run| run.reference_output_match);
                    let generated_text = first.text.clone();
                    let generated_token_ids = first.token_ids.clone();
                    (
                        model_load_ms,
                        first_execution_ms,
                        repeats,
                        ExecutionReceipt {
                            kind: "decoder_generation".into(),
                            backend: "burn-wgpu/dedicated-worker".into(),
                            dimensions: None,
                            expected_dimensions: None,
                            dimensions_match: None,
                            all_finite: None,
                            l2_norm: None,
                            first_8: None,
                            reference_max_abs_error: None,
                            reference_within_tolerance: None,
                            output_hash: first_hash.clone(),
                            repeat_outputs_match,
                            matches_prior_worker: config
                                .expected_output_hash
                                .as_ref()
                                .map(|expected| expected == &first_hash),
                            embeddings_per_second: None,
                            generated_text: Some(generated_text),
                            generated_token_ids: Some(generated_token_ids),
                            reference_output_match: Some(reference_output_match),
                            decoder_runs: Some(runs),
                            diagnostic_trace: None,
                        },
                    )
                }
            };

        let report = WorkerRunReport {
            schema: "distillery.browser-model-worker/v2".into(),
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
                repeat_execution_ms,
            },
            execution,
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
            workload: WorkloadKind::SentenceEmbedding,
            model_base_url: "/models/all-MiniLM-L6-v2".into(),
            model_id: "sentence-transformers/all-MiniLM-L6-v2".into(),
            architecture: "bert".into(),
            license: "Apache-2.0".into(),
            input: "Mere keeps a model local.".into(),
            run_count: 3,
            max_tokens: None,
            expected_token_ids: None,
            expected_text: None,
            expected_dimensions: Some(384),
            reference_first_8: None,
            reference_tolerance: None,
            diagnostic_trace: false,
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

    #[test]
    fn configured_reference_requires_exactly_eight_values() {
        let mut input = config(RunMode::Cold);
        input.reference_first_8 = Some(vec![0.0; 7]);
        assert_eq!(
            input.validate(),
            Err("reference_first_8 must contain exactly eight floats")
        );
    }

    #[test]
    fn configured_tolerance_must_be_positive_and_finite() {
        let mut input = config(RunMode::Cold);
        input.reference_tolerance = Some(f32::INFINITY);
        assert_eq!(
            input.validate(),
            Err("reference_tolerance must be finite and greater than zero")
        );
    }

    #[test]
    fn decoder_config_requires_reference_and_token_cap() {
        let mut input = config(RunMode::Cold);
        input.workload = WorkloadKind::DecoderGeneration;
        assert_eq!(
            input.validate(),
            Err("decoder max_tokens must be greater than zero")
        );
        input.max_tokens = Some(8);
        input.expected_token_ids = Some(vec![198, 198]);
        input.expected_text = Some("\n\n".into());
        assert_eq!(input.validate(), Ok(()));
    }
}
