// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Autodiff PEFT LoRA training for the llama-family decoder (v1).
//!
//! The same trainer contract as [`super::train`] — bytes in, bytes out, the
//! real decoder forward, the loader's own `add_delta` composition, the same
//! PEFT safetensors layout — with the finite differences replaced by real
//! gradients. v0 pays `2N` forwards per step over `N` factor parameters; v1
//! pays one forward and one backward regardless of `N`, which is what buys
//! the extra target modules and per-case targets this trainer allows.
//!
//! Three things make the gradient land where it should:
//!
//! 1. **Autodiff is a property of the device.** ESP's decoder has no
//!    `B: Backend` parameter; the backend is chosen per call site by the
//!    `Device`. So this trainer wraps the caller's device with
//!    `Device::autodiff` internally and builds everything on the wrapper.
//! 2. **The base weights are detached.** They arrive off `Tensor::from_data`,
//!    which records no tape, and are detached again here so the claim is
//!    stated rather than inherited. Only the LoRA factors are tracked leaves.
//! 3. **The composed weight stays a non-leaf.** `base + A^T B^T` is the
//!    tensor the decoder's `Linear` carries, and it must reach the forward
//!    with its graph edge to `A` and `B` intact — see
//!    [`super::attention::adopt_param`] for the burn detail that makes this
//!    work.
//!
//! What v1's *single-case* objective does *not* do: padding with an
//! attention mask. Cases must still tokenize to one shared length there; a
//! per-case target is merely a gather, but a per-case length is a masked
//! objective. That follow-on is [`train_peft_lora_autodiff_sequences`]
//! below: it trains a whole response span per case, right-padded to the
//! batch's longest, rather than one shared-length token. It shares factor
//! init, the composed model, the Adam loop, and the serializer with
//! [`train_peft_lora_autodiff`] — see [`run_adam_loop`] — and differs only in
//! tokenization and the loss.
//!
//! The trainer's *vocabulary* — [`AutodiffLoraSettings`], [`SequenceCase`],
//! and the two version strings — lives next door in
//! [`super::train_autodiff_settings`], under `decoder-lora` rather than
//! `decoder-autodiff`. Only this module, which needs burn's autodiff and
//! optimizer, is behind the heavier feature.

use burn::module::{Module, Param};
use burn::optim::{AdamConfig, GradientsParams, decay::WeightDecayConfig};
use burn::tensor::activation::log_softmax;
use burn::tensor::{Device, Int, Tensor, TensorData};
use tokenizers::Tokenizer;

use super::config::DecoderConfig;
use super::loader::load_decoder_tensors_from_bytes;
use super::lora::{add_delta, dimensions};
use super::model::{DecoderModel, LoadedDecoder};
use super::sample::SplitMix64;
use super::train::{
    TrainedLoraAdapter, TrainingCase, adapter_config_json_for, det_init, expected_id,
    serialize_adapter_modules,
};
use super::train_autodiff_settings::{
    AutodiffLoraSettings, SequenceCase, SequenceTrainingSettings, TRAINED_PEFT_VERSION_AUTODIFF,
};
use crate::infer::provider::InferError;

/// The trainable factors of one `(layer, target module)` slot, in PEFT's
/// storage convention: `a` is `[rank, in]` and `b` is `[out, rank]`, exactly
/// the shapes the safetensors carry, so no transpose survives serialization.
#[derive(Module, Debug)]
struct LoraFactors {
    a: Param<Tensor<2>>,
    b: Param<Tensor<2>>,
}

/// Every trainable factor in the run, in layer-major then target-module
/// order — the same order [`serialize_adapter_modules`] writes.
///
/// This is the only module the optimizer ever sees. The decoder is rebuilt
/// around it each step and thrown away; nothing in the base model is a
/// parameter as far as Adam is concerned.
#[derive(Module, Debug)]
struct LoraFactorSet {
    slots: Vec<LoraFactors>,
}

impl LoraFactorSet {
    /// v0's initialization, generalized: A deterministic non-zero, B exactly
    /// zero, so the step-zero model is bit-identical to the base model and
    /// `initial_loss` is a real baseline rather than a perturbed one.
    ///
    /// The salt is `9000 + layer * modules + module_index`, which is
    /// injective over the slots and — with one target module — reproduces
    /// v0's `9000 + layer` exactly, so the two trainers start from the same
    /// point on the single-module case the gradient check compares.
    fn init(
        config: &DecoderConfig,
        settings: &AutodiffLoraSettings,
        device: &Device,
    ) -> Result<Self, InferError> {
        let rank = usize::from(settings.rank);
        let modules = settings.target_modules.len();
        let mut slots = Vec::with_capacity(config.num_hidden_layers * modules);
        for layer in 0..config.num_hidden_layers {
            for (index, module) in settings.target_modules.iter().enumerate() {
                let (in_features, out_features) = dimensions(config, module)?;
                let a = det_init(rank * in_features, 9000 + layer * modules + index);
                slots.push(LoraFactors {
                    a: Param::from_tensor(Tensor::from_data(
                        TensorData::new(a, [rank, in_features]),
                        device,
                    )),
                    b: Param::from_tensor(Tensor::zeros([out_features, rank], device)),
                });
            }
        }
        Ok(Self { slots })
    }

    /// The factors flattened in serialization order: per slot, all of A then
    /// all of B.
    fn to_flat_params(&self) -> Vec<f32> {
        let mut params = Vec::new();
        for slot in &self.slots {
            for tensor in [slot.a.val(), slot.b.val()] {
                params.extend(
                    tensor
                        .detach()
                        .into_data()
                        .to_vec::<f32>()
                        .expect("LoRA factors are dense f32"),
                );
            }
        }
        params
    }
}

/// One decoder built around the current factors, with every base weight
/// detached and every composed projection left as a tracked non-leaf.
fn compose_model(
    config: &DecoderConfig,
    base: &LoadedDecoder,
    factors: &LoraFactorSet,
    settings: &AutodiffLoraSettings,
    device: &Device,
) -> DecoderModel {
    let mut loaded = base.clone();
    let modules = settings.target_modules.len();
    let scale = settings.scale();
    for (layer, entry) in loaded.layers.iter_mut().enumerate() {
        for (index, module) in settings.target_modules.iter().enumerate() {
            let slot = &factors.slots[layer * modules + index];
            let (a, b) = (slot.a.val(), slot.b.val());
            match module.as_str() {
                "q_proj" => entry.q_w = add_delta(entry.q_w.clone(), a, b, scale),
                "k_proj" => entry.k_w = add_delta(entry.k_w.clone(), a, b, scale),
                "v_proj" => entry.v_w = add_delta(entry.v_w.clone(), a, b, scale),
                "o_proj" => entry.o_w = add_delta(entry.o_w.clone(), a, b, scale),
                other => unreachable!("validate() accepted target module {other:?}"),
            }
        }
    }
    DecoderModel::from_loaded(config.clone(), loaded, device)
}

/// The tokenized batch, shared by every step.
struct Batch {
    ids: Tensor<2, Int>,
    targets: Tensor<2, Int>,
    batch: usize,
    seq: usize,
}

/// Mean next-token cross-entropy at the last position, with a per-case
/// expected token.
///
/// The last position is the whole objective: the fixture supervises what
/// follows a prompt, not the prompt itself, and a rank-limited delta trained
/// on every position would be spending its capacity on reconstructing the
/// input. Per-case targets are a `gather`, which is why lifting v0's
/// shared-token rule costs nothing here.
fn batch_loss(model: &DecoderModel, batch: &Batch) -> Tensor<1> {
    let vocab = model.config().vocab_size;
    let last = model
        .logits(batch.ids.clone(), 0)
        .narrow(1, batch.seq - 1, 1)
        .reshape([batch.batch, vocab]);
    log_softmax(last, 1)
        .gather(1, batch.targets.clone())
        .mean()
        .neg()
}

