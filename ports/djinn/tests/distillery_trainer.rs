// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The receipt for the Djinn-composed trainer: a resident opened from nothing
//! but owner-stated policy trains a real LoRA adapter and leaves it in the
//! persona's own model library.
//!
//! `distillery/tests/trainer.rs` already proves the *resource* works, over a
//! hand-built one-host mesh and a `MemoryBackend`. This file proves the
//! *composition*: the same resource reached by the path a desktop actually
//! takes — [`DjinnResident::open`], the profile's derived mesh author, the
//! device's stated lending posture, a `SystemClock`, and a concrete
//! `RedbBackend` on disk under the persona's data root.
//!
//! Three things are load-bearing here that the resource's own receipt cannot
//! say:
//!
//! 1. **The posture permits the trainer by name.** `allowed_resources` is
//!    exactly `["esp.train.peft-lora/v1"]`, asserted against the *installed*
//!    policy after open, so this run is admitted because the owner said so and
//!    not because the set was empty.
//! 2. **The artifacts land somewhere durable and persona-scoped.** The library
//!    file is asserted to exist at the derived
//!    `<data_root>/models/<profile>/library.redb` — not in the personal-graph
//!    store, and not under a mesh root whose retention policy could collect it.
//! 3. **The gate is honest in both directions.** This file only compiles with
//!    the `trainer` feature; `distillery_lane.rs` carries the matching
//!    `cfg(not(feature = "trainer"))` refusal, so neither feature set is
//!    without a live assertion about the gate.
//!
//! Windows-only for the same reason the lane receipt is: the lane reads
//! `HostFacts::memory_mib` from the operating system and refuses to advertise
//! a capacity it never measured, and only the Windows half of this crate can
//! measure one today.

#![cfg(all(feature = "trainer", windows))]

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use common::{
    CONFIG_JSON, EVAL_PREFIXES, MODEL_ID, TRAIN_PREFIXES, base_weights, lane, lending, now_ms,
    open_vault, owner, profile, save_cases, tokenizer_json, unlock, write_device_settings,
};
use distillery::{
    LoraTrainerSettings, ResidentReceipt, TRAINER_REQUEST_INPUT, TRAINER_RESOURCE, TrainReceipt,
    TrainRequest,
};
use djinn::resident::DjinnResident;
use djinn::settings::TrainerLaneSettings;
use eidetic::models::{EvalMetric, EvalReport, OpaqueBlob, TrainingCorpus};
use eidetic::typed::{load_typed, save_typed};
use eidetic::{
    ModelAdapterManifest, ModelLibrary, NoFetcher, PrivacyClass, ProvenanceRecord, Timestamp,
    TrustEnvelope,
};
use mesh::spec::{DeterminismClass, JobSpec};
use mesh::{BlobSource as _, ResourceId};
use mesh_host::Step;
use tokio::sync::Notify;

