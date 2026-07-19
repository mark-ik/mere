// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

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
//! cargo test -p embed --features bert --test bert_full_pipeline -- --ignored
//! ```
//!
//! These tests verify the **mechanical pipeline** works (load → embed →
//! index → search). Whether the embedding values match HF reference
//! vectors is the separate Tier-1 fixture test in
//! `src/bert/validation.rs`.

#![cfg(feature = "bert")]

use std::collections::HashMap;
use std::path::PathBuf;

use quint::eval::eval_scalar;
use quint::projection::FieldProjection;
use quint::registry::FieldRegistry;
use async_trait::async_trait;
use eidetic::{
    ModelLibrary, ModerationState, NoFetcher, PrivacyClass, ProvenanceOrigin, ProvenanceRecord,
    Store, Timestamp, TrustEnvelope, TrustLevel,
};
use embed::bert::BertEmbeddingProvider;
use embed::{
    EmbeddingProvider, SemanticSearch, VectorIndex, register_query_similarity_field,
};

type B = burn::backend::NdArray<f32>;

/// Minimal in-memory store used by the eidetic round-trip test below.
/// Mirrors the shape of `eidetic::ModelLibrary` tests' helper without
/// pulling in a storage backend (fjall is a separate crate).
// The in-memory test store is muniment's (2026-07-12): the
// hand-rolled one was the same map behind the same seam.
use muniment::MemoryBackend as InMemoryStore;




fn test_provenance() -> ProvenanceRecord {
    ProvenanceRecord {
        origin: ProvenanceOrigin::Imported {
            source: "hf:sentence-transformers/all-MiniLM-L6-v2".to_string(),
        },
        upstream: Vec::new(),
        tooling: Some("embed::bert_full_pipeline".to_string()),
        generated_at: Timestamp(0),
    }
}

fn test_trust() -> TrustEnvelope {
    TrustEnvelope {
        level: TrustLevel::SelfAsserted,
        signatures: Vec::new(),
        moderation_state: ModerationState::Unreviewed,
    }
}

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

/// **Field-bridge with real BERT** — proves the trait abstraction holds:
/// `register_query_similarity_field` (which the `field_bridge` unit tests
/// already exercise with `StubEmbeddingProvider`) produces a field that
/// peaks at semantically-similar nodes when fed real BERT embeddings.
///
/// Setup: 6 nodes, 3 about rust (left side of canvas) + 3 about cooking
/// (right side). Query embedding for "rust language features" should
/// produce a field whose value at left-side positions is higher than at
/// right-side positions.
#[test]
#[ignore = "requires MERE_MINILM_DIR pointing at a real all-MiniLM-L6-v2 directory"]
fn field_bridge_with_real_bert_separates_similar_from_dissimilar() {
    let dir = minilm_dir().expect("MERE_MINILM_DIR must be set");
    let provider: BertEmbeddingProvider<B> =
        BertEmbeddingProvider::load(&dir, Default::default()).expect("load");

    // Lay out 6 nodes on a canvas: rust topics on the left (x < 0),
    // cooking topics on the right (x > 0). The y-axis is irrelevant
    // here; we're testing the x-axis cluster separation.
    let nodes: &[(u32, &str, (f32, f32))] = &[
        (1, "rust async programming", (-200.0, 0.0)),
        (2, "tokio runtime internals", (-200.0, 100.0)),
        (3, "rust ownership and borrowing", (-200.0, -100.0)),
        (4, "cooking pasta with garlic", (200.0, 0.0)),
        (5, "baking bread recipes", (200.0, 100.0)),
        (6, "italian dinner ideas", (200.0, -100.0)),
    ];

    let texts: Vec<&str> = nodes.iter().map(|(_, t, _)| *t).collect();
    let vectors = provider.embed(&texts).expect("embed batch");

    let mut index = VectorIndex::<u32>::new(provider.dimensions(), provider.metric());
    let mut positions = HashMap::new();
    for ((id, _, pos), vec) in nodes.iter().zip(vectors.into_iter()) {
        index.insert(*id, vec).unwrap();
        positions.insert(*id, *pos);
    }

    let query_vec = provider.embed_one("rust language features").unwrap();

    // Build the query-similarity field via the same path
    // graph-canvas hosts use. Sigma chosen ~half the inter-node spacing.
    let mut projection = FieldProjection::new();
    let _id = register_query_similarity_field(
        &mut projection,
        "rust_focus",
        &query_vec,
        &index,
        &positions,
        80.0,
    );

    // Evaluate the field at each node's position.
    let registry = FieldRegistry::new();
    let field = projection.registry.get(_id).expect("field registered");
    let quint::registry::FieldDef::Scalar(field) = field else {
        panic!("expected scalar field");
    };

    let mut rust_total = 0.0_f32;
    let mut cooking_total = 0.0_f32;
    for (id, _, (x, y)) in nodes {
        let v = eval_scalar(field, &registry, *x, *y, 0.0);
        if (*id) <= 3 {
            rust_total += v;
        } else {
            cooking_total += v;
        }
    }

    // Field should peak on the rust side (positive cosine similarity to
    // the query) and be lower on the cooking side. We don't require
    // cooking_total < 0 because some cosine spillover is normal — but the
    // rust-side total should clearly exceed the cooking-side total.
    assert!(
        rust_total > cooking_total,
        "rust side ({rust_total}) should exceed cooking side ({cooking_total}) — \
         indicates real semantic separation in the field"
    );

    // Sanity: at least one rust-side node should have field > 0.5 (the
    // query is semantically similar to its embedding, modulo gaussian
    // leakage from neighbours).
    let rust_peak = nodes
        .iter()
        .filter(|(id, _, _)| *id <= 3)
        .map(|(_, _, (x, y))| eval_scalar(field, &registry, *x, *y, 0.0))
        .fold(f32::MIN, f32::max);
    assert!(
        rust_peak > 0.5,
        "expected at least one rust-side node with field > 0.5, peak was {rust_peak}"
    );
}