fn scalar(loss: &Tensor<1>) -> f64 {
    f64::from(
        loss.clone()
            .detach()
            .into_data()
            .to_vec::<f32>()
            .expect("loss is a dense f32 scalar")[0],
    )
}

/// Every base tensor off the tape, stated rather than assumed.
fn detach_base(mut loaded: LoadedDecoder) -> LoadedDecoder {
    loaded.embed_w = loaded.embed_w.detach();
    loaded.final_norm_gamma = loaded.final_norm_gamma.detach();
    loaded.lm_head_w = loaded.lm_head_w.map(Tensor::detach);
    for layer in &mut loaded.layers {
        layer.input_norm_gamma = layer.input_norm_gamma.clone().detach();
        layer.q_w = layer.q_w.clone().detach();
        layer.k_w = layer.k_w.clone().detach();
        layer.v_w = layer.v_w.clone().detach();
        layer.o_w = layer.o_w.clone().detach();
        layer.post_norm_gamma = layer.post_norm_gamma.clone().detach();
        layer.gate_w = layer.gate_w.clone().detach();
        layer.up_w = layer.up_w.clone().detach();
        layer.down_w = layer.down_w.clone().detach();
    }
    loaded
}

/// Tokenize the batch, holding v0's shared-length rule and lifting its
/// shared-expected-token rule.
fn tokenize(
    tokenizer: &Tokenizer,
    cases: &[TrainingCase],
    device: &Device,
) -> Result<Batch, InferError> {
    let mut seq = 0usize;
    let mut ids = Vec::new();
    let mut targets = Vec::new();
    for case in cases {
        targets.push(expected_id(tokenizer, &case.expected_token)? as i32);
        let encoding = tokenizer
            .encode(case.prompt.as_str(), true)
            .map_err(|error| InferError::InvalidRequest(format!("tokenize: {error}")))?;
        let encoded = encoding.get_ids();
        if encoded.is_empty() {
            return Err(InferError::InvalidRequest("empty training prompt".into()));
        }
        if seq == 0 {
            seq = encoded.len();
        } else if encoded.len() != seq {
            return Err(InferError::InvalidRequest(
                "autodiff trainer cases must tokenize to one shared length; \
                 padding with an attention mask is the named follow-on"
                    .into(),
            ));
        }
        ids.extend(encoded.iter().map(|&id| id as i32));
    }
    let batch = cases.len();
    Ok(Batch {
        ids: Tensor::from_data(TensorData::new(ids, [batch, seq]), device),
        targets: Tensor::from_data(TensorData::new(targets, [batch, 1]), device),
        batch,
        seq,
    })
}

/// The tokenized, right-padded batch for the sequence objective.
///
/// Right-padding needs no attention-mask change: the decoder's mask is
/// causal ([`super::attention::DecoderAttention::forward_cached`]'s
/// `tril_mask`), so a pad placed after a case's real tokens is never
/// attended by them, and every real position's logits come out exactly as
/// they would at that case's own, shorter length — the padded-batch-equals-
/// unpadded-mean test below is what pins that claim down. Only the loss has
/// to ignore the pads, which [`Self`]'s `mask` does.
struct SequenceBatch {
    /// `[batch, seq]`, right-padded with the batch's pad id.
    ids: Tensor<2, Int>,
    /// `[batch, seq - 1]` in target space (`mask[i]` gates the prediction of
    /// `ids[i + 1]`): `1.0` where that target is a response token or the
    /// trailing EOS, `0.0` over the prompt span and the pads.
    mask: Tensor<2>,
    /// Total supervised target positions across the whole batch — the
    /// token-mean denominator. A property of the tokenization, not of
    /// anything the model produces, so it is counted once on the host
    /// rather than read back from a device reduction.
    supervised: f32,
    batch: usize,
    seq: usize,
}

/// Tokenize a batch of [`SequenceCase`]s under the sequence trainer's
/// contract: prompt tokens (through the tokenizer's ordinary
/// `add_special_tokens` handling, the same as every other prompt encode in
/// this crate), then the response tokens with no special tokens of their
/// own, then one appended EOS id — so training teaches the model to stop,
/// not merely to continue.
///
/// **The EOS id** is `config.json`'s `eos_token_id` (first entry, for
/// llama-family configs that carry several, e.g. Llama 3's two). That is the
/// only EOS this crate parses; a bare `tokenizer.json` does not name one on
/// its own, so there is nothing else to ask. The trainer refuses to run
/// without at least one — training a stop token that was never named would
/// be training toward an arbitrary choice this function made up.
///
/// **The pad id** is the tokenizer's own padding configuration
/// (`tokenizer.json`'s `padding.pad_id`) when `tokenizer.json` carries one,
/// else the same EOS id. That fallback is harmless: the loss mask means a
/// pad's identity never reaches the objective, so it only has to name a
/// legal embedding row, and EOS already is one.
fn tokenize_sequences(
    tokenizer: &Tokenizer,
    config: &DecoderConfig,
    cases: &[SequenceCase],
    device: &Device,
) -> Result<SequenceBatch, InferError> {
    let eos_id = *config.eos_token_id.first().ok_or_else(|| {
        InferError::InvalidConfig(
            "the sequence trainer needs config.json to name at least one eos_token_id".into(),
        )
    })?;
    let pad_id = tokenizer
        .get_padding()
        .map(|padding| padding.pad_id)
        .unwrap_or(eos_id);

    struct Encoded {
        ids: Vec<i32>,
        prompt_len: usize,
        response_len: usize,
    }
    let mut encoded = Vec::with_capacity(cases.len());
    let mut seq = 0usize;
    for case in cases {
        let prompt = tokenizer
            .encode(case.prompt.as_str(), true)
            .map_err(|error| InferError::InvalidRequest(format!("tokenize prompt: {error}")))?;
        let prompt_ids = prompt.get_ids();
        if prompt_ids.is_empty() {
            return Err(InferError::InvalidRequest("empty training prompt".into()));
        }
        let response = tokenizer
            .encode(case.response.as_str(), false)
            .map_err(|error| InferError::InvalidRequest(format!("tokenize response: {error}")))?;
        let response_ids = response.get_ids();

        let mut ids: Vec<i32> = prompt_ids.iter().map(|&id| id as i32).collect();
        ids.extend(response_ids.iter().map(|&id| id as i32));
        ids.push(eos_id as i32);
        seq = seq.max(ids.len());
        encoded.push(Encoded {
            ids,
            prompt_len: prompt_ids.len(),
            response_len: response_ids.len(),
        });
    }

    let batch = cases.len();
    let mut padded_ids = vec![pad_id as i32; batch * seq];
    let mut mask = vec![0.0f32; batch * (seq - 1)];
    let mut supervised = 0.0f32;
    for (row, case) in encoded.iter().enumerate() {
        padded_ids[row * seq..row * seq + case.ids.len()].copy_from_slice(&case.ids);
        // Target index i predicts token i + 1. Response tokens occupy input
        // positions [prompt_len, prompt_len + response_len) and the EOS
        // sits immediately after, so the supervised target range is
        // [prompt_len - 1, prompt_len + response_len - 1] inclusive:
        // response_len + 1 positions, one per response token plus one for
        // EOS.
        let start = case.prompt_len - 1;
        let end = case.prompt_len + case.response_len; // exclusive
        for target in start..end {
            mask[row * (seq - 1) + target] = 1.0;
            supervised += 1.0;
        }
    }

    Ok(SequenceBatch {
        ids: Tensor::from_data(TensorData::new(padded_ids, [batch, seq]), device),
        mask: Tensor::from_data(TensorData::new(mask, [batch, seq - 1]), device),
        supervised,
        batch,
        seq,
    })
}

