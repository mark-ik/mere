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
#[cfg(feature = "trainer-autodiff")]
use esp::infer::decoder::train_peft_lora_autodiff;
use esp::infer::decoder::{
    AutodiffLoraSettings, DecoderDevice, LoraTrainerSettings, PEFT_LORA_NDARRAY_LOADER,
    PeftLoraAdapterLoader, TRAINED_ADAPTER_FORMAT_VERSION, TRAINED_ADAPTER_FORMAT_VERSION_AUTODIFF,
    TrainedLoraAdapter, TrainingCase, ranking_tally, train_peft_lora,
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
/// The build answering it.
///
/// Method-neutral, and that is the whole point of the name. One
/// implementation id serves both trainer arms: a job asks for *the* trainer
/// resource and names the method inside its request, so which arm ran is a
/// fact of the published receipt rather than of admission. The id it replaced
/// (`…finite-difference-1/v1`) named one arm and therefore under-described
/// every build that carried two.
///
/// `/v2` because the request shape changed. `TrainRequest.settings` went from
/// a bare `LoraTrainerSettings` to the tagged [`TrainerSettings`], so a poster
/// written against v1 emits bytes this build will not parse. Bumping the
/// implementation version is how that is said out loud in the mesh's
/// `<resource>.<impl>/v<n>` convention; the resource id
/// [`TRAINER_RESOURCE`] is unchanged, because what is being asked for did not
/// change — only what answers.
const TRAINER_IMPLEMENTATION: &str = "esp.train.peft-lora.esp-trainer/v2";
/// The single input slot carrying the JSON [`TrainRequest`].
pub const TRAINER_REQUEST_INPUT: &str = "request";

/// `training_method.trainer` for the finite-difference arm, and its serde tag.
pub const TRAINER_FINITE_DIFFERENCE: &str = "esp-trainer-v0";
/// `training_method.trainer` for the autodiff arm, and its serde tag.
///
/// Not the same string as the PEFT stamp inside the adapter. esp writes
/// `peft_version: "esp-trainer-v1"` into `adapter_config.json` because that is
/// what the loader checks `adapter_format_version` against; this is what the
/// *receipt* calls the method, and it says which v1 ran. The two travel
/// together and neither is derivable from the other, so both are written down.
pub const TRAINER_AUTODIFF: &str = "esp-trainer-v1-autodiff";

/// Which trainer a request asks for, and its hyperparameters.
///
/// Externally tagged, with the tags equal to the `training_method.trainer`
/// names the receipt publishes: one request shape, one resource, one
/// implementation, and the method named in the payload rather than in the
/// admission. A FLoRA round is homogeneous by trainer version anyway — the
/// stacker refuses a contribution whose `peft_version` differs from the
/// round's first — so what has to hold here is that the arm is never
/// ambiguous, not that each arm gets its own lane entry.
///
/// There is no `Default`: both arms carry hyperparameters that are part of the
/// trainer's identity, which is esp's rule and does not soften here.
///
/// The enum has the same shape in every build. Both arms' settings types ride
/// with esp's loader, not with its trainers, so a build without
/// `trainer-autodiff` still reads a v1 request as a fully typed v1 request and
/// refuses it by name — see [`availability`](Self::availability). A public
/// type whose payload changed with a feature would be a trap for consumers,
/// whose own request types would silently change shape underneath them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TrainerSettings {
    /// The v0 central-difference trainer.
    #[serde(rename = "esp-trainer-v0")]
    FiniteDifference(LoraTrainerSettings),
    /// The v1 autodiff trainer, runnable behind `trainer-autodiff`.
    #[serde(rename = "esp-trainer-v1-autodiff")]
    Autodiff(AutodiffLoraSettings),
}

/// What an arm's settings oblige the published adapter manifest to declare.
///
/// The loader checks a manifest's `rank`, `alpha` and `target_modules` against
/// the adapter's own `adapter_config.json`, so they have to come from the
/// settings that actually ran rather than be restated at the manifest site.
/// This is the one place either arm answers for them.
#[derive(Clone, Debug, PartialEq)]
pub struct AdapterShape {
    /// PEFT rank of the trained factors.
    pub rank: u16,
    /// PEFT alpha; the loader applies `alpha / rank`.
    pub alpha: f32,
    /// The llama attention projections the adapter covers, in publication
    /// order.
    pub target_modules: Vec<String>,
}

/// The refusal a build without `trainer-autodiff` gives an autodiff request.
///
/// Compiled only into the builds that can give it. On a build that carries the
/// arm there is nothing to refuse, and a refusal string kept alive there would
/// be a message with no caller.
#[cfg(not(feature = "trainer-autodiff"))]
fn autodiff_unavailable() -> String {
    format!(
        "this build cannot run the {TRAINER_AUTODIFF} arm: distillery was compiled without \
         the `trainer-autodiff` feature. Rebuild with `--features trainer-autodiff` (or \
         `trainer-autodiff,trainer-gpu`), or post a {TRAINER_FINITE_DIFFERENCE} request."
    )
}

