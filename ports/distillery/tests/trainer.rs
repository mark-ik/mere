// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The trainer resource receipt: one real mesh job runs the v0 deterministic
//! LoRA trainer end to end.
//!
//! The poster stages a `TrainRequest` in the transport blob space and posts
//! an `esp.train.peft-lora/v1` job; Distillery drives the host to completion;
//! the committed output is the compact `TrainReceipt`; and the adapter
//! blobs, adapter manifest, and evaluation report land in the composed
//! Eidetic store under exactly the refs the receipt names, with the adapter
//! strictly beating the unchanged baseline on the corpus's held-out cases.

#![cfg(feature = "trainer")]

use std::sync::Arc;
use std::time::Duration;

use distillery::{
    Distillery, RetentionSettings, TRAINER_REQUEST_INPUT, TRAINER_RESOURCE, TrainReceipt,
    TrainRequest, TrainerResource,
};
use eidetic::models::{EvalMetric, EvalReport, OpaqueBlob, TrainingCorpus};
use eidetic::typed::{load_typed, save_typed};
use eidetic::{
    ManifestId, MemoryBackend, ModelAdapterManifest, ModelLibrary, NoFetcher, PrivacyClass,
    ProvenanceRecord, Timestamp, TrustEnvelope,
};
use esp::infer::decoder::{DecoderDevice, LoraTrainerSettings, TrainingCase};
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

const MESH: [u8; 32] = [0xd7; 32];
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
            .expect("save case engram"),
        );
    }
    ids.sort_by_key(ToString::to_string);
    ids
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_trainer_job_publishes_artifacts_and_a_strict_improvement_receipt() {
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
            training_source_engrams: save_cases(&mut store, &TRAIN_PREFIXES).await,
            evaluation_source_engrams: save_cases(&mut store, &EVAL_PREFIXES).await,
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
    let provider = InMemoryProvider::from_seed([0xd7; 32]);
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
        MESH,
    )
    .await
    .expect("join mesh");

    let space = Arc::new(TransportBlobSpace::for_mesh(blobs.clone(), MESH));
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
        adapter_name: "trainer-resource-receipt".into(),
        prompt_template: "{{ prompt }}".into(),
        metric: EvalMetric::RankingAt { limit: 3 },
        settings: LoraTrainerSettings {
            rank: 1,
            alpha: 8.0,
            target_module: "v_proj".into(),
            steps: 40,
            initial_step_length: 1.0,
            minimum_step_length: 1.0e-4,
            epsilon: 0.02,
        },
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
    assert!(
        receipt.adapter.passed > receipt.baseline.passed,
        "the receipt must show a strict held-out improvement: {receipt:?}"
    );
    assert_eq!(receipt.baseline.total, EVAL_PREFIXES.len() as u64);

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
    assert_eq!(manifest.training_corpus_root, Some(corpus_ref));
    assert_eq!(manifest.adapter_blob, receipt.adapter_blob);
    assert_eq!(manifest.adapter_config_blob, receipt.adapter_config_blob);
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
    assert_eq!(report.baseline, receipt.baseline);
    assert_eq!(report.adapter, receipt.adapter);
    assert!(
        report.adapter_beats_baseline().expect("comparable report"),
        "the stored report must show the strict improvement"
    );
    report
        .validate_for_adapter(receipt.adapter_manifest_ref, &manifest)
        .expect("report provenance links match the adapter manifest");
    drop(store);

    distillery.shutdown().await.expect("clean shutdown");
    println!(
        "trainer resource receipt: baseline {}/{} vs adapter {}/{} at RankingAt{{3}}",
        receipt.baseline.passed,
        receipt.baseline.total,
        receipt.adapter.passed,
        receipt.adapter.total,
    );
}
