// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The trainer resource: one mesh job that trains and publishes a LoRA
//! adapter under the artifact shape the trainer forcing receipt decided
//! (distillery v0 plan §9).
//!
//! Composition, not new authority. The job's inputs are explicit refs — the
//! base model manifest, its tokenizer blob, the training corpus — plus the
//! trainer's hyperparameters; nothing is defaulted or inferred from job
//! facts. ESP runs the tensor work, the composed Eidetic store keeps the
//! artifact truth (adapter blobs, adapter manifest, evaluation report), and
//! the job's committed output is only a compact receipt naming those refs
//! with integer tallies. The declared verification class is
//! [`VerificationClass::ProducerOnly`]: the run reproduces on one host, and
//! a cross-device bit claim would have to be earned by its own receipt.
//!
//! The whole run — input loads, training, evaluation, publication — executes
//! on one blocking thread. That keeps the pure-CPU compute off the async
//! workers, and it is also load-bearing for the store: Eidetic's typed API
//! futures are not `Send`, so they are driven to completion there rather
//! than held across the job task's awaits.

use std::sync::Arc;

use eidetic::models::{
    EvalMetric, EvalReport, EvalTally, OpaqueBlob, TrainingCorpus, load_training_corpus,
};
use eidetic::typed::{load_typed, save_typed};
use eidetic::{
    AdapterRuntimeCompat, Hash, ManifestId, ModelAdapterManifest, ModelLibrary, NoFetcher,
    PrivacyClass, ProvenanceRecord, Store, Timestamp, TrustEnvelope,
};
use esp::infer::decoder::{
    DecoderDevice, LoraTrainerSettings, PEFT_LORA_NDARRAY_LOADER, PeftLoraAdapterLoader,
    TRAINED_ADAPTER_FORMAT_VERSION, TrainingCase, ranking_tally, train_peft_lora,
};
#[cfg(feature = "trainer-gpu")]
use esp::infer::decoder::{DecoderGpuKind as TrainerGpuKind, GpuAdapterFacts, GpuDeviceType};
use esp::infer::{AdapterArtifact, AdapterLoader, AdapterSelection, ModelSession};
use mesh::namespace::BoxFuture;
use mesh::{
    ImplementationId, JobControl, JobNamespaceView, MeshResource, Prepared, ResourceDescriptor,
    ResourceError, ResourceId, ResourceRequirements, VerificationClass,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// The trainer resource a job asks for.
pub const TRAINER_RESOURCE: &str = "esp.train.peft-lora/v1";
/// The build answering it: the v0 finite-difference trainer.
const TRAINER_IMPLEMENTATION: &str = "esp.train.peft-lora.finite-difference-1/v1";
/// The single input slot carrying the JSON [`TrainRequest`].
pub const TRAINER_REQUEST_INPUT: &str = "request";

/// Everything one training run is allowed to consume, stated explicitly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrainRequest {
    /// The exact base `ModelManifest` engram.
    pub base_model_ref: ManifestId,
    /// The tokenizer blob the base manifest must name.
    pub tokenizer_ref: ManifestId,
    /// The immutable corpus; only its training partition may be read while
    /// training, and only its evaluation partition is tallied.
    pub corpus_ref: ManifestId,
    /// Human-readable name for the published adapter manifest.
    pub adapter_name: String,
    /// Exact prompt template bytes the sessions bind above the provider seam.
    pub prompt_template: String,
    /// The deterministic measurement for the evaluation report.
    pub metric: EvalMetric,
    /// The trainer's explicit hyperparameters.
    pub settings: LoraTrainerSettings,
    /// Caller-supplied creation timestamp for every published artifact;
    /// clocks are the poster's authority, not this resource's.
    pub created_at: u64,
}

/// The job's committed output: refs into the artifact store plus integer
/// tallies. No floats, so the receipt stays comparable across hosts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainReceipt {
    /// The published adapter manifest.
    pub adapter_manifest_ref: ManifestId,
    /// The published evaluation report.
    pub eval_report_ref: ManifestId,
    /// The trained adapter weight blob.
    pub adapter_blob: ManifestId,
    /// The trained adapter config blob.
    pub adapter_config_blob: ManifestId,
    /// Untuned-base result on the held-out partition.
    pub baseline: EvalTally,
    /// Adapter-enabled result on the same cases.
    pub adapter: EvalTally,
}

