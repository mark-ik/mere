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
//!    without a live assertion about the gate. The same pattern one word in:
//!    the refusal below runs under `cfg(not(feature = "trainer-gpu"))`, and
//!    `distillery_trainer_gpu.rs` is its composing half.
//!
//! 4. **Both arms, or the refusal.** A build carrying `trainer-autodiff` runs
//!    the v1 request on the same lane, right after the v0 one, and its
//!    published manifest is checked for the v1 stamp and trainer name. A build
//!    without it posts the identical request and is checked to *release* it
//!    and commit nothing — refused loudly rather than composed diminished,
//!    the same posture the GPU gate takes about the device.
//!
//! The run itself stays on the CPU under both feature sets. `device: "cpu"` is
//! not incidental here — it is what makes this receipt a statement about the
//! composed *path* rather than about whatever hardware the machine has, and it
//! is what lets the GPU receipt's tallies be compared against something.
//!
//! It runs wherever the lane composes, for the same reason the lane receipt
//! does: the one fact `HostFacts` will not invent is physical memory, and this
//! crate reads that on Windows, Linux and macOS. The device conditions it
//! leans on off Windows are the fixture's stated fallbacks — see
//! `common::lending`, whose docs carry the pairing that keeps them honest.
//! Nothing else here is platform-shaped: the model library path is derived
//! with `Path::join` under a `tempfile` root, and the training is `ndarray` on
//! the CPU.

#![cfg(feature = "trainer")]

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use common::{
    EVAL_PREFIXES, PostedJob, author_fixture, autodiff_settings, finite_difference_settings, lane,
    lending, now_ms, open_vault, owner, profile, run_trainer_job, unlock, write_device_settings,
};
use distillery::{
    TRAINER_AUTODIFF, TRAINER_FINITE_DIFFERENCE, TRAINER_RESOURCE, TrainReceipt, TrainRequest,
};
use djinn::resident::DjinnResident;
use djinn::settings::TrainerLaneSettings;
use eidetic::models::{EvalMetric, EvalReport, OpaqueBlob};
use eidetic::typed::load_typed;
use eidetic::{ModelAdapterManifest, NoFetcher};
use mesh::ResourceId;
use mesh_host::Step;

/// Training on the CPU under a `line-tables-only` debug profile takes tens of
/// seconds. This bound is a *failure* bound — the run is stopped by the
/// observer seeing `Step::Completed`, and the assertions below distinguish the
/// two endings — so it is set well past any honest run rather than tuned close.
const FAILURE_BOUND: Duration = Duration::from_secs(300);

