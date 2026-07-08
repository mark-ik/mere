//! sibylla — a local-embedding and semantic-retrieval seam.
//!
//! One trait ([`EmbeddingProvider`]) that turns text into fixed-dimension
//! vectors, a [`SimilarityMetric`] each provider declares for its output space,
//! and a pure-Rust retrieval core over those vectors:
//!
//! - [`VectorIndex`] — a flat (dense) vector index, `O(N)` per query.
//! - [`SemanticSearch`] — the ingest-text / query-top-k facade over a provider
//!   plus an index.
//! - [`affinity_pairs`] — item-to-item similarity as `(a, b, weight)` clustering
//!   pairs.
//!
//! Two burn-free embedders ship in the default build:
//!
//! - [`LexicalEmbeddingProvider`] — feature-hashing (the "hashing trick"): texts
//!   that share vocabulary get correlated vectors, a real similarity signal with
//!   no model.
//! - [`StubEmbeddingProvider`] — a deterministic test double (same text → same
//!   vector, but unrelated texts → unrelated vectors), for exercising the
//!   embed-to-index-to-search pipeline with no GPU and no model.
//!
//! The default build is `serde` only and wasm-clean. The burn-wgpu BERT provider
//! (the strategic in-process semantic embedder) lands behind `bert` / `bert-wgpu`
//! features; see `design_docs/`.
//!
//! Sibling to vates (generation). Where vates voices and foretells, sibylla is
//! the consulted corpus: it embeds and returns what is asked for.

pub mod affinity;
pub mod index;
#[cfg(feature = "index-burn")]
pub mod index_burn;
pub mod lexical;
pub mod provider;
pub mod search;
pub mod stub;

pub use affinity::affinity_pairs;
pub use index::{IndexError, VectorIndex};
#[cfg(feature = "index-burn")]
pub use index_burn::cosine_top_k;
pub use lexical::LexicalEmbeddingProvider;
pub use provider::{EmbedError, EmbeddingProvider, SimilarityMetric};
pub use search::{SearchError, SemanticSearch};
pub use stub::StubEmbeddingProvider;
