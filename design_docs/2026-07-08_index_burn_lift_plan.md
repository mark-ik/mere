# Index burn-lift: batched cosine as a matmul

**Date:** 2026-07-08
**Status:** P1 (kernel + parity) landing this commit; P2/P3 are roadmap.
**Related:** the founding proposal (`2026-07-07_sibylla_founding_proposal.md`) for
the crate shape; re-homes the scoping first written mere-side as
`intel_vector_index_burn_lift_plan` now that the `VectorIndex` lives here.

## The insight

`VectorIndex::nearest` scores a query against every entry on the CPU, `O(N·d)` per
query — `O(Q·N·d)` for a batch of `Q`. That is a **matmul**: normalize both sides
to unit rows and the cosine similarity matrix is `queries · corpusᵀ`, `[Q,N]`.

Two reasons this earns a kernel rather than a footnote:

1. **It is the same tensor-program shape as a tensorized N-body force pass** — a
   dense pairwise interaction over `[Q,d]` and `[N,d]`, reduced. That program is
   measured to beat naive CPU on burn-wgpu from ~1k entries up (the mere Lane-5
   force-pass timings). All-pairs cosine is that program with a dot-product
   reduction instead of inverse-square.
2. **The flat index is a shared ceiling.** The same `VectorIndex` backs
   `SemanticSearch` (recall) and `affinity_pairs` (clustering). One kernel lifts
   both, plus any consumer downstream in mere / Isometry.

## The kernel (P1, this commit)

`index_burn::cosine_top_k<B: Backend>(queries, corpus, k, device) -> Vec<Vec<(usize, f32)>>`:

- Normalize rows of `queries` and `corpus` to unit L2 on `B`.
- `sim = queries · corpusᵀ` — one matmul, `[Q,N]`, the heavy `O(Q·N·d)` work.
- Read back `[Q,N]` and take each query's top-k on the CPU (cheap `O(N)` per row).

Deliberately uses only proven burn idioms (`matmul`, `swap_dims`, element-wise
square, `sum_dim`, `sqrt`, `recip`, f32 readback) — **no `topk`, no Int-tensor
readback** — so it is identical across the ndarray and wgpu backends without the
per-backend int-element hazard. Gated `index-burn` (ndarray) / `index-burn-wgpu`
(adds burn's wgpu backend); the default build stays serde-only and wasm-clean.

**Tested:** correctness against `VectorIndex::nearest` (the CPU path this
accelerates — same ranked indices, scores within 1e-5), all-pairs ranks self
first, zero-vector query scores zero, edge cases (empty / k=0 / k>N), and an
ndarray↔wgpu parity test gated on `index-burn-wgpu`.

## Roadmap

- **P2 — crossover measurement.** Time CPU-flat vs GPU-batched across `N` and `Q`
  (readback included, GPU warmed), for both all-pairs (`Q=N`, arrangement) and
  few-query (recall) shapes — different crossovers, both matter. Record where the
  matmul path overtakes the flat scan.
- **P3 — route the consumers.** `SemanticSearch::search` and `affinity_pairs` gain
  the burn fast-path above the measured crossover, behind the feature; the flat
  path stays the default below it and when the feature is off.

## Honest bounds

- **Exact, not approximate.** This is brute force on faster hardware, not HNSW; it
  defers the `O(N²)` asymptotics, it does not solve them. An HNSW index is the
  separate algorithmic path for the large tail (and the browser floor, where GPU
  compute is least certain).
- **The `[Q,N]` readback.** Top-k runs CPU-side over the full score matrix, so the
  readback is `O(Q·N)`; the `O(Q·N·d)` matmul is what moves to the GPU (for
  384-d embeddings the matmul dominates, so the readback is a fraction). A GPU
  top-k returning only `[Q,k]` would shrink it — a refinement, not P1.
- **Small N is fine on CPU.** This is a scaling lever; sequence P3 when a consumer
  actually pushes `N` up, not speculatively.
