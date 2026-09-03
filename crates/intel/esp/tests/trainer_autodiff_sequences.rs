// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The sequence objective's forcing receipt (preference adapter experiment
//! plan, Phase 1): whole-response training on the trainer forcing fixture.
//!
//! The synthetic rule the tiny model has to learn: every response is the
//! same fixed two-token continuation (`RESPONSE`) after the trigger,
//! regardless of the prefix before it — a rule with plenty of capacity to
//! spare for a rank-1 delta to represent. Representing it and *reaching* it
//! by gradient descent from this untrained fixture's near-uniform starting
//! point are different things, though: see [`settings`] for how the
//! hyperparameters below were actually chosen. The point of this receipt is
//! that the sequence objective drives held-out `sequence_loss` down, and
//! that what it produces is an ordinary v1 PEFT adapter the unchanged
//! loader accepts.
//!
//! The shared fixture's `config.json` (`tests/common/mod.rs`) does not name
//! an `eos_token_id`; no other test needs one. This file carries its own
//! copy with that one field added, rather than touching the shared fixture:
//! `t0` is the WordLevel tokenizer's `unk_token`, otherwise unused by any
//! prefix the shared fixture defines (`TRAIN_PREFIXES`, `EVAL_PREFIXES`,
//! `TRIGGER`, `EXPECTED` between them use every other id in the 32-token
//! vocabulary), so it is free to double as this fixture's stop token without
//! colliding with real content or exercising the "unknown token" fallback it
//! would otherwise trigger.

#![cfg(feature = "decoder-autodiff")]

use burn::tensor::Device;
use eidetic::{AdapterRuntimeCompat, Hash, ManifestId, ModelAdapterManifest};
use esp::infer::decoder::{
    AutodiffLoraSettings, PEFT_LORA_NDARRAY_LOADER, PeftLoraAdapterLoader, SequenceCase,
    SequenceTrainingSettings, TRAINED_ADAPTER_FORMAT_VERSION_AUTODIFF, TrainedLoraAdapter,
    sequence_loss, train_peft_lora_autodiff_sequences,
};
use esp::infer::{AdapterArtifact, AdapterLoader, AdapterSelection, ModelSession};

mod common;
use common::{
    EVAL_PREFIXES, MODEL_ID, PROMPT_TEMPLATE, TRAIN_PREFIXES, TRIGGER, base_weights, tokenizer_json,
};

const ADAPTER_NAME: &str = "trainer-autodiff-sequences-fixture";
/// The fixed two-token continuation every response repeats — the synthetic
/// rule this receipt asks the tiny model to learn.
const RESPONSE: &str = "t7 t19";

/// The trainer forcing fixture's `config.json`, plus `eos_token_id`. See the
/// module doc for why `t0` is a safe, unused choice on this fixture.
const SEQ_CONFIG_JSON: &str = r#"{
    "vocab_size": 32, "hidden_size": 8, "intermediate_size": 16,
    "num_hidden_layers": 2, "num_attention_heads": 4,
    "num_key_value_heads": 2, "max_position_embeddings": 16,
    "rms_norm_eps": 1e-05, "rope_theta": 10000.0,
    "tie_word_embeddings": false, "eos_token_id": 0
}"#;

/// Tuned for a strict, reproducible decrease in *held-out* `sequence_loss` —
/// a stronger, noisier claim than the v0/v1 receipts' held-out ranking
/// tally, which their own findings call "a coarse, non-monotone proxy on
/// the fixture" precisely because small logit shifts near this untrained
/// model's near-uniform initial distribution reorder ranks far more readily
/// than they move the scalar mean loss. `v_proj` alone (v0/v1's target) held
/// training loss on a plateau within noise of the baseline; adding `q_proj`
/// and dropping the learning rate by an order of magnitude from the other
/// receipts' 0.2 opened a wide, reproducible basin — every combination of
/// 200-400 steps and a learning rate of 0.015-0.025 gave the same strict
/// held-out decrease on this fixture, which is why 300 and 0.02 (roughly the
/// center of that basin) were kept rather than whichever pair scored best.
fn settings(steps: u32) -> AutodiffLoraSettings {
    AutodiffLoraSettings {
        rank: 1,
        alpha: 8.0,
        target_modules: vec!["q_proj".into(), "v_proj".into()],
        steps,
        learning_rate: 0.02,
        beta1: 0.9,
        beta2: 0.999,
        epsilon: 1.0e-8,
        weight_decay: 0.0,
    }
}

