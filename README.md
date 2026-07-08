# sibylla

A local-embedding and semantic-retrieval seam. One `EmbeddingProvider` trait
that turns text into fixed-dimension vectors, a `SimilarityMetric` each provider
declares for its output space, and a pure-Rust retrieval core over those vectors:
a flat `VectorIndex`, a `SemanticSearch` ingest/query facade, and an
`affinity_pairs` clustering helper. Two burn-free embedders ship in the default
build — `LexicalEmbeddingProvider` (feature-hashing: shared vocabulary yields
nearby vectors, a real signal with no model) and a deterministic
`StubEmbeddingProvider` test double. The whole default build is `serde` only and
wasm-clean; the burn-wgpu BERT backend is the porting roadmap.

```rust
use sibylla::{LexicalEmbeddingProvider, SemanticSearch};

let mut search = SemanticSearch::<u32, _>::new(LexicalEmbeddingProvider::new(512)?);
search.ingest(1, "rust async runtime internals")?;
search.ingest(2, "tokio scheduler and futures")?;
search.ingest(3, "italian pasta recipe")?;

// Top-k by cosine — the rust/async docs rank above the recipe.
let hits = search.search("async rust programming", 2)?;
assert_eq!(hits[0].0, 1);
```

Promoted from mere's `intel/embed`. This cut is the seam plus the portable
retrieval core (P0 + P1); the burn-wgpu BERT backend is the roadmap in
[`design_docs/`](design_docs/). Sibling to
[vates](https://github.com/mark-ik/vates) (generation): where vates voices and
foretells, sibylla is the consulted corpus — it embeds and returns what is asked
for.

License: MPL-2.0 (see the proposal's licensing note; the final choice is open).
