/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The own llama-family decoder body (inference plan P1).
//!
//! HF-config-driven: [`config::DecoderConfig`] parses a HuggingFace
//! `config.json` directly, so one body runs the open llama-family class
//! (TinyLlama, Llama 3.2, SmolLM) from the standard artifact triple
//! (`config.json` / `tokenizer.json` / `model.safetensors`) — the same
//! triple `embed`'s BERT uses, and the byte shape the eidetic model
//! corridor stores.
//!
//! Built from burn-nn primitives (`RmsNorm`, `RotaryEncoding`, `SwiGlu`)
//! against the workspace burn pin; the GQA attention wiring in
//! [`attention`] is the piece burn-nn does not provide, informed by
//! tracel-ai/models' llama-burn (MIT OR Apache-2.0) as a reference
//! implementation.
//!
//! Slice status: config + attention + layer + model stack + safetensors
//! loader (this module). The KV-cached generation loop and the
//! `InferenceProvider` impl are the next slices.

pub mod attention;
pub mod config;
pub mod layer;
pub mod loader;
pub mod model;
pub mod tensors;

pub use attention::DecoderAttention;
pub use config::DecoderConfig;
pub use layer::{DecoderLayer, LoadedDecoderLayer};
pub use loader::load_decoder_from_bytes;
pub use model::{DecoderModel, LoadedDecoder};

/// Deterministic-weight helpers shared by the decoder tests: same values
/// on every backend by construction (the `embed::bert::wgpu_parity`
/// pattern).
#[cfg(test)]
pub(crate) mod test_support {
    use burn::tensor::backend::{Backend, BackendTypes};
    use burn::tensor::{Tensor, TensorData};

    use super::config::DecoderConfig;

    pub fn det_vec(n: usize, salt: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (((i + salt * 7919) as f32) * 0.618_034).sin() * 0.05)
            .collect()
    }

    pub fn t2<B: Backend>(
        a: usize,
        b: usize,
        salt: usize,
        dev: &<B as BackendTypes>::Device,
    ) -> Tensor<B, 2> {
        Tensor::from_data(TensorData::new(det_vec(a * b, salt), [a, b]), dev)
    }

    pub fn t1_ones<B: Backend>(n: usize, dev: &<B as BackendTypes>::Device) -> Tensor<B, 1> {
        Tensor::ones([n], dev)
    }

    /// Tiny dims for fast tests: hidden 8, 4 heads (head_dim 2), GQA 2.
    pub fn tiny_config() -> DecoderConfig {
        DecoderConfig {
            vocab_size: 32,
            hidden_size: 8,
            intermediate_size: 16,
            num_hidden_layers: 2,
            num_attention_heads: 4,
            num_key_value_heads: Some(2),
            max_position_embeddings: 16,
            rms_norm_eps: 1.0e-5,
            rope_theta: 10_000.0,
            tie_word_embeddings: false,
        }
    }
}
