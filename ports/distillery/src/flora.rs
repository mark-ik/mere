// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Exact federated-LoRA (FLoRA) factor stacking.
//!
//! This module is a pure artifact transformer. Gemot owns round membership,
//! weights, governance, and durable receipts; Eidetic owns persistence. The
//! caller supplies stable contribution ids. Aggregation canonicalizes them
//! lexicographically, so every peer with the same governed set emits identical
//! bytes regardless of arrival order. The result is an ordinary PEFT artifact
//! plus a deterministic description suitable for an Eidetic record.
//!
//! For a participant `i` with PEFT factors `A_i [r_i, in]`, `B_i [out, r_i]`,
//! contribution weight `w_i`, and PEFT scale `alpha_i / r_i`, the emitted
//! factors are:
//!
//! ```text
//! A* = vertical_concat(A_1, ..., A_n)
//! B* = horizontal_concat(s_1 B_1, ..., s_n B_n)
//! s_i = w_i * alpha_i / r_i
//! ```
//!
//! The aggregate sets PEFT `r = R = sum(r_i)` and `lora_alpha = R`. ESP's
//! ordinary loader therefore applies `R / R == 1` and does not scale the
//! already-scaled `B*` a second time. This is exact factor stacking, not a
//! rank-reduction, merge, or approximation.
//!
//! Adapter tensors can encode training information. This module has no privacy
//! authority and cannot improve a policy: the caller must persist or share the
//! aggregate under a policy at least as strict as the strictest input policy.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use eidetic::{AdapterRuntimeCompat, ManifestId, ModelAdapterManifest};
use esp::infer::decoder::PEFT_LORA_NDARRAY_LOADER;
use safetensors::SafeTensors;
use safetensors::tensor::{Dtype, TensorView};
use serde::{Deserialize, Serialize};

/// The stable description of exact factor stacking.
pub const FLORA_METHOD: &str = "flora-exact-factor-stacking/v1";
/// Schema name callers may store with a [`FloraReceipt`].
pub const FLORA_RECEIPT_SCHEMA: &str = "distillery.flora/v1";

/// Exact rational contribution weight from a governed FLoRA round.
///
/// Distillery converts this ratio to the adapter tensor dtype, combines it with
/// the source adapter's `alpha / rank`, and applies that effective scale to `B`
/// exactly once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FloraWeight {
    /// Positive numerator of the governed round weight.
    pub numerator: u32,
    /// Positive denominator of the governed round weight.
    pub denominator: u32,
}

/// One adapter accepted into a caller-governed FLoRA contribution order.
#[derive(Clone, Debug)]
pub struct FloraContribution {
    /// Stable governed participant id; it must be unique within a request and
    /// its lexical order is the canonical factor order.
    pub contribution_id: String,
    /// Content id of the source [`ModelAdapterManifest`].
    pub manifest_ref: ManifestId,
    /// Source adapter envelope.
    pub manifest: ModelAdapterManifest,
    /// Exact source `adapter_config.json` bytes.
    pub adapter_config_json: Vec<u8>,
    /// Exact source `adapter_model.safetensors` bytes.
    pub adapter_safetensors: Vec<u8>,
    /// Caller-governed positive rational round weight.
    pub weight: FloraWeight,
}

/// The complete exact aggregation request.
#[derive(Clone, Debug)]
pub struct FloraRequest {
    /// Human-readable name for the emitted adapter manifest.
    pub output_name: String,
    /// Hard cap on `sum(r_i)`; exact aggregation rejects rather than compresses.
    pub rank_budget: u16,
    /// Contributions; aggregation canonicalizes them by `contribution_id`.
    pub contributions: Vec<FloraContribution>,
}

/// PEFT adapter bytes and their manifest, ready for caller-owned Eidetic storage.
#[derive(Clone, Debug)]
pub struct FloraAggregate {
    /// The emitted PEFT LoRA manifest with content ids for the sibling bytes.
    pub manifest: ModelAdapterManifest,
    /// Emitted canonical `adapter_config.json` bytes.
    pub adapter_config_json: Vec<u8>,
    /// Emitted F32 `adapter_model.safetensors` bytes.
    pub adapter_safetensors: Vec<u8>,
    /// Deterministic aggregation/provenance description; the caller persists it.
    pub receipt: FloraReceipt,
}

/// Deterministic, caller-persistable provenance for one FLoRA output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FloraReceipt {
    /// Versioned receipt schema name.
    pub schema: String,
    /// Exact aggregation method.
    pub method: String,
    /// The factor receiving each participant's effective scale.
    pub scaled_factor: String,
    /// Canonical lexicographic contribution-id order used for the output bytes.
    pub contribution_order: Vec<FloraContributionReceipt>,
    /// Caller-provided upper bound that guarded aggregation.
    pub rank_budget: u16,
    /// Emitted aggregate PEFT rank.
    pub aggregate_rank: u16,
    /// Emitted PEFT alpha; equal to `aggregate_rank` so ESP applies unit scale.
    pub aggregate_alpha: f32,
    /// Caller-boundary privacy warning for the derived adapter bytes.
    pub privacy_boundary: String,
}

