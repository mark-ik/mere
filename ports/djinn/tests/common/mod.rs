// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Fixtures shared by the Distillery lane receipts.
//!
//! Three integration tests open the same resident from the same kind of owner
//! statement — `distillery_lane.rs` proves the lane runs a job at all,
//! `distillery_trainer.rs` proves the trainer it composes produces real
//! artifacts, and `distillery_trainer_gpu.rs` proves the same composition on
//! this machine's discrete GPU — and a second copy of the vault, profile,
//! device-settings and lending-posture setup would be a place for the receipts
//! to silently disagree about what "the same device" means.
//!
//! The GPU receipt sharing this fixture is load-bearing rather than tidy: its
//! tallies are only interpretable against the CPU receipt's because both runs
//! train on byte-identical weights and the same held-out partition.
//!
//! The trainer half (behind the `trainer` feature) additionally carries the
//! tiny synthetic llama fixture. That is one more copy of the fixture that
//! lives in `distillery/tests/trainer.rs`, and it is deliberate: the two
//! crates share no dev-dependency seam, and inventing a fixture crate to hold
//! sixty lines of deterministic weights would cost more than the duplication
//! does.

// Each test binary compiles this module whole and uses a different part of it,
// so an unused helper here means "the other receipt wanted it", not "nobody
// does".
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use djinn::resident::DjinnResident;
use djinn::settings::{DistilleryLaneSettings, OwnerSettings};
use pandect::{DeviceSettings, MeshLendingSettings, StatedConditionSettings};
use personae::bootstrap::{self, Unlock};
use personae::{IdentityVault, ProfileId};

pub const PASSPHRASE: &[u8] = b"djinn-distillery-lane-receipt-passphrase";

pub fn unlock() -> Unlock {
    Unlock::passphrase(PASSPHRASE)
}

