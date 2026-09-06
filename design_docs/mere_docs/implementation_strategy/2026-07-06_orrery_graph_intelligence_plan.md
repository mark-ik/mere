# Orrery Graph Intelligence Plan (burn brief, Lane 5)

**Date**: 2026-07-06
**Status**: P1-P6 landed (2026-07-06). Force pass (P1-P3) + semantic-arrangement
bridge (P4) + live meerkat content-affinity wiring (P5) + blended affinity and
content-text enrichment (P6) are all in and tested. Open: the `semantic-embeddings`
BERT-provider upgrade (a separate slice), the off-thread embedding actor (raw-body
text + the intel-index lift), and the P3 live force-pass injection (held — niche).
**Related**: [burn_utilization_brief](../research/2026-07-04_burn_utilization_brief.md) (Lane 5), [burn_wgpu_flip_plan](2026-07-04_burn_wgpu_flip_plan.md) (L1: shipped burn-wgpu embeddings + aether field lowering + the CPU-vs-GPU timing methodology this reuses), [graph_signals_layer_plan](../../archive_docs/2026-08-20_completed_plans/2026-06-22_graph_signals_layer_plan.md) (the consumer for similarity edges), `crates/orrery/aether` *(historical citation)* <!-- doc-audit: historical-path --> (the burn-lowering home; gyre stays burn-free), `crates/intel/embed` *(historical citation)* <!-- doc-audit: historical-path --> (`field_bridge` / `canvas_search`: the embedding→field seam already built).

## Scope

Lane 5's two done-conditions, in leverage/tractability order:

1. **Tensorized force pass** (this session): an N-body repulsion computed on
   burn, measured CPU (ndarray) vs GPU (wgpu) across N, to find where GPU
   beats gyre's CPU force path. Closes the L1 finding that GPU needs a heavy
   program to pay — O(N²) is that program, unlike L1's cheap elementwise field.
2. **Semantic arrangement**: embedding clusters drive an orrery arrangement
   (the `field_bridge` similarity field already exists); similarity edges feed
   the graph-signals layer.

gyre stays burn-free: the force pass lives in aether (the field source), gyre
consumes the resulting forces. Out of scope: Lane 4 (training), the cond-1
`<orrery>` element.

## Honest framing (the cap the brief names)

- **N-body does not fit the field-algebra AST.** aether's `ScalarField`/
  `VectorField` are fields over canvas space with *fixed* parameters (one
  Gaussian center, one Linear normal). An N-body force is parameterized by all
  N *dynamic* source positions, so it is a dedicated burn function in aether
  (`forces::repulsion`), not a lowering of an AST node. The brief's "written in
  the field algebra" is aspirational; the honest implementation is a burn
  kernel that lives in aether beside `lower_burn`, backend-generic the same way.
- **Positions round-trip to the CPU** while gnodes are DOM transforms, so the
  near-term win is raw force-compute throughput at large N, not a render
  shortcut. gyre already has Barnes-Hut (O(N log N) CPU), so a naive O(N²) GPU
  pass only wins above some crossover N — finding that N *is* the deliverable.

## Phases

### P1 — The force kernel + parity (this session)

`aether::forces::repulsion<B: Backend>(xs, ys, params) -> (fx, fy)`: pairwise
inverse-square (softened) repulsion, `[N]` in → `[N]` force components out, via
broadcasting (`[N,1]` vs `[1,N]` differences). Gated on `field-burn` /
`field-burn-wgpu` like `lower_burn`. Softening length avoids the self / near-zero
singularity. ndarray↔wgpu parity test (the L1 pattern). Done when repulsion runs
on both backends with matching output.

### P2 — CPU-vs-GPU timing across N (this session)

Ignored timing test across N (256 / 1k / 4k / 16k), CPU vs GPU, readback
included, GPU warmed — the L1 harness shape. Records the crossover N where GPU
wins. Done when the numbers are in the plan and the brief's L5 force-pass
done-condition has its measured N.

### P3 — gyre integration seam (landed 2026-07-06)