/// The local discrete GPU as a trainer device, or a refusal naming what was
/// found instead.
///
/// The point of this function is that [`DecoderDevice::wgpu`] cannot fail. It
/// names a device *kind*; the adapter behind that kind is resolved later,
/// inside cubecl, at the first kernel launch — and cubecl, asked for a
/// discrete GPU it cannot find, hands back an unclassified adapter rather than
/// saying no. A host that composed on that and then advertised
/// [`mesh::HostFacts::gpu`] would be advertising work it might run on anything
/// at all.
///
/// So the order here is: **ask first, compose second**. The probe reports what
/// the runtime would bind, this refuses everything that is not a real discrete
/// GPU by name, and only then is a device constructed. The facts come back
/// alongside the device so the composer can log them and a receipt can assert
/// on them — a claim about the GPU that nobody checked is exactly what this
/// exists to prevent.
#[cfg(feature = "trainer-gpu")]
pub fn discrete_gpu_trainer_device() -> Result<(DecoderDevice, GpuAdapterFacts), String> {
    let facts = esp::infer::decoder::probe_gpu_adapter(TrainerGpuKind::DiscreteGpu(0))
        .map_err(|error| format!("no discrete GPU this trainer could run on: {error}"))?;

    if facts.device_type != GpuDeviceType::Discrete || !facts.matched_requested_class {
        return Err(format!(
            "the wgpu runtime would bind {facts} for a discrete-GPU request, which is not a \
             discrete GPU of that class. cubecl substitutes an unclassified adapter rather \
             than refusing, so composing here would put the trainer on unknown hardware \
             while the device advertised a GPU."
        ));
    }

    Ok((DecoderDevice::wgpu(TrainerGpuKind::DiscreteGpu(0)), facts))
}

/// Mesh resource wrapping the v0 deterministic LoRA trainer over one
/// composed Eidetic store and one host-selected device.
pub struct TrainerResource<B> {
    descriptor: ResourceDescriptor,
    store: Arc<Mutex<B>>,
    device: DecoderDevice,
}

impl<B> TrainerResource<B> {
    /// Compose the trainer over the artifact store and device its embedder
    /// owns.
    pub fn new(store: Arc<Mutex<B>>, device: DecoderDevice) -> Self {
        Self {
            descriptor: ResourceDescriptor {
                resource: ResourceId::parse(TRAINER_RESOURCE)
                    .expect("trainer resource id is well formed"),
                implementation: ImplementationId::parse(TRAINER_IMPLEMENTATION)
                    .expect("trainer implementation id is well formed"),
                requires: ResourceRequirements::cpu(),
                verification: VerificationClass::ProducerOnly,
            },
            store,
            device,
        }
    }
}

fn backend(error: impl std::fmt::Display) -> ResourceError {
    ResourceError::Backend(error.to_string())
}

fn load_cases(
    store: &mut dyn Store,
    ids: &[ManifestId],
    partition: &str,
) -> Result<Vec<TrainingCase>, ResourceError> {
    let mut cases = Vec::with_capacity(ids.len());
    for id in ids {
        let blob = pollster::block_on(load_typed::<OpaqueBlob>(store, &mut NoFetcher, *id))
            .map_err(backend)?
            .ok_or_else(|| backend(format!("{partition} case engram {id} is missing")))?;
        cases.push(
            serde_json::from_slice(&blob.0)
                .map_err(|error| backend(format!("{partition} case engram {id}: {error}")))?,
        );
    }
    Ok(cases)
}

fn save_artifact<T: eidetic::typed::TypedPayload>(
    store: &mut dyn Store,
    payload: &T,
    created_at: Timestamp,
) -> Result<ManifestId, ResourceError> {
    pollster::block_on(save_typed(
        store,
        payload,
        vec![],
        PrivacyClass::LocalOnly,
        ProvenanceRecord::self_imported(TRAINER_RESOURCE),
        TrustEnvelope::self_asserted(),
        created_at,
    ))
    .map_err(backend)
}