fn sequence_case(prefix: &str) -> SequenceCase {
    SequenceCase {
        prompt: format!("{prefix} {TRIGGER}"),
        response: RESPONSE.into(),
    }
}

fn train_cases() -> Vec<SequenceCase> {
    TRAIN_PREFIXES.iter().map(|p| sequence_case(p)).collect()
}

fn held_out_cases() -> Vec<SequenceCase> {
    EVAL_PREFIXES.iter().map(|p| sequence_case(p)).collect()
}

/// `batch_size` covers the whole six-case training corpus, so every step
/// still sees a full-batch mini-batch (Phase 1's behaviour, up to row
/// order) — this receipt's point is the objective and the artifact
/// dataflow, not the batching Phase 2 adds on top of it.
fn mini_batch() -> SequenceTrainingSettings {
    SequenceTrainingSettings {
        batch_size: TRAIN_PREFIXES.len() as u32,
        seed: 11,
        max_sequence_tokens: 32,
    }
}

fn train(settings: &AutodiffLoraSettings, device: &Device) -> TrainedLoraAdapter {
    train_peft_lora_autodiff_sequences(
        SEQ_CONFIG_JSON.as_bytes(),
        tokenizer_json().as_bytes(),
        &base_weights(),
        MODEL_ID,
        &train_cases(),
        settings,
        &mini_batch(),
        device,
    )
    .expect("train sequence adapter")
}

/// The manifest the loader checks the adapter against, built directly the
/// way `trainer_autodiff.rs`'s does: the point here is the loader's
/// admission checks, not the eidetic corridor.
fn manifest(
    settings: &AutodiffLoraSettings,
    trained: &TrainedLoraAdapter,
    base_model_ref: ManifestId,
    tokenizer_ref: ManifestId,
) -> ModelAdapterManifest {
    ModelAdapterManifest {
        name: ADAPTER_NAME.into(),
        base_model_ref,
        adapter_blob: ManifestId::of_blob(&trained.adapter_safetensors),
        adapter_config_blob: ManifestId::of_blob(&trained.adapter_config_json),
        adapter_format: "peft-lora".into(),
        adapter_format_version: TRAINED_ADAPTER_FORMAT_VERSION_AUTODIFF.into(),
        runtime_compat: AdapterRuntimeCompat {
            minimum_capabilities: vec!["peft-lora".into()],
            known_loaders: vec![PEFT_LORA_NDARRAY_LOADER.into()],
            converter_lineage: vec![],
        },
        rank: settings.rank,
        alpha: settings.alpha,
        target_modules: settings.target_modules.clone(),
        tokenizer_ref,
        prompt_template_hash: Hash::of(PROMPT_TEMPLATE),
        quantization_assumption: None,
        training_corpus_root: None,
        training_method: serde_json::json!({
            "trainer": "esp-trainer-v1",
            "objective": "sequence",
            "settings": serde_json::to_value(settings).unwrap(),
        }),
        eval_results: None,
    }
}