/// Token-mean cross-entropy over the response span (every response token
/// plus the trailing EOS), by construction of [`SequenceBatch::mask`]: the
/// prompt span and the pads score zero and are excluded from the mean by the
/// mask, not by a Rust-side loop over positions.
///
/// Targets are shifted by one (`ids[i + 1]` predicted from `logits[i]`), the
/// ordinary next-token setup. [`TrainingCase`]'s single supervised position
/// is *not* simply this objective's `response` of one token at the same
/// weight — the EOS rule adds a second supervised target even then, which is
/// why the equivalence test below compares against a construction that
/// accounts for it rather than against `batch_loss` directly.
fn sequence_batch_loss(model: &DecoderModel, batch: &SequenceBatch) -> Tensor<1> {
    let logits = model.logits(batch.ids.clone(), 0);
    let next = logits.narrow(1, 0, batch.seq - 1);
    let targets: Tensor<3, Int> =
        batch
            .ids
            .clone()
            .narrow(1, 1, batch.seq - 1)
            .reshape([batch.batch, batch.seq - 1, 1]);
    let picked = log_softmax(next, 2)
        .gather(2, targets)
        .reshape([batch.batch, batch.seq - 1]);
    picked
        .mul(batch.mask.clone())
        .sum()
        .neg()
        .div_scalar(batch.supervised)
}

// ── Mini-batching (Phase 2) ─────────────────────────────────────────────

/// One [`SequenceCase`], tokenized once and — if it would not otherwise
/// fit — left-truncated to [`SequenceTrainingSettings::max_sequence_tokens`].
/// Encoding every case exactly once up front, rather than per mini-batch,
/// means a corpus this size is tokenized once regardless of how many Adam
/// steps draw from it.
#[derive(Debug)]
struct EncodedSequenceCase {
    /// Prompt tokens (post-truncation) then response tokens then EOS, the
    /// same order [`tokenize_sequences`] builds.
    ids: Vec<i32>,
    /// Tokens kept from the (possibly truncated) prompt.
    prompt_len: usize,
    response_len: usize,
}

/// Tokenize every case once under the mini-batch trainer's truncation rule:
/// a case whose prompt, response, and trailing EOS together exceed
/// `max_sequence_tokens` has its prompt cut from the left, keeping the
/// prompt's tail and the whole response intact; a case whose response plus
/// EOS alone already exceeds `max_sequence_tokens` cannot be represented at
/// any prompt length and is refused by name instead of silently truncating
/// the span the objective supervises.
///
/// Returns the pad id (same rule as [`tokenize_sequences`]) alongside the
/// encoded cases, since [`pad_batch`] needs it for every mini-batch drawn
/// from the result.
fn encode_sequences_for_training(
    tokenizer: &Tokenizer,
    config: &DecoderConfig,
    cases: &[SequenceCase],
    max_sequence_tokens: usize,
) -> Result<(u32, Vec<EncodedSequenceCase>), InferError> {
    let eos_id = *config.eos_token_id.first().ok_or_else(|| {
        InferError::InvalidConfig(
            "the sequence trainer needs config.json to name at least one eos_token_id".into(),
        )
    })?;
    let pad_id = tokenizer
        .get_padding()
        .map(|padding| padding.pad_id)
        .unwrap_or(eos_id);

    let mut encoded = Vec::with_capacity(cases.len());
    for (index, case) in cases.iter().enumerate() {
        let prompt = tokenizer
            .encode(case.prompt.as_str(), true)
            .map_err(|error| InferError::InvalidRequest(format!("tokenize prompt: {error}")))?;
        if prompt.get_ids().is_empty() {
            return Err(InferError::InvalidRequest("empty training prompt".into()));
        }
        let response = tokenizer
            .encode(case.response.as_str(), false)
            .map_err(|error| InferError::InvalidRequest(format!("tokenize response: {error}")))?;
        let response_ids = response.get_ids();
        // +1 for the trailing EOS, exactly `tokenize_sequences`'s supervised
        // range: the response is never the part that gives way to the
        // budget, so this is checked before any truncation happens.
        let response_span = response_ids.len() + 1;
        if response_span > max_sequence_tokens {
            return Err(InferError::InvalidRequest(format!(
                "training case {index} ({:?}): response plus EOS is {response_span} tokens, \
                 which exceeds max_sequence_tokens {max_sequence_tokens}; the response is what \
                 the trainer supervises and cannot be truncated to fit",
                case.response
            )));
        }
        let prompt_budget = max_sequence_tokens - response_span;
        let mut prompt_ids: Vec<i32> = prompt.get_ids().iter().map(|&id| id as i32).collect();
        if prompt_ids.len() > prompt_budget {
            // Drop from the left: keep the prompt's tail, not its head.
            let drop = prompt_ids.len() - prompt_budget;
            prompt_ids.drain(0..drop);
        }
        let prompt_len = prompt_ids.len();
        let mut ids = prompt_ids;
        ids.extend(response_ids.iter().map(|&id| id as i32));
        ids.push(eos_id as i32);
        encoded.push(EncodedSequenceCase {
            ids,
            prompt_len,
            response_len: response_ids.len(),
        });
    }
    Ok((pad_id, encoded))
}

/// Build one right-padded [`SequenceBatch`] from a subset of
/// [`EncodedSequenceCase`]s named by `indices` — the per-mini-batch tail of
/// [`tokenize_sequences`], generalized to select rows rather than assume
/// "all of them".
fn pad_batch(
    encoded: &[EncodedSequenceCase],
    indices: &[usize],
    pad_id: u32,
    device: &Device,
) -> SequenceBatch {
    let seq = indices
        .iter()
        .map(|&i| encoded[i].ids.len())
        .max()
        .unwrap_or(1);
    let batch = indices.len();
    let mut padded_ids = vec![pad_id as i32; batch * seq];
    let mut mask = vec![0.0f32; batch * (seq - 1)];
    let mut supervised = 0.0f32;
    for (row, &index) in indices.iter().enumerate() {
        let case = &encoded[index];
        padded_ids[row * seq..row * seq + case.ids.len()].copy_from_slice(&case.ids);
        // Same accounting as `tokenize_sequences`, `saturating_sub` guarding
        // the one edge truncation can reach that the untruncated tokenizer
        // never could: a prompt cut all the way to nothing, where the first
        // response token has no preceding position to be predicted from
        // and supervision starts one token later than usual.
        let start = case.prompt_len.saturating_sub(1);
        let end = case.prompt_len + case.response_len;
        for target in start..end {
            mask[row * (seq - 1) + target] = 1.0;
            supervised += 1.0;
        }
    }
    SequenceBatch {
        ids: Tensor::from_data(TensorData::new(padded_ids, [batch, seq]), device),
        mask: Tensor::from_data(TensorData::new(mask, [batch, seq - 1]), device),
        supervised,
        batch,
        seq,
    }
}

/// A deterministic without-replacement mini-batch schedule over `0..len`:
/// draw `batch_size` indices at a time from a shuffled order, and reshuffle
/// — from the same running RNG stream, never reseeded — whenever a draw
/// would run past the end of the current epoch. The whole schedule (batch
/// composition, epoch boundaries, everything) is therefore a pure function
/// of the seed it was built from.
struct MiniBatchSchedule {
    order: Vec<usize>,
    cursor: usize,
    rng: SplitMix64,
    batch_size: usize,
}

impl MiniBatchSchedule {
    fn new(len: usize, settings: &SequenceTrainingSettings) -> Self {
        let mut schedule = Self {
            order: Vec::new(),
            cursor: 0,
            rng: SplitMix64::new(settings.seed),
            batch_size: settings.batch_size as usize,
        };
        schedule.reshuffle(len);
        schedule
    }

