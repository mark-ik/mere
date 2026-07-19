// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! End-to-end pipeline: embed a corpus, build an index, run a query.
//!
//! Demonstrates the canonical first-feature flow that this crate enables:
//! semantic search via `index.nearest(query_embedding, k)`. The hashed
//! provider has no semantic structure (different inputs produce
//! uncorrelated vectors), so this test only verifies the *pipeline*
//! mechanics; semantic-quality tests arrive with the BERT provider in a
//! follow-up slice.

use embed::{EmbeddingProvider, StubEmbeddingProvider, VectorIndex};

#[test]
fn semantic_search_pipeline_roundtrip() {
    // 1. Provider — same shape a real BERT provider will have.
    let provider = StubEmbeddingProvider::new(64).unwrap();

    // 2. Corpus of node-text pairs. NodeKey here is a u32 stand-in.
    let nodes: Vec<(u32, &str)> = vec![
        (1, "rust async fundamentals"),
        (2, "tokio runtime internals"),
        (3, "python typing protocol"),
        (4, "haskell monads"),
        (5, "rust ownership and lifetimes"),
    ];

    // 3. Build the index from the provider's batch output.
    let texts: Vec<&str> = nodes.iter().map(|(_, t)| *t).collect();
    let vectors = provider.embed(&texts).unwrap();
    let mut index = VectorIndex::<u32>::new(provider.dimensions(), provider.metric());
    for ((id, _), vec) in nodes.iter().zip(vectors.into_iter()) {
        index.insert(*id, vec).unwrap();
    }

    assert_eq!(index.len(), nodes.len());

    // 4. Query: embed a query, find top-k neighbours.
    let query_vec = provider.embed_one("rust ownership and lifetimes").unwrap();
    let result = index.nearest(&query_vec, 3).unwrap();

    assert_eq!(result.len(), 3);
    // Identical input must return cosine ≈ 1 as the top hit.
    assert_eq!(result[0].0, 5);
    assert!((result[0].1 - 1.0).abs() < 1.0e-5);
}

#[test]
fn empty_corpus_returns_no_results() {
    let provider = StubEmbeddingProvider::new(32).unwrap();
    let index = VectorIndex::<u32>::new(provider.dimensions(), provider.metric());
    let query = provider.embed_one("anything").unwrap();
    let result = index.nearest(&query, 5).unwrap();
    assert!(result.is_empty());
}

#[test]
fn updating_a_node_overwrites_its_vector() {
    let provider = StubEmbeddingProvider::new(32).unwrap();
    let mut index = VectorIndex::<u32>::new(provider.dimensions(), provider.metric());

    let v1 = provider.embed_one("first version").unwrap();
    index.insert(42, v1).unwrap();

    let v2 = provider
        .embed_one("second version, totally different content")
        .unwrap();
    index.insert(42, v2.clone()).unwrap();

    assert_eq!(index.len(), 1);
    assert_eq!(*index.get(&42).unwrap(), v2);
}

#[test]
fn removing_a_node_excludes_it_from_search() {
    let provider = StubEmbeddingProvider::new(32).unwrap();
    let mut index = VectorIndex::<u32>::new(provider.dimensions(), provider.metric());
    index
        .insert(1, provider.embed_one("alpha").unwrap())
        .unwrap();
    index
        .insert(2, provider.embed_one("beta").unwrap())
        .unwrap();
    index
        .insert(3, provider.embed_one("gamma").unwrap())
        .unwrap();

    let query = provider.embed_one("alpha").unwrap();
    let before = index.nearest(&query, 5).unwrap();
    assert_eq!(before[0].0, 1);

    index.remove(&1);
    let after = index.nearest(&query, 5).unwrap();
    assert_eq!(after.len(), 2);
    assert!(after.iter().all(|(k, _)| *k != 1));
}

#[test]
fn batch_embed_then_insert_matches_one_by_one() {
    let provider = StubEmbeddingProvider::new(32).unwrap();
    let texts = ["a", "b", "c", "d"];

    let batch = provider.embed(&texts).unwrap();
    let mut idx_batch = VectorIndex::<u32>::new(provider.dimensions(), provider.metric());
    for (i, v) in batch.into_iter().enumerate() {
        idx_batch.insert(i as u32, v).unwrap();
    }

    let mut idx_each = VectorIndex::<u32>::new(provider.dimensions(), provider.metric());
    for (i, t) in texts.iter().enumerate() {
        idx_each
            .insert(i as u32, provider.embed_one(t).unwrap())
            .unwrap();
    }

    for k in 0..texts.len() as u32 {
        assert_eq!(idx_batch.get(&k), idx_each.get(&k));
    }
}
