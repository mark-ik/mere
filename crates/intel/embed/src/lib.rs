// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mere's statistical-intelligence glue over [`esp::embed`].
//!
//! The portable retrieval core — the [`EmbeddingProvider`] trait, [`VectorIndex`],
//! [`SemanticSearch`], the [`LexicalEmbeddingProvider`] + [`StubEmbeddingProvider`]
//! embedders, [`affinity_pairs`], and (behind the `bert` feature) the Burn-backed
//! BERT provider — lives in `esp::embed` and is re-exported here at the same paths.
//! This crate keeps only the pieces coupled to mere's own subsystems:
//!
//! - `persistence` — save/load a `VectorIndex` through eidetic's typed-payload API.
//! - `field_bridge` / `canvas_search` — project query similarity into quint's
//!   field algebra over the graph canvas.
//!
//! See `repos/mere/design_docs/mere_docs/research/2026-05-08_local_intelligence_integration_research.md`
//! for the architectural anchor and ESP's consolidation plan for the split.
//!
//! ## Quick start (with the `bert` feature)
//!
//! ```no_run
//! # #[cfg(feature = "bert")]
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use embed::SemanticSearch;
//!
//! // One-shot load: config.json + tokenizer.json + model.safetensors, on the
//! // portable CPU backend. No Burn type is named, so no Burn dependency.
//! let provider = embed::bert::load_cpu("/path/to/all-MiniLM-L6-v2")?;
//!
//! // Wrap in the search facade.
//! let mut search = SemanticSearch::<u32, _>::new(provider);
//! search.ingest(1, "rust async programming")?;
//! search.ingest(2, "tokio runtime internals")?;
//! search.ingest(3, "italian dinner recipes")?;
//!
//! // Query — top-k node ids ranked by cosine similarity.
//! let hits = search.search("rust language", 2)?;
//! for (id, score) in hits {
//!     println!("node {id}: {score:.4}");
//! }
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "bert"))]
//! # fn main() {}
//! ```

// The portable retrieval core is owned by ESP; re-export its modules at the
// same paths so the mere glue below (`use crate::index::…`, `crate::provider::…`,
// `crate::search::…`) and the `bert` provider bind to ESP's types with no churn.
pub use esp::embed::{affinity, index, lexical, provider, search, stub};

#[cfg(feature = "bert")]
pub use esp::embed::bert;
pub mod canvas_search;
pub mod field_bridge;
pub mod persistence;

pub use affinity::affinity_pairs;
#[cfg(feature = "bert")]
pub use bert::{
    BGE_MICRO_V2, BertConfig, BertEmbeddingProvider, MINILM_L6_V2, SNOWFLAKE_ARCTIC_EMBED_XS,
};
pub use canvas_search::CanvasSearchSurface;
pub use field_bridge::{build_query_similarity_field, register_query_similarity_field};
pub use index::{IndexError, VectorIndex};
pub use lexical::LexicalEmbeddingProvider;
pub use stub::StubEmbeddingProvider;

/// Deprecated alias for [`StubEmbeddingProvider`]. The old name read like a usable
/// provider; it is a test double whose vectors are not semantically meaningful (see
/// its docs). Use [`LexicalEmbeddingProvider`] for a real burn-free similarity
/// signal, or the BERT provider for semantic similarity.
#[deprecated(
    since = "0.0.1",
    note = "renamed to StubEmbeddingProvider (it is a test double); use LexicalEmbeddingProvider for a real burn-free signal"
)]
pub type HashedEmbeddingProvider = StubEmbeddingProvider;
pub use persistence::{list_from_eidetic, load_from_eidetic, save_to_eidetic};
pub use provider::{EmbedError, EmbeddingProvider, SimilarityMetric};
pub use search::SemanticSearch;
