/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Embedding-provider trait, deterministic test provider, Burn-backed BERT
//! provider, pure-Rust flat vector index, and field-algebra bridge for
//! Mere's statistical-intelligence tier.
//!
//! See `repos/mere/design_docs/mere_docs/research/2026-05-08_local_intelligence_integration_research.md`
//! for the architectural anchor.
//!
//! ## Quick start (with the `bert` feature)
//!
//! ```no_run
//! # #[cfg(feature = "bert")]
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use embed::bert::BertEmbeddingProvider;
//! use embed::SemanticSearch;
//!
//! type B = burn::backend::NdArray<f32>;
//!
//! // One-shot load: config.json + tokenizer.json + model.safetensors.
//! let provider: BertEmbeddingProvider<B> =
//!     BertEmbeddingProvider::load("/path/to/all-MiniLM-L6-v2", Default::default())?;
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

pub mod affinity;
#[cfg(feature = "bert")]
pub mod bert;
pub mod canvas_search;
pub mod field_bridge;
pub mod index;
pub mod lexical;
pub mod stub;
pub mod persistence;
pub mod provider;
pub mod search;

#[cfg(feature = "bert")]
pub use bert::{
    BGE_MICRO_V2, BertConfig, BertEmbeddingProvider, MINILM_L6_V2, SNOWFLAKE_ARCTIC_EMBED_XS,
};
pub use affinity::affinity_pairs;
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
pub use persistence::{load_from_eidetic, save_to_eidetic};
pub use provider::{EmbedError, EmbeddingProvider, SimilarityMetric};
pub use search::SemanticSearch;