    /// Fisher-Yates over `0..len`, continuing the schedule's own RNG stream
    /// rather than starting a fresh one — the source of "reshuffled every
    /// epoch" rather than "the same order every epoch".
    fn reshuffle(&mut self, len: usize) {
        let mut order: Vec<usize> = (0..len).collect();
        for i in (1..order.len()).rev() {
            let j = (self.rng.next_u64() as usize) % (i + 1);
            order.swap(i, j);
        }
        self.order = order;
        self.cursor = 0;
    }

    /// The next mini-batch's indices: `batch_size` of them, or fewer at the
    /// tail of an epoch that does not divide evenly, or the whole corpus at
    /// once (reshuffled every call) when `batch_size >= len`.
    fn next_indices(&mut self, len: usize) -> Vec<usize> {
        if self.cursor >= self.order.len() {
            self.reshuffle(len);
        }
        let end = (self.cursor + self.batch_size).min(self.order.len());
        let batch = self.order[self.cursor..end].to_vec();
        self.cursor = end;
        batch
    }
}

/// Per-step mutable state for the sequence trainer's mini-batches: the
/// once-tokenized (and, where needed, left-truncated) corpus, the epoch
/// scheduler drawing this step's rows from it, and the device the resulting
/// tensors land on. [`run_adam_loop`] threads this through as `&mut T`,
/// calling [`Self::next_batch`] once per step (and once more for the final
/// recompute) — the mini-batch equivalent of the single-token trainer's
/// static [`Batch`].
struct SequenceBatchState {
    encoded: Vec<EncodedSequenceCase>,
    schedule: MiniBatchSchedule,
    pad_id: u32,
    device: Device,
}

impl SequenceBatchState {
    fn next_batch(&mut self) -> SequenceBatch {
        let indices = self.schedule.next_indices(self.encoded.len());
        pad_batch(&self.encoded, &indices, self.pad_id, &self.device)
    }
}

/// The Adam loop shared by every objective this module trains: rebuild the
/// composed model each step, evaluate `loss_fn`, backward, step. Returns the
/// trained factors, the loss before the first step, and the loss after the
/// last one — recomputed against the *final* factors rather than carried
/// over from the last gradient step, so it is the loss of the model the
/// caller actually gets.
///
/// `state` is threaded through as `&mut T` and `loss_fn` as `FnMut` — not
/// `&T` / `Fn` — so a call site can rotate what one step trains on: the
/// single-token trainer's `Batch` uses this trivially (nothing to mutate,
/// one batch reused every call, exactly Phase 1's behaviour); the sequence
/// trainer's mini-batch scheduler is a real use of it, drawing a fresh batch
/// from `state` on every call including the final recompute.
fn run_adam_loop<T>(
    config: &DecoderConfig,
    base: &LoadedDecoder,
    settings: &AutodiffLoraSettings,
    train_device: &Device,
    state: &mut T,
    mut loss_fn: impl FnMut(&DecoderModel, &mut T) -> Tensor<1>,
) -> Result<(LoraFactorSet, f64, f64), InferError> {
    let mut factors = LoraFactorSet::init(config, settings, train_device)?;
    let mut adam = AdamConfig::new()
        .with_beta_1(settings.beta1)
        .with_beta_2(settings.beta2)
        .with_epsilon(settings.epsilon)
        // A zero penalty is the absence of weight decay, not a decay of
        // zero: keeping it `None` means the gradient is untouched rather
        // than incremented by an exact zero on every parameter every step.
        .with_weight_decay(
            (settings.weight_decay > 0.0).then(|| WeightDecayConfig::new(settings.weight_decay)),
        )
        .init();

    let mut initial_loss = 0.0f64;
    for step in 0..settings.steps {
        let model = compose_model(config, base, &factors, settings, train_device);
        let loss = loss_fn(&model, state);
        if step == 0 {
            initial_loss = scalar(&loss);
            if !initial_loss.is_finite() {
                return Err(InferError::Backend(
                    "training loss is not finite at step 0".into(),
                ));
            }
        }
        let grads = GradientsParams::from_grads(loss.backward(), &factors);
        if grads.is_empty() {
            return Err(InferError::Backend(format!(
                "no LoRA factor received a gradient at step {step}; the composed \
                 weights are not on the autodiff tape"
            )));
        }
        factors = adam.step(settings.learning_rate, factors, grads);
    }

    let trained_loss = scalar(&loss_fn(
        &compose_model(config, base, &factors, settings, train_device),
        state,
    ));
    Ok((factors, initial_loss, trained_loss))
}

/// Train one PEFT LoRA adapter over the supplied base artifact triple, with
/// real gradients.
///
/// `device` is the **ordinary inner device** the caller would use for
/// inference — `Device::ndarray()`, `Device::wgpu(..)`. Autodiff is a
/// property of the device in Burn's dispatch backend, so this function wraps
/// it with `Device::autodiff` itself and runs the whole training loop on the
/// wrapper; the caller never has to know that. (An already-autodiff device is
/// accepted and passed through, because wrapping one twice panics in Burn.)
///
/// Every case must tokenize to the same length; unlike v0, cases may carry
/// different expected tokens and the adapter may cover several attention
/// projections. The result reproduces exactly on one host for one input set;
/// no cross-device bit claim is made — floating-point summation order differs
/// between the CPU and GPU lanes, which is why the GPU receipt asserts strict
/// improvement rather than equality.
pub fn train_peft_lora_autodiff(
    config_bytes: &[u8],
    tokenizer_bytes: &[u8],
    base_weights: &[u8],
    model_id: &str,
    cases: &[TrainingCase],
    settings: &AutodiffLoraSettings,
    device: &Device,
) -> Result<TrainedLoraAdapter, InferError> {
    settings.validate()?;
    if cases.is_empty() {
        return Err(InferError::InvalidRequest(
            "trainer received no training cases".into(),
        ));
    }
    let config = DecoderConfig::from_json_bytes(config_bytes)?;
    for module in &settings.target_modules {
        dimensions(&config, module)?;
    }
    let tokenizer = Tokenizer::from_bytes(tokenizer_bytes)
        .map_err(|error| InferError::InvalidConfig(format!("tokenizer.json parse: {error}")))?;

    let train_device = if device.is_autodiff() {
        device.clone()
    } else {
        device.clone().autodiff()
    };
    let mut batch = tokenize(&tokenizer, cases, &train_device)?;
    let base = detach_base(load_decoder_tensors_from_bytes(
        &config,
        base_weights,
        &train_device,
    )?);

    let (factors, initial_loss, trained_loss) = run_adam_loop(
        &config,
        &base,
        settings,
        &train_device,
        &mut batch,
        |model, batch| batch_loss(model, batch),
    )?;

    let params = factors.to_flat_params();
    Ok(TrainedLoraAdapter {
        adapter_safetensors: serialize_adapter_modules(
            &config,
            usize::from(settings.rank),
            &settings.target_modules,
            &params,
        ),
        adapter_config_json: adapter_config_json_for(
            model_id,
            TRAINED_PEFT_VERSION_AUTODIFF,
            settings.rank,
            settings.alpha,
            &settings.target_modules,
        ),
        initial_loss,
        trained_loss,
    })
}