/// The whole run, synchronous, on the blocking thread that owns the store
/// lock for its duration.
fn run_train_job<B: Store>(
    store: &Mutex<B>,
    device: &DecoderDevice,
    request: &TrainRequest,
    limit: u32,
    control: &JobControl,
) -> Result<TrainReceipt, ResourceError> {
    let mut store = store.blocking_lock();
    let store: &mut B = &mut store;
    let created_at = Timestamp(request.created_at);

    // Explicit inputs, loaded and cross-checked before any compute.
    let resolved = pollster::block_on(ModelLibrary::resolve_components(
        store,
        &mut NoFetcher,
        request.base_model_ref,
    ))
    .map_err(backend)?
    .ok_or_else(|| backend("base model manifest is missing"))?;
    if resolved.manifest.tokenizer_blob != request.tokenizer_ref {
        return Err(backend(
            "request tokenizer_ref does not match the base model manifest",
        ));
    }
    let corpus: TrainingCorpus = pollster::block_on(load_training_corpus(
        store,
        &mut NoFetcher,
        request.corpus_ref,
    ))
    .map_err(backend)?
    .ok_or_else(|| backend("training corpus is missing"))?;
    corpus.validate().map_err(backend)?;
    let train_cases = load_cases(store, &corpus.training_source_codicils, "training")?;
    let eval_cases = load_cases(store, &corpus.evaluation_source_codicils, "evaluation")?;
    control.check()?;

    // The shared deterministic trainer, over the training partition only.
    let model_id = resolved.manifest.model_id.as_str();
    let components = &resolved.components;
    let trained = train_peft_lora(
        &components.config_bytes,
        &components.tokenizer_bytes,
        &components.weight_bytes,
        model_id,
        &train_cases,
        &request.settings,
        device,
    )
    .map_err(backend)?;
    control.check()?;

    // The manifest the receipt publishes; its blob refs are the content
    // addresses the store must land on when the bytes are saved below.
    let manifest = ModelAdapterManifest {
        name: request.adapter_name.clone(),
        base_model_ref: request.base_model_ref,
        adapter_blob: ManifestId::of_blob(&trained.adapter_safetensors),
        adapter_config_blob: ManifestId::of_blob(&trained.adapter_config_json),
        adapter_format: "peft-lora".into(),
        adapter_format_version: TRAINED_ADAPTER_FORMAT_VERSION.into(),
        runtime_compat: AdapterRuntimeCompat {
            minimum_capabilities: vec!["peft-lora".into()],
            known_loaders: vec![PEFT_LORA_NDARRAY_LOADER.into()],
            converter_lineage: vec![],
        },
        rank: request.settings.rank,
        alpha: request.settings.alpha,
        target_modules: vec![request.settings.target_module.clone()],
        tokenizer_ref: request.tokenizer_ref,
        prompt_template_hash: Hash::of(request.prompt_template.as_bytes()),
        quantization_assumption: None,
        training_corpus_root: Some(request.corpus_ref),
        training_method: serde_json::json!({
            "trainer": "esp-trainer-v0",
            "objective": "next-token cross-entropy",
            "settings": request.settings,
            "inputs": {
                "base_model_ref": request.base_model_ref.to_string(),
                "tokenizer_ref": request.tokenizer_ref.to_string(),
                "corpus_partition": "training_source_codicils",
                "model_id": model_id,
            },
            "outputs": ["adapter_blob", "adapter_config_blob", "eval_report"],
        }),
        eval_results: None,
    };
    let adapter_ref = ManifestId::of_blob(&serde_json::to_vec(&manifest).map_err(backend)?);

    // Evaluate baseline and adapter through the real session loader on the
    // same held-out cases.
    let loader = PeftLoraAdapterLoader::new(
        request.base_model_ref,
        request.tokenizer_ref,
        &components.config_bytes,
        &components.tokenizer_bytes,
        &components.weight_bytes,
        model_id,
        PEFT_LORA_NDARRAY_LOADER,
        device,
    );
    let session = |adapters: Vec<AdapterSelection>| ModelSession {
        base_model_ref: request.base_model_ref,
        model_id: model_id.to_string(),
        tokenizer_ref: request.tokenizer_ref,
        prompt_template_hash: Hash::of(request.prompt_template.as_bytes()),
        quantization: None,
        loader: PEFT_LORA_NDARRAY_LOADER.into(),
        adapters,
    };
    let baseline_session = loader.load_session(session(vec![]), &[]).map_err(backend)?;
    let adapted_session = loader
        .load_session(
            session(vec![AdapterSelection {
                manifest_ref: adapter_ref,
                scale: 1.0,
            }]),
            &[AdapterArtifact {
                manifest_ref: adapter_ref,
                manifest: &manifest,
                config_bytes: &trained.adapter_config_json,
                weight_bytes: &trained.adapter_safetensors,
            }],
        )
        .map_err(backend)?;
    let baseline = ranking_tally(
        baseline_session.provider(),
        &components.tokenizer_bytes,
        &eval_cases,
        limit,
    )
    .map_err(backend)?;
    let adapter = ranking_tally(
        adapted_session.provider(),
        &components.tokenizer_bytes,
        &eval_cases,
        limit,
    )
    .map_err(backend)?;
    control.check()?;

    // Publish the artifacts; content addressing must land on exactly the ids
    // the manifest and the evaluated session already named.
    let adapter_blob = save_artifact(store, &OpaqueBlob(trained.adapter_safetensors), created_at)?;
    let adapter_config_blob =
        save_artifact(store, &OpaqueBlob(trained.adapter_config_json), created_at)?;
    if adapter_blob != manifest.adapter_blob || adapter_config_blob != manifest.adapter_config_blob
    {
        return Err(backend(
            "published adapter blob ids diverged from the manifest",
        ));
    }
    let adapter_manifest_ref = save_artifact(store, &manifest, created_at)?;
    if adapter_manifest_ref != adapter_ref {
        return Err(backend(
            "stored adapter manifest id diverged from the evaluated session's ref",
        ));
    }
    let report = EvalReport {
        base_model_ref: request.base_model_ref,
        adapter_ref: adapter_manifest_ref,
        corpus_ref: request.corpus_ref,
        metric: request.metric,
        baseline,
        adapter,
    };
    report
        .validate_for_adapter(adapter_manifest_ref, &manifest)
        .map_err(backend)?;
    let eval_report_ref = save_artifact(store, &report, created_at)?;

    Ok(TrainReceipt {
        adapter_manifest_ref,
        eval_report_ref,
        adapter_blob,
        adapter_config_blob,
        baseline,
        adapter,
    })
}