/// **Phase 5 done condition** — closes the eidetic↔embed
/// boundary with real MiniLM bytes.
///
/// Reads `config.json` / `tokenizer.json` / `model.safetensors` from
/// `MERE_MINILM_DIR`, saves them through `eidetic::ModelLibrary`,
/// resolves them back via `resolve_components`, and builds a
/// `BertEmbeddingProvider` from the resolved bytes. Embedding output
/// must match a directly-loaded provider over the same model dir to
/// within float-rounding tolerance — proving the eidetic round-trip is
/// byte-faithful end to end (BLAKE3 hash check on weights + tokenizer,
/// inline JSON config preserved through serde round-trip).
#[test]
#[ignore = "requires MERE_MINILM_DIR pointing at a real all-MiniLM-L6-v2 directory"]
fn minilm_round_trips_through_eidetic_and_inference_matches_direct_load() {
    let dir = minilm_dir().expect("MERE_MINILM_DIR must be set");

    // Reference: build a provider directly from the model dir, embed a
    // sentence. The eidetic-resolved provider must reproduce this output.
    let direct: BertEmbeddingProvider<B> =
        BertEmbeddingProvider::load(&dir, Default::default()).expect("direct load");
    let reference = direct
        .embed_one("This is a sample sentence.")
        .expect("reference embedding");

    // Read the three artifacts as bytes — the format eidetic stores them in.
    let config_bytes = std::fs::read(dir.join("config.json")).expect("read config.json");
    let tokenizer_bytes = std::fs::read(dir.join("tokenizer.json")).expect("read tokenizer.json");
    let weights_bytes =
        std::fs::read(dir.join("model.safetensors")).expect("read model.safetensors");

    // Save through the layered stack: weight + tokenizer get content-addressed
    // opaque-blob manifests, the ModelManifest engram references both.
    let config_value: serde_json::Value =
        serde_json::from_slice(&config_bytes).expect("parse config.json");
    let mut store = InMemoryStore::default();
    let mut fetcher = NoFetcher;
    let manifest_id = pollster::block_on(ModelLibrary::save_model_with_components(
        &mut store,
        "sentence-transformers/all-MiniLM-L6-v2",
        "bert",
        "Apache-2.0",
        config_value,
        &weights_bytes,
        &tokenizer_bytes,
        Vec::new(),
        Vec::new(),
        PrivacyClass::LocalOnly,
        test_provenance(),
        test_trust(),
        Timestamp(0),
    ))
    .expect("save model through eidetic");

    // Resolve back — runs Layer 2 BLAKE3 verification on each blob.
    let resolved_model = pollster::block_on(ModelLibrary::resolve_components(
        &mut store,
        &mut fetcher,
        manifest_id,
    ))
    .expect("resolve_components ok")
    .expect("components present after save");

    assert_eq!(
        resolved_model.manifest.model_id,
        "sentence-transformers/all-MiniLM-L6-v2"
    );
    assert_eq!(resolved_model.manifest.architecture, "bert");
    assert_eq!(
        resolved_model.manifest.license.as_deref(),
        Some("Apache-2.0")
    );
    assert_eq!(
        resolved_model.components.weight_bytes, weights_bytes,
        "weight bytes preserved"
    );
    assert_eq!(
        resolved_model.components.tokenizer_bytes, tokenizer_bytes,
        "tokenizer bytes preserved"
    );

    // Build a provider from the eidetic-resolved bytes.
    let resolved: BertEmbeddingProvider<B> = BertEmbeddingProvider::from_bytes(
        &resolved_model.components.config_bytes,
        &resolved_model.components.tokenizer_bytes,
        &resolved_model.components.weight_bytes,
        Default::default(),
    )
    .expect("from_bytes via eidetic-resolved bytes");
    assert_eq!(resolved.dimensions(), 384);

    // Inference must match the directly-loaded reference. Same input,
    // same weights, same config → same output (modulo float determinism
    // within a single backend, which is exact here).
    let via_eidetic = resolved
        .embed_one("This is a sample sentence.")
        .expect("embedding via eidetic-resolved provider");
    assert_eq!(reference.len(), via_eidetic.len());
    let max_diff = reference
        .iter()
        .zip(via_eidetic.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        max_diff < 1.0e-5,
        "eidetic round-trip changed embedding output: max_diff = {max_diff} \
         (expected ~0 — same model bytes, same input)"
    );
}
