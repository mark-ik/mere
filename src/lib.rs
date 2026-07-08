/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! sibylla — a local-embedding and semantic-retrieval seam.
//!
//! One trait ([`EmbeddingProvider`]) that turns text into fixed-dimension
//! vectors, a [`SimilarityMetric`] each provider declares for its output space,
//! and a deterministic [`StubEmbeddingProvider`] so the embedding-to-index-to-
//! search pipeline tests with no GPU and no model. Model-backed and retrieval
//! pieces land behind features as they are ported from mere's `intel/embed`: a
//! pure-Rust vector index, a `SemanticSearch` facade, a burn-free lexical
//! embedder, and a burn-wgpu BERT backend (see `design_docs/`).
//!
//! Sibling to vates (generation). Where vates voices and foretells, sibylla is
//! the consulted corpus: it embeds and returns what is asked for.
//!
//! This founding cut is the seam plus the stub; the roadmap is in `design_docs/`.

pub mod provider;
pub mod stub;

pub use provider::{EmbedError, EmbeddingProvider, SimilarityMetric};
pub use stub::StubEmbeddingProvider;
