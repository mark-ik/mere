// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A FLoRA round over adapters the autodiff trainer really produced.
//!
//! The stacker's existing receipts run on synthetic factors, which proves the
//! arithmetic. This one proves the *seam*: two participants train real v1
//! adapters on disjoint slices of the shared fixture, and their bytes — the
//! ones esp's serializer emitted, with esp's v1 version stamp — go through
//! `aggregate_exact` unchanged.
//!
//! Two claims:
//!
//! 1. **Order cannot change the aggregate.** Contributions canonicalize by
//!    id, so both arrival orders emit identical bytes. A federated round where
//!    the answer depended on who spoke first would not be an answer.
//! 2. **A round is homogeneous by trainer version.** A v0 and a v1 adapter in
//!    one round are refused by name. The stacker already checks
//!    `adapter_format_version` and the PEFT `peft_version`; this is the pair
//!    those checks exist for, so it is worth showing them firing on it rather
//!    than on a hand-edited field.

#![cfg(all(feature = "flora", feature = "trainer-autodiff"))]

use distillery::flora::{FloraContribution, FloraRequest, FloraWeight, aggregate_exact};
use distillery::{AutodiffLoraSettings, LoraTrainerSettings, TrainingCase};
use eidetic::{AdapterRuntimeCompat, Hash, ManifestId, ModelAdapterManifest};
use esp::infer::decoder::{
    DecoderDevice, PEFT_LORA_NDARRAY_LOADER, TRAINED_ADAPTER_FORMAT_VERSION,
    TRAINED_ADAPTER_FORMAT_VERSION_AUTODIFF, TrainedLoraAdapter, train_peft_lora,
    train_peft_lora_autodiff,
};
use safetensors::tensor::{Dtype, TensorView};

const MODEL_ID: &str = "fixture/flora-autodiff-round";
const TEMPLATE: &[u8] = b"{{ prompt }}";
const TRIGGER: &str = "t29";

/// Two disjoint local corpora — the point of a federated round is that the
/// participants did not see each other's data.
const FIRST_PREFIXES: [&str; 3] = ["t3 t11 t5", "t18 t2 t26", "t9 t14 t1"];
const SECOND_PREFIXES: [&str; 3] = ["t22 t6 t13", "t4 t27 t10", "t15 t8 t21"];
/// Each participant supervises a different next token, so their adapters
/// really do pull in different directions.
const FIRST_EXPECTED: &str = "t7";
const SECOND_EXPECTED: &str = "t19";

// ── The shared tiny base model ──────────────────────────────────────────────

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

fn cases(prefixes: &[&str], expected: &str) -> Vec<TrainingCase> {
    prefixes
        .iter()
        .map(|prefix| TrainingCase {
            prompt: format!("{prefix} {TRIGGER}"),
            expected_token: expected.to_string(),
        })
        .collect()
}

fn autodiff_settings() -> AutodiffLoraSettings {
    AutodiffLoraSettings {
        rank: 1,
        alpha: 8.0,
        target_modules: vec!["v_proj".into()],
        steps: 6,
        learning_rate: 0.2,
        beta1: 0.9,
        beta2: 0.999,
        epsilon: 1.0e-8,
        weight_decay: 0.0,
    }
}

fn train_v1(prefixes: &[&str], expected: &str) -> TrainedLoraAdapter {
    train_peft_lora_autodiff(
        CONFIG_JSON.as_bytes(),
        tokenizer_json().as_bytes(),
        &base_weights(),
        MODEL_ID,
        &cases(prefixes, expected),
        &autodiff_settings(),
        &DecoderDevice::ndarray(),
    )
    .expect("train a v1 adapter")
}

fn train_v0(prefixes: &[&str], expected: &str) -> TrainedLoraAdapter {
    train_peft_lora(
        CONFIG_JSON.as_bytes(),
        tokenizer_json().as_bytes(),
        &base_weights(),
        MODEL_ID,
        &cases(prefixes, expected),
        &LoraTrainerSettings {
            rank: 1,
            alpha: 8.0,
            target_module: "v_proj".into(),
            steps: 2,
            initial_step_length: 1.0,
            minimum_step_length: 1.0e-4,
            epsilon: 0.02,
        },
        &DecoderDevice::ndarray(),
    )
    .expect("train a v0 adapter")
}

/// One participant's contribution, built the way a real publisher would: the
/// trainer's own bytes, and a manifest whose refs are their content
/// addresses.
fn contribution(
    id: &str,
    trained: &TrainedLoraAdapter,
    adapter_format_version: &str,
    weight: FloraWeight,
) -> FloraContribution {
    let manifest = ModelAdapterManifest {
        name: id.into(),
        base_model_ref: ManifestId::of_blob(b"flora autodiff base"),
        adapter_blob: ManifestId::of_blob(&trained.adapter_safetensors),
        adapter_config_blob: ManifestId::of_blob(&trained.adapter_config_json),
        adapter_format: "peft-lora".into(),
        adapter_format_version: adapter_format_version.into(),
        runtime_compat: AdapterRuntimeCompat {
            minimum_capabilities: vec!["peft-lora".into()],
            known_loaders: vec![PEFT_LORA_NDARRAY_LOADER.into()],
            converter_lineage: vec![],
        },
        rank: 1,
        alpha: 8.0,
        target_modules: vec!["v_proj".into()],
        tokenizer_ref: ManifestId::of_blob(b"flora autodiff tokenizer"),
        prompt_template_hash: Hash::of(TEMPLATE),
        quantization_assumption: None,
        training_corpus_root: None,
        training_method: serde_json::Value::Null,
        eval_results: None,
    };
    FloraContribution {
        contribution_id: id.into(),
        manifest_ref: ManifestId::of_blob(&serde_json::to_vec(&manifest).unwrap()),
        manifest,
        adapter_config_json: trained.adapter_config_json.clone(),
        adapter_safetensors: trained.adapter_safetensors.clone(),
        weight,
    }
}