pub fn profile() -> ProfileId {
    ProfileId("works".into())
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// Open the shared Personae vault the way `bin/djinn.rs` does, and keep it
/// open.
///
/// Holding it across [`DjinnResident::open`] is the point: the lane derives its
/// mesh author from the same vault directory and therefore opens it a *second*
/// time in this process. If Personae's storage opens were exclusive, every test
/// here would fail at that second open rather than at its assertion.
pub fn open_vault(root: &Path) -> (PathBuf, IdentityVault<Box<dyn personae::IdentityStorage>>) {
    let vault_dir = root.join("vault");
    let opened = bootstrap::open_storage(&vault_dir, unlock()).expect("open vault storage");
    let (record, _created) =
        bootstrap::load_or_create_profile(&*opened.storage, &profile()).expect("create profile");
    (
        vault_dir,
        IdentityVault::with_profile(opened.storage, record),
    )
}

/// A lending posture whose one enabled rule is idle time.
///
/// Every other check is switched off *by the owner*, and each `0` here is a
/// statement rather than a shrug:
///
/// - `max_thermal_c: 0` because this build has no thermometer, which is
///   precisely what the lane's second refusal test proves is refused when it
///   is non-zero.
/// - `min_network: "offline"` because a network floor cannot be rescued by a
///   stated value: a sensed `Offline` is a real observation and beats the
///   owner's word, so a machine with no default gateway (a CI box, a laptop on
///   a plane) would withhold forever and these receipts would hang instead of
///   failing. The floor is exercised by unit conversion, not by these runs.
/// - `quiet_hours: None` because a wall-clock window would make these receipts
///   pass or fail depending on the hour they were run.
///
/// `min_idle_ms: 1` is the enabled rule the sensor genuinely covers: sensed
/// from `GetLastInputInfo` on Windows, and stated below so the same settings
/// are coverable anywhere. A machine actively driving a test run is rarely
/// idle, so `stated.idle_ms` is what a run actually leans on — the owner's
/// word standing in for a reading that would otherwise withhold forever.
///
/// Taken together that is what lets these receipts run off Windows, where
/// nothing is sensed at all: `validate_policy_coverage` refuses an enabled
/// rule resting on an absent signal, one rule is enabled, and that one rule
/// has a stated value. The invariant to keep is that pairing — enabling a rule
/// here without stating a fallback for it would compose on Windows and refuse
/// everywhere else, which is precisely the shape of bug a Windows-only receipt
/// used to hide.
///
/// `allowed_resources` and `accepted_checkpoints` are stated narrowly rather
/// than left `[]`: `["mesh.blake3/v1"]` is the one job the lane receipt
/// actually posts, so the posture explicitly permits the job under test rather
/// than permitting everything by omission, and `["restart"]` matches the
/// checkpoint class `JobSpec::simple` gives it. The trainer receipt narrows
/// `allowed_resources` to the trainer's own id in exactly the same way.
pub fn lending() -> MeshLendingSettings {
    MeshLendingSettings {
        min_idle_ms: 1,
        min_battery_pct: 0,
        max_thermal_c: 0,
        min_network: "offline".into(),
        max_bandwidth_in_use_kbps: 0,
        quiet_hours: None,
        max_concurrent_jobs: 1,
        allowed_resources: vec!["mesh.blake3/v1".into()],
        accepted_checkpoints: vec!["restart".into()],
        reclaim_grace_ms: 0,
        supervises_leases: true,
        stated: StatedConditionSettings {
            idle_ms: Some(600_000),
            battery_pct: None,
            on_mains: None,
            thermal_c: None,
            network: Some("wired".into()),
            bandwidth_in_use_kbps: None,
        },
    }
}

/// The lane the receipts run, composing no trainer.
///
/// The trainer receipt takes this and says `trainer: Some(...)` itself, so the
/// difference between the two runs is exactly the one field under test.
pub fn lane() -> DistilleryLaneSettings {
    DistilleryLaneSettings {
        tick_every_ms: 25,
        // Explicit-only: these receipts are about running work, and a
        // maintenance cadence firing mid-run would put a checkpoint between the
        // job and the assertion about it.
        maintenance_every_ms: None,
        blob_gc_every_ms: 1_000,
        collect_after_checkpoint: true,
        retention_revision: "3f".repeat(32),
        promised_floor: "forever".into(),
        privacy_ceiling: "until-checkpoint".into(),
        erase_terminal_at_checkpoint: true,
        max_skew_ms: 0,
        trainer: None,
    }
}

pub fn write_device_settings(data_root: &Path, mesh_lending: Option<MeshLendingSettings>) {
    pandect::save_device_settings(
        data_root,
        &DeviceSettings {
            mesh_lending,
            ..DeviceSettings::default()
        },
    )
    .expect("write device settings");
}

/// The refusal text, or a failure that closes the resident it should not have
/// been handed. [`DjinnResident`] is not `Debug`, so `expect_err` cannot be
/// used, and leaking an opened blob store on the failing path would bury the
/// real message under a store-lock error in the next test.
pub async fn refusal_from(opened: Result<DjinnResident, String>, why: &str) -> String {
    match opened {
        Err(refusal) => refusal,
        Ok(resident) => {
            let _ = resident.shutdown().await;
            panic!("{why}");
        }
    }
}

pub fn owner(lane: Option<DistilleryLaneSettings>) -> OwnerSettings {
    OwnerSettings {
        distillery: lane,
        ..OwnerSettings::default()
    }
}

// ── The tiny synthetic base model and corpus the trainer receipt trains on ──
//
// Copied from `distillery/tests/trainer.rs` so the Djinn receipt trains on
// exactly the fixture the resource's own receipt did: a different fixture
// would make any difference in the tallies uninterpretable.

/// The model id the fixture publishes under.
#[cfg(feature = "trainer")]
pub const MODEL_ID: &str = "fixture/trainer-resource";
/// The token every prompt ends with.
#[cfg(feature = "trainer")]
pub const TRIGGER: &str = "t29";
/// The token the adapter must learn to rank after the trigger.
#[cfg(feature = "trainer")]
pub const EXPECTED: &str = "t7";
/// The six prompts the trainer may read.
#[cfg(feature = "trainer")]
pub const TRAIN_PREFIXES: [&str; 6] = [
    "t3 t11 t5",
    "t18 t2 t26",
    "t9 t14 t1",
    "t22 t6 t13",
    "t4 t27 t10",
    "t15 t8 t21",
];
/// The six held-out prompts only the evaluation tallies.
#[cfg(feature = "trainer")]
pub const EVAL_PREFIXES: [&str; 6] = [
    "t12 t25 t3",
    "t7 t19 t30",
    "t24 t1 t16",
    "t5 t28 t9",
    "t17 t20 t2",
    "t31 t10 t23",
];

/// The synthetic llama config: two layers, eight hidden, a 32-token vocabulary.
#[cfg(feature = "trainer")]
pub const CONFIG_JSON: &str = r#"{
    "vocab_size": 32, "hidden_size": 8, "intermediate_size": 16,
    "num_hidden_layers": 2, "num_attention_heads": 4,
    "num_key_value_heads": 2, "max_position_embeddings": 16,
    "rms_norm_eps": 1e-05, "rope_theta": 10000.0,
    "tie_word_embeddings": false
}"#;

