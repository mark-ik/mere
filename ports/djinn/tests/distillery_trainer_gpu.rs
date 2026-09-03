// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The receipt for the GPU-composed trainer: the same lane, the same synthetic
//! fixture, the same forcing question — what does `device: "gpu"` do on *this*
//! machine, with the adapter named rather than assumed.
//!
//! `distillery_trainer.rs` already proves the composition on the CPU. This file
//! changes exactly one field, `distillery.trainer.device` from `"cpu"` to
//! `"gpu"`, and there are two honest answers to that, decided by hardware and
//! not by the test: a machine with a discrete adapter must train on it, and a
//! machine without one must be *refused*. The receipt asks
//! [`distillery::probe_gpu_adapter`] which machine it is on, prints the
//! answer, and then proves the corresponding claim. Neither branch is a skip;
//! a log reader can tell from the printed branch line which of the two this
//! run proved.
//!
//! ## The discrete branch
//!
//! Three things that word is supposed to mean:
//!
//! 1. **The adapter is the discrete GPU, and it was checked.**
//!    [`ResidentDistillery::trainer_adapter`] reports the adapter the wgpu
//!    runtime named *before* the device was constructed, and it is asserted to
//!    be `Discrete`, matched out of the discrete class rather than substituted.
//!    This is the load-bearing assertion of the whole file. cubecl's
//!    `select_from_adapter_list`, asked for `DiscreteGpu(0)` on a machine with
//!    no discrete adapter, falls through to the unclassified `Other` list and
//!    returns whatever is there; it panics only when that list is short too.
//!    A receipt that merely observed "the job completed on device gpu" would
//!    pass identically on a machine that had quietly run on an integrated
//!    adapter, which is the exact failure this lane was built to refuse.
//! 2. **The advertised fact followed the composition.** `HostFacts::gpu` is
//!    read back off the *installed* host, the same way the CPU receipt reads
//!    the installed policy, and asserted `true`. Composing a GPU trainer and
//!    advertising no GPU, or advertising a GPU with no GPU resource behind it,
//!    are both lies to the ring; this checks the one that can happen here.
//! 3. **Real training happened on it.** The same synthetic llama fixture, the
//!    same held-out partition, the same trainer job — driven through the
//!    resident on `SystemClock` to `Step::Completed`, and the committed
//!    `TrainReceipt` read back out of the blob space. Both trainer arms this
//!    build carries, in sequence on the one composed adapter: the lane hands
//!    the resource the *inner* wgpu device it probed, and the autodiff trainer
//!    wraps that with `Device::autodiff` itself, so the arm changes nothing
//!    about the composition and everything about what runs on it.
//!
//! The tally assertion is deliberately *strict improvement* and nothing
//! sharper. The CPU receipt's 6/6-against-4/6 numbers are ndarray's; f32
//! reductions on a GPU need not reproduce them bit for bit, and pinning the
//! exact figures would make this a test about floating-point luck rather than
//! about the trainer. What the receipt owes is the measurement it actually
//! took, printed, plus the claim that the adapter beat the untuned base on
//! held-out cases.
//!
//! ## The refusing branch
//!
//! On a machine with no discrete adapter — an integrated-only laptop, an Apple
//! GPU, which wgpu classes as `IntegratedGpu`, a headless box — the claim under
//! test is the *refusal*, and it is the more valuable of the two: it is the
//! branch where cubecl would have handed the lane something it did not ask for.
//! The receipt asserts that opening the resident with `device: "gpu"` fails,
//! that the refusal names both the setting it refused and the adapter facts
//! that were found instead (or the absence, when nothing was found), and that
//! no resident was composed. Passing here without training anything is the
//! whole point: the lane declining to advertise a GPU it cannot name is the
//! behaviour, not a shortfall of it.
//!
//! It runs wherever the lane composes at all — Windows, Linux and macOS, the
//! three platforms this crate can read physical memory on, which is the one
//! fact `HostFacts` refuses to invent.