fn half() -> FloraWeight {
    FloraWeight {
        numerator: 1,
        denominator: 2,
    }
}

#[test]
fn two_autodiff_participants_stack_to_the_same_bytes_in_either_order() {
    let first_trained = train_v1(&FIRST_PREFIXES, FIRST_EXPECTED);
    let second_trained = train_v1(&SECOND_PREFIXES, SECOND_EXPECTED);
    assert_ne!(
        first_trained.adapter_safetensors, second_trained.adapter_safetensors,
        "the two participants must have learned different factors for this receipt to mean anything"
    );

    // The v1 stamp travels with the bytes: esp writes `esp-trainer-v1` into
    // `adapter_config.json`, and the manifest must say `peft-esp-trainer-v1`
    // or the loader refuses the pair.
    let config: serde_json::Value =
        serde_json::from_slice(&first_trained.adapter_config_json).unwrap();
    assert_eq!(config["peft_version"], "esp-trainer-v1");

    let first = contribution(
        "aaaa-first",
        &first_trained,
        TRAINED_ADAPTER_FORMAT_VERSION_AUTODIFF,
        half(),
    );
    let second = contribution(
        "bbbb-second",
        &second_trained,
        TRAINED_ADAPTER_FORMAT_VERSION_AUTODIFF,
        half(),
    );

    let request = |contributions: Vec<FloraContribution>| FloraRequest {
        output_name: "flora-autodiff-round".into(),
        rank_budget: 8,
        contributions,
    };
    let forward = aggregate_exact(request(vec![first.clone(), second.clone()]))
        .expect("stack two v1 contributions");
    let reversed =
        aggregate_exact(request(vec![second, first])).expect("stack them the other way round");

    assert_eq!(forward.adapter_safetensors, reversed.adapter_safetensors);
    assert_eq!(forward.adapter_config_json, reversed.adapter_config_json);
    assert_eq!(forward.receipt, reversed.receipt);
    assert_eq!(
        forward.manifest.adapter_blob,
        ManifestId::of_blob(&forward.adapter_safetensors)
    );
    assert_eq!(
        forward
            .receipt
            .contribution_order
            .iter()
            .map(|entry| entry.contribution_id.as_str())
            .collect::<Vec<_>>(),
        vec!["aaaa-first", "bbbb-second"]
    );

    // The aggregate stays a v1 artifact: rank 2 = 1 + 1, alpha equal to the
    // rank so esp's loader applies unit scale over the already-scaled B*.
    assert_eq!(forward.manifest.rank, 2);
    assert_eq!(forward.manifest.alpha, 2.0);
    assert_eq!(
        forward.manifest.adapter_format_version,
        TRAINED_ADAPTER_FORMAT_VERSION_AUTODIFF
    );
    let aggregate_config: serde_json::Value =
        serde_json::from_slice(&forward.adapter_config_json).unwrap();
    assert_eq!(aggregate_config["peft_version"], "esp-trainer-v1");
    println!(
        "flora autodiff round: rank {} aggregate from {} v1 contributions, {} bytes",
        forward.manifest.rank,
        forward.receipt.contribution_order.len(),
        forward.adapter_safetensors.len(),
    );
}

#[test]
fn a_round_mixing_v0_and_v1_contributions_is_refused_by_name() {
    let v1 = contribution(
        "aaaa-autodiff",
        &train_v1(&FIRST_PREFIXES, FIRST_EXPECTED),
        TRAINED_ADAPTER_FORMAT_VERSION_AUTODIFF,
        half(),
    );
    let v0 = contribution(
        "bbbb-finite-difference",
        &train_v0(&SECOND_PREFIXES, SECOND_EXPECTED),
        TRAINED_ADAPTER_FORMAT_VERSION,
        half(),
    );

    let error = aggregate_exact(FloraRequest {
        output_name: "mixed-round".into(),
        rank_budget: 8,
        contributions: vec![v1.clone(), v0.clone()],
    })
    .expect_err("a round must not mix trainer versions")
    .to_string();
    assert!(
        error.contains("adapter_format_version"),
        "{error} must name the field that disagreed"
    );

    // The refusal is a property of the round, not of arrival order.
    let reversed = aggregate_exact(FloraRequest {
        output_name: "mixed-round".into(),
        rank_budget: 8,
        contributions: vec![v0.clone(), v1.clone()],
    })
    .expect_err("the same round, the other way round, is still refused")
    .to_string();
    assert!(reversed.contains("adapter_format_version"), "{reversed}");

    // And the PEFT stamp inside the bytes disagrees too, so the manifest
    // field is not the only thing standing between a v0 and a v1 in one
    // round: forcing the manifests to agree still leaves the config check.
    let mut forced_v0 = v0;
    forced_v0
        .manifest
        .adapter_format_version
        .clone_from(&v1.manifest.adapter_format_version);
    forced_v0.manifest_ref = ManifestId::of_blob(&serde_json::to_vec(&forced_v0.manifest).unwrap());
    let error = aggregate_exact(FloraRequest {
        output_name: "mixed-round".into(),
        rank_budget: 8,
        contributions: vec![v1, forced_v0],
    })
    .expect_err("the PEFT version inside the bytes must refuse the mix as well")
    .to_string();
    assert!(
        error.contains("PEFT version"),
        "{error} must name the PEFT version disagreement"
    );
}