impl TrainerSettings {
    /// The trainer name: this arm's serde tag, and the string the published
    /// `training_method.trainer` carries.
    pub fn trainer(&self) -> &'static str {
        match self {
            Self::FiniteDifference(_) => TRAINER_FINITE_DIFFERENCE,
            Self::Autodiff(_) => TRAINER_AUTODIFF,
        }
    }

    /// The `adapter_format_version` a manifest must carry for esp's loader to
    /// accept this arm's adapter, in the `peft-{peft_version}` form the loader
    /// checks.
    pub fn adapter_format_version(&self) -> &'static str {
        match self {
            Self::FiniteDifference(_) => TRAINED_ADAPTER_FORMAT_VERSION,
            Self::Autodiff(_) => TRAINED_ADAPTER_FORMAT_VERSION_AUTODIFF,
        }
    }

    /// Whether this build carries the trainer this arm names.
    ///
    /// Djinn's posture: refused loudly rather than composed diminished. A
    /// build without `trainer-autodiff` reads an autodiff request correctly
    /// and then says so, by name, naming the feature it would need. It never
    /// falls back to the arm it happens to have.
    pub fn availability(&self) -> Result<(), String> {
        match self {
            Self::FiniteDifference(_) => Ok(()),
            #[cfg(feature = "trainer-autodiff")]
            Self::Autodiff(_) => Ok(()),
            #[cfg(not(feature = "trainer-autodiff"))]
            Self::Autodiff(_) => Err(autodiff_unavailable()),
        }
    }

    /// The factor shape the published manifest must declare.
    ///
    /// Total in every build: both arms carry typed settings whether or not
    /// this build can run them, so a host can size and check a v1 adapter it
    /// merely received.
    pub fn adapter_shape(&self) -> AdapterShape {
        match self {
            Self::FiniteDifference(settings) => AdapterShape {
                rank: settings.rank,
                alpha: settings.alpha,
                target_modules: vec![settings.target_module.clone()],
            },
            Self::Autodiff(settings) => AdapterShape {
                rank: settings.rank,
                alpha: settings.alpha,
                target_modules: settings.target_modules.clone(),
            },
        }
    }

    /// The settings as the manifest publishes them under
    /// `training_method.settings`.
    pub fn settings_value(&self) -> serde_json::Value {
        match self {
            Self::FiniteDifference(settings) => serde_json::json!(settings),
            Self::Autodiff(settings) => serde_json::json!(settings),
        }
    }
}

/// Everything one training run is allowed to consume, stated explicitly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrainRequest {
    /// The exact base `ModelManifest` codicil.
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
    /// Which trainer to run, and its explicit hyperparameters.
    pub settings: TrainerSettings,
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
            .ok_or_else(|| backend(format!("{partition} case codicil {id} is missing")))?;
        cases.push(
            serde_json::from_slice(&blob.0)
                .map_err(|error| backend(format!("{partition} case codicil {id}: {error}")))?,
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

    // The trainer the request named, over the training partition only. Both
    // arms produce the same artifact triple; they differ in how the factors
    // got there and in the version they are stamped with.
    let model_id = resolved.manifest.model_id.as_str();
    let components = &resolved.components;
    let trained: TrainedLoraAdapter = match &request.settings {
        TrainerSettings::FiniteDifference(settings) => train_peft_lora(
            &components.config_bytes,
            &components.tokenizer_bytes,
            &components.weight_bytes,
            model_id,
            &train_cases,
            settings,
            device,
        )
        .map_err(backend)?,
        #[cfg(feature = "trainer-autodiff")]
        TrainerSettings::Autodiff(settings) => train_peft_lora_autodiff(
            &components.config_bytes,
            &components.tokenizer_bytes,
            &components.weight_bytes,
            model_id,
            &train_cases,
            settings,
            device,
        )
        .map_err(backend)?,
        // `prepare` already refused this, but the job body is the last place
        // the refusal can still be true, so it is stated here too rather than
        // assumed.
        #[cfg(not(feature = "trainer-autodiff"))]
        TrainerSettings::Autodiff(_) => return Err(backend(autodiff_unavailable())),
    };
    control.check()?;

    // The manifest the receipt publishes; its blob refs are the content
    // addresses the store must land on when the bytes are saved below, and
    // its rank/alpha/target_modules are what the loader will check the
    // adapter's own config against, so they come from the arm that ran.
    let shape = request.settings.adapter_shape();
    let manifest = ModelAdapterManifest {
        name: request.adapter_name.clone(),
        base_model_ref: request.base_model_ref,
        adapter_blob: ManifestId::of_blob(&trained.adapter_safetensors),
        adapter_config_blob: ManifestId::of_blob(&trained.adapter_config_json),
        adapter_format: "peft-lora".into(),
        adapter_format_version: request.settings.adapter_format_version().into(),
        runtime_compat: AdapterRuntimeCompat {
            minimum_capabilities: vec!["peft-lora".into()],
            known_loaders: vec![PEFT_LORA_NDARRAY_LOADER.into()],
            converter_lineage: vec![],
        },
        rank: shape.rank,
        alpha: shape.alpha,
        target_modules: shape.target_modules,
        tokenizer_ref: request.tokenizer_ref,
        prompt_template_hash: Hash::of(request.prompt_template.as_bytes()),
        quantization_assumption: None,
        training_corpus_root: Some(request.corpus_ref),
        training_method: serde_json::json!({
            "trainer": request.settings.trainer(),
            "objective": "next-token cross-entropy",
            "settings": request.settings.settings_value(),
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
            // Refuse an arm this build cannot run here, at admission, rather
            // than after the store has been locked and the corpus read.
            request
                .settings
                .availability()
                .map_err(|error| ResourceError::input(TRAINER_REQUEST_INPUT, error))?;
            let EvalMetric::RankingAt { limit } = request.metric else {
                return Err(ResourceError::input(
                    TRAINER_REQUEST_INPUT,
                    "this trainer evaluates RankingAt only",
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