#![cfg(feature = "trainer-gpu")]

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use common::{
    EVAL_PREFIXES, PostedJob, author_fixture, autodiff_settings, finite_difference_settings, lane,
    lending, now_ms, open_vault, owner, profile, refusal_from, run_trainer_job, unlock,
    write_device_settings,
};
use distillery::{
    GpuDeviceType, TRAINER_AUTODIFF, TRAINER_FINITE_DIFFERENCE, TRAINER_RESOURCE, TrainReceipt,
    TrainRequest, TrainerGpuKind, probe_gpu_adapter,
};
use djinn::resident::DjinnResident;
use djinn::settings::TrainerLaneSettings;
use eidetic::models::{EvalMetric, EvalReport, OpaqueBlob};
use eidetic::typed::load_typed;
use eidetic::{ModelAdapterManifest, NoFetcher};
use mesh::ResourceId;
use mesh_host::Step;

/// A failure bound, not an expectation. The run is stopped by the observer
/// seeing `Step::Completed`; this only decides how long a *stuck* run is given
/// before the assertions below call it. Generous, because a first GPU run pays
/// shader compilation on top of the training itself.
const FAILURE_BOUND: Duration = Duration::from_secs(600);

/// Whichever of the two claims this machine's hardware supports, proved.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_gpu_lane_trains_on_a_discrete_adapter_or_refuses_to_compose_at_all() {
    let directory = tempfile::tempdir().expect("temporary resident root");
    let data_root = directory.path().join("data");
    let (vault_dir, identity) = open_vault(directory.path());

    // The posture explicitly permits the trainer, and nothing else. An empty
    // `allowed_resources` would admit this job by permitting everything, which
    // would prove nothing about the owner having agreed to train.
    let mut posture = lending();
    posture.allowed_resources = vec![TRAINER_RESOURCE.into()];
    posture.accepted_checkpoints = vec!["restart".into()];
    write_device_settings(&data_root, Some(posture));

    // The one field that differs from the CPU receipt.
    let mut trains = lane();
    trains.trainer = Some(TrainerLaneSettings {
        device: "gpu".into(),
        library_root: None,
    });

    // Which machine is this? The same question `discrete_gpu_trainer_device`
    // is about to ask inside the composition, asked here first so the receipt
    // knows which of the two claims it owes — and printed either way, because
    // a reader of the log is entitled to know what the machine proved rather
    // than only that something passed.
    let probe = probe_gpu_adapter(TrainerGpuKind::DiscreteGpu(0));
    match &probe {
        Ok(facts) => println!("djinn GPU probe: {facts}"),
        Err(error) => println!("djinn GPU probe: no adapter — {error}"),
    }
    let is_discrete = matches!(
        &probe,
        Ok(facts) if facts.device_type == GpuDeviceType::Discrete && facts.matched_requested_class
    );

    if !is_discrete {
        println!(
            "branch: no discrete adapter on this machine — proving the lane REFUSES \
             `device: \"gpu\"` rather than composing on what is here"
        );
        // What the refusal has to carry: the adapter it found instead, by name,
        // or the runtime's own account of finding nothing. Either way the owner
        // reading this message can tell what their machine actually has, which
        // is the difference between a refusal and a wall.
        let found = match &probe {
            Ok(facts) => facts.name.clone(),
            Err(error) => error.to_string(),
        };

        let refusal = refusal_from(
            DjinnResident::open(
                &identity,
                &data_root,
                &profile(),
                owner(Some(trains)),
                &vault_dir,
                unlock(),
            )
            .await,
            "a lane that says `device: \"gpu\"` on a machine with no discrete adapter must \
             refuse rather than compose on whatever cubecl would have substituted",
        )
        .await;
        println!("djinn GPU lane refusal: {refusal}");

        assert!(
            refusal.contains("distillery.trainer.device"),
            "the refusal must name the setting it refused: {refusal}"
        );
        assert!(
            refusal.contains("\"gpu\""),
            "the refusal must name the value it refused: {refusal}"
        );
        assert!(
            refusal.contains(&found),
            "the refusal must name what this machine has instead of a discrete GPU \
             ({found}): {refusal}"
        );
        assert!(
            !refusal.contains("close blob custody"),
            "the refusal must unwind cleanly, not report a second failure: {refusal}"
        );
        // `refusal_from` is the "nothing was composed" assertion: an `Ok` here
        // would be a live resident, which it shuts down and then fails on.
        return;
    }

    println!(
        "branch: a discrete adapter is present — proving the lane composes on it and \
         trains real work there"
    );

    let mut resident = DjinnResident::open(
        &identity,
        &data_root,
        &profile(),
        owner(Some(trains)),
        &vault_dir,
        unlock(),
    )
    .await
    .expect("compose the Djinn resident with a GPU trainer on this machine's discrete GPU");
    assert!(resident.distillery_enabled());
    let mut works = resident
        .take_distillery()
        .expect("the lane was composed and can be driven");

    // 1. The adapter, named. Not "a GPU" — this one.
    let adapter = works
        .trainer_adapter()
        .expect("a lane that composed a GPU trainer knows which adapter it composed on")
        .clone();
    println!("djinn GPU trainer adapter: {adapter}");
    assert_eq!(
        adapter.device_type,
        GpuDeviceType::Discrete,
        "the lane composed on an adapter that is not a discrete GPU: {adapter:?}"
    );
    assert!(
        adapter.matched_requested_class,
        "the adapter came out of cubecl's unclassified fallback list rather than the \
         discrete class, which is the silent substitution this lane refuses: {adapter:?}"
    );
    assert!(
        !adapter.name.trim().is_empty(),
        "an adapter with no name is not an adapter anyone could check: {adapter:?}"
    );
    assert!(!adapter.backend.trim().is_empty(), "{adapter:?}");

    // 2. The owner's posture and the composed GPU fact both reached the
    //    supervisor. The facts are read off the installed host rather than
    //    recomputed, for the same reason the policy is: a composition that
    //    claims it reached the supervisor has to be checkable there.
    let installed = works.authority().host().policy().clone();
    assert_eq!(
        installed.allowed_resources,
        BTreeSet::from([ResourceId::parse(TRAINER_RESOURCE).unwrap()]),
        "the trainer runs because the device's stated posture permits it by \
         name, not because the allowed set was empty"
    );
    let facts = *works.authority().host().facts();
    assert!(
        facts.gpu,
        "a lane that composed a GPU trainer must advertise the GPU it composed: {facts:?}"
    );
    assert!(facts.memory_mib >= 512, "{facts:?}");

    let library = works
        .model_library()
        .expect("a lane that stated a trainer composed a model library");

    // 3. Author the base model triple and the corpus into that library —
    //    through the resident's own handle, because redb is single-writer and
    //    the trainer is about to write into the same file. Byte-identical to
    //    the CPU receipt's fixture, so the two runs' tallies are about the
    //    device and not about the data.
    let (base_model_ref, tokenizer_ref, corpus_ref) = {
        let mut store = library.lock().await;
        author_fixture(&mut store, "djinn-gpu-trainer-receipt").await
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

    // 4. The v0 arm on the discrete adapter. The same hyperparameters the CPU
    //    receipt uses, for the same reason.
    let posted = run_trainer_job(
        &mut works,
        &request("djinn-gpu-trainer-receipt", finite_difference_settings()),
        1,
        FAILURE_BOUND,
        &mut observed,
    )
    .await;
    let PostedJob::Committed { receipt, elapsed } = posted else {
        panic!("the v0 arm must commit on the discrete adapter: steps were {observed:?}");
    };
    println!("observed step sequence: {observed:?}");
    assert!(
        observed
            .iter()
            .any(|step| matches!(step, Step::Claimed { .. })),
        "a completion with no claim would mean the board was not really driven: {observed:?}"
    );
    assert!(
        observed
            .iter()
            .any(|step| matches!(step, Step::Started { .. })),
        "the job must have been started on this device, not merely reported done: {observed:?}"
    );
    assert_published_adapter(
        &library,
        &receipt,
        corpus_ref,
        "peft-esp-trainer-v0",
        TRAINER_FINITE_DIFFERENCE,
    )
    .await;
    println!(
        "djinn GPU trainer receipt ({TRAINER_FINITE_DIFFERENCE}): baseline {}/{} vs adapter \
         {}/{} at RankingAt{{3}}, trained in {:.1}s on {} ({}, {} adapter)",
        receipt.baseline.passed,
        receipt.baseline.total,
        receipt.adapter.passed,
        receipt.adapter.total,
        elapsed.as_secs_f64(),
        adapter.name,
        adapter.backend,
        adapter.device_type,
    );

    // 5. The autodiff arm on the same adapter.
    //
    //    Djinn hands the resource the *inner* wgpu device that
    //    `discrete_gpu_trainer_device` probed and composed — the trainer wraps
    //    it with `Device::autodiff` itself, and burn panics on a device that
    //    is already autodiff, so wrapping here would turn the one lane that
    //    knows which adapter it is on into the one lane that cannot run v1.
    //    Nothing about the composition changes for the arm; only the request
    //    does.
    #[cfg(feature = "trainer-autodiff")]
    {
        let posted = run_trainer_job(
            &mut works,
            &request("djinn-gpu-trainer-receipt-autodiff", autodiff_settings()),
            2,
            FAILURE_BOUND,
            &mut observed,
        )
        .await;
        let PostedJob::Committed { receipt, elapsed } = posted else {
            panic!(
                "a build carrying `trainer-autodiff` must run the autodiff arm on the \
                 discrete adapter, not release it: steps were {observed:?}"
            );
        };
        assert_published_adapter(
            &library,
            &receipt,
            corpus_ref,
            distillery::TRAINED_ADAPTER_FORMAT_VERSION_AUTODIFF,
            TRAINER_AUTODIFF,
        )
        .await;
        println!(
            "djinn GPU trainer receipt ({TRAINER_AUTODIFF}): baseline {}/{} vs adapter {}/{} \
             at RankingAt{{3}}, trained in {:.1}s on {} ({}, {} adapter)",
            receipt.baseline.passed,
            receipt.baseline.total,
            receipt.adapter.passed,
            receipt.adapter.total,
            elapsed.as_secs_f64(),
            adapter.name,
            adapter.backend,
            adapter.device_type,
        );
    }

    // 6. And the library is a real file, at the derived persona-scoped path.
    let derived = data_root.join("models").join("works").join("library.redb");
    assert!(
        derived.is_file(),
        "the model library must exist at {}",
        derived.display()
    );

    resident.restore_distillery(Some(works));
    resident
        .shutdown()
        .await
        .expect("clean ordered resident shutdown");
}

/// Everything a published adapter owes its receipt, checked in the persona's
/// own library.
///
/// The tally assertion is *strict improvement* and nothing sharper, on both
/// arms. The CPU receipt's figures are ndarray's; f32 reductions on a GPU need
/// not reproduce them bit for bit, and pinning the exact numbers would make
/// this a test about floating-point luck rather than about the trainer.
async fn assert_published_adapter(
    library: &Arc<tokio::sync::Mutex<muniment::RedbBackend>>,
    receipt: &TrainReceipt,
    corpus_ref: eidetic::ManifestId,
    adapter_format_version: &str,
    trainer: &str,
) {
    assert_eq!(
        receipt.baseline.total,
        EVAL_PREFIXES.len() as u64,
        "the tally must be over the held-out partition, whole"
    );
    assert_eq!(receipt.adapter.total, receipt.baseline.total);
    assert!(
        receipt.adapter.passed > receipt.baseline.passed,
        "the receipt must show a strict held-out improvement on the GPU: {receipt:?}"
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
    report
        .validate_for_adapter(receipt.adapter_manifest_ref, &manifest)
        .expect("report provenance links match the adapter manifest");
}
