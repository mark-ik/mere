/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Burn-backed BERT embedding provider (scaffold).
//!
//! ## Status
//!
//! This module currently ships the **dep additions, configuration struct,
//! and provider seam** — enough to verify that `burn` and `tokenizers`
//! integrate with the `EmbeddingProvider` trait without the workspace
//! breaking. The actual BERT layer implementations, safetensors weight
//! loading, and tokenizer wiring are a follow-up sub-slice.
//!
//! Until the layers + loader land, [`BertEmbeddingProvider::embed`] returns
//! [`crate::EmbedError::ModelNotLoaded`].
//!
//! ## Implementation path for the follow-up slice
//!
//! Each item below is a separate, testable piece. The dep additions in this
//! commit make all of them compile-ready.
//!
//! 1. **`embeddings.rs`** — `BertEmbeddings` struct using `burn::nn::Embedding`
//!    for word + position embeddings, `burn::nn::LayerNorm` for the post-add
//!    norm. Forward: `input_ids → embed → + positions → layernorm`.
//! 2. **`attention.rs`** — `BertSelfAttention` with Q/K/V `Linear`
//!    projections, scaled dot-product attention via
//!    `burn::tensor::activation::softmax(QK^T / sqrt(d_k)) @ V`. Heads
//!    handled by reshape+transpose.
//! 3. **`feed_forward.rs`** — `BertIntermediate` (`Linear` + GELU) +
//!    `BertOutput` (`Linear` + residual + `LayerNorm`).
//! 4. **`layer.rs`** — `BertLayer` combining attention + feed-forward;
//!    `BertEncoder` stacking N `BertLayer`s.
//! 5. **`model.rs`** — `BertModel` combining embeddings + encoder; sentence
//!    pooling via `[CLS]` token or mean-pool over sequence; final
//!    L2-normalize for cosine compatibility.
//! 6. **`tokenizer.rs`** — wrap `tokenizers::Tokenizer` for word-piece
//!    encoding; map text → input_ids + attention_mask.
//! 7. **`loader.rs`** — read safetensors via `burn::record::SafetensorsRecorder`
//!    (or hand-rolled), map HuggingFace tensor names to Burn module fields.
//!    HF names follow the `bert.embeddings.word_embeddings.weight` pattern;
//!    map to `model.embeddings.word_embeddings.weight` or whatever the Burn
//!    module hierarchy ends up being.
//! 8. **Validation** — load
//!    `sentence-transformers/all-MiniLM-L6-v2`, embed `["a sample sentence"]`,
//!    compare top-N components against the reference output from a
//!    Python `sentence-transformers` run. Tolerance ~1e-4. Without this
//!    step the implementation is structurally correct but unverified.

pub mod attention;
pub mod config;
pub mod construct;
pub mod embeddings;
pub mod encoder;
pub mod feed_forward;
pub mod layer;
pub mod loaded;
pub mod loader;
pub mod model;
pub mod provider;
pub mod safetensors_io;
pub mod tokenizer;
pub mod validation;
#[cfg(all(test, feature = "bert-wgpu"))]
mod wgpu_parity;

pub use attention::{BertAttention, BertSelfAttention, BertSelfOutput};
pub use config::{BGE_MICRO_V2, BertConfig, MINILM_L6_V2, SNOWFLAKE_ARCTIC_EMBED_XS};
pub use embeddings::BertEmbeddings;
pub use encoder::BertEncoder;
pub use feed_forward::{BertIntermediate, BertOutput};
pub use layer::BertLayer;
pub use loader::{LoaderError, ModelArtifacts};
pub use model::{BertModel, Pooling};
pub use provider::BertEmbeddingProvider;
pub use tokenizer::{BertTokenizer, EncodedBatch};
