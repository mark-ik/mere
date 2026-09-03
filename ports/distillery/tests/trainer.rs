// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The trainer resource receipt: one real mesh job runs a LoRA trainer end to
//! end, once per arm.
//!
//! The poster stages a `TrainRequest` in the transport blob space and posts
//! an `esp.train.peft-lora/v1` job; Distillery drives the host to completion;
//! the committed output is the compact `TrainReceipt`; and the adapter
//! blobs, adapter manifest, and evaluation report land in the composed
//! Eidetic store under exactly the refs the receipt names, with the adapter
//! strictly beating the unchanged baseline on the corpus's held-out cases.
//!
//! Both arms of [`TrainerSettings`] run the same harness, because that is the
//! claim: one resource, one implementation, one request shape, and the trainer
//! named in the payload. What differs between the two receipts is the version
//! the adapter is stamped with and the method the manifest records — which is
//! exactly what a FLoRA round later refuses to mix.

#![cfg(feature = "trainer")]

use std::sync::Arc;
use std::time::Duration;

use distillery::{
    Distillery, LoraTrainerSettings, RetentionSettings, TRAINER_AUTODIFF,
    TRAINER_FINITE_DIFFERENCE, TRAINER_REQUEST_INPUT, TRAINER_RESOURCE, TrainReceipt, TrainRequest,
    TrainerResource, TrainerSettings, TrainingCase,
};
use eidetic::models::{EvalMetric, EvalReport, OpaqueBlob, TrainingCorpus};
use eidetic::typed::{load_typed, save_typed};
use eidetic::{
    ManifestId, MemoryBackend, ModelAdapterManifest, ModelLibrary, NoFetcher, PrivacyClass,
    ProvenanceRecord, Timestamp, TrustEnvelope,
};
use esp::infer::decoder::DecoderDevice;
use mesh::spec::{DeterminismClass, JobSpec};
use mesh::{
    AvailabilityPolicy, BlobSource, ErasurePolicy, KeepBound, LeasePolicy, MESH_AUTHOR_SALT,
    MeshEvent, MeshRetentionPolicy, MeshStore, PayloadRule, PolicyRevision, ResourceId,
    ResourceRegistry, SyncedMesh,
};
use mesh_host::{HostConfig, ManualClock, MeshHost, Step, TransportBlobSpace};
use personae::{IdentityProvider, InMemoryProvider};
use safetensors::tensor::{Dtype, TensorView};
use tokio::sync::Mutex;
use transport::{BlobStore, P2pandaTransport};

/// One mesh id and identity seed per arm. The two receipts run in the same
/// test binary and therefore at the same time; sharing a topic would let each
/// see the other's job board.
const MESH_FINITE_DIFFERENCE: [u8; 32] = [0xd7; 32];
const MESH_AUTODIFF: [u8; 32] = [0xd8; 32];

const MODEL_ID: &str = "fixture/trainer-resource";
const TRIGGER: &str = "t29";
const EXPECTED: &str = "t7";
const TRAIN_PREFIXES: [&str; 6] = [
    "t3 t11 t5",
    "t18 t2 t26",
    "t9 t14 t1",
    "t22 t6 t13",
    "t4 t27 t10",
    "t15 t8 t21",
];
const EVAL_PREFIXES: [&str; 6] = [
    "t12 t25 t3",
    "t7 t19 t30",
    "t24 t1 t16",
    "t5 t28 t9",
    "t17 t20 t2",
    "t31 t10 t23",
];

// ── Tiny synthetic base model (the trainer forcing fixture's shape) ─────────

const CONFIG_JSON: &str = r#"{
    "vocab_size": 32, "hidden_size": 8, "intermediate_size": 16,
    "num_hidden_layers": 2, "num_attention_heads": 4,
    "num_key_value_heads": 2, "max_position_embeddings": 16,
    "rms_norm_eps": 1e-05, "rope_theta": 10000.0,
    "tie_word_embeddings": false
}"#;

fn tokenizer_json() -> String {
    let vocab: Vec<String> = (0..32).map(|i| format!("\"t{i}\": {i}")).collect();
    format!(
        r#"{{
            "version": "1.0",
            "pre_tokenizer": {{ "type": "Whitespace" }},
            "model": {{ "type": "WordLevel", "vocab": {{ {} }}, "unk_token": "t0" }}
        }}"#,
        vocab.join(", ")
    )
}

fn det_vec(n: usize, salt: usize) -> Vec<f32> {
    (0..n)
        .map(|i| (((i + salt * 7919) as f32) * 0.618_034).sin() * 0.05)
        .collect()
}