/// Training on the CPU under a `line-tables-only` debug profile takes tens of
/// seconds. This bound is a *failure* bound — the run is stopped by the
/// observer seeing `Step::Completed`, and the assertions below distinguish the
/// two endings — so it is set well past any honest run rather than tuned close.
const FAILURE_BOUND: Duration = Duration::from_secs(300);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_composed_trainer_trains_a_real_adapter_into_the_personas_model_library() {
    let directory = tempfile::tempdir().expect("temporary resident root");
    let data_root = directory.path().join("data");
    let (vault_dir, identity) = open_vault(directory.path());

    // The posture explicitly permits the trainer, and nothing else. An empty
    // `allowed_resources` would admit this job by permitting everything, which
    // would prove nothing about the owner having agreed to train.
    let mut posture = lending();
    posture.allowed_resources = vec![TRAINER_RESOURCE.into()];
    posture.accepted_checkpoints = vec!["restart".into()];
    assert_eq!(
        posture.max_thermal_c, 0,
        "the thermal rule stays disabled: this build has no thermometer"
    );
    assert_eq!(posture.min_idle_ms, 1);
    assert_eq!(
        posture.stated.idle_ms,
        Some(600_000),
        "a machine driving a test run is rarely idle, so the owner's stated \
         fallback is what the idle rule actually rests on here"
    );
    write_device_settings(&data_root, Some(posture));

    let mut trains = lane();
    trains.trainer = Some(TrainerLaneSettings {
        device: "cpu".into(),
        // The derived persona-scoped path is the thing under test.
        library_root: None,
    });

    let mut resident = DjinnResident::open(
        &identity,
        &data_root,
        &profile(),
        owner(Some(trains)),
        &vault_dir,
        unlock(),
    )
    .await
    .expect("compose the Djinn resident with a trainer-carrying Distillery lane");
    assert!(resident.distillery_enabled());
    let mut works = resident
        .take_distillery()
        .expect("the lane was composed and can be driven");

    // 1. The owner's posture reached the supervisor, and it names the trainer.
    let installed = works.authority().host().policy().clone();
    assert_eq!(
        installed.allowed_resources,
        BTreeSet::from([ResourceId::parse(TRAINER_RESOURCE).unwrap()]),
        "the trainer runs because the device's stated posture permits it by \
         name, not because the allowed set was empty"
    );
    assert_ne!(
        installed,
        mesh::DevicePolicy::permissive(),
        "a lane that fell back to permissive() would lend on nobody's authority"
    );

    let library = works
        .model_library()
        .expect("a lane that stated a trainer composed a model library");

    // 2. Author the base model triple and the corpus into that library —
    //    through the resident's own handle, because redb is single-writer and
    //    the trainer is about to write into the same file.
    let (base_model_ref, tokenizer_ref, corpus_ref) = {
        let mut store = library.lock().await;
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
            ProvenanceRecord::self_imported("djinn-trainer-receipt"),
            TrustEnvelope::self_asserted(),
            Timestamp(0),
        )
        .await
        .expect("save the base model into the persona's library");
        let manifest = ModelLibrary::load_model(&mut *store, base_model_ref)
            .await
            .expect("load model manifest")
            .expect("model manifest present");
        let corpus = TrainingCorpus {
            training_source_engrams: save_cases(&mut *store, &TRAIN_PREFIXES).await,
            evaluation_source_engrams: save_cases(&mut *store, &EVAL_PREFIXES).await,
        };
        let corpus_ref = save_typed(
            &mut *store,
            &corpus,
            vec![],
            PrivacyClass::LocalOnly,
            ProvenanceRecord::self_imported("djinn-trainer-receipt"),
            TrustEnvelope::self_asserted(),
            Timestamp(1),
        )
        .await
        .expect("save the corpus");
        (base_model_ref, manifest.tokenizer_blob, corpus_ref)
    };

    // 3. Stage the request and post the job as this device. Every ref and
    //    hyperparameter is explicit; nothing is inferred from job facts.
    let request = TrainRequest {
        base_model_ref,
        tokenizer_ref,
        corpus_ref,
        adapter_name: "djinn-trainer-receipt".into(),
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
        created_at: now_ms(),
    };
    let request_blob = works
        .space()
        .put(&serde_json::to_vec(&request).unwrap())
        .await
        .expect("stage the training request");
    works
        .post_job(
            JobSpec::simple(
                ResourceId::parse(TRAINER_RESOURCE).unwrap(),
                TRAINER_REQUEST_INPUT,
                request_blob,
                "receipt",
                64 * 1024,
                // The trainer declares `VerificationClass::ProducerOnly`, so
                // asking for anything stricter than `Observed` would be asking
                // for a claim nobody can make.
                DeterminismClass::Observed,
            ),
            1,
            now_ms(),
        )
        .await
        .expect("post the trainer job as this device");

    // 4. Drive the resident until the job commits.
    let stop = Arc::new(Notify::new());
    let signal = Arc::clone(&stop);
    let mut observed: Vec<Step> = Vec::new();
    let mut completed = false;
    let started = std::time::Instant::now();
    works
        .run_until(
            async move {
                tokio::select! {
                    _ = signal.notified() => {}
                    _ = tokio::time::sleep(FAILURE_BOUND) => {}
                }
            },
            |receipt| {
                if let ResidentReceipt::Tick { steps } = &receipt {
                    for step in steps {
                        if !matches!(step, Step::Idle) {
                            observed.push(step.clone());
                        }
                    }
                    if steps
                        .iter()
                        .any(|step| matches!(step, Step::Completed { .. }))
                    {
                        completed = true;
                        stop.notify_one();
                    }
                }
            },
        )
        .await
        .expect("the works ran to the stop request");
    let elapsed = started.elapsed();

    println!("observed step sequence: {observed:?}");
    assert!(
        completed,
        "the composed trainer never completed the job in {FAILURE_BOUND:?}; \
         steps were {observed:?}"
    );
    assert!(
        observed
            .iter()
            .any(|step| matches!(step, Step::Claimed { .. })),
        "a completion with no claim would mean the board was not really driven: {observed:?}"
    );

    // 5. The committed output is the compact receipt.
    let board = works
        .authority()
        .host()
        .synced()
        .board()
        .await
        .expect("fold the board");
    let output = board
        .jobs()
        .find_map(|job| match &job.state {
            mesh::JobState::Committed { output, .. } => Some((**output).clone()),
            _ => None,
        })
        .expect("the trainer job committed an output");
    let receipt_bytes = works
        .space()
        .fetch(&output.blob)
        .await
        .expect("fetch the committed receipt")
        .expect("committed receipt bytes present");
    let receipt: TrainReceipt = serde_json::from_slice(&receipt_bytes).expect("receipt JSON");
    assert!(
        receipt.adapter.passed > receipt.baseline.passed,
        "the receipt must show a strict held-out improvement: {receipt:?}"
    );
    assert_eq!(
        receipt.baseline.total,
        EVAL_PREFIXES.len() as u64,
        "the tally must be over the held-out partition, whole"
    );

    // 6. The artifacts are in the persona's library, under the receipt's refs.
    {
        let mut store = library.lock().await;
        let manifest = load_typed::<ModelAdapterManifest>(
            &mut *store,
            &mut NoFetcher,
            receipt.adapter_manifest_ref,
        )
        .await
        .expect("load the adapter manifest")
        .expect("adapter manifest present");
        assert_eq!(
            manifest.training_corpus_root,
            Some(corpus_ref),
            "the published adapter must name the corpus it was actually trained on"
        );
        assert_eq!(manifest.adapter_blob, receipt.adapter_blob);
        assert_eq!(manifest.adapter_config_blob, receipt.adapter_config_blob);
        for blob in [receipt.adapter_blob, receipt.adapter_config_blob] {
            assert!(
                load_typed::<OpaqueBlob>(&mut *store, &mut NoFetcher, blob)
                    .await
                    .expect("load the adapter blob")
                    .is_some(),
                "adapter blob {blob} must be stored"
            );
        }
        let report = load_typed::<EvalReport>(&mut *store, &mut NoFetcher, receipt.eval_report_ref)
            .await
            .expect("load the evaluation report")
            .expect("evaluation report present");
        assert_eq!(report.baseline, receipt.baseline);
        assert_eq!(report.adapter, receipt.adapter);
        assert!(
            report.adapter_beats_baseline().expect("comparable report"),
            "the stored report must show the strict improvement"
        );
        report
            .validate_for_adapter(receipt.adapter_manifest_ref, &manifest)
            .expect("report provenance links match the adapter manifest");
    }

    // 7. And the library is a real file, at the derived persona-scoped path.
    //    Written out longhand rather than by calling the derivation, so this
    //    asserts the location rather than agreeing with it.
    let derived = data_root.join("models").join("works").join("library.redb");
    assert!(
        derived.is_file(),
        "the model library must exist at {}",
        derived.display()
    );
    assert!(
        !derived.starts_with(works.mesh_root()),
        "a trained adapter must outlive any one mesh: {} is under {}",
        derived.display(),
        works.mesh_root().display()
    );

    println!(
        "djinn trainer receipt: baseline {}/{} vs adapter {}/{} at RankingAt{{3}}, \
         trained in {:.1}s into {}",
        receipt.baseline.passed,
        receipt.baseline.total,
        receipt.adapter.passed,
        receipt.adapter.total,
        elapsed.as_secs_f64(),
        derived.display(),
    );

    resident.restore_distillery(Some(works));
    resident
        .shutdown()
        .await
        .expect("clean ordered resident shutdown");
}

/// A GPU trainer is not composable while this lane reports `HostFacts.gpu` as
/// false, and the refusal names the value rather than falling back to the CPU
/// the owner did not ask for.
#[test]
fn a_gpu_trainer_is_refused_by_name_before_anything_is_opened() {
    let mut asks_for_a_gpu = lane();
    asks_for_a_gpu.trainer = Some(TrainerLaneSettings {
        device: "gpu".into(),
        library_root: None,
    });
    let refusal = asks_for_a_gpu
        .validate()
        .expect_err("a GPU trainer is not composable on a device that reports no GPU");
    assert!(refusal.contains("distillery.trainer.device"), "{refusal}");
    assert!(
        refusal.contains("\"gpu\""),
        "the refusal must name the value it refused: {refusal}"
    );
    assert!(
        refusal.contains("HostFacts.gpu"),
        "the refusal must say why, not just that: {refusal}"
    );
}
