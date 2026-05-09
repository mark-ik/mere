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
//! use intelligence_embeddings::bert::BertEmbeddingProvider;
//! use intelligence_embeddings::SemanticSearch;
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

#[cfg(feature = "bert")]
pub mod bert;
pub mod field_bridge;
pub mod hashed;
pub mod index;
pub mod persistence;
pub mod provider;
pub mod search;

#[cfg(feature = "bert")]
pub use bert::{BertConfig, BertEmbeddingProvider, MINILM_L6_V2};
pub use field_bridge::{build_query_similarity_field, register_query_similarity_field};
pub use hashed::HashedEmbeddingProvider;
pub use index::{IndexError, VectorIndex};
pub use persistence::{load_from_eidetic, save_to_eidetic};
pub use provider::{EmbedError, EmbeddingProvider, SimilarityMetric};
pub use search::SemanticSearch;
