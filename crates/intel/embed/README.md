# mere-embed

Mere's glue over `esp::embed`: eidetic persistence for vector
indexes, and the quint field-algebra bridge that renders query similarity on the
graph canvas. Package `mere-embed`, library name `embed`.

The portable core (`EmbeddingProvider`, `SimilarityMetric`, `VectorIndex`,
`SemanticSearch`, `LexicalEmbeddingProvider`, `StubEmbeddingProvider`,
`affinity_pairs`, and the Burn-backed BERT provider behind `bert`) lives in
`esp::embed` and is re-exported here at the same paths, so
`embed::VectorIndex` and `esp::embed::VectorIndex` are the same type.

## Modules

| Module | Contents |
| --- | --- |
| `persistence` | `save_to_eidetic`, `load_from_eidetic`, `list_from_eidetic`, `vector_index_schema_ref`, `VECTOR_INDEX_SCHEMA_REF`. Async, over eidetic's typed-payload API (`save_typed` / `load_typed` / `list_typed`); JSON via serde, keyed by the returned `ManifestId`. |
| `field_bridge` | `build_query_similarity_field`, `register_query_similarity_field`. Renders per-node query similarity as a sum of weighted gaussians into a `quint::ast::ScalarField`, placed from a caller-supplied `HashMap<K, (f32, f32)>` of canvas positions. |
| `canvas_search` | `CanvasSearchSurface<K, P>`: a `SemanticSearch` plus node positions and an optional focus query. `ingest`, `forget`, `move_node`, `set_focus_query`, `clear_focus`, `search`, `search_focus`, `register_focus_field`, `set_sigma`. |

Re-exported from ESP: the `affinity`, `index`, `lexical`, `provider`,
`search`, `stub` modules, and `bert` under the `bert` feature.
`HashedEmbeddingProvider` is a deprecated alias for `StubEmbeddingProvider`.

## Features

| Feature | Effect |
| --- | --- |
| default | The three glue modules plus the `esp::embed` re-exports. |
| `bert` | Enables `esp/bert` and pulls `burn` (ndarray) for the backend type the integration test names. |
| `bert-wgpu` | `bert` plus `esp/bert-wgpu`, for `BertEmbeddingProvider<Wgpu>`. |

## Dependencies

`esp` (path, `crates/intel/esp`), `eidetic`, `quint`, `serde`, `serde_json`.
`burn` is optional, behind `bert`. Dev-dependencies:
`muniment` (its `MemoryBackend` is the test store), `pollster`.

## Tests

`tests/semantic_search.rs` runs on the default build.
`tests/bert_full_pipeline.rs` covers loading a real model through eidetic and is
`#[ignore]`d; it needs `MERE_MINILM_DIR` pointing at an `all-MiniLM-L6-v2`
directory in HuggingFace layout:

```bash
export MERE_MINILM_DIR=/path/to/all-MiniLM-L6-v2
cargo test -p mere-embed --features bert --test bert_full_pipeline -- --ignored
```

`scripts/capture_minilm_fixtures.py` captures reference vectors for ESP's
`bert::validation::FIXTURES`.

## Next

- Providers, index, search, BERT, and the Burn cosine kernel: `crates/intel/esp/src/embed`.
- `design_docs/mere_docs/research/2026-05-08_local_intelligence_integration_research.md`
- `design_docs/mere_docs/implementation_strategy/2026-07-06_intel_vector_index_burn_lift_plan.md`