/// Train one PEFT LoRA adapter that learns whole response spans, not one
/// token.
///
/// Everything is shared with [`train_peft_lora_autodiff`] except
/// tokenization and the loss: factor initialization, the composed model, the
/// Adam loop ([`run_adam_loop`]), and the serializer are the same code. See
/// [`SequenceCase`] for the tokenization contract and [`sequence_batch_loss`]
/// for the objective — token-mean cross-entropy over the response span
/// (response tokens plus the trailing EOS), prompt and pad positions
/// excluded by a mask rather than by a shared-length restriction.
///
/// `device` is the ordinary inner device, exactly as in
/// `train_peft_lora_autodiff`; the same autodiff-wrapping and cross-device
/// floating-point posture apply unchanged.
///
/// `mini_batch` governs how the corpus is drawn from between steps —
/// [`SequenceTrainingSettings`]'s own doc comment has the batching and
/// truncation rules — and is validated before `settings` touches a single
/// byte of the model, same as `settings` itself.
#[allow(clippy::too_many_arguments)]
pub fn train_peft_lora_autodiff_sequences(
    config_bytes: &[u8],
    tokenizer_bytes: &[u8],
    base_weights: &[u8],
    model_id: &str,
    cases: &[SequenceCase],
    settings: &AutodiffLoraSettings,
    mini_batch: &SequenceTrainingSettings,
    device: &Device,
) -> Result<TrainedLoraAdapter, InferError> {
    settings.validate()?;
    mini_batch.validate()?;
    if cases.is_empty() {
        return Err(InferError::InvalidRequest(
            "trainer received no training cases".into(),
        ));
    }
    let config = DecoderConfig::from_json_bytes(config_bytes)?;
    for module in &settings.target_modules {
        dimensions(&config, module)?;
    }
    let tokenizer = Tokenizer::from_bytes(tokenizer_bytes)
        .map_err(|error| InferError::InvalidConfig(format!("tokenizer.json parse: {error}")))?;

    let train_device = if device.is_autodiff() {
        device.clone()
    } else {
        device.clone().autodiff()
    };
    let (pad_id, encoded) = encode_sequences_for_training(
        &tokenizer,
        &config,
        cases,
        mini_batch.max_sequence_tokens as usize,
    )?;
    let mut state = SequenceBatchState {
        schedule: MiniBatchSchedule::new(encoded.len(), mini_batch),
        encoded,
        pad_id,
        device: train_device.clone(),
    };
    let base = detach_base(load_decoder_tensors_from_bytes(
        &config,
        base_weights,
        &train_device,
    )?);

    let (factors, initial_loss, trained_loss) = run_adam_loop(
        &config,
        &base,
        settings,
        &train_device,
        &mut state,
        |model, state| sequence_batch_loss(model, &state.next_batch()),
    )?;

    let params = factors.to_flat_params();
    Ok(TrainedLoraAdapter {
        adapter_safetensors: serialize_adapter_modules(
            &config,
            usize::from(settings.rank),
            &settings.target_modules,
            &params,
        ),
        adapter_config_json: adapter_config_json_for(
            model_id,
            TRAINED_PEFT_VERSION_AUTODIFF,
            settings.rank,
            settings.alpha,
            &settings.target_modules,
        ),
        initial_loss,
        trained_loss,
    })
}

/// Row-chunk size for [`sequence_loss`]'s held-out evaluation.
///
/// The objective materializes `[batch, seq, vocab]` logits in `f32`; at
/// SmolLM2's 49,152-token vocabulary and this trainer's default 256-token
/// `max_sequence_tokens`, one row of that block alone is
/// `256 * 49_152 * 4` bytes ≈ 50 MB. A held-out or blind split is measured
/// as a single call with however many cases it has, so an unchunked eval
/// batch would spend device memory on the split's size rather than on
/// anything the caller chose; eight rows keeps one chunk under half a
/// gigabyte at that width regardless of how large the split is.
const SEQUENCE_LOSS_CHUNK_ROWS: usize = 8;

/// Held-out token-mean response-span cross-entropy for an already-loaded
/// model — base or adapted — on its own device, computed with exactly
/// [`sequence_batch_loss`], the objective [`train_peft_lora_autodiff_sequences`]
/// descends.
///
/// Takes a `&DecoderModel` rather than a session or provider, because the
/// harness needs to measure a bare base model as well as an adapted one, and
/// both are reachable as a `DecoderModel` — a base model via
/// [`super::provider::DecoderProvider::model`] on either session, adapted or
/// not, since `PeftLoraAdapterLoader` composes the delta into the same model
/// type it would build with no adapters at all.
///
/// Measured in chunks of [`SEQUENCE_LOSS_CHUNK_ROWS`] and combined
/// token-weighted, which is exactly the single-batch value: causal masking
/// means a chunk's real positions read out identically whether or not other
/// cases share its batch (the padded-batch-equals-unpadded-mean test on
/// [`sequence_batch_loss`] is what pins that claim down), so the grand sum
/// of masked log-probabilities is the same total distributed over however
/// many chunks it is computed in, and only the chunk boundary — not the
/// answer — moves when the split's size does.
pub fn sequence_loss(
    model: &DecoderModel,
    tokenizer_bytes: &[u8],
    cases: &[SequenceCase],
) -> Result<f64, InferError> {
    if cases.is_empty() {
        return Err(InferError::InvalidRequest(
            "sequence_loss received no cases".into(),
        ));
    }
    let tokenizer = Tokenizer::from_bytes(tokenizer_bytes)
        .map_err(|error| InferError::InvalidConfig(format!("tokenizer.json parse: {error}")))?;
    let device = model.device();
    let mut weighted_sum = 0.0f64;
    let mut total_supervised = 0.0f64;
    for chunk in cases.chunks(SEQUENCE_LOSS_CHUNK_ROWS) {
        let batch = tokenize_sequences(&tokenizer, model.config(), chunk, &device)?;
        let supervised = f64::from(batch.supervised);
        weighted_sum += scalar(&sequence_batch_loss(model, &batch)) * supervised;
        total_supervised += supervised;
    }
    Ok(weighted_sum / total_supervised)
}

#[cfg(test)]
mod tests {
    use super::super::model::tests::det_loaded;
    use super::super::test_support::tiny_config;
    use super::super::train::Objective;
    use super::*;

    fn settings() -> AutodiffLoraSettings {
        AutodiffLoraSettings {
            rank: 1,
            alpha: 8.0,
            target_modules: vec!["v_proj".into()],
            steps: 8,
            learning_rate: 0.05,
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1.0e-8,
            weight_decay: 0.0,
        }
    }

    /// A mini-batch schedule that reproduces Phase 1's full-batch behaviour:
    /// `batch_size >= corpus_size` means every step's mini-batch is the
    /// whole corpus (reshuffled row order only), and `max_sequence_tokens`
    /// is generous enough that nothing on these tiny fixtures ever
    /// truncates.
    fn mini_batch_settings(corpus_size: u32) -> SequenceTrainingSettings {
        SequenceTrainingSettings {
            batch_size: corpus_size,
            seed: 42,
            max_sequence_tokens: 32,
        }
    }

