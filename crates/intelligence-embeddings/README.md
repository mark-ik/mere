# intelligence-embeddings

Embedding-provider trait, deterministic test provider, and pure-Rust flat vector index for Mere's statistical-intelligence tier.

This crate provides:

- `EmbeddingProvider` — the trait every embedding source implements (text → fixed-dimension vector).
- `HashedEmbeddingProvider` — a deterministic hash-based provider. Same input always produces the same vector; different inputs produce different but uncorrelated vectors. Useful for testing pipelines and demos that want stable embeddings without loading a model.
- `VectorIndex<K>` — a flat (dense) cosine/euclidean/dot-product index keyed by node identifiers. `O(N)` per query — fine for graphs up to ~10k nodes; HNSW comes in a follow-up slice.
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
cargo test -p intelligence-embeddings --features bert --test bert_full_pipeline -- --ignored
```

## Field-algebra integration

This crate intentionally does **not** depend on `graph-canvas`. The high-dimensional embedding space and the 2D field-algebra space don't compose cleanly without a chosen bridge (query-similarity scalar field, node-pair springs, or pre-projected 2D targets). The first feature served by this crate is **semantic search** (command-palette → `index.nearest(query, k)` → node keys), which needs no field-algebra wiring.

When a bridge is needed, it lives in a separate adapter crate or in `graph-canvas/scene_physics` (per the research report §5.3).
