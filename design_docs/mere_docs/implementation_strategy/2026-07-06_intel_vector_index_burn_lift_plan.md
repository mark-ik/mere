# Intelligence-Tier Vector Index: the burn lift (and HNSW alternative)

**Date**: 2026-07-06
**Status**: scoped, not started. A cross-cutting scaling investment surfaced by the
Lane 5 P5 wiring; deliberately its own plan because it lifts three consumers at
once, not just arrangement.
**Related**: [burn_utilization_brief](../research/2026-07-04_burn_utilization_brief.md)
(Lane 1 GPU findings, Lane 5 force pass), [orrery_graph_intelligence_plan](2026-07-06_orrery_graph_intelligence_plan.md)
(where the O(N²) affinity scan lives), `crates/intel/embed/src/index.rs` (the flat
index this lifts), `crates/eidetic/eidetic-search` + `crates/intel/embed/src/canvas_search.rs`
(the other two consumers).

## The insight

The Lane 5 P4/P5 wiring runs `affinity_pairs`, which does a per-node
`VectorIndex::nearest` over a flat (dense, `O(N)`-per-query) index. Per node that
is `O(N)`, so the whole affinity signal is **`O(N²)`** cosine work on the CPU.

Two facts make this worth a dedicated plan rather than a buried "large graphs want
an actor" note:

1. **It is the same kernel shape as the Lane 5 force pass.** `aether::forces::repulsion`
   already computes an `O(N²)` all-pairs interaction as a burn tensor program
   (`[N,d]` broadcast against `[N,d]`, reduce), and Lane 5 P2 measured it beating
   naive CPU from ~7× at 1k to ~17× at 4k on burn-wgpu. All-pairs cosine is that
   exact program with a different reduction (dot / norms instead of inverse-square).
   So the affinity signal is not stuck at CPU `O(N²)`; it can be a burn kernel that
   already has a proven sibling in the codebase.
2. **The flat index is a shared ceiling, not an arrangement-only one.** The same
   `embed::index::VectorIndex` backs:
   - **arrangement** (`affinity_pairs`, this session),
   - **recall** (`eidetic-search` fuses the vector half against the lexical half),
   - **canvas search** (`embed::canvas_search` / `field_bridge`, the query-similarity
     field over canvas space).
   `index.rs`'s own docs say "an HNSW-backed implementation will follow once scale
   becomes a real constraint." One lift raises all three consumers together.

So this is a single investment with three beneficiaries and a ready-made kernel
template. That is unusual leverage and the reason it earns its own plan.

## Two lift paths (not mutually exclusive)

### Path A — batched cosine on burn (the "brute force is fine on GPU" lift)

Keep the flat, exhaustive index, but compute the all-pairs / query-vs-all cosine as
a **backend-generic burn tensor program**, sibling to `aether::forces`:

- `embed::index_burn::top_k_batched<B: Backend>(queries: [Q,d], corpus: [N,d], k) -> [Q,k]`
  (indices + scores), L2-normalized cosine as a matmul `[Q,d] · [N,d]^T -> [Q,N]`
  then a top-k reduce.
- Gated `index-burn` / `index-burn-wgpu` exactly like `field-burn` / `bert` — the
  default embed build stays pure-Rust and burn-free (the lexical/CPU path).
- `affinity_pairs` gets a burn fast-path above a crossover N (the L5 pattern: measure
  the crossover, route above it, keep the CPU path below where dispatch/readback
  dominate).

Wins where Path A is right: exact results, tiny code (one matmul + top-k), and it
reuses the L5 device-sharing story (D1). It stays `O(N²)` in FLOPs but on hardware
that eats `O(N²)` matmuls; the L5 numbers say that is a win from ~1k nodes.

### Path B — HNSW (the algorithmic lift)

An approximate-nearest-neighbour graph index (`O(log N)`-ish per query), CPU, no
burn. The right answer at large N where even a GPU `O(N²)` sweep stops paying, and
the answer for the browser/PWA target where GPU compute is least certain.

Path A and B compose: A is the cheap, exact, GPU-shaped lift that helps now (and is
one kernel); B is the algorithmic lift for the tail. A good sequencing is A first
(small, proven kernel shape, immediate three-consumer win at mid-N), B when a real
corpus makes `O(N²)`-anything the wrong asymptotics.

## Phases

### P1 — Batched-cosine burn kernel + parity
`embed::index_burn::top_k_batched`, backend-generic, `index-burn` / `index-burn-wgpu`
gated. ndarray↔wgpu parity test + a CPU-reference (naive flat `nearest`) correctness
anchor, the L5 `forces` test pattern. Done when batched top-k runs on both backends
matching the flat index's results.

### P2 — Crossover measurement across N and Q
Timing sweep (N, Q) CPU-flat vs GPU-batched, readback included, warmed, the L5
harness. Records where the batched GPU path beats the flat CPU scan for (a) all-pairs
(arrangement: Q=N) and (b) single/few queries (recall, canvas search: Q small). These
are different crossovers and both matter.

### P3 — Route the three consumers above the crossover
`affinity_pairs` (arrangement), the `eidetic-search` vector half (recall), and
`canvas_search` (query field) each gain the burn fast-path above their measured N,
behind the feature. Default build unchanged.

### P4 (optional / later) — HNSW for the tail
Only if a real corpus makes the GPU `O(N²)` sweep the wrong shape. Pure-Rust, all
targets, the browser answer.

## Done conditions

- The affinity / recall / canvas-search paths can run their nearest-neighbour work on
  burn (Path A) with recorded crossover N per query-shape, default build still
  burn-free.
- The brief's Lane 5 "large-graph force pass" story gains its sibling: the *affinity
  computation itself* is a burn kernel, not only the physics force.

## Honest bounds

- **The flat CPU index is fine at current N.** This is a scaling investment, not a
  present bottleneck; typical graphs are far below any crossover. Sequence it when a
  consumer actually pushes N up (a large imported corpus, a full-graph re-embed),
  not speculatively.
- **Readback shape.** Like the L5 force pass, GPU-computed neighbours round-trip to
  the CPU (the index consumers are CPU-side). The win is compute throughput, not a
  zero-copy path, until/unless a resident-data consumer exists (the D1 question).
- **Path A is still `O(N²)` FLOPs.** It defers the algorithmic problem by throwing
  proven-cheap GPU matmul at it; it does not solve the asymptotics. That is Path B.
- **Browser target.** GPU compute in the browser is the least-certain leg (Lane 1's
  wasm-embed receipt is a named follow-on). Path B (HNSW, CPU) is the portable floor
  there.
```
