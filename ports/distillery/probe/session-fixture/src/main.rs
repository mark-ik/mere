use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use eidetic::models::OpaqueBlob;
use eidetic::typed::{load_typed, save_typed};
use eidetic::{
    AdapterRuntimeCompat, Hash, ModelAdapterManifest, ModelLibrary, NoFetcher, PrivacyClass,
    ProvenanceRecord, Timestamp, TrustEnvelope,
};
use esp::infer::decoder::{DecoderProvider, PEFT_LORA_NDARRAY_LOADER, PeftLoraAdapterLoader};
use esp::infer::{
    AdapterArtifact, AdapterLoader, AdapterSelection, GenerationRequest, ModelSession,
};
use muniment::MemoryBackend;
use ndarray::ArrayView2;
use safetensors::SafeTensors;
use safetensors::tensor::{Dtype, TensorView};
use serde::Serialize;
use sha2::{Digest, Sha256};

const MODEL_ID: &str = "HuggingFaceTB/SmolLM2-135M-Instruct";
const BASE_REVISION: &str = "12fd25f77366fa6b3b4b768ec3050bf629380bac";
const ADAPTER_ID: &str = "spellicer/SmolLM2-135M-Instruct-contradiction";
const ADAPTER_REVISION: &str = "919fa899e7635df123b68eb9c266c0d98757d954";
const ADAPTER_LICENSE: &str = "Apache-2.0";
const PROMPT: &str = "<|im_start|>system\nYou are a helpful AI assistant named SmolLM, trained by Hugging Face<|im_end|>\n<|im_start|>user\nExplain whether these claims contradict each other: the model is local; the request crosses a network.<|im_end|>\n<|im_start|>assistant\n";

#[derive(Serialize)]
struct ArtifactReceipt {
    id: &'static str,
    revision: &'static str,
    license: &'static str,
    bytes: usize,
    sha256: String,
}

#[derive(Serialize)]
struct PublishedCheckpointAudit {
    target_tensor_count: usize,
    published_vs_base_target_max_abs_difference: f32,
    expected_lora_target_max_abs_delta: f32,
    contains_adapter: bool,
}

#[derive(Serialize)]
struct NumericalReceipt {
    dimensions: usize,
    all_finite: bool,
    adapted_vs_independent_merge_max_abs_error: f32,
    base_vs_adapted_max_abs_difference: f32,
    independent_merge_tolerance: f32,
    adapted_matches_independent_tokens: bool,
    base_token_ids: Vec<u32>,
    adapted_token_ids: Vec<u32>,
    independent_merge_token_ids: Vec<u32>,
    adapted_text: String,
    independent_merge_text: String,
}

#[derive(Serialize)]
struct TimingReceipt {
    eidetic_save_resolve_ms: f64,
    base_load_and_execute_ms: f64,
    adapted_load_and_execute_ms: f64,
    independent_merge_and_execute_ms: f64,
}

#[derive(Serialize)]
struct Receipt {
    schema: &'static str,
    source_commit: String,
    owned_paths_dirty: bool,
    passes: bool,
    model: ArtifactReceipt,
    adapter: ArtifactReceipt,
    adapter_config: ArtifactReceipt,
    published_checkpoint: ArtifactReceipt,
    published_checkpoint_audit: PublishedCheckpointAudit,
    prompt_template_sha256: String,
    session_id: String,
    base_model_manifest: String,
    adapter_manifest: String,
    ordered_adapter_count: usize,
    mismatch_rejected_before_execution: bool,
    eidetic_round_trip: bool,
    numerical: NumericalReceipt,
    timings_ms: TimingReceipt,
}