/// Both arms the build carries, through one composed lane, in sequence.
///
/// Sequence rather than two tests, and one resident rather than two: a resident
/// owns a redb file, a transport and a mesh author, and two of those racing in
/// one process is not a thing this receipt claims. Running the arms one after
/// the other on the same lane also makes the comparison sharper — the library,
/// the posture and the fixture are literally the same objects, so what differs
/// between the two published adapters is the trainer and nothing else.
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

    // 2. Author the base model triple and the corpus into that library.
    let (base_model_ref, tokenizer_ref, corpus_ref) = {
        let mut store = library.lock().await;
        author_fixture(&mut store, "djinn-trainer-receipt").await
    };

    let request = |adapter_name: &str, settings: distillery::TrainerSettings| TrainRequest {
        base_model_ref,
        tokenizer_ref,
        corpus_ref,
        adapter_name: adapter_name.into(),
        prompt_template: "{{ prompt }}".into(),
        metric: EvalMetric::RankingAt { limit: 3 },
        settings,
        created_at: now_ms(),
    };
    let mut observed: Vec<Step> = Vec::new();

    // 3. The v0 arm, end to end through the composed path.
    let v0 = run_trainer_job(
        &mut works,
        &request("djinn-trainer-receipt", finite_difference_settings()),
        1,
        FAILURE_BOUND,
        &mut observed,
    )
    .await;
    let PostedJob::Committed { receipt, elapsed } = v0 else {
        panic!("the v0 arm must commit on every build that carries the trainer at all");
    };
    assert!(
        observed
            .iter()
            .any(|step| matches!(step, Step::Claimed { .. })),
        "a completion with no claim would mean the board was not really driven: {observed:?}"
    );
    let published = assert_published_adapter(
        &library,
        &receipt,
        corpus_ref,
        "peft-esp-trainer-v0",
        TRAINER_FINITE_DIFFERENCE,
    )
    .await;
    assert_eq!(published, vec!["v_proj".to_string()]);
    println!(
        "djinn trainer receipt ({TRAINER_FINITE_DIFFERENCE}): baseline {}/{} vs adapter {}/{} \
         at RankingAt{{3}}, trained in {:.1}s",
        receipt.baseline.passed,
        receipt.baseline.total,
        receipt.adapter.passed,
        receipt.adapter.total,
        elapsed.as_secs_f64(),
    );

    // 4. The v1 arm, on a build that carries it — or its refusal, on a build
    //    that does not. Both halves post the *same* request; only the build
    //    differs, which is what makes the pair a statement about the gate.
    #[cfg(feature = "trainer-autodiff")]
    {
        let posted = run_trainer_job(
            &mut works,
            &request("djinn-trainer-receipt-autodiff", autodiff_settings()),
            2,
            FAILURE_BOUND,
            &mut observed,
        )
        .await;
        let PostedJob::Committed { receipt, elapsed } = posted else {
            panic!("a build carrying `trainer-autodiff` must run the autodiff arm, not release it");
        };
        let published = assert_published_adapter(
            &library,
            &receipt,
            corpus_ref,
            distillery::TRAINED_ADAPTER_FORMAT_VERSION_AUTODIFF,
            TRAINER_AUTODIFF,
        )
        .await;
        assert_eq!(published, vec!["v_proj".to_string()]);
        println!(
            "djinn trainer receipt ({TRAINER_AUTODIFF}): baseline {}/{} vs adapter {}/{} \
             at RankingAt{{3}}, trained in {:.1}s",
            receipt.baseline.passed,
            receipt.baseline.total,
            receipt.adapter.passed,
            receipt.adapter.total,
            elapsed.as_secs_f64(),
        );
    }
    #[cfg(not(feature = "trainer-autodiff"))]
    {
        let asked = request("djinn-trainer-receipt-autodiff", autodiff_settings());
        // The refusal's wording, asserted where it is produced. The mesh board
        // carries Posted / Claimed / Done / Committed and lease release
        // reasons — no failure text — so this is the only place the message
        // exists, and it is the same string the resource returned to the run
        // the lane just released.
        let refusal = asked
            .settings
            .availability()
            .expect_err("a build without `trainer-autodiff` must refuse the autodiff arm");
        assert!(
            refusal.contains("trainer-autodiff"),
            "the refusal must name the feature that is missing: {refusal}"
        );
        assert!(
            refusal.contains(TRAINER_AUTODIFF),
            "the refusal must name the arm it refuses: {refusal}"
        );

        // And the behaviour, through the lane: released, never committed.
        let before = committed_jobs(&works).await;
        let posted = run_trainer_job(&mut works, &asked, 2, FAILURE_BOUND, &mut observed).await;
        assert!(
            matches!(posted, PostedJob::Released),
            "a build without `trainer-autodiff` must release the job rather than \
             committing an adapter trained with the arm nobody asked for"
        );
        assert_eq!(
            committed_jobs(&works).await,
            before,
            "a refused run must publish nothing: quietly training the v0 arm and stamping \
             it v0 would look exactly like success to an owner who asked for v1"
        );
        println!("djinn trainer receipt: the lane refused {TRAINER_AUTODIFF} — {refusal}");
    }

    // 5. And the library is a real file, at the derived persona-scoped path.
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

    resident.restore_distillery(Some(works));
    resident
        .shutdown()
        .await
        .expect("clean ordered resident shutdown");
}

