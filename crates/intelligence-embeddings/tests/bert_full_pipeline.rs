/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! End-to-end pipeline test using a real BERT model.
//!
//! All tests are `#[ignore]`d by default — they require
//! `MERE_MINILM_DIR` pointing at a directory containing
//! `config.json`, `tokenizer.json`, and `model.safetensors`
//! (HF layout for `sentence-transformers/all-MiniLM-L6-v2`).
//!
//! Run:
//!
//! ```bash
//! export MERE_MINILM_DIR=/path/to/all-MiniLM-L6-v2
//! cargo test -p intelligence-embeddings --features bert --test bert_full_pipeline -- --ignored
//! ```
//!
//! These tests verify the **mechanical pipeline** works (load → embed →
//! index → search). Whether the embedding values match HF reference
//! vectors is the separate Tier-1 fixture test in
//! `src/bert/validation.rs`.

#![cfg(feature = "bert")]

use std::path::PathBuf;

use intelligence_embeddings::bert::BertEmbeddingProvider;
use intelligence_embeddings::{EmbeddingProvider, SemanticSearch};

type B = burn::backend::NdArray<f32>;

fn minilm_dir() -> Option<PathBuf> {
    std::env::var("MERE_MINILM_DIR").ok().map(PathBuf::from)
}

#[test]
#[ignore = "requires MERE_MINILM_DIR pointing at a real all-MiniLM-L6-v2 directory"]
fn loads_real_minilm_and_embeds_a_sentence() {
    let dir = minilm_dir().expect("MERE_MINILM_DIR must be set");
    let provider: BertEmbeddingProvider<B> =
        BertEmbeddingProvider::load(&dir, Default::default()).expect("load");

    assert!(provider.is_loaded());
    assert_eq!(provider.dimensions(), 384);

    let v = provider
        .embed_one("This is a sample sentence.")
        .expect("embed");
    assert_eq!(v.len(), 384);

    // Verify L2-normalized.
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!(
        (norm - 1.0).abs() < 1.0e-4,
        "expected L2-norm ~1.0, got {norm}"
    );

    // Verify no NaN/Inf.
    assert!(v.iter().all(|x| !x.is_nan() && !x.is_infinite()));
}

#[test]
#[ignore = "requires MERE_MINILM_DIR pointing at a real all-MiniLM-L6-v2 directory"]
fn semantic_search_with_real_minilm_finds_self_match() {
    let dir = minilm_dir().expect("MERE_MINILM_DIR must be set");
    let provider: BertEmbeddingProvider<B> =
        BertEmbeddingProvider::load(&dir, Default::default()).expect("load");

    let mut search = SemanticSearch::<u32, _>::new(provider);
    search.ingest(1, "This is a sample sentence.").unwrap();
    search.ingest(2, "The quick brown fox jumps.").unwrap();
    search.ingest(3, "Embeddings encode meaning.").unwrap();

    // Query identical to node 1's text → it should be the top hit with
    // cosine ~1.0.
    let result = search.search("This is a sample sentence.", 3).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].0, 1, "self-match should be top-1");
    assert!(
        (result[0].1 - 1.0).abs() < 1.0e-4,
        "self-match cosine should be ~1.0, got {}",
        result[0].1
    );
}

#[test]
#[ignore = "requires MERE_MINILM_DIR pointing at a real all-MiniLM-L6-v2 directory"]
fn semantic_search_with_real_minilm_clusters_similar_topics() {
    let dir = minilm_dir().expect("MERE_MINILM_DIR must be set");
    let provider: BertEmbeddingProvider<B> =
        BertEmbeddingProvider::load(&dir, Default::default()).expect("load");

    let mut search = SemanticSearch::<u32, _>::new(provider);
    // Three rust topics (1, 2, 3) interleaved with three cooking topics (4, 5, 6).
    search.ingest(1, "rust async programming").unwrap();
    search.ingest(4, "cooking pasta with garlic").unwrap();
    search.ingest(2, "tokio runtime internals").unwrap();
    search.ingest(5, "baking bread recipes").unwrap();
    search.ingest(3, "rust ownership and borrowing").unwrap();
    search.ingest(6, "italian dinner ideas").unwrap();

    // Query about rust → top 3 should all be rust topics (1, 2, 3).
    let result = search.search("rust language features", 3).unwrap();
    assert_eq!(result.len(), 3);
    let top_keys: Vec<u32> = result.iter().map(|(k, _)| *k).collect();
    let rust_keys = [1u32, 2, 3];
    let cooking_keys = [4u32, 5, 6];
    let rust_count = top_keys.iter().filter(|k| rust_keys.contains(k)).count();
    let cooking_count = top_keys.iter().filter(|k| cooking_keys.contains(k)).count();
    assert!(
        rust_count >= 2,
        "expected at least 2 rust topics in top-3, got {rust_count} \
         (this would fail if the embedding implementation has semantic drift \
         from HF reference — see bert/validation.rs Tier-1 fixtures)"
    );
    assert!(
        cooking_count <= 1,
        "expected at most 1 cooking topic in top-3, got {cooking_count}"
    );
}