/// Training on the synthetic corpus strictly reduces `sequence_loss` on
/// unseen held-out cases drawn from the same rule, and the produced adapter
/// loads through the unchanged `PeftLoraAdapterLoader` — the sequence
/// trainer's output is byte-for-byte an ordinary v1 PEFT adapter; nothing
/// about the whole-response objective touches the artifact contract.
#[test]
fn training_strictly_reduces_held_out_sequence_loss_and_loads_through_the_unchanged_loader() {
    let device = Device::ndarray();
    let base_model_ref = ManifestId::of_blob(b"autodiff sequence base");
    let tokenizer_ref = ManifestId::of_blob(b"autodiff sequence tokenizer");
    let tokenizer_bytes = tokenizer_json();
    let weights = base_weights();

    let loader = PeftLoraAdapterLoader::new(
        base_model_ref,
        tokenizer_ref,
        SEQ_CONFIG_JSON.as_bytes(),
        tokenizer_bytes.as_bytes(),
        &weights,
        MODEL_ID,
        PEFT_LORA_NDARRAY_LOADER,
        &device,
    );
    let session = |adapters: Vec<AdapterSelection>| ModelSession {
        base_model_ref,
        model_id: MODEL_ID.into(),
        tokenizer_ref,
        prompt_template_hash: Hash::of(PROMPT_TEMPLATE),
        quantization: None,
        loader: PEFT_LORA_NDARRAY_LOADER.into(),
        adapters,
    };

    let held_out = held_out_cases();
    let baseline_session = loader
        .load_session(session(vec![]), &[])
        .expect("baseline session");
    let baseline_loss = sequence_loss(
        baseline_session.provider().model(),
        tokenizer_bytes.as_bytes(),
        &held_out,
    )
    .expect("baseline held-out sequence loss");
    assert!(
        baseline_loss.is_finite() && baseline_loss > 0.0,
        "receipt is trivial unless the untrained baseline has real loss to reduce: {baseline_loss}"
    );

    let settings = settings(300);
    let trained = train(&settings, &device);
    assert!(
        trained.trained_loss < trained.initial_loss,
        "training must reduce the training loss: {} -> {}",
        trained.initial_loss,
        trained.trained_loss
    );

    let adapter_ref = ManifestId::of_blob(b"autodiff sequence adapter manifest");
    let manifest = manifest(&settings, &trained, base_model_ref, tokenizer_ref);
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
        .expect("the sequence adapter must pass the unchanged loader's checks");
    let adapted_loss = sequence_loss(
        adapted_session.provider().model(),
        tokenizer_bytes.as_bytes(),
        &held_out,
    )
    .expect("adapted held-out sequence loss");

    println!(
        "sequence trainer forcing: held-out loss {baseline_loss:.4} -> {adapted_loss:.4} \
         over {} steps",
        settings.steps
    );
    assert!(
        adapted_loss < baseline_loss,
        "the sequence adapter must strictly reduce held-out loss: {baseline_loss} -> {adapted_loss}"
    );
}

/// The same loss-decrease claim on the discrete GPU, in the v0/v1 GPU
/// receipts' posture: strict improvement only, since f32 summation order
/// differs between the ndarray and wgpu lanes.
#[test]
#[cfg(feature = "decoder-wgpu")]
fn training_strictly_reduces_the_loss_on_the_discrete_gpu() {
    let device = Device::wgpu(burn::tensor::DeviceKind::DiscreteGpu(0));
    let settings = settings(300);

    // Twice, so the reported number is not charged for wgpu's kernel
    // compilation and adapter acquisition on the first run.
    let cold_started = std::time::Instant::now();
    let trained = train(&settings, &device);
    let cold = cold_started.elapsed();
    let warm_started = std::time::Instant::now();
    let repeat = train(&settings, &device);
    let warm = warm_started.elapsed();

    for run in [&trained, &repeat] {
        assert!(
            run.trained_loss < run.initial_loss,
            "gpu sequence training must reduce the loss: {} -> {}",
            run.initial_loss,
            run.trained_loss
        );
    }
    println!(
        "autodiff v1 sequence objective, discrete gpu: loss {:.6} -> {:.6} over {} steps, \
         cold {cold:?}, warm {warm:?}",
        trained.initial_loss, trained.trained_loss, settings.steps,
    );
}