/// One source's deterministic contribution facts in a [`FloraReceipt`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FloraContributionReceipt {
    /// Stable source position identifier supplied by the caller.
    pub contribution_id: String,
    /// Content id of the source adapter manifest.
    pub manifest_ref: ManifestId,
    /// PEFT rank carried by the source adapter.
    pub rank: u16,
    /// Exact numerator of the caller's accepted round weight.
    pub weight_numerator: u32,
    /// Exact denominator of the caller's accepted round weight.
    pub weight_denominator: u32,
    /// IEEE-754 bits of the source PEFT alpha.
    pub source_alpha_bits: u32,
    /// IEEE-754 bits of `weight * alpha / rank`, applied only to `B`.
    pub effective_scale_bits: u32,
}

/// An exact aggregation input is malformed, incompatible, or over budget.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FloraError {
    /// The source violates the strict FLoRA/ESP PEFT contract.
    #[error("invalid FLoRA aggregation input: {0}")]
    Invalid(String),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PeftLoraConfig {
    base_model_name_or_path: String,
    peft_type: String,
    peft_version: String,
    r: usize,
    lora_alpha: f32,
    target_modules: Vec<String>,
    bias: String,
    #[serde(default)]
    fan_in_fan_out: bool,
    #[serde(default)]
    use_dora: bool,
    #[serde(default)]
    use_rslora: bool,
    #[serde(default)]
    use_qalora: bool,
    #[serde(default)]
    rank_pattern: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    alpha_pattern: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    modules_to_save: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TensorKey {
    layer: usize,
    module: String,
}

#[derive(Clone, Copy, Debug)]
enum Factor {
    A,
    B,
}

#[derive(Clone, Debug)]
struct Matrix {
    rows: usize,
    columns: usize,
    values: Vec<f32>,
}

#[derive(Clone, Debug)]
struct FactorPair {
    a: Matrix,
    b: Matrix,
}

#[derive(Debug)]
struct ParsedContribution {
    config: PeftLoraConfig,
    pairs: BTreeMap<TensorKey, FactorPair>,
    effective_scale: f32,
}

fn invalid(message: impl Into<String>) -> FloraError {
    FloraError::Invalid(message.into())
}

/// Aggregate ordinary ESP-compatible PEFT LoRA adapters exactly.
///
/// Contributions are ordered canonically by their stable ids. The function
/// rejects every source it cannot prove compatible, including a rank-budget
/// overflow, rather than substituting a lossy merge or best-effort conversion.
pub fn aggregate_exact(mut request: FloraRequest) -> Result<FloraAggregate, FloraError> {
    if request.output_name.trim().is_empty() {
        return Err(invalid("output_name is empty"));
    }
    if request.rank_budget == 0 {
        return Err(invalid("rank_budget must be greater than zero"));
    }
    if request.contributions.is_empty() {
        return Err(invalid("at least one contribution is required"));
    }

    request
        .contributions
        .sort_by(|left, right| left.contribution_id.cmp(&right.contribution_id));

    let mut ids = HashSet::with_capacity(request.contributions.len());
    let mut manifests = HashSet::with_capacity(request.contributions.len());
    let mut total_rank = 0u16;
    let mut parsed = Vec::with_capacity(request.contributions.len());

    for contribution in &request.contributions {
        if contribution.contribution_id.trim().is_empty() {
            return Err(invalid("contribution_id is empty"));
        }
        if !ids.insert(contribution.contribution_id.as_str()) {
            return Err(invalid(format!(
                "duplicate contribution_id {:?}",
                contribution.contribution_id
            )));
        }
        if !manifests.insert(contribution.manifest_ref) {
            return Err(invalid(format!(
                "duplicate source manifest {}",
                contribution.manifest_ref
            )));
        }
        if contribution.weight.numerator == 0 || contribution.weight.denominator == 0 {
            return Err(invalid(format!(
                "contribution {} has a zero weight numerator or denominator",
                contribution.contribution_id
            )));
        }
        contribution.manifest.validate().map_err(|error| {
            invalid(format!(
                "contribution {} manifest: {error}",
                contribution.contribution_id
            ))
        })?;
        let manifest_bytes = serde_json::to_vec(&contribution.manifest).map_err(|error| {
            invalid(format!(
                "contribution {} manifest serialization: {error}",
                contribution.contribution_id
            ))
        })?;
        if ManifestId::of_blob(&manifest_bytes) != contribution.manifest_ref {
            return Err(invalid(format!(
                "contribution {} manifest_ref does not match its manifest bytes",
                contribution.contribution_id
            )));
        }
        if ManifestId::of_blob(&contribution.adapter_config_json)
            != contribution.manifest.adapter_config_blob
        {
            return Err(invalid(format!(
                "contribution {} config bytes do not match its manifest",
                contribution.contribution_id
            )));
        }
        if ManifestId::of_blob(&contribution.adapter_safetensors)
            != contribution.manifest.adapter_blob
        {
            return Err(invalid(format!(
                "contribution {} safetensors bytes do not match its manifest",
                contribution.contribution_id
            )));
        }
        total_rank = total_rank
            .checked_add(contribution.manifest.rank)
            .ok_or_else(|| invalid("aggregate rank overflows u16"))?;
        if total_rank > request.rank_budget {
            return Err(invalid(format!(
                "aggregate rank {total_rank} exceeds rank budget {}",
                request.rank_budget
            )));
        }
        parsed.push(parse_contribution(contribution)?);
    }

    let first = &request.contributions[0];
    let first_parsed = &parsed[0];
    for (contribution, source) in request.contributions.iter().zip(&parsed).skip(1) {
        require_equal(
            "base_model_ref",
            &first.manifest.base_model_ref,
            &contribution.manifest.base_model_ref,
        )?;
        require_equal(
            "tokenizer_ref",
            &first.manifest.tokenizer_ref,
            &contribution.manifest.tokenizer_ref,
        )?;
        require_equal(
            "prompt_template_hash",
            &first.manifest.prompt_template_hash,
            &contribution.manifest.prompt_template_hash,
        )?;
        require_equal(
            "target_modules",
            &first.manifest.target_modules,
            &contribution.manifest.target_modules,
        )?;
        require_equal(
            "adapter_format",
            &first.manifest.adapter_format,
            &contribution.manifest.adapter_format,
        )?;
        require_equal(
            "adapter_format_version",
            &first.manifest.adapter_format_version,
            &contribution.manifest.adapter_format_version,
        )?;
        require_equal(
            "quantization_assumption",
            &first.manifest.quantization_assumption,
            &contribution.manifest.quantization_assumption,
        )?;
        require_equal(
            "runtime_compat",
            &first.manifest.runtime_compat,
            &contribution.manifest.runtime_compat,
        )?;
        require_equal(
            "PEFT base_model_name_or_path",
            &first_parsed.config.base_model_name_or_path,
            &source.config.base_model_name_or_path,
        )?;
        require_equal(
            "PEFT version",
            &first_parsed.config.peft_version,
            &source.config.peft_version,
        )?;
        require_equal(
            "PEFT target_modules",
            &first_parsed.config.target_modules,
            &source.config.target_modules,
        )?;
        if !first_parsed.pairs.keys().eq(source.pairs.keys()) {
            return Err(invalid("contributions disagree on LoRA tensor names"));
        }
        for key in first_parsed.pairs.keys() {
            let left = &first_parsed.pairs[key];
            let right = &source.pairs[key];
            if left.a.columns != right.a.columns || left.b.rows != right.b.rows {
                return Err(invalid(format!(
                    "contribution {} tensor {} layer {} has incompatible non-rank shape",
                    contribution.contribution_id, key.module, key.layer
                )));
            }
        }
    }

    let adapter_safetensors = stack_tensors(&parsed, total_rank)?;
    let aggregate_alpha = f32::from(total_rank);
    let adapter_config_json = serde_json::to_vec(&serde_json::json!({
        "base_model_name_or_path": first_parsed.config.base_model_name_or_path,
        "peft_type": "LORA",
        "peft_version": first_parsed.config.peft_version,
        "r": total_rank,
        "lora_alpha": aggregate_alpha,
        "target_modules": first.manifest.target_modules,
        "bias": "none",
    }))
    .map_err(|error| invalid(format!("serialize aggregate PEFT config: {error}")))?;

    let receipt = FloraReceipt {
        schema: FLORA_RECEIPT_SCHEMA.into(),
        method: FLORA_METHOD.into(),
        scaled_factor: "B".into(),
        contribution_order: request
            .contributions
            .iter()
            .zip(&parsed)
            .map(|(contribution, parsed)| FloraContributionReceipt {
                contribution_id: contribution.contribution_id.clone(),
                manifest_ref: contribution.manifest_ref,
                rank: contribution.manifest.rank,
                weight_numerator: contribution.weight.numerator,
                weight_denominator: contribution.weight.denominator,
                source_alpha_bits: contribution.manifest.alpha.to_bits(),
                effective_scale_bits: parsed.effective_scale.to_bits(),
            })
            .collect(),
        rank_budget: request.rank_budget,
        aggregate_rank: total_rank,
        aggregate_alpha,
        privacy_boundary: "Adapter output can expose training information; caller persistence and sharing policy must not be less strict than any input policy.".into(),
    };
    let source_refs = request
        .contributions
        .iter()
        .map(|contribution| contribution.manifest_ref)
        .collect::<Vec<_>>();
    let manifest = ModelAdapterManifest {
        name: request.output_name,
        base_model_ref: first.manifest.base_model_ref,
        adapter_blob: ManifestId::of_blob(&adapter_safetensors),
        adapter_config_blob: ManifestId::of_blob(&adapter_config_json),
        adapter_format: "peft-lora".into(),
        adapter_format_version: first.manifest.adapter_format_version.clone(),
        runtime_compat: AdapterRuntimeCompat {
            minimum_capabilities: first.manifest.runtime_compat.minimum_capabilities.clone(),
            known_loaders: first.manifest.runtime_compat.known_loaders.clone(),
            converter_lineage: source_refs,
        },
        rank: total_rank,
        alpha: aggregate_alpha,
        target_modules: first.manifest.target_modules.clone(),
        tokenizer_ref: first.manifest.tokenizer_ref,
        prompt_template_hash: first.manifest.prompt_template_hash,
        quantization_assumption: first.manifest.quantization_assumption.clone(),
        training_corpus_root: None,
        training_method: serde_json::json!({
            "method": FLORA_METHOD,
            "factor_scale": "B",
            "formula": "B* A* = sum_i (weight_i * alpha_i / rank_i) B_i A_i",
            "contribution_order": receipt.contribution_order,
            "rank_budget": request.rank_budget,
        }),
        eval_results: None,
    };
    manifest
        .validate()
        .map_err(|error| invalid(format!("emitted adapter manifest: {error}")))?;
    Ok(FloraAggregate {
        manifest,
        adapter_config_json,
        adapter_safetensors,
        receipt,
    })
}

fn require_equal<T: PartialEq + std::fmt::Debug>(
    field: &str,
    expected: &T,
    actual: &T,
) -> Result<(), FloraError> {
    if expected != actual {
        return Err(invalid(format!("contributions disagree on {field}")));
    }
    Ok(())
}

fn parse_contribution(contribution: &FloraContribution) -> Result<ParsedContribution, FloraError> {
    let manifest = &contribution.manifest;
    if manifest.adapter_format != "peft-lora" {
        return Err(invalid(format!(
            "contribution {} has unsupported adapter format {}",
            contribution.contribution_id, manifest.adapter_format
        )));
    }
    if !manifest
        .runtime_compat
        .known_loaders
        .iter()
        .any(|loader| loader == PEFT_LORA_NDARRAY_LOADER)
    {
        return Err(invalid(format!(
            "contribution {} was not verified for ESP loader {}",
            contribution.contribution_id, PEFT_LORA_NDARRAY_LOADER
        )));
    }
    if !manifest
        .runtime_compat
        .minimum_capabilities
        .iter()
        .any(|capability| capability == "peft-lora")
        || manifest
            .runtime_compat
            .minimum_capabilities
            .iter()
            .any(|capability| {
                capability != "peft-lora" && capability != "llama-attention-projections"
            })
    {
        return Err(invalid(format!(
            "contribution {} has unsupported ESP PEFT capabilities",
            contribution.contribution_id
        )));
    }

    let config: PeftLoraConfig = serde_json::from_slice(&contribution.adapter_config_json)
        .map_err(|error| {
            invalid(format!(
                "contribution {} PEFT config is outside the strict ESP subset: {error}",
                contribution.contribution_id
            ))
        })?;
    validate_config(contribution, &config)?;
    let round_weight =
        contribution.weight.numerator as f32 / contribution.weight.denominator as f32;
    let effective_scale = round_weight * manifest.alpha / f32::from(manifest.rank);
    if !effective_scale.is_finite() || effective_scale <= 0.0 {
        return Err(invalid(format!(
            "contribution {} effective scale is non-finite or non-positive",
            contribution.contribution_id
        )));
    }
    let pairs = parse_pairs(contribution, &config)?;
    Ok(ParsedContribution {
        config,
        pairs,
        effective_scale,
    })
}

fn validate_config(
    contribution: &FloraContribution,
    config: &PeftLoraConfig,
) -> Result<(), FloraError> {
    let manifest = &contribution.manifest;
    if config.base_model_name_or_path.trim().is_empty()
        || !config.peft_type.eq_ignore_ascii_case("lora")
        || config.peft_version.trim().is_empty()
        || config.r != usize::from(manifest.rank)
        || config.lora_alpha.to_bits() != manifest.alpha.to_bits()
        || config.target_modules != manifest.target_modules
        || config.bias != "none"
        || config.fan_in_fan_out
        || config.use_dora
        || config.use_rslora
        || config.use_qalora
        || !config.rank_pattern.is_empty()
        || !config.alpha_pattern.is_empty()
        || config.modules_to_save.is_some()
        || config
            .target_modules
            .iter()
            .any(|module| !matches!(module.as_str(), "q_proj" | "k_proj" | "v_proj" | "o_proj"))
    {
        return Err(invalid(format!(
            "contribution {} requires an unsupported or incompatible PEFT LoRA variant",
            contribution.contribution_id
        )));
    }
    let expected_version = format!("peft-{}", config.peft_version);
    if manifest.adapter_format_version != expected_version {
        return Err(invalid(format!(
            "contribution {} PEFT version does not match its manifest",
            contribution.contribution_id
        )));
    }
    Ok(())
}

fn parse_pairs(
    contribution: &FloraContribution,
    config: &PeftLoraConfig,
) -> Result<BTreeMap<TensorKey, FactorPair>, FloraError> {
    let tensors = SafeTensors::deserialize(&contribution.adapter_safetensors).map_err(|error| {
        invalid(format!(
            "contribution {} adapter safetensors: {error}",
            contribution.contribution_id
        ))
    })?;
    let mut partial: BTreeMap<TensorKey, (Option<Matrix>, Option<Matrix>)> = BTreeMap::new();
    for name in tensors.names() {
        let (key, factor) = parse_tensor_name(name, &config.target_modules).map_err(|error| {
            invalid(format!(
                "contribution {} tensor {name}: {error}",
                contribution.contribution_id
            ))
        })?;
        let matrix = f32_matrix(
            &tensors
                .tensor(name)
                .map_err(|error| invalid(format!("adapter tensor {name}: {error}")))?,
        )?;
        let slot = partial.entry(key).or_insert((None, None));
        let destination = match factor {
            Factor::A => &mut slot.0,
            Factor::B => &mut slot.1,
        };
        if destination.replace(matrix).is_some() {
            return Err(invalid(format!(
                "contribution {} has duplicate A/B tensor pair",
                contribution.contribution_id
            )));
        }
    }

    let mut pairs = BTreeMap::new();
    for (key, (a, b)) in partial {
        let a = a.ok_or_else(|| {
            invalid(format!(
                "contribution {} misses LoRA A for {} layer {}",
                contribution.contribution_id, key.module, key.layer
            ))
        })?;
        let b = b.ok_or_else(|| {
            invalid(format!(
                "contribution {} misses LoRA B for {} layer {}",
                contribution.contribution_id, key.module, key.layer
            ))
        })?;
        if a.rows != config.r || b.columns != config.r || a.columns == 0 || b.rows == 0 {
            return Err(invalid(format!(
                "contribution {} has malformed LoRA shapes for {} layer {}: A [{}, {}], B [{}, {}], rank {}",
                contribution.contribution_id,
                key.module,
                key.layer,
                a.rows,
                a.columns,
                b.rows,
                b.columns,
                config.r
            )));
        }
        pairs.insert(key, FactorPair { a, b });
    }
    validate_layer_grid(
        &pairs,
        &config.target_modules,
        &contribution.contribution_id,
    )?;
    if tensors.len() != pairs.len() * 2 {
        return Err(invalid(format!(
            "contribution {} has tensors outside the strict LoRA A/B subset",
            contribution.contribution_id
        )));
    }
    Ok(pairs)
}

fn parse_tensor_name(name: &str, targets: &[String]) -> Result<(TensorKey, Factor), &'static str> {
    let (without_suffix, factor) = if let Some(value) = name.strip_suffix(".lora_A.weight") {
        (value, Factor::A)
    } else if let Some(value) = name.strip_suffix(".lora_B.weight") {
        (value, Factor::B)
    } else {
        return Err("not a PEFT LoRA A/B tensor name");
    };
    let layer = without_suffix
        .strip_prefix("base_model.model.model.layers.")
        .ok_or("does not use ESP's base_model.model.model.layers prefix")?;
    let (number, module) = layer
        .split_once(".self_attn.")
        .ok_or("does not name an ESP llama attention projection")?;
    let layer = number
        .parse::<usize>()
        .map_err(|_| "layer number is not a non-negative integer")?;
    if layer.to_string() != number {
        return Err("layer number is not canonical");
    }
    if !targets.iter().any(|target| target == module) {
        return Err("module is absent from PEFT target_modules");
    }
    Ok((
        TensorKey {
            layer,
            module: module.to_owned(),
        },
        factor,
    ))
}