    /// Gradient check on the tiny fixture at the initial point.
    ///
    /// The autodiff gradient of every factor parameter is compared against a
    /// central difference of **v0's own `Objective::loss`** — the same
    /// function the finite-difference trainer descends — so a disagreement
    /// means the two trainers do not share an objective, which is the thing
    /// worth catching.
    ///
    /// Tolerance. The forward is f32 (the decoder's dtype), so the loss
    /// carries roughly 1e-6 of absolute noise; a central difference divides
    /// that by `2h`, giving about 5e-5 of noise floor at `h = 0.01`, plus an
    /// `O(h^2)` truncation term. The bound below is
    /// `1e-4 + 0.005 * |finite difference|`: absolute where the gradient is
    /// near zero (noise-dominated), relative where it is not
    /// (truncation-dominated). Observed max error on this fixture is 7.8e-7,
    /// two orders inside the absolute term, so the bound is headroom for f32
    /// rather than a threshold the run sits against — and it is far too
    /// tight to pass a wrong gradient, since a transposed factor or a
    /// dropped `scale` is off by whole factors, not by half a percent.
    #[test]
    fn autodiff_gradient_matches_v0_central_difference() {
        let config = tiny_config();
        let inner = Device::ndarray();
        let train_device = inner.clone().autodiff();
        let settings = settings();
        let module = settings.target_modules[0].clone();
        let (in_features, out_features) = dimensions(&config, &module).unwrap();
        let rank = usize::from(settings.rank);

        // One shared expected token and one shared length: v0's Objective
        // only speaks that dialect, and the point here is the gradient.
        let batch_ids: Vec<i32> = vec![1, 5, 9, 2, 7, 3, 11, 4, 6, 8];
        let (batch, seq) = (2usize, 5usize);
        let expected = 7usize;

        let base_inner = detach_base(det_loaded(&config, &inner));
        let objective = Objective {
            config: &config,
            base: &base_inner,
            target_module: &module,
            rank,
            in_features,
            out_features,
            scale: settings.alpha / f32::from(settings.rank),
            batch_ids: batch_ids.clone(),
            batch,
            seq,
            expected_id: expected,
            device: inner.clone(),
        };

        let factors = LoraFactorSet::init(&config, &settings, &train_device).unwrap();
        let base_train = detach_base(det_loaded(&config, &train_device));
        let model = compose_model(&config, &base_train, &factors, &settings, &train_device);
        let batch_tensors = Batch {
            ids: Tensor::from_data(TensorData::new(batch_ids, [batch, seq]), &train_device),
            targets: Tensor::from_data(
                TensorData::new(vec![expected as i32; batch], [batch, 1]),
                &train_device,
            ),
            batch,
            seq,
        };
        let loss = batch_loss(&model, &batch_tensors);
        assert!(
            (scalar(&loss) - objective.loss(&factors.to_flat_params())).abs() < 1.0e-4,
            "the two trainers must agree on the loss before their gradients can be compared"
        );
        let grads = loss.backward();
        let autodiff: Vec<f32> = factors
            .slots
            .iter()
            .flat_map(|slot| {
                let mut values = Vec::new();
                for param in [&slot.a, &slot.b] {
                    values.extend(
                        param
                            .val()
                            .grad(&grads)
                            .expect("every LoRA factor must receive a gradient")
                            .into_data()
                            .to_vec::<f32>()
                            .unwrap(),
                    );
                }
                values
            })
            .collect();

        let mut params = factors.to_flat_params();
        assert_eq!(autodiff.len(), params.len());
        let h = 0.01f32;
        let mut worst = 0.0f64;
        for index in 0..params.len() {
            let original = params[index];
            params[index] = original + h;
            let plus = objective.loss(&params);
            params[index] = original - h;
            let minus = objective.loss(&params);
            params[index] = original;
            let finite = (plus - minus) / (2.0 * f64::from(h));
            let error = (f64::from(autodiff[index]) - finite).abs();
            worst = worst.max(error);
            assert!(
                error <= 1.0e-4 + 0.005 * finite.abs(),
                "parameter {index}: autodiff {} vs central difference {finite}",
                autodiff[index]
            );
        }
        println!(
            "gradient check: {} parameters, max error {worst:e}",
            params.len()
        );
    }

    #[test]
    fn factor_set_reproduces_v0_initialization_for_one_module() {
        let config = tiny_config();
        let device = Device::ndarray().autodiff();
        let settings = settings();
        let factors = LoraFactorSet::init(&config, &settings, &device).unwrap();
        let (in_features, out_features) = dimensions(&config, &settings.target_modules[0]).unwrap();
        let rank = usize::from(settings.rank);
        for layer in 0..config.num_hidden_layers {
            assert_eq!(
                factors.slots[layer]
                    .a
                    .val()
                    .detach()
                    .into_data()
                    .to_vec::<f32>()
                    .unwrap(),
                det_init(rank * in_features, 9000 + layer),
                "layer {layer} A must start where v0 starts"
            );
            assert!(
                factors.slots[layer]
                    .b
                    .val()
                    .detach()
                    .into_data()
                    .to_vec::<f32>()
                    .unwrap()
                    .iter()
                    .all(|value| *value == 0.0),
                "layer {layer} B must start at zero so step zero is the base model"
            );
            assert_eq!(factors.slots[layer].b.val().dims(), [out_features, rank]);
        }
    }

    // ── Sequence objective ──────────────────────────────────────────────

    /// `tiny_config`, plus an `eos_token_id`. The shared fixture leaves it
    /// empty because nothing else needs one; the sequence trainer refuses to
    /// run without one, so these tests supply their own. `0` is an ordinary
    /// vocabulary id here (`t0` in [`tiny_tokenizer_json`]), not a sentinel.
    fn seq_config() -> DecoderConfig {
        let mut config = tiny_config();
        config.eos_token_id = vec![0];
        config
    }

    /// A real `tokenizer.json`, WordLevel over `t0..t31` — the same
    /// vocabulary `tiny_config`'s dimensions assume — so `tokenize_sequences`
    /// exercises the same parse path a checkpoint's tokenizer would.
    fn tiny_tokenizer_json() -> String {
        let vocab: Vec<String> = (0..32).map(|i| format!("\"t{i}\": {i}")).collect();
        format!(
            r#"{{"version":"1.0","pre_tokenizer":{{"type":"Whitespace"}},"model":{{"type":"WordLevel","vocab":{{{}}},"unk_token":"t0"}}}}"#,
            vocab.join(", ")
        )
    }