fn read(path: &Path, name: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    fs::read(path.join(name)).map_err(|error| format!("read {name}: {error}").into())
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn max_abs(a: &[f32], b: &[f32]) -> Result<f32, Box<dyn Error>> {
    if a.len() != b.len() {
        return Err(format!("vector length mismatch: {} != {}", a.len(), b.len()).into());
    }
    Ok(a.iter()
        .zip(b)
        .map(|(left, right)| (left - right).abs())
        .fold(0.0_f32, f32::max))
}

fn decode(view: &TensorView<'_>) -> Result<Vec<f32>, Box<dyn Error>> {
    let values = match view.dtype() {
        Dtype::F32 => view
            .data()
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect(),
        Dtype::BF16 => view
            .data()
            .chunks_exact(2)
            .map(|bytes| {
                f32::from_bits(u32::from(u16::from_le_bytes(bytes.try_into().unwrap())) << 16)
            })
            .collect(),
        Dtype::F16 => view
            .data()
            .chunks_exact(2)
            .map(|bytes| half::f16::from_le_bytes(bytes.try_into().unwrap()).to_f32())
            .collect(),
        dtype => return Err(format!("reference merger does not support {dtype:?}").into()),
    };
    Ok(values)
}

fn adapter_targets(adapter: &SafeTensors<'_>) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut targets = BTreeMap::new();
    for name in adapter.names() {
        let Some(prefix) = name.strip_suffix(".lora_A.weight") else {
            continue;
        };
        let base_prefix = prefix
            .strip_prefix("base_model.model.")
            .ok_or_else(|| format!("unexpected PEFT tensor prefix: {name}"))?;
        let base_name = format!("{base_prefix}.weight");
        let b_name = format!("{prefix}.lora_B.weight");
        adapter.tensor(&b_name)?;
        targets.insert(base_name, prefix.to_owned());
    }
    if targets.is_empty() {
        return Err("adapter contains no LoRA A/B tensor pairs".into());
    }
    if adapter.names().len() != targets.len() * 2 {
        return Err("adapter contains tensors outside the strict LoRA A/B subset".into());
    }
    Ok(targets)
}

fn merge_target(
    base: &TensorView<'_>,
    adapter: &SafeTensors<'_>,
    adapter_prefix: &str,
    scale: f32,
) -> Result<(Vec<f32>, f32), Box<dyn Error>> {
    let a = adapter.tensor(&format!("{adapter_prefix}.lora_A.weight"))?;
    let b = adapter.tensor(&format!("{adapter_prefix}.lora_B.weight"))?;
    let [out, input]: [usize; 2] = base
        .shape()
        .try_into()
        .map_err(|_| "base target is not a matrix")?;
    let [rank, a_input]: [usize; 2] = a.shape().try_into().map_err(|_| "LoRA A is not a matrix")?;
    let [b_out, b_rank]: [usize; 2] = b.shape().try_into().map_err(|_| "LoRA B is not a matrix")?;
    if a_input != input || b_out != out || b_rank != rank {
        return Err(format!(
            "LoRA shape mismatch: base={:?}, A={:?}, B={:?}",
            base.shape(),
            a.shape(),
            b.shape()
        )
        .into());
    }
    let a_values = decode(&a)?;
    let b_values = decode(&b)?;
    let delta = ArrayView2::from_shape((out, rank), &b_values)?
        .dot(&ArrayView2::from_shape((rank, input), &a_values)?)
        * scale;
    let delta_max = delta.iter().map(|value| value.abs()).fold(0.0, f32::max);
    let mut merged = decode(base)?;
    for (value, delta) in merged.iter_mut().zip(delta.iter()) {
        *value += *delta;
    }
    Ok((merged, delta_max))
}

fn independent_peft_merge(
    base_bytes: &[u8],
    adapter_bytes: &[u8],
    scale: f32,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let base = SafeTensors::deserialize(base_bytes)?;
    let adapter = SafeTensors::deserialize(adapter_bytes)?;
    let targets = adapter_targets(&adapter)?;
    let mut names: Vec<String> = base.names().into_iter().cloned().collect();
    names.sort();

    let mut header = serde_json::Map::new();
    let mut offset = 0usize;
    for name in &names {
        let view = base.tensor(name)?;
        let modified = targets.contains_key(name);
        let byte_len = if modified {
            view.shape().iter().product::<usize>() * 4
        } else {
            view.data().len()
        };
        header.insert(
            name.clone(),
            serde_json::json!({
                "dtype": if modified { "F32".to_owned() } else { format!("{:?}", view.dtype()) },
                "shape": view.shape(),
                "data_offsets": [offset, offset + byte_len]
            }),
        );
        offset += byte_len;
    }
    header.insert(
        "__metadata__".into(),
        serde_json::json!({"format": "pt", "reference": "independent-cpu-peft-merge"}),
    );
    let header_bytes = serde_json::to_vec(&header)?;
    let padded_header_len = (header_bytes.len() + 7) & !7;
    let mut output = Vec::with_capacity(8 + padded_header_len + offset);
    output.extend_from_slice(&(padded_header_len as u64).to_le_bytes());
    output.extend_from_slice(&header_bytes);
    output.resize(8 + padded_header_len, b' ');

    for name in &names {
        let view = base.tensor(name)?;
        if let Some(prefix) = targets.get(name) {
            let (merged, _) = merge_target(&view, &adapter, prefix, scale)?;
            for value in merged {
                output.extend_from_slice(&value.to_le_bytes());
            }
        } else {
            output.extend_from_slice(view.data());
        }
    }
    SafeTensors::deserialize(&output)?;
    Ok(output)
}

fn audit_published_checkpoint(
    base_bytes: &[u8],
    adapter_bytes: &[u8],
    published_bytes: &[u8],
    scale: f32,
) -> Result<PublishedCheckpointAudit, Box<dyn Error>> {
    let base = SafeTensors::deserialize(base_bytes)?;
    let adapter = SafeTensors::deserialize(adapter_bytes)?;
    let published = SafeTensors::deserialize(published_bytes)?;
    let targets = adapter_targets(&adapter)?;
    let mut published_difference = 0.0_f32;
    let mut expected_delta = 0.0_f32;
    for (name, prefix) in &targets {
        let base_view = base.tensor(name)?;
        let published_view = published.tensor(name)?;
        published_difference =
            published_difference.max(max_abs(&decode(&base_view)?, &decode(&published_view)?)?);
        let (_, delta_max) = merge_target(&base_view, &adapter, prefix, scale)?;
        expected_delta = expected_delta.max(delta_max);
    }
    Ok(PublishedCheckpointAudit {
        target_tensor_count: targets.len(),
        published_vs_base_target_max_abs_difference: published_difference,
        expected_lora_target_max_abs_delta: expected_delta,
        contains_adapter: published_difference >= expected_delta * 0.5,
    })
}

fn generate(provider: &DecoderProvider) -> Result<(String, Vec<u32>), Box<dyn Error>> {
    let request = GenerationRequest {
        prompt: PROMPT.into(),
        max_tokens: 12,
        ..Default::default()
    };
    let generation = provider.generate_streaming_observed(
        &request,
        &mut |_| std::ops::ControlFlow::Continue(()),
        &mut |_| {},
    )?;
    Ok((generation.text, generation.token_ids))
}

fn main() -> Result<(), Box<dyn Error>> {
    let model_dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: distillery-model-session-fixture <model-dir>")?;
    let config_bytes = read(&model_dir, "config.json")?;
    let tokenizer_bytes = read(&model_dir, "tokenizer.json")?;
    let base_weights = read(&model_dir, "model.safetensors")?;
    let adapter_config_bytes = read(&model_dir, "adapter_config.json")?;
    let adapter_weights = read(&model_dir, "adapter_model.safetensors")?;
    let prompt_template = read(&model_dir, "chat_template.jinja")?;

    let eidetic_started = Instant::now();
    let mut store = MemoryBackend::default();
    let base_manifest = pollster::block_on(ModelLibrary::save_model_with_components(
        &mut store,
        MODEL_ID,
        "llama",
        "Apache-2.0",
        serde_json::from_slice(&config_bytes)?,
        &base_weights,
        &tokenizer_bytes,
        vec![],
        vec![],
        PrivacyClass::LocalOnly,
        ProvenanceRecord::self_imported(format!("hf://{MODEL_ID}@{BASE_REVISION}")),
        TrustEnvelope::self_asserted(),
        Timestamp::ZERO,
    ))?;
    let resolved_model = pollster::block_on(ModelLibrary::resolve_components(
        &mut store,
        &mut NoFetcher,
        base_manifest,
    ))?
    .ok_or("saved base model did not resolve")?;

    let adapter_config_blob = pollster::block_on(save_typed(
        &mut store,
        &OpaqueBlob(adapter_config_bytes.clone()),
        vec![],
        PrivacyClass::LocalOnly,
        ProvenanceRecord::self_imported(format!("hf://{ADAPTER_ID}@{ADAPTER_REVISION}")),
        TrustEnvelope::self_asserted(),
        Timestamp::ZERO,
    ))?;
    let adapter_blob = pollster::block_on(save_typed(
        &mut store,
        &OpaqueBlob(adapter_weights.clone()),
        vec![],
        PrivacyClass::LocalOnly,
        ProvenanceRecord::self_imported(format!("hf://{ADAPTER_ID}@{ADAPTER_REVISION}")),
        TrustEnvelope::self_asserted(),
        Timestamp::ZERO,
    ))?;
    let adapter_manifest_value = ModelAdapterManifest {
        name: ADAPTER_ID.into(),
        base_model_ref: base_manifest,
        adapter_blob,
        adapter_config_blob,
        adapter_format: "peft-lora".into(),
        adapter_format_version: "peft-0.19.1".into(),
        runtime_compat: AdapterRuntimeCompat {
            minimum_capabilities: vec!["peft-lora".into(), "llama-attention-projections".into()],
            known_loaders: vec![PEFT_LORA_NDARRAY_LOADER.into()],
            converter_lineage: vec![],
        },
        rank: 8,
        alpha: 32.0,
        target_modules: vec![
            "k_proj".into(),
            "o_proj".into(),
            "q_proj".into(),
            "v_proj".into(),
        ],
        tokenizer_ref: resolved_model.manifest.tokenizer_blob,
        prompt_template_hash: Hash::of(&prompt_template),
        quantization_assumption: None,
        training_corpus_root: None,
        training_method: serde_json::json!({
            "source": "external PEFT SFT artifact",
            "source_revision": ADAPTER_REVISION,
            "training_corpus": "not published as an Eidetic engram"
        }),
        eval_results: None,
    };
    adapter_manifest_value.validate()?;
    let adapter_manifest = pollster::block_on(save_typed(
        &mut store,
        &adapter_manifest_value,
        vec![],
        PrivacyClass::LocalOnly,
        ProvenanceRecord::self_imported(format!("hf://{ADAPTER_ID}@{ADAPTER_REVISION}")),
        TrustEnvelope::self_asserted(),
        Timestamp::ZERO,
    ))?;
    let loaded_manifest = pollster::block_on(load_typed::<ModelAdapterManifest>(
        &mut store,
        &mut NoFetcher,
        adapter_manifest,
    ))?
    .ok_or("saved adapter manifest did not resolve")?;
    let loaded_config = pollster::block_on(load_typed::<OpaqueBlob>(
        &mut store,
        &mut NoFetcher,
        adapter_config_blob,
    ))?
    .ok_or("saved adapter config did not resolve")?;
    let loaded_adapter = pollster::block_on(load_typed::<OpaqueBlob>(
        &mut store,
        &mut NoFetcher,
        adapter_blob,
    ))?
    .ok_or("saved adapter weights did not resolve")?;
    let eidetic_round_trip = loaded_config.0 == adapter_config_bytes
        && loaded_adapter.0 == adapter_weights
        && resolved_model.components.weight_bytes == base_weights
        && resolved_model.components.tokenizer_bytes == tokenizer_bytes;
    if !eidetic_round_trip {
        return Err("Eidetic model or adapter round trip changed bytes".into());
    }
    let eidetic_ms = eidetic_started.elapsed().as_secs_f64() * 1000.0;
    drop(store);
    drop(base_weights);

    let device = Default::default();
    let base_started = Instant::now();
    let base_provider = DecoderProvider::from_bytes(
        &resolved_model.components.config_bytes,
        &resolved_model.components.tokenizer_bytes,
        &resolved_model.components.weight_bytes,
        MODEL_ID,
        "burn-ndarray/base-v1",
        &device,
    )?;
    let base_logits = base_provider.next_token_logits(PROMPT)?;
    let (_, base_token_ids) = generate(&base_provider)?;
    let base_ms = base_started.elapsed().as_secs_f64() * 1000.0;
    drop(base_provider);

    let session = ModelSession {
        base_model_ref: base_manifest,
        model_id: MODEL_ID.into(),
        tokenizer_ref: resolved_model.manifest.tokenizer_blob,
        prompt_template_hash: Hash::of(&prompt_template),
        quantization: None,
        loader: PEFT_LORA_NDARRAY_LOADER.into(),
        adapters: vec![AdapterSelection {
            manifest_ref: adapter_manifest,
            scale: 1.0,
        }],
    };
    let artifact = AdapterArtifact {
        manifest_ref: adapter_manifest,
        manifest: &loaded_manifest,
        config_bytes: &loaded_config.0,
        weight_bytes: &loaded_adapter.0,
    };
    let loader = PeftLoraAdapterLoader::new(
        base_manifest,
        resolved_model.manifest.tokenizer_blob,
        &resolved_model.components.config_bytes,
        &resolved_model.components.tokenizer_bytes,
        &resolved_model.components.weight_bytes,
        MODEL_ID,
        PEFT_LORA_NDARRAY_LOADER,
        &device,
    );
    let mut wrong_session = session.clone();
    wrong_session.prompt_template_hash = Hash::of(b"wrong template");
    let mismatch_rejected = loader
        .load_session(wrong_session, std::slice::from_ref(&artifact))
        .is_err();
    if !mismatch_rejected {
        return Err("mismatched session was accepted".into());
    }

    let adapted_started = Instant::now();
    let bound = loader.load_session(session, std::slice::from_ref(&artifact))?;
    let session_id = bound.session_id();
    let prepared = bound.prepare(
        GenerationRequest {
            prompt: PROMPT.into(),
            max_tokens: 12,
            ..Default::default()
        },
        &prompt_template,
    )?;
    let prepared_text = bound.generate_prepared(&prepared)?;
    let adapted_logits = bound.provider().next_token_logits(PROMPT)?;
    let (adapted_text, adapted_token_ids) = generate(bound.provider())?;
    if prepared_text != adapted_text {
        return Err("prepared request and direct observation diverged".into());
    }
    let adapted_ms = adapted_started.elapsed().as_secs_f64() * 1000.0;
    drop(bound);
    drop(loader);

    let published_weights = read(&model_dir, "merged_model.safetensors")?;
    let published_checkpoint = ArtifactReceipt {
        id: "spellicer/SmolLM2-135M-Instruct-contradiction/merged_model.safetensors",
        revision: ADAPTER_REVISION,
        license: ADAPTER_LICENSE,
        bytes: published_weights.len(),
        sha256: sha256(&published_weights),
    };
    let published_checkpoint_audit = audit_published_checkpoint(
        &resolved_model.components.weight_bytes,
        &loaded_adapter.0,
        &published_weights,
        4.0,
    )?;
    if published_checkpoint_audit.contains_adapter {
        return Err("published checkpoint unexpectedly passed the missing-adapter audit".into());
    }
    drop(published_weights);

    let independent_started = Instant::now();
    let independent_weights = independent_peft_merge(
        &resolved_model.components.weight_bytes,
        &loaded_adapter.0,
        4.0,
    )?;
    let independent_provider = DecoderProvider::from_bytes(
        &resolved_model.components.config_bytes,
        &resolved_model.components.tokenizer_bytes,
        &independent_weights,
        MODEL_ID,
        "burn-ndarray/independent-peft-merge-reference",
        &device,
    )?;
    let independent_logits = independent_provider.next_token_logits(PROMPT)?;
    let (independent_merge_text, independent_merge_token_ids) = generate(&independent_provider)?;
    let independent_ms = independent_started.elapsed().as_secs_f64() * 1000.0;

    let independent_error = max_abs(&adapted_logits, &independent_logits)?;
    let base_difference = max_abs(&base_logits, &adapted_logits)?;
    let independent_tolerance = 2.0e-3_f32;
    let all_finite = adapted_logits.iter().all(|value| value.is_finite())
        && independent_logits.iter().all(|value| value.is_finite());
    let tokens_match = adapted_token_ids == independent_merge_token_ids;
    let passes = mismatch_rejected
        && eidetic_round_trip
        && all_finite
        && independent_error <= independent_tolerance
        && base_difference > 1.0e-5
        && tokens_match;
    if !passes {
        return Err(format!(
            "forcing receipt failed: independent_error={independent_error}, base_difference={base_difference}, tokens_match={tokens_match}"
        )
        .into());
    }

    let receipt = Receipt {
        schema: "distillery.model-session-peft-lora/v1",
        source_commit: std::env::var("ESP_MODEL_SESSION_COMMIT").unwrap_or_default(),
        owned_paths_dirty: std::env::var("ESP_MODEL_SESSION_DIRTY").as_deref() == Ok("true"),
        passes,
        model: ArtifactReceipt {
            id: MODEL_ID,
            revision: BASE_REVISION,
            license: "Apache-2.0",
            bytes: resolved_model.components.weight_bytes.len(),
            sha256: sha256(&resolved_model.components.weight_bytes),
        },
        adapter: ArtifactReceipt {
            id: ADAPTER_ID,
            revision: ADAPTER_REVISION,
            license: ADAPTER_LICENSE,
            bytes: loaded_adapter.0.len(),
            sha256: sha256(&loaded_adapter.0),
        },
        adapter_config: ArtifactReceipt {
            id: ADAPTER_ID,
            revision: ADAPTER_REVISION,
            license: ADAPTER_LICENSE,
            bytes: loaded_config.0.len(),
            sha256: sha256(&loaded_config.0),
        },
        published_checkpoint,
        published_checkpoint_audit,
        prompt_template_sha256: sha256(&prompt_template),
        session_id: session_id.to_string(),
        base_model_manifest: base_manifest.to_string(),
        adapter_manifest: adapter_manifest.to_string(),
        ordered_adapter_count: 1,
        mismatch_rejected_before_execution: mismatch_rejected,
        eidetic_round_trip,
        numerical: NumericalReceipt {
            dimensions: adapted_logits.len(),
            all_finite,
            adapted_vs_independent_merge_max_abs_error: independent_error,
            base_vs_adapted_max_abs_difference: base_difference,
            independent_merge_tolerance: independent_tolerance,
            adapted_matches_independent_tokens: tokens_match,
            base_token_ids,
            adapted_token_ids,
            independent_merge_token_ids,
            adapted_text,
            independent_merge_text,
        },
        timings_ms: TimingReceipt {
            eidetic_save_resolve_ms: eidetic_ms,
            base_load_and_execute_ms: base_ms,
            adapted_load_and_execute_ms: adapted_ms,
            independent_merge_and_execute_ms: independent_ms,
        },
    };
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}