fn f32_matrix(view: &TensorView<'_>) -> Result<Matrix, FloraError> {
    if view.dtype() != Dtype::F32 {
        return Err(invalid(format!(
            "unsupported LoRA tensor dtype {:?}; exact FLoRA accepts only F32",
            view.dtype()
        )));
    }
    let [rows, columns]: [usize; 2] = view
        .shape()
        .try_into()
        .map_err(|_| invalid("LoRA tensors must be matrices"))?;
    let expected_len = rows
        .checked_mul(columns)
        .ok_or_else(|| invalid("LoRA tensor element count overflows usize"))?;
    let bytes = view.data();
    if bytes.len() != expected_len * std::mem::size_of::<f32>() {
        return Err(invalid("F32 tensor byte length disagrees with its shape"));
    }
    let values = bytes
        .chunks_exact(std::mem::size_of::<f32>())
        .map(|chunk| f32::from_le_bytes(chunk.try_into().expect("four-byte chunks")))
        .collect::<Vec<_>>();
    if values.iter().any(|value| !value.is_finite()) {
        return Err(invalid("LoRA tensor contains a non-finite F32 value"));
    }
    Ok(Matrix {
        rows,
        columns,
        values,
    })
}

fn validate_layer_grid(
    pairs: &BTreeMap<TensorKey, FactorPair>,
    targets: &[String],
    contribution_id: &str,
) -> Result<(), FloraError> {
    if pairs.is_empty() {
        return Err(invalid(format!(
            "contribution {contribution_id} contains no LoRA A/B pairs"
        )));
    }
    let layers = pairs.keys().map(|key| key.layer).collect::<BTreeSet<_>>();
    for (expected, actual) in layers.iter().enumerate() {
        if expected != *actual {
            return Err(invalid(format!(
                "contribution {contribution_id} has non-contiguous ESP layer indices"
            )));
        }
    }
    for layer in layers {
        for module in targets {
            let key = TensorKey {
                layer,
                module: module.clone(),
            };
            if !pairs.contains_key(&key) {
                return Err(invalid(format!(
                    "contribution {contribution_id} misses {module} in layer {layer}"
                )));
            }
        }
    }
    Ok(())
}