/// A whitespace word-level tokenizer over exactly that vocabulary.
#[cfg(feature = "trainer")]
pub fn tokenizer_json() -> String {
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

/// Deterministic weights: the same salt gives the same tensor on every run, so
/// the receipt's tallies are a property of the trainer rather than of chance.
#[cfg(feature = "trainer")]
pub fn det_vec(n: usize, salt: usize) -> Vec<f32> {
    (0..n)
        .map(|i| (((i + salt * 7919) as f32) * 0.618_034).sin() * 0.05)
        .collect()
}

/// The synthetic base checkpoint as safetensors bytes.
#[cfg(feature = "trainer")]
pub fn base_weights() -> Vec<u8> {
    use safetensors::tensor::{Dtype, TensorView};

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

/// Save one partition's training cases as opaque codicils, in the sorted order
/// a corpus partition carries.
#[cfg(feature = "trainer")]
pub async fn save_cases(
    store: &mut dyn eidetic::Store,
    prefixes: &[&str],
) -> Vec<eidetic::ManifestId> {
    use eidetic::models::OpaqueBlob;
    use eidetic::typed::save_typed;
    use eidetic::{PrivacyClass, ProvenanceRecord, Timestamp, TrustEnvelope};

    let mut ids = Vec::new();
    for prefix in prefixes {
        let case = distillery::TrainingCase {
            prompt: format!("{prefix} {TRIGGER}"),
            expected_token: EXPECTED.to_string(),
        };
        ids.push(
            save_typed(
                store,
                &OpaqueBlob(serde_json::to_vec(&case).unwrap()),
                vec![],
                PrivacyClass::LocalOnly,
                ProvenanceRecord::self_imported("djinn-trainer-receipt"),
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

// ── Running one trainer job through an opened lane ──────────────────────────

/// The trainer settings the v0 arm's receipts post.
///
/// v0's forty line-searched central-difference steps and v1's twelve Adam
/// steps are the counts their own forcing receipts settled on. Matching them
/// to each other would be matching the wrong thing: what a lane receipt owes
/// is a strict held-out improvement from each arm through the composed path,
/// not a like-for-like step budget.
#[cfg(feature = "trainer")]
pub fn finite_difference_settings() -> distillery::TrainerSettings {
    distillery::TrainerSettings::FiniteDifference(distillery::LoraTrainerSettings {
        rank: 1,
        alpha: 8.0,
        target_module: "v_proj".into(),
        steps: 40,
        initial_step_length: 1.0,
        minimum_step_length: 1.0e-4,
        epsilon: 0.02,
    })
}

/// The autodiff arm's settings, in the shape a lane operator would write them.
///
/// Available on every `trainer` build, not only under `trainer-autodiff`: the
/// refusal receipt has to be able to *post* this request in order to prove it
/// is refused, and esp keeps the v1 vocabulary with its loader rather than
/// with its trainer for exactly that reason.
#[cfg(feature = "trainer")]
pub fn autodiff_settings() -> distillery::TrainerSettings {
    distillery::TrainerSettings::Autodiff(distillery::AutodiffLoraSettings {
        rank: 1,
        alpha: 8.0,
        target_modules: vec!["v_proj".into()],
        steps: 12,
        learning_rate: 0.2,
        beta1: 0.9,
        beta2: 0.999,
        epsilon: 1.0e-8,
        weight_decay: 0.0,
    })
}

/// How a posted trainer job ended, as the lane could see it.
#[cfg(feature = "trainer")]
pub enum PostedJob {
    /// The job committed; this is the receipt it committed, and how long the
    /// run took wall-clock.
    Committed {
        receipt: distillery::TrainReceipt,
        elapsed: std::time::Duration,
    },
    /// The job was released back to the board instead of committing. The
    /// board's vocabulary is Posted / Claimed / Done / Committed plus lease
    /// release reasons, so a refused run is visible as this and never as text
    /// — a receipt that wants the refusal's wording asserts it at its source.
    Released,
}

/// Author the base model triple and the corpus into a lane's model library.
///
/// Written through the resident's own handle because redb is single-writer and
/// the trainer is about to write into the same file.
#[cfg(feature = "trainer")]
pub async fn author_fixture(
    store: &mut muniment::RedbBackend,
    provenance: &str,
) -> (
    eidetic::ManifestId,
    eidetic::ManifestId,
    eidetic::ManifestId,
) {
    use eidetic::models::TrainingCorpus;
    use eidetic::typed::save_typed;
    use eidetic::{ModelLibrary, PrivacyClass, ProvenanceRecord, Timestamp, TrustEnvelope};

    let base_model_ref = ModelLibrary::save_model_with_components(
        store,
        MODEL_ID,
        "llama",
        "MIT",
        serde_json::from_str(CONFIG_JSON).unwrap(),
        &base_weights(),
        tokenizer_json().as_bytes(),
        Vec::new(),
        Vec::new(),
        PrivacyClass::LocalOnly,
        ProvenanceRecord::self_imported(provenance),
        TrustEnvelope::self_asserted(),
        Timestamp(0),
    )
    .await
    .expect("save the base model into the persona's library");
    let manifest = ModelLibrary::load_model(store, base_model_ref)
        .await
        .expect("load model manifest")
        .expect("model manifest present");
    let corpus = TrainingCorpus {
        training_source_codicils: save_cases(store, &TRAIN_PREFIXES).await,
        evaluation_source_codicils: save_cases(store, &EVAL_PREFIXES).await,
    };
    let corpus_ref = save_typed(
        store,
        &corpus,
        vec![],
        PrivacyClass::LocalOnly,
        ProvenanceRecord::self_imported(provenance),
        TrustEnvelope::self_asserted(),
        Timestamp(1),
    )
    .await
    .expect("save the corpus");
    (base_model_ref, manifest.tokenizer_blob, corpus_ref)
}

/// Post one trainer job onto an opened lane and drive the resident until it
/// either commits or is released.
///
/// The committed output is found by the request blob its own spec names, not
/// by taking whatever committed last: a receipt that runs two arms in sequence
/// leaves two committed jobs on one board, and picking the wrong one would
/// make both assertions pass for the wrong reason.
#[cfg(feature = "trainer")]
pub async fn run_trainer_job(
    works: &mut djinn::resident_distillery::ResidentDistillery,
    request: &distillery::TrainRequest,
    nonce: u64,
    failure_bound: std::time::Duration,
    observed: &mut Vec<mesh_host::Step>,
) -> PostedJob {
    use std::sync::Arc;

    use distillery::{ResidentReceipt, TRAINER_REQUEST_INPUT, TRAINER_RESOURCE};
    use mesh::spec::{DeterminismClass, JobSpec};
    use mesh::{BlobSource as _, ResourceId};
    use mesh_host::Step;
    use tokio::sync::Notify;

    let request_blob = works
        .space()
        .put(&serde_json::to_vec(request).unwrap())
        .await
        .expect("stage the training request");
    // Kept so the committed output can be found by the request that caused it.
    let posted_blob = request_blob.clone();
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
            nonce,
            now_ms(),
        )
        .await
        .expect("post the trainer job as this device");

    // Stop on the first terminal answer in either direction. A refused job is
    // released back to the board and this device is free to claim it again, so
    // waiting out the failure bound would spin on a question already answered.
    let stop = Arc::new(Notify::new());
    let signal = Arc::clone(&stop);
    let mut settled: Option<bool> = None;
    let started = std::time::Instant::now();
    works
        .run_until(
            async move {
                tokio::select! {
                    _ = signal.notified() => {}
                    _ = tokio::time::sleep(failure_bound) => {}
                }
            },
            |receipt| {
                if let ResidentReceipt::Tick { steps } = &receipt {
                    for step in steps {
                        if !matches!(step, Step::Idle) {
                            observed.push(step.clone());
                        }
                        match step {
                            Step::Completed { .. } => {
                                settled = Some(true);
                                stop.notify_one();
                            }
                            Step::Released { .. } => {
                                settled = Some(false);
                                stop.notify_one();
                            }
                            _ => {}
                        }
                    }
                }
            },
        )
        .await
        .expect("the works ran to the stop request");
    let elapsed = started.elapsed();

    match settled {
        None => panic!(
            "the trainer job neither committed nor was released in {failure_bound:?}; \
             steps were {observed:?}"
        ),
        Some(false) => PostedJob::Released,
        Some(true) => {
            let board = works
                .authority()
                .host()
                .synced()
                .board()
                .await
                .expect("fold the board");
            let output = board
                .jobs()
                .find(|job| {
                    job.spec.as_ref().is_some_and(|spec| {
                        spec.inputs.iter().any(|input| input.blob == posted_blob)
                    })
                })
                .and_then(|job| match &job.state {
                    mesh::JobState::Committed { output, .. } => Some((**output).clone()),
                    _ => None,
                })
                .expect("this request's trainer job committed an output");
            let bytes = works
                .space()
                .fetch(&output.blob)
                .await
                .expect("fetch the committed receipt")
                .expect("committed receipt bytes present");
            PostedJob::Committed {
                receipt: serde_json::from_slice(&bytes).expect("receipt JSON"),
                elapsed,
            }
        }
    }
}