    /// Base-model safetensors bytes for `tiny_config`'s dimensions, in HF
    /// names. The trainer's public entry points take bytes, not a
    /// `LoadedDecoder`, so the equivalence tests below — which call
    /// `train_peft_lora_autodiff` and `train_peft_lora_autodiff_sequences`
    /// side by side — need real bytes rather than
    /// `super::super::model::tests::det_loaded`'s in-memory tensors.
    fn tiny_base_weights_bytes(config: &DecoderConfig) -> Vec<u8> {
        use safetensors::tensor::{Dtype, TensorView};

        let (h, kv, inter, vocab) = (
            config.hidden_size,
            config.kv_heads() * config.head_dim(),
            config.intermediate_size,
            config.vocab_size,
        );
        let mut table: Vec<(String, Vec<usize>, Vec<f32>)> = Vec::new();
        let mut push = |name: String, shape: Vec<usize>, salt: usize| {
            let n: usize = shape.iter().product();
            table.push((name, shape, det_init(n, salt)));
        };
        push("model.embed_tokens.weight".into(), vec![vocab, h], 7);
        for i in 0..config.num_hidden_layers {
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

    /// `seq_config`'s fields, as the JSON bytes the trainer's public entry
    /// points parse — the same construction `one_token_response_equals_...`
    /// inlines once; factored out for the mini-batch tests that need it too.
    fn seq_config_json_bytes(config: &DecoderConfig) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "vocab_size": config.vocab_size,
            "hidden_size": config.hidden_size,
            "intermediate_size": config.intermediate_size,
            "num_hidden_layers": config.num_hidden_layers,
            "num_attention_heads": config.num_attention_heads,
            "num_key_value_heads": config.num_key_value_heads,
            "max_position_embeddings": config.max_position_embeddings,
            "rms_norm_eps": config.rms_norm_eps,
            "rope_theta": config.rope_theta,
            "tie_word_embeddings": config.tie_word_embeddings,
            "eos_token_id": config.eos_token_id,
        }))
        .unwrap()
    }

    /// Done condition: the padded-batch loss equals the token-weighted mean
    /// of the unpadded per-case losses.
    ///
    /// Three cases of different lengths, batched together, versus each run
    /// alone (batch of one, its own length, no padding at all). Causal
    /// masking means a pad placed after a case's real tokens cannot affect
    /// that case's own logits — the batched and standalone forwards read the
    /// exact same values at every real position — so the only source of
    /// disagreement is floating-point summation order across a different
    /// batch/seq shape, which is why the tolerance is tight rather than
    /// exact.
    #[test]
    fn padded_batch_loss_equals_token_weighted_mean_of_unpadded_cases() {
        let config = seq_config();
        let device = Device::ndarray();
        let tokenizer_bytes = tiny_tokenizer_json().into_bytes();
        let model =
            DecoderModel::from_loaded(config.clone(), det_loaded(&config, &device), &device);

        let cases = vec![
            SequenceCase {
                prompt: "t1 t2".into(),
                response: "t3".into(),
            },
            SequenceCase {
                prompt: "t4".into(),
                response: "t5 t6 t7".into(),
            },
            SequenceCase {
                prompt: "t8 t9 t10".into(),
                response: String::new(),
            },
        ];

        let mut weighted_sum = 0.0f64;
        let mut total_supervised = 0.0f64;
        for case in &cases {
            let alone =
                sequence_loss(&model, &tokenizer_bytes, std::slice::from_ref(case)).unwrap();
            // response tokens plus one for the trailing EOS, matching
            // `tokenize_sequences`'s supervised-range accounting.
            let supervised = case.response.split_whitespace().count() as f64 + 1.0;
            weighted_sum += alone * supervised;
            total_supervised += supervised;
        }
        let expected = weighted_sum / total_supervised;

        let batched = sequence_loss(&model, &tokenizer_bytes, &cases).unwrap();
        assert!(
            (batched - expected).abs() < 1.0e-4,
            "batched loss {batched} should equal the token-weighted mean of the \
             unpadded per-case losses {expected}"
        );
    }

    /// `TrainingCase`'s single supervised position is the degenerate
    /// instance of the sequence objective at `response` of one token — but
    /// not at equal weight, because the EOS rule adds a second supervised
    /// target `TrainingCase` never had. This test pins down exactly what
    /// that means: the sequence loss at the initial point equals the mean of
    /// two ordinary `train_peft_lora_autodiff` losses — one predicting the
    /// response token from the prompt (v0's shape exactly), one predicting
    /// EOS from the prompt-plus-response-token (one token longer) — because
    /// both parts supervise the same number of cases and so carry equal
    /// weight in the token mean.
    ///
    /// All three calls share one `settings`, `config_bytes`, and
    /// `base_weights`, so `LoraFactorSet::init` starts every one of them at
    /// the identical point; only the tokenization and the read-out position
    /// differ. Causal masking then means the shared prefix's logits are
    /// exactly reproduced across the three tokenizations, which is why the
    /// tolerance below is tight rather than a training-noise allowance.
    #[test]
    fn one_token_response_equals_v0_loss_plus_the_eos_target_it_adds() {
        let config = seq_config();
        let config_bytes = serde_json::to_vec(&serde_json::json!({
            "vocab_size": config.vocab_size,
            "hidden_size": config.hidden_size,
            "intermediate_size": config.intermediate_size,
            "num_hidden_layers": config.num_hidden_layers,
            "num_attention_heads": config.num_attention_heads,
            "num_key_value_heads": config.num_key_value_heads,
            "max_position_embeddings": config.max_position_embeddings,
            "rms_norm_eps": config.rms_norm_eps,
            "rope_theta": config.rope_theta,
            "tie_word_embeddings": config.tie_word_embeddings,
            "eos_token_id": config.eos_token_id,
        }))
        .unwrap();
        let base_bytes = tiny_base_weights_bytes(&config);
        let tokenizer_bytes = tiny_tokenizer_json().into_bytes();
        let device = Device::ndarray();
        let settings = settings();

        let prompts = ["t1 t2 t3", "t4 t5 t6"];
        let response_token = "t7";

        // Part A: predict the response token from the prompt — v0's shape.
        let cases_a: Vec<TrainingCase> = prompts
            .iter()
            .map(|p| TrainingCase {
                prompt: (*p).into(),
                expected_token: response_token.into(),
            })
            .collect();
        let a = train_peft_lora_autodiff(
            &config_bytes,
            &tokenizer_bytes,
            &base_bytes,
            "fixture/seq",
            &cases_a,
            &settings,
            &device,
        )
        .expect("part A trains");

        // Part B: predict EOS from the prompt plus the response token.
        let cases_b: Vec<TrainingCase> = prompts
            .iter()
            .map(|p| TrainingCase {
                prompt: format!("{p} {response_token}"),
                expected_token: "t0".into(),
            })
            .collect();
        let b = train_peft_lora_autodiff(
            &config_bytes,
            &tokenizer_bytes,
            &base_bytes,
            "fixture/seq",
            &cases_b,
            &settings,
            &device,
        )
        .expect("part B trains");

        // The sequence objective at the same initial point.
        let cases_seq: Vec<SequenceCase> = prompts
            .iter()
            .map(|p| SequenceCase {
                prompt: (*p).into(),
                response: response_token.into(),
            })
            .collect();
        let seq = train_peft_lora_autodiff_sequences(
            &config_bytes,
            &tokenizer_bytes,
            &base_bytes,
            "fixture/seq",
            &cases_seq,
            &settings,
            &mini_batch_settings(cases_seq.len() as u32),
            &device,
        )
        .expect("sequence trainer trains");

        let expected = (a.initial_loss + b.initial_loss) / 2.0;
        assert!(
            (seq.initial_loss - expected).abs() < 1.0e-4,
            "sequence initial loss {} should equal the mean of the response-token \
             loss {} and the eos-target loss {} the EOS rule adds",
            seq.initial_loss,
            a.initial_loss,
            b.initial_loss,
        );
    }

    /// Gradient check for the sequence objective at the initial point,
    /// mirroring `autodiff_gradient_matches_v0_central_difference` but
    /// finite-differencing `sequence_batch_loss` itself rather than v0's
    /// `Objective`: there is no independent hand-rolled implementation of
    /// this loss to check against, so the oracle is the same forward
    /// function evaluated at perturbed parameters, which is still a real
    /// check on `loss.backward()` — the two code paths (autodiff and
    /// repeated forward evaluation) share nothing but the tensor ops
    /// themselves.
    ///
    /// Tolerance: identical reasoning to the v0-comparison check
    /// (`1e-4 + 0.005 * |finite difference|` at `h = 0.01`), since it is the
    /// same f32 forward and the same central-difference noise floor.
    #[test]
    fn sequence_gradient_matches_central_difference_at_init() {
        let config = seq_config();
        let inner = Device::ndarray();
        let train_device = inner.clone().autodiff();
        let settings = settings();
        let tokenizer_bytes = tiny_tokenizer_json().into_bytes();
        let tokenizer = Tokenizer::from_bytes(&tokenizer_bytes).unwrap();

        let cases = vec![
            SequenceCase {
                prompt: "t1 t2".into(),
                response: "t3 t4".into(),
            },
            SequenceCase {
                prompt: "t5".into(),
                response: "t6".into(),
            },
        ];
        let batch = tokenize_sequences(&tokenizer, &config, &cases, &train_device).unwrap();
        let base = detach_base(det_loaded(&config, &train_device));

        let factors = LoraFactorSet::init(&config, &settings, &train_device).unwrap();
        let model = compose_model(&config, &base, &factors, &settings, &train_device);
        let loss = sequence_batch_loss(&model, &batch);
        let grads = loss.backward();
        let autodiff: Vec<f32> = factors
            .slots
            .iter()
            .flat_map(|slot| {
                let mut values = Vec::new();
                for param in [&slot.a, &slot.b] {
                    values.extend(
                        param
                            .val()
                            .grad(&grads)
                            .expect("every LoRA factor must receive a gradient")
                            .into_data()
                            .to_vec::<f32>()
                            .unwrap(),
                    );
                }
                values
            })
            .collect();

        let mut params = factors.to_flat_params();
        assert_eq!(autodiff.len(), params.len());
        let rank = usize::from(settings.rank);
        let module = settings.target_modules[0].clone();
        let (in_features, out_features) = dimensions(&config, &module).unwrap();
        let per_layer = rank * (in_features + out_features);

        let loss_at = |params: &[f32]| -> f64 {
            let mut slots = Vec::with_capacity(config.num_hidden_layers);
            for layer in 0..config.num_hidden_layers {
                let offset = layer * per_layer;
                let a_len = rank * in_features;
                let a = Tensor::from_data(
                    TensorData::new(params[offset..offset + a_len].to_vec(), [rank, in_features]),
                    &train_device,
                );
                let b = Tensor::from_data(
                    TensorData::new(
                        params[offset + a_len..offset + per_layer].to_vec(),
                        [out_features, rank],
                    ),
                    &train_device,
                );
                slots.push(LoraFactors {
                    a: Param::from_tensor(a),
                    b: Param::from_tensor(b),
                });
            }
            let factors = LoraFactorSet { slots };
            let model = compose_model(&config, &base, &factors, &settings, &train_device);
            scalar(&sequence_batch_loss(&model, &batch))
        };

        let h = 0.01f32;
        let mut worst = 0.0f64;
        for index in 0..params.len() {
            let original = params[index];
            params[index] = original + h;
            let plus = loss_at(&params);
            params[index] = original - h;
            let minus = loss_at(&params);
            params[index] = original;
            let finite = (plus - minus) / (2.0 * f64::from(h));
            let error = (f64::from(autodiff[index]) - finite).abs();
            worst = worst.max(error);
            assert!(
                error <= 1.0e-4 + 0.005 * finite.abs(),
                "parameter {index}: autodiff {} vs central difference {finite}",
                autodiff[index]
            );
        }
        println!(
            "sequence gradient check: {} parameters, max error {worst:e}",
            params.len()
        );
    }

    // ── Mini-batching (Phase 2) ─────────────────────────────────────────

    /// Two runs with the same seed draw the exact same sequence of
    /// mini-batches and therefore train to identical adapter bytes; a
    /// different seed draws a different sequence — a real difference in
    /// which cases are grouped together per step, not floating-point noise
    /// — and therefore trains to different bytes. Checked on the actual
    /// trained artifact, not on the schedule in isolation, since that is
    /// what a reproducible receipt actually needs.
    #[test]
    fn mini_batch_training_is_deterministic_by_seed_and_varies_across_seeds() {
        let config = seq_config();
        let config_bytes = seq_config_json_bytes(&config);
        let base_bytes = tiny_base_weights_bytes(&config);
        let tokenizer_bytes = tiny_tokenizer_json().into_bytes();
        let device = Device::ndarray();
        let settings = settings();

        let cases: Vec<SequenceCase> = [
            ("t1 t2", "t3"),
            ("t4 t5", "t6"),
            ("t7 t8", "t9"),
            ("t10 t11", "t12"),
            ("t13 t14", "t15"),
            ("t16 t17", "t18"),
        ]
        .into_iter()
        .map(|(prompt, response)| SequenceCase {
            prompt: prompt.into(),
            response: response.into(),
        })
        .collect();

        let train = |seed: u64| {
            let mini_batch = SequenceTrainingSettings {
                batch_size: 2,
                seed,
                max_sequence_tokens: 32,
            };
            train_peft_lora_autodiff_sequences(
                &config_bytes,
                &tokenizer_bytes,
                &base_bytes,
                "fixture/seq-minibatch",
                &cases,
                &settings,
                &mini_batch,
                &device,
            )
            .expect("mini-batch training runs")
        };

        let a = train(7);
        let b = train(7);
        assert_eq!(
            a.adapter_safetensors, b.adapter_safetensors,
            "same seed must draw the same mini-batch sequence and train to the same bytes"
        );

        let c = train(8);
        assert_ne!(
            a.adapter_safetensors, c.adapter_safetensors,
            "a different seed must draw a different mini-batch sequence"
        );
    }

    /// The truncation half of [`SequenceTrainingSettings::max_sequence_tokens`]:
    /// a case too long for the budget drops prompt tokens from the left,
    /// keeping the prompt's tail and the whole response untouched.
    #[test]
    fn left_truncation_keeps_the_prompt_tail_and_the_whole_response() {
        let config = seq_config();
        let tokenizer = Tokenizer::from_bytes(tiny_tokenizer_json().as_bytes()).unwrap();
        let cases = vec![SequenceCase {
            prompt: "t1 t2 t3 t4 t5".into(),
            response: "t6 t7".into(),
        }];
        // response + EOS = 3 tokens, leaving a 3-token prompt budget out of
        // 6; the 5-token prompt must lose its first two tokens, not its
        // last two.
        let (pad_id, encoded) = encode_sequences_for_training(&tokenizer, &config, &cases, 6)
            .expect("fits after truncation");
        assert_eq!(encoded.len(), 1);
        let case = &encoded[0];
        assert_eq!(
            case.ids,
            vec![3, 4, 5, 6, 7, 0],
            "must keep the prompt's tail (t3 t4 t5), the whole response (t6 t7), then EOS"
        );
        assert_eq!(case.prompt_len, 3);
        assert_eq!(case.response_len, 2);
        assert_eq!(
            pad_id, 0,
            "no padding configured on this tokenizer, so pad_id falls back to eos_id"
        );
    }

    /// The refusal half: a response (plus EOS) that alone exceeds the
    /// budget cannot be represented at any prompt length, so it is refused
    /// by name rather than truncated — truncating the response would be
    /// truncating the only thing the trainer supervises.
    #[test]
    fn response_plus_eos_too_long_for_the_budget_is_refused_by_name() {
        let config = seq_config();
        let tokenizer = Tokenizer::from_bytes(tiny_tokenizer_json().as_bytes()).unwrap();
        let cases = vec![SequenceCase {
            prompt: "t1 t2".into(),
            response: "t6 t7 t8 t9 t10 t11 t12 t13".into(), // 8 tokens + EOS = 9
        }];
        let error = encode_sequences_for_training(&tokenizer, &config, &cases, 8)
            .expect_err("response plus EOS (9 tokens) exceeds the 8-token budget");
        let message = error.to_string();
        assert!(message.contains("case 0"), "{message}");
        assert!(message.contains("response"), "{message}");
        assert!(message.contains('9'), "{message}");
    }

    /// Deliverable 2's done condition: [`sequence_loss`]'s chunked
    /// evaluation, token-weighted across chunks, equals the value a single
    /// unchunked batch over the same cases would produce.
    #[test]
    fn sequence_loss_chunks_are_token_weighted_and_equal_the_single_batch_value() {
        let config = seq_config();
        let device = Device::ndarray();
        let tokenizer_bytes = tiny_tokenizer_json().into_bytes();
        let tokenizer = Tokenizer::from_bytes(&tokenizer_bytes).unwrap();
        let model =
            DecoderModel::from_loaded(config.clone(), det_loaded(&config, &device), &device);

        // Ten cases: more than `SEQUENCE_LOSS_CHUNK_ROWS`, so `sequence_loss`
        // measures them in two chunks (8 then 2) rather than one batch.
        let cases: Vec<SequenceCase> = (0..10)
            .map(|i| SequenceCase {
                prompt: format!("t{} t{}", 1 + i % 5, 2 + i % 5),
                response: format!("t{}", 10 + i % 5),
            })
            .collect();

        let chunked = sequence_loss(&model, &tokenizer_bytes, &cases).expect("chunked eval");

        let single_batch = tokenize_sequences(&tokenizer, &config, &cases, &device).unwrap();
        let unchunked = scalar(&sequence_batch_loss(&model, &single_batch));

        assert!(
            (chunked - unchunked).abs() < 1.0e-4,
            "chunked sequence_loss {chunked} should equal the single-batch value {unchunked}"
        );
    }
}