A host-injected `RepulsionSolver` closure on the `Simulation`
(`set_repulsion_solver`): above a threshold `NodeExclusion` routes its
all-pairs scan to it instead of the naive O(n²) path. gyre stays burn-free —
the closure is a plain `Fn(&[f32],&[f32],f32,f32)->(Vec,Vec)`; the host builds
it from `aether::forces::repulsion_wgpu`. Landed with a routing test (mock
solver: below threshold naive, above threshold the solver's forces reach the
bodies) and an end-to-end settle benchmark behind a `gpu-bench` feature (the
only path that compiles burn into gyre's build).

**Measured, in the real gyre tick (naive-cpu vs the wgpu solver):**

| N | naive CPU | GPU solver | whole-tick speedup |
| --- | --- | --- | --- |
| 2,000 | 12.75ms | 6.35ms | 2.01× |
| 4,000 | 31.55ms | 19.05ms | 1.66× |
| 8,000 | 86.45ms | 58.45ms | 1.48× |
| 16,000 | 271.2ms | 190.9ms | 1.42× |

**The honest correction to P2**: in-context the GPU win is **1.4-2×**, not the
17× isolated number, for two reasons — (1) gyre's `NodeExclusion` has a
`cutoff` that cheaply skips far pairs, so its real CPU work is far below a
cutoff-free O(n²); (2) rapier's own step is a shared floor that grows with N
and dominates more at scale (which is why the *ratio shrinks* with N even as
absolute savings grow, 6→80ms/tick). Barnes-Hut (also in the crate, not the
default) would narrow the CPU gap further. So the GPU pass is a real but modest
large-graph win, and it only activates above the threshold (default 1000), so
typical small orreries never touch it.

**Not yet wired into meerkat's live orrery** — the seam exists and the value is
established, but the payoff is niche (1000+ node graphs), so the live injection
(build the closure behind a meerkat feature, call `set_repulsion_solver` where
the orrery builds its sim) is held pending whether large-graph settle is a
priority. The measurement is the deliverable; the wiring is a thin, low-urgency
follow-on.

### P4 — Semantic arrangement (mechanism landed 2026-07-06)

The mechanism turned out to already exist on the gyre side: `AffinitySpring`
(a weighted attract-only pairwise spring, docs: "structural Jaccard now, a
content-embedding cosine later") clusters high-affinity pairs under the
force-directed layout. So P4 is the **embedding→affinity bridge**, not a new
arrangement type: `embed::affinity::affinity_pairs(index, top_k, min_sim)`
turns a node-embedding `VectorIndex` into the `(a, b, weight)` triples
`AffinitySpring::new` consumes (each node's top-K nearest above a threshold,
weight = clamped cosine, symmetric-deduped). Pure over the index, no gyre dep;
the host maps `K → NodeKey` and installs the spring.

Verified **end-to-end at the seam**, each half tested: embed's bridge produces
a signal that reflects clusters (a two-cluster fixture yields every
intra-cluster pair and no cross-cluster leak) and gyre's existing
`AffinitySpring` tests prove high-affinity pairs draw together (and out-pull a
low-affinity pair). The composition is a direct type match:
`AffinitySpring::new(affinity_pairs(&index, k, thresh)?)`.

**Not yet wired into meerkat's live orrery** — that needs node-content
embeddings (an `EmbeddingProvider` over each node) plus the recompute-on-
mutation discipline `set_affinity_force` already expects, then the orrery hook.
Higher value than P3's wiring (semantic clustering helps at any graph size),
and the natural next slice. The similarity-edges-as-graph-signals producer
(the [graph_signals_layer_plan](../../archive_docs/2026-08-20_completed_plans/2026-06-22_graph_signals_layer_plan.md)
consumer) rides the same `affinity_pairs` output.

### P5 — Live meerkat wiring, content-affinity half (landed 2026-07-06)

P4 is now driven **end to end in meerkat's focused orrery**, behind an
off-by-default `content-affinity` feature (the shipped build's affinity toggle
stays structural). Three pieces, each tested:

- **Orrery seam (burn-free).** `Orrery::set_content_affinity(Option<Vec<(NodeKey,
  NodeKey, f32)>>)` injects a host-computed content signal that **supersedes the
  internal structural-Jaccard one under the existing `cluster_by_affinity`
  toggle**. Dirty-gated (host-fresh — it tracks node *content*, so a topology-only
  revision bump must not reinstall it), `Some(empty)` is authoritative-but-inert
  (clears the force rather than falling back to structural), `None` reverts to
  structural. The single physics affinity-force slot makes content and structural
  mutually exclusive by construction. 3 orrery tests: supersede + revert,
  empty-inert, and reinstall-across-a-toggle-cycle (the dirty flag is re-armed on
  toggle-off, else off→on would leave content uninstalled). gyre/orrery never see a
  tensor type — plain `(NodeKey, NodeKey, f32)` triples cross the seam.

- **Lexical provider (embed).** A new `embed::LexicalEmbeddingProvider` — the
  feature-hashing "hashing trick", pure-Rust, **burn-free**. The distinction from
  the existing `HashedEmbeddingProvider` is load-bearing: that one hashes the
  *whole string* to a seed, so it is deliberately *semantically meaningless* (a
  test double); this hashes *per token*, so texts that **share vocabulary get
  correlated vectors** — a real, cheap *lexical* clustering signal with no model.
  It is the honest light default; deep-semantic is the BERT upgrade. 7 embed tests
  (shared-token pairs out-score disjoint, case/punct fold, L2-normalized, zero
  vector on tokenless text).

- **Meerkat driver.** `content_affinity` module (mirrors `infer_host`):
  `ContentArrangement` bundles the provider + a recompute gate; `compute_content_
  affinity` embeds each node's **title + tags**, indexes it, and derives top-K
  cosine pairs via `embed::affinity_pairs`. Driven from `render_orrery_scene` while
  the toggle is on — `maybe_recompute` fires only when the graph revision moved
  **and** a 750 ms throttle floor elapsed (a settled graph costs nothing; a
  mutation burst is coalesced, a throttled pass retries later). The arrangement is
  `take`n out to break the self-borrow (it lives on `content`, the graph on the
  pane — the gnode-pool idiom). 4 meerkat tests (valid/symmetric/deduped pairs,
  two-node floor, revision-gate, throttle).

**Honest bounds (carried forward):**

- **Lexical, not semantic, today.** Shared surface vocabulary clusters; a
  paraphrase with no shared tokens does not. True semantic similarity is the
  `semantic-embeddings` upgrade — a Burn-backed BERT provider — a **separate
  slice**: it wants an embed boxed loader mirroring
  `infer::decoder::load_wgpu_provider` (so meerkat never names a burn type), a
  checkpoint + env var (the `infer_host` pattern), and — for the wgpu backend — the
  D1 device decision. `build_embedding_provider` is already the single extension
  point (it returns `Box<dyn EmbeddingProvider>`), so only that function changes.
- **O(N²) per recompute.** `affinity_pairs` runs a per-node nearest scan over the
  flat index; the throttle + revision gate keep it off the steady-state path, but
  BERT-scale or large-graph embedding wants an off-thread actor (the infer actor is
  the template). Noted, not built.
- **Focused pane only.** Secondary orreries keep structural affinity this slice.
- **Node text = title + tags** (+ literal property descriptions as of P6). The
  cheap, always-present content; the full raw page body is a P6-noted follow-on.

### P6 — Blended affinity + content-text enrichment (landed 2026-07-06)

Two refinements to the P4/P5 signal, both landed and tested.

**Blended affinity.** Structural (Jaccard) and content (embedding) affinity were
mutually exclusive under P5 (content superseded structural). They are genuinely
different signals — two nodes can share neighbours, share meaning, or both — so
they now *combine*. A new `AffinityBlend` mode on the orrery selects how:

- `Blend` (default): a **noisy-OR** of the two weights, `w = 1 − (1−s)(1−c)`. A
  pair is drawn together if either signal likes it, harder if both (0.8 with 0.8 →
  0.96). Degrades to whichever signal is present, so it is structural-only when no
  content is injected — the default meerkat build is unchanged.
- `ContentOnly`: the P5 supersede behaviour (content wins when present).
- `StructuralOnly`: ignore any injected content.

`set_affinity_blend` forces a rebuild; the merge is a pure `blend_affinity_pairs`
over the two pair lists, keyed by unordered pair. gyre still receives one
`AffinitySpring` (the single force slot) — the blend happens before install. The
affinity seam now has 5 orrery tests (supersede+revert and empty-inert under
`ContentOnly`, reinstall-across-toggle, blend-unions-the-pairs, and the noisy-OR
weight math); full orrery suite 90/90.

**Content-text enrichment.** `node_text` (the meerkat embedding input) now folds
in each node's literal **property values** — the `schema:description` /
`og:description` and kin that ingest already extracts and stores on the node —
alongside title + tags. That is the page's own summary of its content: real
content text, already on the node, so no cache read and no capture-consent gate
(properties are graph data, not the browsing trail). URL-valued properties are
dropped (scheme/host tokens are noise). The full **raw page body** (the eidetic
content cache — on by default, gated only on the store opening, *not*
consent-gated) is a richer but noisier source; its per-node `engine_document_for`
parse over the whole graph is O(N) on the UI thread, so it rides the off-thread
embedding actor (the same actor the P3/P5 scaling notes call for), not this slice.
The intel-tier index lift that actor would also want is scoped in
[intel_vector_index_burn_lift_plan](2026-07-06_intel_vector_index_burn_lift_plan.md).

## Findings

- 2026-07-06: `embed::field_bridge` (233 LOC) + `canvas_search` (349 LOC)
  already build a query-similarity scalar field over canvas space and search
  it — the semantic-arrangement foundation (P4) exists; P4 is consumption, not
  new seam work.
- 2026-07-06, **force-pass timing** (release, Windows laptop, default wgpu
  adapter, readback included, GPU warmed; naive O(N²) both sides):

  | N | ndarray CPU | wgpu GPU | speedup |
  | --- | --- | --- | --- |
  | 256 | 0.86ms | 1.80ms | 0.48× (CPU wins) |
  | 1,000 | 11.3ms | 1.48ms | 7.6× |
  | 4,000 | 177ms | 10.5ms | 17× |
  | 16,000 | 3,389ms | 252ms | 13.5× |

  Crossover ~500-1000 nodes. Below it, dispatch + readback dominate the small
  O(N²) and CPU wins; above it GPU pulls away hard (16k: CPU an unusable 3.4s
  vs GPU 0.25s). This is the **inverse** of the L1 field-eval result, where the
  cheap elementwise program had CPU winning through 100k — the heavy O(N²)
  program is exactly the "heavier program" L1 said GPU needs to pay off. The
  brief's L5 force-pass done-condition ("beats the CPU path at a measured node
  count") is met, crossover measured.
- **Caveat carried to P3**: this is GPU-O(N²) vs CPU-O(N²). gyre's real force
  path is Barnes-Hut O(N log N) on CPU, much faster than naive O(N²) CPU, so
  the crossover *against gyre's actual path* is higher than ~1000 and is P3's
  measurement — the real product question is where the GPU O(N²) pass beats
  gyre's Barnes-Hut, not naive CPU.

## Progress

- 2026-07-06 — **P6 landed (blended affinity + content-text enrichment)**. Structural
  and content affinity now combine via an `AffinityBlend` mode (default `Blend` = a
  noisy-OR `1−(1−s)(1−c)` of the two weights; `ContentOnly`/`StructuralOnly` retained),
  the merge a pure `blend_affinity_pairs` before the single gyre force install; orrery
  suite 90/90 (2 new blend tests + the 2 supersede tests moved to `ContentOnly`).
  `node_text` folds in each node's literal property descriptions (schema/OG), the
  page's own content summary already on the node — no cache read, no consent gate. The
  raw page body (eidetic content cache, on by default, not consent-gated) is the
  richer-but-noisier follow-on that wants the off-thread embedding actor (O(N) parse).
  Also this session: the meaningless-by-design `HashedEmbeddingProvider` renamed to
  `StubEmbeddingProvider` (deprecated alias kept) so it stops reading like a usable
  provider, and D1 re-scoped in the brief now that embedding is a third GPU consumer.
- 2026-07-06 — **P5 landed (live meerkat wiring, content-affinity half)**. The P4
  bridge is now driven end-to-end in the focused orrery behind an off-by-default
  `content-affinity` feature. Three verified pieces: (1) `Orrery::set_content_affinity`
  — a burn-free injected signal that supersedes structural Jaccard under the existing
  toggle (dirty-gated, empty-is-inert, reverts on `None`); 3 orrery tests, and the
  full orrery suite stays green (88/88 — no regression from the `sync_affinity_force`
  rework). (2) A new burn-free `embed::LexicalEmbeddingProvider` (feature-hashing
  lexical similarity — the *honest* light default, unlike the whole-string
  `HashedEmbeddingProvider`); 7 embed tests. (3) A meerkat `content_affinity` driver
  (`ContentArrangement` recompute-gate: revision + 750 ms throttle; embeds
  title + tags; injects in `render_orrery_scene`, focused pane only); 4 meerkat tests.
  Lexical now, BERT (`semantic-embeddings`) the upgrade — `build_embedding_provider`
  is the extension point. gyre/orrery stay burn-free (plain triples); the default
  meerkat build is unchanged (feature off → structural affinity, compiles clean).
- 2026-07-06 — **P4 mechanism landed**. `embed::affinity::affinity_pairs`
  bridges an embedding `VectorIndex` to gyre `AffinitySpring`'s `(a,b,weight)`
  signal (top-K nearest above a threshold, clamped-cosine weight, symmetric
  dedup). embed tests prove the signal reflects clusters (two-cluster fixture:
  all intra-cluster pairs, no cross-cluster leak; weights clamped + ordered);
  gyre's existing `AffinitySpring` tests prove the consumption (high-affinity
  pairs cluster). End-to-end at the seam, direct type match; live meerkat orrery
  wiring (node-content embeddings + recompute discipline) is the next slice.
  Full embed lib suite green (65 tests).
- 2026-07-06 — **P3 landed** (seam + real measurement). gyre gained a burn-free
  `RepulsionSolver` closure seam (`set_repulsion_solver` + threshold;
  `NodeExclusion` routes above it), `aether::forces::repulsion_wgpu` is the
  host-facing convenience the closure wraps, routing verified by test, and the
  `gpu-bench`-gated settle benchmark measured the real in-context win: 1.4-2×
  whole-tick at 2k-16k nodes (not the 17× isolated number — gyre's cutoff +
  rapier's step floor; see the P3 table + correction). gyre stays burn-free by
  default (verified: no burn/cubecl in the default tree; 53 tests pass). Live
  meerkat injection held — niche payoff (1000+ node graphs), thin follow-on.
- 2026-07-06 — plan written; **P1 + P2 landed**. `aether::forces::repulsion`
  (softened inverse-square N-body, backend-generic, `field-burn` /
  `field-burn-wgpu` gated) with a naive-Rust `repulsion_reference` correctness
  anchor. Tests: matches the reference (proving the *formula*, not just
  cross-backend agreement), two-bodies-repel-apart + self-term-zero physical
  sanity, ndarray↔wgpu parity green on the real GPU, and the ignored timing
  sweep recorded above. The N-body force does not fit the field AST (dynamic
  N sources), so it is a dedicated burn kernel in aether, honestly noted. Next:
  P3 gyre integration seam (route the repulsion step to the GPU pass above a
  crossover N, measured against gyre's Barnes-Hut) and P4 semantic arrangement.
