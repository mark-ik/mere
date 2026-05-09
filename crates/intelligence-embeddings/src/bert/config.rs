/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! BERT configuration. Maps directly onto the HuggingFace `config.json`
//! shape so loading is a one-to-one parse when the loader lands.

use serde::{Deserialize, Serialize};

/// BERT architecture configuration.
///
/// Field names match the HuggingFace `config.json` keys for direct deserialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BertConfig {
    pub vocab_size: usize,
    pub hidden_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub intermediate_size: usize,
    pub max_position_embeddings: usize,
    pub type_vocab_size: usize,
    pub layer_norm_eps: f64,
    /// Hidden activation. Always "gelu" in practice; reserved for future
    /// alternatives.
    pub hidden_act: String,
    /// Pad token id used in attention masking.
    pub pad_token_id: usize,
}

impl BertConfig {
    /// Per-attention-head dimension: `hidden_size / num_attention_heads`.
    /// Panics if not evenly divisible (a misconfigured model).
    pub fn head_dim(&self) -> usize {
        assert!(
            self.hidden_size % self.num_attention_heads == 0,
            "BertConfig: hidden_size ({}) must be divisible by num_attention_heads ({})",
            self.hidden_size,
            self.num_attention_heads
        );
        self.hidden_size / self.num_attention_heads
    }
}

/// Configuration for `sentence-transformers/all-MiniLM-L6-v2` — the
/// recommended first-bundled embedding model (Apache 2.0, ~22 MB on disk,
/// 384-dim output).
pub const MINILM_L6_V2: BertConfig = BertConfig {
    vocab_size: 30_522,
    hidden_size: 384,
    num_hidden_layers: 6,
    num_attention_heads: 12,
    intermediate_size: 1_536,
    max_position_embeddings: 512,
    type_vocab_size: 2,
    layer_norm_eps: 1.0e-12,
    hidden_act: String::new(), // "gelu" — populated by deserialization
    pad_token_id: 0,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn minilm_clone() -> BertConfig {
        let mut c = MINILM_L6_V2.clone();
        c.hidden_act = "gelu".to_string();
        c
    }

    #[test]
    fn minilm_l6_v2_shape() {
        let c = minilm_clone();
        assert_eq!(c.vocab_size, 30_522);
        assert_eq!(c.hidden_size, 384);
        assert_eq!(c.num_hidden_layers, 6);
        assert_eq!(c.num_attention_heads, 12);
        assert_eq!(c.intermediate_size, 1_536);
        assert_eq!(c.max_position_embeddings, 512);
    }

    #[test]
    fn head_dim_is_integer() {
        let c = minilm_clone();
        assert_eq!(c.head_dim(), 32);
        // 384 / 12 = 32 — confirms even division.
    }

    #[test]
    fn config_serde_roundtrip() {
        let c = minilm_clone();
        let json = serde_json::to_string(&c).unwrap();
        let back: BertConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn config_deserialises_huggingface_style() {
        // A config.json shape from HF is a strict superset of our fields;
        // here we test the subset we read.
        let json = r#"{
            "vocab_size": 30522,
            "hidden_size": 384,
            "num_hidden_layers": 6,
            "num_attention_heads": 12,
            "intermediate_size": 1536,
            "max_position_embeddings": 512,
            "type_vocab_size": 2,
            "layer_norm_eps": 1e-12,
            "hidden_act": "gelu",
            "pad_token_id": 0
        }"#;
        let c: BertConfig = serde_json::from_str(json).unwrap();
        assert_eq!(c.head_dim(), 32);
    }
}