/// Everything a published adapter owes its receipt, checked in the persona's
/// own library. Returns the manifest's target modules so the caller can assert
/// on the arm's own shape.
async fn assert_published_adapter(
    library: &Arc<tokio::sync::Mutex<muniment::RedbBackend>>,
    receipt: &TrainReceipt,
    corpus_ref: eidetic::ManifestId,
    adapter_format_version: &str,
    trainer: &str,
) -> Vec<String> {
    assert!(
        receipt.adapter.passed > receipt.baseline.passed,
        "the receipt must show a strict held-out improvement: {receipt:?}"
    );
    assert_eq!(
        receipt.baseline.total,
        EVAL_PREFIXES.len() as u64,
        "the tally must be over the held-out partition, whole"
    );

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
    // The two strings the arm is answerable for. The format version is what
    // esp's loader checks the adapter's own `peft_version` against, and the
    // trainer name is what a FLoRA round and a reproduction attempt read.
    assert_eq!(
        manifest.adapter_format_version, adapter_format_version,
        "the published adapter must carry the arm's own version stamp"
    );
    assert_eq!(
        manifest.training_method["trainer"],
        serde_json::json!(trainer),
        "the receipt must record which trainer actually ran"
    );
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
    manifest.target_modules
}

/// How many jobs on this lane's board have committed an output. Used only to
/// prove that a refused run added none.
#[cfg(not(feature = "trainer-autodiff"))]
async fn committed_jobs(works: &djinn::resident_distillery::ResidentDistillery) -> usize {
    works
        .authority()
        .host()
        .synced()
        .board()
        .await
        .expect("fold the board")
        .jobs()
        .filter(|job| matches!(job.state, mesh::JobState::Committed { .. }))
        .count()
}

/// The GPU gate's refusing half: this build carries the trainer but not the
/// GPU trainer, so a lane that says `device: "gpu"` is refused by name rather
/// than trained on the CPU.
///
/// The failure this exists against is the one that would look like success. A
/// composition that fell back to `ndarray` would produce this file's other
/// receipt exactly — real adapter, real tallies, real artifacts — while the
/// owner read their own settings file as proof the device trains on its GPU.
/// The refusal is checked to name the feature, because "not supported" without
/// the way out is a wall.
///
/// It is raised before any host fact is read, so it needs no GPU, and it
/// disappears under `trainer-gpu`, where `distillery_trainer_gpu.rs` asserts
/// the composing half. Neither feature set is left without a live assertion
/// about this gate.
#[cfg(not(feature = "trainer-gpu"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_build_with_no_gpu_trainer_refuses_a_lane_that_states_one() {
    let directory = tempfile::tempdir().expect("temporary resident root");
    let data_root = directory.path().join("data");
    let (vault_dir, identity) = open_vault(directory.path());
    // A perfectly good lending posture: the refusal under test must be about
    // the missing GPU trainer, not about anything else being wrong.
    write_device_settings(&data_root, Some(lending()));

    let mut asks_for_a_gpu = lane();
    asks_for_a_gpu.trainer = Some(TrainerLaneSettings {
        device: "gpu".into(),
        library_root: None,
    });
    // The settings themselves are valid — `"gpu"` is inside the closed
    // vocabulary — and only this *build* cannot honour them. That split is
    // deliberate: the same settings file is valid on the machine that can run
    // it, and a validator that refused here would make the file's meaning
    // depend on what happened to be plugged in.
    assert_eq!(asks_for_a_gpu.validate(), Ok(()));

    let refusal = common::refusal_from(
        DjinnResident::open(
            &identity,
            &data_root,
            &profile(),
            owner(Some(asks_for_a_gpu)),
            &vault_dir,
            unlock(),
        )
        .await,
        "a build with no GPU trainer must refuse a lane that states one rather than \
         composing a CPU trainer the owner did not ask for",
    )
    .await;

    assert!(refusal.contains("distillery.trainer.device"), "{refusal}");
    assert!(
        refusal.contains("\"gpu\""),
        "the refusal must name the value it refused: {refusal}"
    );
    assert!(
        refusal.contains("`trainer-gpu` feature"),
        "the refusal must name the way out: {refusal}"
    );
}
