# embed

Embedding-provider trait, deterministic test provider, and pure-Rust flat
vector index for Mere's statistical-intelligence tier. Target topology:
`crates/intel/embed/`.

This crate provides:

- `EmbeddingProvider` — the trait every embedding source implements (text → fixed-dimension vector).
- `LexicalEmbeddingProvider` — pure-Rust, burn-free feature-hashing (the "hashing trick"). Texts that share vocabulary get correlated vectors, so it is a real (if shallow) lexical similarity signal with no model. The right default when you want clustering/recall without loading weights.
- `StubEmbeddingProvider` — a deterministic **test double** (formerly `HashedEmbeddingProvider`, now a deprecated alias). Same input → same vector, but different inputs → *uncorrelated* vectors, so cosine over it is meaningless except for exact-string matches. For exercising the pipeline in tests/demos only, never for real clustering or recall — reach for `LexicalEmbeddingProvider` (lexical) or the BERT provider (semantic) instead.
- `VectorIndex<K>` — a flat (dense) cosine/euclidean/dot-product index keyed by node identifiers. `O(N)` per query — fine for graphs up to ~10k nodes; the burn-batched-cosine and HNSW lifts are scoped in `design_docs/.../2026-07-06_intel_vector_index_burn_lift_plan.md`.
- `SimilarityMetric` enum.

Pure Rust, no GPU required. Compiles to `wasm32-unknown-unknown` for browser/PWA delivery.

## Status

The Burn-backed BERT provider is wired end-to-end (`features = ["bert"]`):

- `BertEmbeddingProvider::<B>::load(model_dir, device)` — single-call constructor that reads `config.json` + `tokenizer.json` + `model.safetensors` (HF layout) and returns a working provider.
- All BERT layers (`BertEmbeddings`, `BertSelfAttention`, `BertSelfOutput`, `BertAttention`, `BertIntermediate`, `BertOutput`, `BertLayer`, `BertEncoder`, `BertModel`) implemented in Burn 0.21 with `from_loaded` constructors that bypass random init.
- PyTorch `[out, in]` → Burn `[in, out]` Linear-weight transpose handled at the safetensors-extraction boundary.
- HF `LayerNorm.weight/bias` → Burn `gamma/beta` mapped at the construct boundary.

Outstanding empirical work (see `bert/validation.rs`):

1. Capture reference fixtures from any source (script provided: `scripts/capture_minilm_fixtures.py`).
2. Run the tier-1 fixture test against real weights → first run reveals whether numerical adjustments are needed (Linear transpose direction, GELU variant, attention scale factor — all small adjustments at known sites).
3. Tier-2 continuous validation (gated on `bert-validation` feature) wires a reference engine for drift detection.

End-to-end integration test in `tests/bert_full_pipeline.rs` runs against `MERE_MINILM_DIR`:

```bash
export MERE_MINILM_DIR=/path/to/all-MiniLM-L6-v2
cargo test -p embed --features bert --test bert_full_pipeline -- --ignored
```

## Field-algebra integration

This crate currently includes the Bridge-A graph-canvas integration:
`field_bridge` renders query similarity as a 2D scalar field, and
`canvas_search` packages semantic search + node positions + field
registration into a host-agnostic surface. Because of that transitional
bridge, the crate depends on `graph-canvas` today.

The long-term split is sharper:

- `intel/embed` owns providers, vector indexes, semantic search, and typed
  intelligence-signal production.
- `eidetic` stores model artifacts and persisted vector indexes; it does not
  own embedding/model logic.
- Graph-canvas-specific field adapters live beside
  `graphshell/graph/graph-canvas` or in an explicit adapter crate once the
  topology pass separates the graph bridge from the embedding core.