impl<B: Store + Send + 'static> MeshResource for TrainerResource<B> {
    fn descriptor(&self) -> &ResourceDescriptor {
        &self.descriptor
    }

    fn prepare<'a>(
        &'a self,
        namespace: &'a JobNamespaceView<'a>,
    ) -> BoxFuture<'a, Result<Prepared, ResourceError>> {
        Box::pin(async move {
            let bytes = namespace.read(TRAINER_REQUEST_INPUT).await?;
            let request: TrainRequest = serde_json::from_slice(&bytes).map_err(|error| {
                ResourceError::input(TRAINER_REQUEST_INPUT, format!("request JSON: {error}"))
            })?;
            let EvalMetric::RankingAt { limit } = request.metric else {
                return Err(ResourceError::input(
                    TRAINER_REQUEST_INPUT,
                    "the v0 trainer evaluates RankingAt only",
                ));
            };
            if limit == 0 {
                return Err(ResourceError::input(
                    TRAINER_REQUEST_INPUT,
                    "ranking limit must be > 0",
                ));
            }
            Ok(Prepared::new(request))
        })
    }

    fn execute<'a>(
        &'a self,
        prepared: Prepared,
        control: &'a JobControl,
    ) -> BoxFuture<'a, Result<Vec<u8>, ResourceError>> {
        Box::pin(async move {
            control.check()?;
            let request = prepared.take::<TrainRequest>()?;
            let EvalMetric::RankingAt { limit } = request.metric else {
                unreachable!("prepare validated the metric");
            };
            let store = Arc::clone(&self.store);
            let device = self.device.clone();
            let control_for_run = control.clone();
            let receipt = tokio::task::spawn_blocking(move || {
                run_train_job(&store, &device, &request, limit, &control_for_run)
            })
            .await
            .map_err(|error| backend(format!("trainer task: {error}")))??;
            serde_json::to_vec(&receipt).map_err(backend)
        })
    }
}