fn stack_tensors(sources: &[ParsedContribution], total_rank: u16) -> Result<Vec<u8>, FloraError> {
    let mut buffers = Vec::with_capacity(sources[0].pairs.len() * 2);
    for (key, first) in &sources[0].pairs {
        let mut a_values = Vec::with_capacity(usize::from(total_rank) * first.a.columns);
        for source in sources {
            a_values.extend_from_slice(&source.pairs[key].a.values);
        }
        let mut b_values = Vec::with_capacity(first.b.rows * usize::from(total_rank));
        for row in 0..first.b.rows {
            for source in sources {
                let pair = &source.pairs[key];
                let start = row * pair.b.columns;
                for value in &pair.b.values[start..start + pair.b.columns] {
                    let scaled = source.effective_scale * value;
                    if !scaled.is_finite() {
                        return Err(invalid(format!(
                            "scaled B tensor {} layer {} is non-finite",
                            key.module, key.layer
                        )));
                    }
                    b_values.push(scaled);
                }
            }
        }
        let prefix = format!(
            "base_model.model.model.layers.{}.self_attn.{}",
            key.layer, key.module
        );
        buffers.push((
            format!("{prefix}.lora_A.weight"),
            vec![usize::from(total_rank), first.a.columns],
            f32_bytes(&a_values),
        ));
        buffers.push((
            format!("{prefix}.lora_B.weight"),
            vec![first.b.rows, usize::from(total_rank)],
            f32_bytes(&b_values),
        ));
    }
    let views = buffers
        .iter()
        .map(|(name, shape, bytes)| {
            TensorView::new(Dtype::F32, shape.clone(), bytes)
                .map(|view| (name.as_str(), view))
                .map_err(|error| invalid(format!("serialize aggregate {name}: {error}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    safetensors::serialize(views, &None)
        .map_err(|error| invalid(format!("serialize aggregate safetensors: {error}")))
}

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use esp::infer::{AdapterArtifact, AdapterLoader, AdapterSelection, ModelSession};

    const MODEL_ID: &str = "fixture/flora";
    const TEMPLATE: &[u8] = b"{{ prompt }}";

    fn weight(numerator: u32, denominator: u32) -> FloraWeight {
        FloraWeight {
            numerator,
            denominator,
        }
    }

    fn config_bytes(rank: u16, alpha: f32) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "base_model_name_or_path": MODEL_ID,
            "peft_type": "LORA",
            "peft_version": "esp-trainer-v0",
            "r": rank,
            "lora_alpha": alpha,
            "target_modules": ["q_proj"],
            "bias": "none",
        }))
        .unwrap()
    }

    fn adapter_bytes(
        rank: u16,
        in_features: usize,
        out_features: usize,
        layers: usize,
        start: f32,
    ) -> Vec<u8> {
        let mut buffers = Vec::new();
        for layer in 0..layers {
            let prefix = format!("base_model.model.model.layers.{layer}.self_attn.q_proj");
            let a = (0..usize::from(rank) * in_features)
                .map(|index| start + index as f32 + layer as f32 * 100.0)
                .collect::<Vec<_>>();
            let b = (0..out_features * usize::from(rank))
                .map(|index| start + 20.0 + index as f32 + layer as f32 * 100.0)
                .collect::<Vec<_>>();
            buffers.push((
                format!("{prefix}.lora_A.weight"),
                vec![usize::from(rank), in_features],
                f32_bytes(&a),
            ));
            buffers.push((
                format!("{prefix}.lora_B.weight"),
                vec![out_features, usize::from(rank)],
                f32_bytes(&b),
            ));
        }
        let views = buffers
            .iter()
            .map(|(name, shape, bytes)| {
                (
                    name.as_str(),
                    TensorView::new(Dtype::F32, shape.clone(), bytes).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        safetensors::serialize(views, &None).unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn contribution(
        id: &str,
        rank: u16,
        alpha: f32,
        weight: FloraWeight,
        in_features: usize,
        out_features: usize,
        layers: usize,
        start: f32,
    ) -> FloraContribution {
        let config = config_bytes(rank, alpha);
        let weights = adapter_bytes(rank, in_features, out_features, layers, start);
        let manifest = ModelAdapterManifest {
            name: id.into(),
            base_model_ref: ManifestId::of_blob(b"base"),
            adapter_blob: ManifestId::of_blob(&weights),
            adapter_config_blob: ManifestId::of_blob(&config),
            adapter_format: "peft-lora".into(),
            adapter_format_version: "peft-esp-trainer-v0".into(),
            runtime_compat: AdapterRuntimeCompat {
                minimum_capabilities: vec!["peft-lora".into()],
                known_loaders: vec![PEFT_LORA_NDARRAY_LOADER.into()],
                converter_lineage: vec![],
            },
            rank,
            alpha,
            target_modules: vec!["q_proj".into()],
            tokenizer_ref: ManifestId::of_blob(b"tokenizer"),
            prompt_template_hash: eidetic::Hash::of(TEMPLATE),
            quantization_assumption: None,
            training_corpus_root: None,
            training_method: serde_json::Value::Null,
            eval_results: None,
        };
        let manifest_ref = ManifestId::of_blob(&serde_json::to_vec(&manifest).unwrap());
        FloraContribution {
            contribution_id: id.into(),
            manifest_ref,
            manifest,
            adapter_config_json: config,
            adapter_safetensors: weights,
            weight,
        }
    }

    fn aggregate(contributions: Vec<FloraContribution>) -> FloraAggregate {
        aggregate_exact(FloraRequest {
            output_name: "aggregate".into(),
            rank_budget: 16,
            contributions,
        })
        .unwrap()
    }

    fn matrix(bytes: &[u8], name: &str) -> Matrix {
        let tensors = SafeTensors::deserialize(bytes).unwrap();
        f32_matrix(&tensors.tensor(name).unwrap()).unwrap()
    }

    fn product(b: &Matrix, a: &Matrix) -> Vec<f32> {
        let mut result = vec![0.0; b.rows * a.columns];
        for row in 0..b.rows {
            for column in 0..a.columns {
                for rank in 0..a.rows {
                    result[row * a.columns + column] +=
                        b.values[row * b.columns + rank] * a.values[rank * a.columns + column];
                }
            }
        }
        result
    }

    #[test]
    fn heterogeneous_ranks_stack_exact_factors_and_apply_scale_only_to_b() {
        let first = contribution("first", 1, 2.0, weight(1, 2), 2, 3, 1, 1.0);
        let second = contribution("second", 2, 6.0, weight(1, 4), 2, 3, 1, 6.0);
        let aggregate = aggregate(vec![first, second]);
        let a = matrix(
            &aggregate.adapter_safetensors,
            "base_model.model.model.layers.0.self_attn.q_proj.lora_A.weight",
        );
        let b = matrix(
            &aggregate.adapter_safetensors,
            "base_model.model.model.layers.0.self_attn.q_proj.lora_B.weight",
        );
        assert_eq!([a.rows, a.columns], [3, 2]);
        assert_eq!([b.rows, b.columns], [3, 3]);
        assert_eq!(a.values, vec![1.0, 2.0, 6.0, 7.0, 8.0, 9.0]);
        // first scale = .5 * 2 / 1 = 1; second scale = .25 * 6 / 2 = .75.
        assert_eq!(
            b.values,
            vec![21.0, 19.5, 20.25, 22.0, 21.0, 21.75, 23.0, 22.5, 23.25]
        );
        assert_eq!(aggregate.manifest.rank, 3);
        assert_eq!(aggregate.manifest.alpha, 3.0);
        assert_eq!(aggregate.receipt.scaled_factor, "B");
    }

    #[test]
    fn stacked_product_equals_the_weighted_source_products() {
        let first = contribution("first", 1, 2.0, weight(1, 2), 2, 3, 1, 1.0);
        let second = contribution("second", 2, 6.0, weight(1, 4), 2, 3, 1, 6.0);
        let first_a = matrix(
            &first.adapter_safetensors,
            "base_model.model.model.layers.0.self_attn.q_proj.lora_A.weight",
        );
        let first_b = matrix(
            &first.adapter_safetensors,
            "base_model.model.model.layers.0.self_attn.q_proj.lora_B.weight",
        );
        let second_a = matrix(
            &second.adapter_safetensors,
            "base_model.model.model.layers.0.self_attn.q_proj.lora_A.weight",
        );
        let second_b = matrix(
            &second.adapter_safetensors,
            "base_model.model.model.layers.0.self_attn.q_proj.lora_B.weight",
        );
        let aggregate = aggregate(vec![first, second]);
        let a = matrix(
            &aggregate.adapter_safetensors,
            "base_model.model.model.layers.0.self_attn.q_proj.lora_A.weight",
        );
        let b = matrix(
            &aggregate.adapter_safetensors,
            "base_model.model.model.layers.0.self_attn.q_proj.lora_B.weight",
        );
        let actual = product(&b, &a);
        let expected = product(&first_b, &first_a)
            .into_iter()
            .zip(product(&second_b, &second_a))
            .map(|(left, right)| left * 1.0 + right * 0.75)
            .collect::<Vec<_>>();
        for (actual, expected) in actual.iter().zip(&expected) {
            assert!(
                (actual - expected).abs() <= 1.0e-5,
                "{actual} != {expected}"
            );
        }
    }

    #[test]
    fn contribution_arrival_order_cannot_change_output_bytes() {
        let first = contribution("first", 1, 2.0, weight(1, 2), 2, 3, 1, 1.0);
        let second = contribution("second", 2, 6.0, weight(1, 4), 2, 3, 1, 6.0);
        let once = aggregate(vec![first.clone(), second.clone()]);
        let twice = aggregate(vec![second, first]);
        assert_eq!(once.adapter_config_json, twice.adapter_config_json);
        assert_eq!(once.adapter_safetensors, twice.adapter_safetensors);
        assert_eq!(once.receipt, twice.receipt);
        assert_eq!(
            once.receipt
                .contribution_order
                .iter()
                .map(|entry| entry.contribution_id.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
    }

    #[test]
    fn rank_budget_and_compatibility_failures_are_rejected() {
        let first = contribution("first", 1, 2.0, weight(1, 2), 2, 3, 1, 1.0);
        let second = contribution("second", 2, 6.0, weight(1, 4), 2, 3, 1, 6.0);
        let err = aggregate_exact(FloraRequest {
            output_name: "aggregate".into(),
            rank_budget: 2,
            contributions: vec![first.clone(), second.clone()],
        })
        .unwrap_err();
        assert!(err.to_string().contains("rank"));

        let mut incompatible = second;
        incompatible.manifest.tokenizer_ref = ManifestId::of_blob(b"other tokenizer");
        incompatible.manifest_ref =
            ManifestId::of_blob(&serde_json::to_vec(&incompatible.manifest).unwrap());
        let err = aggregate_exact(FloraRequest {
            output_name: "aggregate".into(),
            rank_budget: 16,
            contributions: vec![first, incompatible],
        })
        .unwrap_err();
        assert!(err.to_string().contains("tokenizer_ref"));

        let mut invalid_weight = contribution("invalid-weight", 1, 2.0, weight(0, 1), 2, 3, 1, 1.0);
        invalid_weight.manifest_ref =
            ManifestId::of_blob(&serde_json::to_vec(&invalid_weight.manifest).unwrap());
        let err = aggregate_exact(FloraRequest {
            output_name: "aggregate".into(),
            rank_budget: 16,
            contributions: vec![invalid_weight],
        })
        .unwrap_err();
        assert!(err.to_string().contains("zero weight"));
    }

    #[test]
    fn malformed_pairs_and_non_f32_tensors_are_rejected() {
        let mut broken = contribution("broken", 1, 1.0, weight(1, 1), 2, 3, 1, 1.0);
        let bytes = vec![0u8; 8];
        let view = TensorView::new(Dtype::I64, vec![1, 1], &bytes).unwrap();
        broken.adapter_safetensors = safetensors::serialize(
            vec![(
                "base_model.model.model.layers.0.self_attn.q_proj.lora_A.weight",
                view,
            )],
            &None,
        )
        .unwrap();
        broken.manifest.adapter_blob = ManifestId::of_blob(&broken.adapter_safetensors);
        broken.manifest_ref = ManifestId::of_blob(&serde_json::to_vec(&broken.manifest).unwrap());
        let err = aggregate_exact(FloraRequest {
            output_name: "aggregate".into(),
            rank_budget: 2,
            contributions: vec![broken],
        })
        .unwrap_err();
        assert!(err.to_string().contains("unsupported LoRA tensor dtype"));
    }

    fn loader_config() -> &'static [u8] {
        br#"{
            "vocab_size": 4, "hidden_size": 4, "intermediate_size": 4,
            "num_hidden_layers": 1, "num_attention_heads": 2,
            "num_key_value_heads": 1, "max_position_embeddings": 8,
            "tie_word_embeddings": false
        }"#
    }

    fn loader_tokenizer() -> Vec<u8> {
        br#"{
            "version": "1.0",
            "pre_tokenizer": {"type": "Whitespace"},
            "model": {"type": "WordLevel", "vocab": {"t0": 0, "t1": 1, "t2": 2, "t3": 3}, "unk_token": "t0"}
        }"#
        .to_vec()
    }

    fn loader_base_weights() -> Vec<u8> {
        let mut buffers = Vec::new();
        let mut push = |name: &str, shape: Vec<usize>| {
            buffers.push((
                name.to_owned(),
                shape.clone(),
                vec![0.1_f32; shape.iter().product::<usize>()],
            ));
        };
        push("model.embed_tokens.weight", vec![4, 4]);
        push("model.layers.0.input_layernorm.weight", vec![4]);
        push("model.layers.0.self_attn.q_proj.weight", vec![4, 4]);
        push("model.layers.0.self_attn.k_proj.weight", vec![2, 4]);
        push("model.layers.0.self_attn.v_proj.weight", vec![2, 4]);
        push("model.layers.0.self_attn.o_proj.weight", vec![4, 4]);
        push("model.layers.0.post_attention_layernorm.weight", vec![4]);
        push("model.layers.0.mlp.gate_proj.weight", vec![4, 4]);
        push("model.layers.0.mlp.up_proj.weight", vec![4, 4]);
        push("model.layers.0.mlp.down_proj.weight", vec![4, 4]);
        push("model.norm.weight", vec![4]);
        push("lm_head.weight", vec![4, 4]);
        let bytes = buffers
            .iter()
            .map(|(name, shape, values)| (name.clone(), shape.clone(), f32_bytes(values)))
            .collect::<Vec<_>>();
        let views = bytes
            .iter()
            .map(|(name, shape, bytes)| {
                (
                    name.as_str(),
                    TensorView::new(Dtype::F32, shape.clone(), bytes).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        safetensors::serialize(views, &None).unwrap()
    }

    #[test]
    fn emitted_adapter_loads_through_the_esp_peft_loader() {
        let mut first = contribution("first", 1, 2.0, weight(1, 2), 4, 4, 1, 1.0);
        let mut second = contribution("second", 2, 6.0, weight(1, 4), 4, 4, 1, 6.0);
        for contribution in [&mut first, &mut second] {
            contribution.manifest.base_model_ref = ManifestId::of_blob(b"loader base");
            contribution.manifest.tokenizer_ref = ManifestId::of_blob(b"loader tokenizer");
            contribution.manifest.prompt_template_hash = eidetic::Hash::of(TEMPLATE);
            contribution.manifest_ref =
                ManifestId::of_blob(&serde_json::to_vec(&contribution.manifest).unwrap());
        }
        let aggregate = aggregate(vec![first, second]);
        let base_model_ref = aggregate.manifest.base_model_ref;
        let tokenizer_ref = aggregate.manifest.tokenizer_ref;
        let tokenizer = loader_tokenizer();
        let weights = loader_base_weights();
        #[allow(deprecated)]
        let device = esp::infer::decoder::DecoderDevice::ndarray();
        let loader = esp::infer::decoder::PeftLoraAdapterLoader::new(
            base_model_ref,
            tokenizer_ref,
            loader_config(),
            &tokenizer,
            &weights,
            MODEL_ID,
            PEFT_LORA_NDARRAY_LOADER,
            &device,
        );
        let manifest_ref = ManifestId::of_blob(&serde_json::to_vec(&aggregate.manifest).unwrap());
        let session = ModelSession {
            base_model_ref,
            model_id: MODEL_ID.into(),
            tokenizer_ref,
            prompt_template_hash: eidetic::Hash::of(TEMPLATE),
            quantization: None,
            loader: PEFT_LORA_NDARRAY_LOADER.into(),
            adapters: vec![AdapterSelection {
                manifest_ref,
                scale: 1.0,
            }],
        };
        loader
            .load_session(
                session,
                &[AdapterArtifact {
                    manifest_ref,
                    manifest: &aggregate.manifest,
                    config_bytes: &aggregate.adapter_config_json,
                    weight_bytes: &aggregate.adapter_safetensors,
                }],
            )
            .expect("exact FLoRA output loads through ESP's ordinary PEFT loader");
    }
}