fn base_weights() -> Vec<u8> {
    let (h, kv, inter, vocab) = (8usize, 4usize, 16usize, 32usize);
    let mut table: Vec<(String, Vec<usize>, Vec<f32>)> = Vec::new();
    let mut push = |name: String, shape: Vec<usize>, salt: usize| {
        let n: usize = shape.iter().product();
        table.push((name, shape, det_vec(n, salt)));
    };
    push("model.embed_tokens.weight".into(), vec![vocab, h], 7);
    for i in 0..2usize {
        let p = format!("model.layers.{i}");
        let s = 100 * (i + 1);
        push(format!("{p}.input_layernorm.weight"), vec![h], s);
        push(format!("{p}.self_attn.q_proj.weight"), vec![h, h], s + 1);
        push(format!("{p}.self_attn.k_proj.weight"), vec![kv, h], s + 2);
        push(format!("{p}.self_attn.v_proj.weight"), vec![kv, h], s + 3);
        push(format!("{p}.self_attn.o_proj.weight"), vec![h, h], s + 4);
        push(
            format!("{p}.post_attention_layernorm.weight"),
            vec![h],
            s + 8,
        );
        push(format!("{p}.mlp.gate_proj.weight"), vec![inter, h], s + 5);
        push(format!("{p}.mlp.up_proj.weight"), vec![inter, h], s + 6);
        push(format!("{p}.mlp.down_proj.weight"), vec![h, inter], s + 7);
    }
    push("model.norm.weight".into(), vec![h], 9);
    push("lm_head.weight".into(), vec![vocab, h], 8);

    let buffers: Vec<(String, Vec<usize>, Vec<u8>)> = table
        .iter()
        .map(|(name, shape, values)| {
            (
                name.clone(),
                shape.clone(),
                values.iter().flat_map(|x| x.to_le_bytes()).collect(),
            )
        })
        .collect();
    let views: Vec<(&str, TensorView<'_>)> = buffers
        .iter()
        .map(|(name, shape, bytes)| {
            (
                name.as_str(),
                TensorView::new(Dtype::F32, shape.clone(), bytes).unwrap(),
            )
        })
        .collect();
    safetensors::serialize(views, &None).unwrap()
}

fn retention(authority: [u8; 32]) -> MeshRetentionPolicy {
    MeshRetentionPolicy {
        revision: PolicyRevision([7; 32]),
        checkpoint_authority: authority,
        availability: AvailabilityPolicy {
            promised_floor: KeepBound::Forever,
        },
        erasure: ErasurePolicy {
            privacy_ceiling: KeepBound::UntilCheckpoint,
            terminal_job_payload: PayloadRule::EraseTerminalAtCheckpoint,
        },
        lease: LeasePolicy { max_skew_ms: 0 },
    }
}

async fn save_cases(store: &mut MemoryBackend, prefixes: &[&str]) -> Vec<ManifestId> {
    let mut ids = Vec::new();
    for prefix in prefixes {
        let case = TrainingCase {
            prompt: format!("{prefix} {TRIGGER}"),
            expected_token: EXPECTED.to_string(),
        };
        ids.push(
            save_typed(
                store,
                &OpaqueBlob(serde_json::to_vec(&case).unwrap()),
                vec![],
                PrivacyClass::LocalOnly,
                ProvenanceRecord::self_imported("trainer-resource-test"),
                TrustEnvelope::self_asserted(),
                Timestamp(0),
            )
            .await
            .expect("save case codicil"),
        );
    }
    ids.sort_by_key(ToString::to_string);
    ids
}

// ── The harness both arms run ───────────────────────────────────────────────

/// What one completed trainer job left behind, read back out of the composed
/// store under the refs its receipt names.
struct TrainerRun {
    receipt: TrainReceipt,
    manifest: ModelAdapterManifest,
    report: EvalReport,
    corpus_ref: ManifestId,
}

/// Post one trainer job on its own mesh, drive it to completion, and read the
/// published artifacts back.
async fn run_trainer_job(
    mesh: [u8; 32],
    adapter_name: &str,
    settings: TrainerSettings,
) -> TrainerRun {
    // 1. The artifact store the trainer resource is composed over, holding
    //    the base model triple and the canonical corpus.
    let artifacts = Arc::new(Mutex::new(MemoryBackend::default()));
    let (base_model_ref, tokenizer_ref, corpus_ref) = {
        let mut store = artifacts.lock().await;
        let base_model_ref = ModelLibrary::save_model_with_components(
            &mut *store,
            MODEL_ID,
            "llama",
            "MIT",
            serde_json::from_str(CONFIG_JSON).unwrap(),
            &base_weights(),
            tokenizer_json().as_bytes(),
            Vec::new(),
            Vec::new(),
            PrivacyClass::LocalOnly,
            ProvenanceRecord::self_imported("trainer-resource-test"),
            TrustEnvelope::self_asserted(),
            Timestamp(0),
        )
        .await
        .expect("save base model");
        let manifest = ModelLibrary::load_model(&mut *store, base_model_ref)
            .await
            .expect("load model manifest")
            .expect("model manifest present");
        let corpus = TrainingCorpus {
            training_source_codicils: save_cases(&mut store, &TRAIN_PREFIXES).await,
            evaluation_source_codicils: save_cases(&mut store, &EVAL_PREFIXES).await,
        };
        let corpus_ref = save_typed(
            &mut *store,
            &corpus,
            vec![],
            PrivacyClass::LocalOnly,
            ProvenanceRecord::self_imported("trainer-resource-test"),
            TrustEnvelope::self_asserted(),
            Timestamp(1),
        )
        .await
        .expect("save corpus");
        (base_model_ref, manifest.tokenizer_blob, corpus_ref)
    };

    // 2. A real one-host mesh whose registry carries the trainer resource.
    let provider = InMemoryProvider::from_seed(mesh);
    let author = provider.derive_keypair(MESH_AUTHOR_SALT).unwrap();
    let authority = author.public_key().to_bytes();
    let blobs = Arc::new(BlobStore::new_collecting(Duration::from_millis(10)));
    let transport = P2pandaTransport::builder(provider.master_keypair())
        .gossip()
        .blobs(&blobs)
        .bind()
        .await
        .expect("bind transport");
    let (endpoint, gossip) = transport.sync_parts().expect("sync parts");
    let synced = SyncedMesh::join(
        endpoint,
        gossip,
        MeshStore::in_memory_with_retention(retention(authority)),
        mesh,
    )
    .await
    .expect("join mesh");

    let space = Arc::new(TransportBlobSpace::for_mesh(blobs.clone(), mesh));
    let mut registry = ResourceRegistry::builtin();
    registry
        .register(Arc::new(TrainerResource::new(
            artifacts.clone(),
            DecoderDevice::ndarray(),
        )))
        .expect("register trainer resource");
    let mut config = HostConfig::supervised(space.clone());
    config.registry = registry;
    config.clock = Arc::new(ManualClock::at(1_000));
    let host = MeshHost::new(synced, author.clone(), config);
    let mut distillery = Distillery::new(host, space.clone(), RetentionSettings::default());

    // 3. Post the trainer job: one staged request, explicit refs and
    //    hyperparameters, an Observed determinism ask for a ProducerOnly
    //    resource.
    let request = TrainRequest {
        base_model_ref,
        tokenizer_ref,
        corpus_ref,
        adapter_name: adapter_name.into(),
        prompt_template: "{{ prompt }}".into(),
        metric: EvalMetric::RankingAt { limit: 3 },
        settings,
        created_at: 1_000,
    };
    let request_blob = space
        .put(&serde_json::to_vec(&request).unwrap())
        .await
        .expect("stage request");
    distillery
        .host()
        .synced()
        .author(
            &author,
            &MeshEvent::JobPostedV2 {
                spec: Box::new(JobSpec::simple(
                    ResourceId::parse(TRAINER_RESOURCE).unwrap(),
                    TRAINER_REQUEST_INPUT,
                    request_blob,
                    "receipt",
                    64 * 1024,
                    DeterminismClass::Observed,
                )),
                nonce: 1,
                at_ms: 1_000,
            },
        )
        .await
        .expect("post trainer job");

    // 4. Drive the host until the training job commits. Debug-profile
    //    training takes tens of seconds; the loop bounds it, not the tick.
    let mut completed = false;
    for _ in 0..1_200 {
        let steps = distillery.tick().await.expect("supervisor tick");
        if steps
            .iter()
            .any(|step| matches!(step, Step::Completed { .. }))
        {
            completed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(completed, "the trainer job must run to completion");

    // 5. The committed output is the compact receipt.
    let board = distillery
        .host()
        .synced()
        .board()
        .await
        .expect("fold board");
    let output = board
        .jobs()
        .find_map(|job| match &job.state {
            mesh::JobState::Committed { output, .. } => Some((**output).clone()),
            _ => None,
        })
        .expect("trainer job committed an output");
    let receipt_bytes = space
        .fetch(&output.blob)
        .await
        .expect("fetch committed receipt")
        .expect("committed receipt bytes present");
    let receipt: TrainReceipt = serde_json::from_slice(&receipt_bytes).expect("receipt JSON");

    // 6. The artifacts landed in the composed store under the receipt's refs.
    let mut store = artifacts.lock().await;
    let manifest = load_typed::<ModelAdapterManifest>(
        &mut *store,
        &mut NoFetcher,
        receipt.adapter_manifest_ref,
    )
    .await
    .expect("load adapter manifest")
    .expect("adapter manifest present");
    for blob in [receipt.adapter_blob, receipt.adapter_config_blob] {
        assert!(
            load_typed::<OpaqueBlob>(&mut *store, &mut NoFetcher, blob)
                .await
                .expect("load adapter blob")
                .is_some(),
            "adapter blob {blob} must be stored"
        );
    }
    let report = load_typed::<EvalReport>(&mut *store, &mut NoFetcher, receipt.eval_report_ref)
        .await
        .expect("load eval report")
        .expect("eval report present");
    drop(store);

    distillery.shutdown().await.expect("clean shutdown");
    TrainerRun {
        receipt,
        manifest,
        report,
        corpus_ref,
    }
}

/// The claims that hold whichever arm ran: a strict held-out improvement, a
/// manifest whose blob refs are the receipt's, and a report whose provenance
/// links back to that manifest.
fn assert_common_receipt(run: &TrainerRun) {
    let TrainerRun {
        receipt,
        manifest,
        report,
        corpus_ref,
    } = run;
    assert!(
        receipt.adapter.passed > receipt.baseline.passed,
        "the receipt must show a strict held-out improvement: {receipt:?}"
    );
    assert_eq!(receipt.baseline.total, EVAL_PREFIXES.len() as u64);
    assert_eq!(manifest.training_corpus_root, Some(*corpus_ref));
    assert_eq!(manifest.adapter_blob, receipt.adapter_blob);
    assert_eq!(manifest.adapter_config_blob, receipt.adapter_config_blob);
    assert_eq!(report.baseline, receipt.baseline);
    assert_eq!(report.adapter, receipt.adapter);
    assert!(
        report.adapter_beats_baseline().expect("comparable report"),
        "the stored report must show the strict improvement"
    );
    report
        .validate_for_adapter(receipt.adapter_manifest_ref, manifest)
        .expect("report provenance links match the adapter manifest");
}

// ── The receipts ────────────────────────────────────────────────────────────

/// Both arms, in sequence, in one test.
///
/// Sequence is not tidiness: each receipt binds a real transport, joins a real
/// mesh and drives a real supervisor, and two of those racing inside one
/// process is not a thing either receipt claims — a shared machine will make
/// the race visible sooner or later. Running them one after the other keeps
/// each receipt about what it says it is about.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn each_trainer_arm_publishes_artifacts_and_a_strict_improvement_receipt() {
    finite_difference_receipt().await;
    #[cfg(feature = "trainer-autodiff")]
    autodiff_receipt().await;
}

async fn finite_difference_receipt() {
    let run = run_trainer_job(
        MESH_FINITE_DIFFERENCE,
        "trainer-resource-receipt",
        TrainerSettings::FiniteDifference(LoraTrainerSettings {
            rank: 1,
            alpha: 8.0,
            target_module: "v_proj".into(),
            steps: 40,
            initial_step_length: 1.0,
            minimum_step_length: 1.0e-4,
            epsilon: 0.02,
        }),
    )
    .await;
    assert_common_receipt(&run);
    assert_eq!(run.manifest.adapter_format_version, "peft-esp-trainer-v0");
    assert_eq!(
        run.manifest.training_method["trainer"],
        serde_json::json!(TRAINER_FINITE_DIFFERENCE)
    );
    assert_eq!(run.manifest.target_modules, vec!["v_proj".to_string()]);
    // The v0 manifest is what it was before the arms existed: the settings
    // sit under `training_method.settings` bare, not wrapped in their tag.
    // Wrapping them would be invisible from the trainer's side and fatal to a
    // receipt reader. (The `f32` fields widen to JSON doubles here exactly as
    // they always did — `0.02f32` prints as `0.019999999552965164` — so the
    // guard is the key set and the exact integers, not the whole literal.)
    let published = &run.manifest.training_method["settings"];
    assert!(
        published.get(TRAINER_FINITE_DIFFERENCE).is_none()
            && published.get(TRAINER_AUTODIFF).is_none(),
        "the published settings must be bare, not tagged: {published}"
    );
    let keys: Vec<&str> = published
        .as_object()
        .expect("settings is an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        vec![
            "alpha",
            "epsilon",
            "initial_step_length",
            "minimum_step_length",
            "rank",
            "steps",
            "target_module",
        ]
    );
    assert_eq!(published["rank"], serde_json::json!(1));
    assert_eq!(published["steps"], serde_json::json!(40));
    assert_eq!(published["target_module"], serde_json::json!("v_proj"));
    println!(
        "trainer resource receipt ({TRAINER_FINITE_DIFFERENCE}): baseline {}/{} vs adapter {}/{} at RankingAt{{3}}",
        run.receipt.baseline.passed,
        run.receipt.baseline.total,
        run.receipt.adapter.passed,
        run.receipt.adapter.total,
    );
}

/// The same job, the same resource, the autodiff arm.
///
/// The manifest assertions are the point: the published adapter is stamped
/// `peft-esp-trainer-v1`, which is what makes it loadable by the unchanged
/// loader *and* unmixable with a v0 adapter in a FLoRA round.
#[cfg(feature = "trainer-autodiff")]
async fn autodiff_receipt() {
    use distillery::AutodiffLoraSettings;

    let run = run_trainer_job(
        MESH_AUTODIFF,
        "trainer-resource-receipt-autodiff",
        TrainerSettings::Autodiff(AutodiffLoraSettings {
            rank: 1,
            alpha: 8.0,
            target_modules: vec!["v_proj".into()],
            steps: 12,
            learning_rate: 0.2,
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1.0e-8,
            weight_decay: 0.0,
        }),
    )
    .await;
    assert_common_receipt(&run);
    assert_eq!(
        run.manifest.adapter_format_version,
        distillery::TRAINED_ADAPTER_FORMAT_VERSION_AUTODIFF
    );
    assert_eq!(run.manifest.adapter_format_version, "peft-esp-trainer-v1");
    assert_eq!(
        run.manifest.training_method["trainer"],
        serde_json::json!(TRAINER_AUTODIFF)
    );
    assert_eq!(
        run.manifest.training_method["settings"]["learning_rate"],
        serde_json::json!(0.2)
    );
    assert_eq!(run.manifest.target_modules, vec!["v_proj".to_string()]);
    println!(
        "trainer resource receipt ({TRAINER_AUTODIFF}): baseline {}/{} vs adapter {}/{} at RankingAt{{3}}",
        run.receipt.baseline.passed,
        run.receipt.baseline.total,
        run.receipt.adapter.passed,
        run.receipt.adapter.total,
    );
}

// ── The request shape, in every build ───────────────────────────────────────

/// The settings object a v1 request carries, as bytes on the wire.
fn autodiff_settings_json() -> serde_json::Value {
    serde_json::json!({
        "rank": 1,
        "alpha": 8.0,
        "target_modules": ["v_proj"],
        "steps": 12,
        "learning_rate": 0.2,
        "beta1": 0.9,
        "beta2": 0.999,
        "epsilon": 1.0e-8,
        "weight_decay": 0.0,
    })
}

fn autodiff_request_json() -> serde_json::Value {
    serde_json::json!({
        "base_model_ref": ManifestId::of_blob(b"base").to_string(),
        "tokenizer_ref": ManifestId::of_blob(b"tokenizer").to_string(),
        "corpus_ref": ManifestId::of_blob(b"corpus").to_string(),
        "adapter_name": "round-trip",
        "prompt_template": "{{ prompt }}",
        "metric": { "RankingAt": { "limit": 3 } },
        "settings": { TRAINER_AUTODIFF: autodiff_settings_json() },
        "created_at": 1_000,
    })
}

/// A request carrying the autodiff tag is read as an autodiff request in
/// every build, and survives a serde round trip unchanged.
///
/// This is the whole reason the arm exists unconditionally. A build that
/// dropped the arm would fail to parse the request and report it as malformed
/// JSON, which reads as the poster's fault; a build that fell back to v0 would
/// train the wrong thing and stamp it with the wrong version. Neither is
/// acceptable, so every build reads the tag.
#[test]
fn an_autodiff_request_round_trips_through_serde() {
    let bytes = serde_json::to_vec(&autodiff_request_json()).unwrap();
    let request: TrainRequest = serde_json::from_slice(&bytes).expect("autodiff request parses");
    assert_eq!(request.settings.trainer(), TRAINER_AUTODIFF);
    assert_eq!(
        request.settings.adapter_format_version(),
        "peft-esp-trainer-v1"
    );
    // The settings the manifest publishes are compared field by field, not
    // against the authored literal. A build with `trainer-autodiff` parses
    // them into esp's `AutodiffLoraSettings`, whose betas and epsilon are
    // `f32`, and re-emits them as JSON doubles: `0.9f32` comes back as
    // `0.8999999761581421`. That is the trainer's real precision surfacing,
    // and it is why a receipt's settings are compared as numbers rather than
    // as bytes. The request itself still round-trips exactly, which is what
    // the job depends on.
    let published = request.settings.settings_value();
    for (field, expected) in [
        ("rank", serde_json::json!(1)),
        ("steps", serde_json::json!(12)),
        ("target_modules", serde_json::json!(["v_proj"])),
        ("learning_rate", serde_json::json!(0.2)),
    ] {
        assert_eq!(published[field], expected, "settings.{field}");
    }
    for (field, expected) in [
        ("alpha", 8.0f64),
        ("beta1", 0.9),
        ("beta2", 0.999),
        ("epsilon", 1.0e-8),
        ("weight_decay", 0.0),
    ] {
        let actual = published[field]
            .as_f64()
            .unwrap_or_else(|| panic!("settings.{field} is a number"));
        assert!(
            (actual - expected).abs() <= expected.abs() * 1.0e-6 + f64::EPSILON,
            "settings.{field}: {actual} is not {expected} to f32 precision"
        );
    }

    let reserialized: TrainRequest =
        serde_json::from_slice(&serde_json::to_vec(&request).unwrap()).unwrap();
    assert_eq!(reserialized, request);

    // The tag is the trainer name the receipt publishes, not a separate
    // vocabulary a reader would have to map.
    let value = serde_json::to_value(&request).unwrap();
    assert!(
        value["settings"].get(TRAINER_AUTODIFF).is_some(),
        "the external tag must be the trainer name: {}",
        value["settings"]
    );
}

/// A v0 request keeps its own tag, and a build reads the two apart.
#[test]
fn the_two_arms_are_told_apart_by_their_tags() {
    let v0 = TrainerSettings::FiniteDifference(LoraTrainerSettings {
        rank: 1,
        alpha: 8.0,
        target_module: "v_proj".into(),
        steps: 40,
        initial_step_length: 1.0,
        minimum_step_length: 1.0e-4,
        epsilon: 0.02,
    });
    assert_eq!(v0.trainer(), TRAINER_FINITE_DIFFERENCE);
    assert_eq!(v0.adapter_format_version(), "peft-esp-trainer-v0");
    v0.availability().expect("every build runs the v0 arm");
    let shape = v0.adapter_shape();
    assert_eq!(shape.rank, 1);
    assert_eq!(shape.target_modules, vec!["v_proj".to_string()]);

    let value = serde_json::to_value(&v0).unwrap();
    assert!(
        value.get(TRAINER_FINITE_DIFFERENCE).is_some(),
        "the v0 external tag must be its trainer name: {value}"
    );
    assert!(value.get(TRAINER_AUTODIFF).is_none());
}

/// A build without `trainer-autodiff` reads the request correctly and then
/// refuses it, naming the feature it would need.
#[cfg(not(feature = "trainer-autodiff"))]
#[test]
fn an_autodiff_request_is_refused_by_name_without_the_feature() {
    let bytes = serde_json::to_vec(&autodiff_request_json()).unwrap();
    let request: TrainRequest = serde_json::from_slice(&bytes).expect("autodiff request parses");
    assert_eq!(request.settings.trainer(), TRAINER_AUTODIFF);
    let error = request
        .settings
        .availability()
        .expect_err("a build without the feature must refuse the autodiff arm");
    assert!(
        error.contains("trainer-autodiff"),
        "{error} must name the missing feature"
    );
    assert!(
        error.contains(TRAINER_AUTODIFF),
        "{error} must name the arm it refuses"
    );
    // Refusing to *run* the arm is not refusing to understand it: the
    // settings are fully typed here, so this build can still size, check and
    // forward a v1 adapter it merely received.
    let shape = request.settings.adapter_shape();
    assert_eq!(shape.rank, 1);
    assert_eq!(shape.target_modules, vec!["v_proj".to_string()]);
}
