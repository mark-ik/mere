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

/// Save one partition's training cases as opaque engrams, in the sorted order
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
            .expect("save case engram"),
        );
    }
    ids.sort_by_key(ToString::to_string);
    ids
}
