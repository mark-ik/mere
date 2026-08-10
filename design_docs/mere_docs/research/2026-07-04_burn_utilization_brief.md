# Burn Utilization Brief

**Date**: 2026-07-04
**Status**: direction brief. All five lanes endorsed (Mark, 2026-07-04): burn is a crucial component and the app gets shaped around it. Each lane spins out to its own `implementation_strategy/` plan when picked up.
**Related**: [local_intelligence_integration_research](2026-05-08_local_intelligence_integration_research.md) (the original burn-first stance; this brief extends it ecosystem-wide), [local_models_harness_brief](2026-06-24_local_models_harness_brief.md) (the provider seams + backend-by-target table), [geist_models_brief](2026-05-10_geist_models_brief.md) (LoRA adapters as engrams), [communal_compute_tiers_brief](2026-06-10_communal_compute_tiers_brief.md), [mesh_lease_scheduler_plan](../../archive_docs/2026-08-09_completed_plans/2026-06-30_mesh_lease_scheduler_plan.md) (burn-remote-over-iroh prior art), [data_oriented_doctrine_brief](../../2026-07-02_data_oriented_doctrine_brief.md), [dependency_footprint_brief](../../2026-07-04_dependency_footprint_brief.md).

## Thesis

Burn is already load-bearing in three crates, but everything runs on the ndarray
CPU backend, and the capabilities burn is actually differentiated at (GPU
execution, remote execution over iroh, browser-reaching inference, on-device
training) are all unclaimed. This brief maps the five lanes that claim them, in
leverage order, and names what "shaped around burn" commits the architecture to.

Shaping the app around burn does not mean burn types spreading through crates.
It means three structural commitments (§Commitments) that keep every crate
tensor-ready while burn itself stays behind a handful of seams.

## Current footprint (code-verified 2026-07-04)

- **`crates/intel/embed`**: `EmbeddingProvider` trait with the `hashed` backend
  shipped and a full BERT implementation behind the `bert` feature
  (attention / encoder / feed-forward / safetensors loader / validation, plus
  `tests/bert_full_pipeline.rs`). Backend: burn 0.21, ndarray, CPU. No wgpu
  feature declared.
- **`crates/eidetic/eidetic-search`**: consumes embed as the vector half of
  fused recall; depends on burn only to name the backend type.
- **`crates/orrery/aether`**: `lower_burn.rs` walks the field-algebra AST and
  emits fused tensor programs, with solid scalar/vector operator coverage
  including analytic gradients. Default backend ndarray; `field-burn-wgpu`
  is declared in the manifest but nothing wires or measures it.
- **`crates/orrery/gyre`**: burn-free by design (manifest comment: no
  rhai/burn pulled into the simulator). Aether is the burn boundary for
  anything physics-shaped.
- burn 0.21 was latest stable at the 2026-07-04 dependency audit.

## Lane 1: turn the wgpu backend on

The cheapest unlock: everything below rides GPU execution, and the flag half
already exists. Work: a wgpu feature in embed mirroring aether's
`field-burn-wgpu`; wire both; measure CPU vs GPU on real workloads (a batch of
BERT embeds, a field program over a large position batch); confirm the wasm
build reaches WebGPU.

The real decision underneath is device policy (D1): burn-wgpu can initialize
from an existing wgpu device/queue, so burn and netrender could share one
device, making tensors plain wgpu buffers resident where rendering lives.
Zero-copy interop versus queue contention with the frame budget. Measure
before binding.

**Done when** embeddings and field eval run on burn-wgpu natively with
recorded CPU-vs-GPU numbers, a wasm embed runs via WebGPU, and D1 is decided
from those receipts.

**Status 2026-07-05**: landed and measured; aether's wasm build receipt is
green (getrandom 0.4 `wasm_js` + uuid `js`), embed's wasm receipt is a named
follow-on slice (ahash-pulled getrandom 0.3 + tokenizers/onig); see the
[burn_wgpu_flip_plan](../implementation_strategy/2026-07-04_burn_wgpu_flip_plan.md).
Headline: BERT is decisively GPU (3.2x at batch 1 up to 38x at 32×128);
field eval stays CPU-default (ndarray wins through 100k positions on a
cheap program; GPU needs resident data or heavier programs to pay).

## Lane 2: burn-remote over iroh as the fleet compute protocol

The mesh lease scheduler's compute half, ready-made (prior art recorded in the
[mesh_lease_scheduler_plan](../../archive_docs/2026-08-09_completed_plans/2026-06-30_mesh_lease_scheduler_plan.md)):
burn mounts as an ALPN on murm's existing `iroh::protocol::Router`, so it
rides the same endpoint/identity/relay policy with no second connection pool.
`RemoteTicket` / `PeerAuthorizer` is exactly the tessera/kith authorization
seam. Lease semantics (owner-reclaim, heartbeat) stay ours. Their wasm-client
remote-compute example is the shape of the no-JIT browser inference lane,
with the caveat that browser-side transport is websocket for now.

**Gate refreshed 2026-08-09**: Burn and Burn Remote exist only as
`0.22.0-pre.1`; the stable 0.22 migration has its own
[plan](../implementation_strategy/2026-08-09_burn_0_22_migration_plan.md).
**Done when** a second machine executes tensor ops for the first over
murm's endpoint under a lease, authorized by a tessera/kith credential.

## Lane 3: inference, burn-first

The [local_models_harness_brief](2026-06-24_local_models_harness_brief.md)
already concluded burn-wgpu is the only backend that reaches the browser/PWA
target, and that target is load-bearing. This lane commits the default:
`InferenceProvider`'s first real backend is burn-wgpu, with model bodies
borrowed from tracel-ai/models (Llama-family, BERT) rather than written
fresh. Native heavy runtimes (mistral.rs, llama.cpp) stay behind the same
seam as capability-gated escalation, per that brief; nothing here reopens
the vendor question.

**Done when** a small model (1-3B) streams tokens through `InferenceProvider`
on burn-wgpu, native and wasm, and the harness brief's two open measurements
(wasm model-size ceiling; burn-wgpu vs native-runtime competitiveness) have
recorded answers.

**Status 2026-07-05 (end of day)**: the lane is real. `crates/intel/infer`
carries the seam, the actor (with mid-stream cancellation), and the own
llama-family decoder body — validated on the actual TinyLlama-1.1B
checkpoint ("The capital of France is" → "Paris, …") at **9.95 tok/s on
burn-wgpu vs 0.09 on ndarray CPU (110x)**, greedy output byte-identical
across backends. Remaining per the
[inference_provider_plan](../implementation_strategy/2026-07-05_inference_provider_plan.md):
sampling, eidetic loading (P2), the wasm half of the done-condition.

## Lane 4: training and LoRA on-device

Burn's moat relative to candle/ort: the same code trains and infers, on wgpu,
in Rust. That opens personalization nobody else in the stack offers: LoRA
adapters trained over the user's own graph (the bunsen group-optimizer borrow,
geist §12), learned ranking for recall, embeddings tuned to the user's corpus.
Adapters are engrams ([geist_models_brief](2026-05-10_geist_models_brief.md));
training data is the graph and never leaves the device or the fleet. Lane 2
turns the personal fleet into the training pool; the
[communal_compute_tiers_brief](2026-06-10_communal_compute_tiers_brief.md)
is the moot-scale extension of the same shape.

**Done when** a LoRA adapter trains on-device against the user's graph and
measurably improves a recall or ranking task over the untuned baseline.

## Lane 5: graph intelligence in the orrery

Embeddings driving arrangement: semantic clustering as an arrangement source,
suggested edges from similarity (a producer for the
[graph_signals_layer_plan](../implementation_strategy/2026-06-22_graph_signals_layer_plan.md)),
and the borrowed-ideas "semantic neighbors" living-document block. For large
graphs, a tensorized force pass written in the field algebra rides aether's
existing lowering; gyre stays burn-free and integrates whatever the field
source produces.

One honest cap: while gnodes are DOM transforms, GPU-computed positions
round-trip back to the CPU regardless, so the near-term payoff is compute
throughput at large N, not a render-path shortcut. The cap lifts if the
unified-document-host cond-1 `<orrery>` element (engine-owned placement) ever
lands; D1's shared device is what would make that lift zero-copy.

**Done when** an arrangement can be driven by embedding clusters end-to-end,
and a large-graph force pass through the aether burn path beats the CPU path
at a measured node count.

**Status 2026-07-06**: both halves have their mechanism, per the
[orrery_graph_intelligence_plan](../implementation_strategy/2026-07-06_orrery_graph_intelligence_plan.md).

- **Force pass (P1-P3)**: `aether::forces::repulsion` (N-body on burn,
  ndarray/wgpu) is wired into gyre via a burn-free `RepulsionSolver` closure
  seam. Isolated it beats naive CPU up to 17×, **but in the real gyre tick the
  win is 1.4-2×** at 2k-16k nodes — gyre's cutoff + rapier's step floor, an
  important honest correction. Only activates above ~1000 nodes, so it is a
  niche large-graph win; live meerkat injection held.
- **Semantic arrangement (P4)**: `embed::affinity::affinity_pairs` bridges an
  embedding index to gyre's existing `AffinitySpring` clustering signal, tested
  end-to-end at the seam. Helps at any graph size.
- **Live wiring (P5, landed 2026-07-06)**: P4 now runs end-to-end in meerkat's
  focused orrery behind an off-by-default `content-affinity` feature. A burn-free
  `Orrery::set_content_affinity` seam takes host-computed `(NodeKey, NodeKey, f32)`
  triples that supersede structural Jaccard under the existing toggle; a new
  burn-free `embed::LexicalEmbeddingProvider` (feature-hashing, the honest light
  default) embeds each node's title + tags; the meerkat driver recomputes on graph
  mutation (revision + throttle gate) and injects. gyre/orrery stay tensor-free.
  **Lexical today; the deep-semantic BERT provider (`semantic-embeddings`) is the
  upgrade** — a separate slice wanting an embed boxed loader (mirroring
  `infer::decoder::load_wgpu_provider`) plus the D1 device decision for wgpu.
- **Blended affinity + enrichment (P6, landed 2026-07-06)**: structural (Jaccard)
  and content (embedding) affinity now *combine* rather than one superseding the
  other — an `AffinityBlend` mode on the orrery, default a noisy-OR of the two
  weights, so topology and meaning are complementary clustering forces. Node
  embedding text folds in the node's extracted property descriptions (schema/OG),
  the page's own content summary already on the node. Two intelligence-tier
  hygiene items rode along: `HashedEmbeddingProvider` (semantically meaningless by
  design) renamed to `StubEmbeddingProvider` so it stops reading like a usable
  provider, and a cross-cutting scaling plan for the O(N²) flat vector index (the
  affinity scan is the same kernel shape as the L5 force pass; one burn lift raises
  arrangement + recall + canvas-search together) —
  [intel_vector_index_burn_lift_plan](../implementation_strategy/2026-07-06_intel_vector_index_burn_lift_plan.md).

## Commitments (what "shaped around burn" means)

1. **Hot data stays columnar.** The
   [data_oriented_doctrine_brief](../../2026-07-02_data_oriented_doctrine_brief.md)
   and tensorization are the same discipline: data already kept in columns is
   data burn consumes without a marshalling layer. Already policy; this brief
   adds the second reason to hold it.
2. **Burn stays behind the seams.** `EmbeddingProvider`, `InferenceProvider`,
   and aether's field registry are the only burn boundaries. gyre, genet,
   meerkat, and the kernel never see a tensor type. Widening the burn tree in
   a crate is a gate question per the dependency footprint brief, not a
   convenience import.
3. **One device policy, decided early (D1).** Shared-with-netrender versus
   separate device decides whether compute/render interop is zero-copy or a
   copy boundary, permanently. Lane 1's measurements decide it; until then no
   lane hard-codes either assumption.

## Open decisions

- **D1 device policy**: shared wgpu device with netrender (zero-copy, but
  inference and the renderer contend for one queue) vs a separate device
  (isolation, copy boundary). Verify burn 0.21's existing-device init seam as
  part of Lane 1. **Receipt 2026-07-05**: seam verified —
  `init_device(WgpuSetup{instance, adapter, device, queue, ..}, options)`
  registers an existing device, and burn's cubecl-wgpu pins the same
  `wgpu = "29"` the workspace pins, so sharing is mechanically possible
  today (one burn device per adapter). The scheduling half of the decision
  stays open until a resident-data consumer exists.
  **Scoping 2026-07-06 — the forcing function arrived.** D1 was framed as
  hypothetical contention; it is now measured, and a *third* GPU consumer is
  materializing:
  - **Consumers.** netrender (vello raster, per-frame), infer (`>ask`
    generation, bursty), and now embed (content-affinity node embedding under
    `semantic-embeddings`/`bert-wgpu`, bursty on graph mutation). Three burn +
    render clients, one adapter.
  - **Measured contention.** The first headed `>ask` streamed a full chrome
    repaint per token; with burn's matmuls and vello's raster sharing one queue
    in one process, a 200-token answer ran >40s and per-token cost ~doubled. A
    repaint coalesce (≤~150ms) brought the same answer to ~18s, and in-app
    throughput sat well below the standalone 9.95 tok/s — inference and render
    serialize on the shared queue (inference_provider_plan, 2026-07-06). That is
    the concrete cost of *not* isolating burn's device.
  - **The decision, now actionable.** Two shapes: (a) a **separate burn compute
    device/queue** from the render device — isolation, a copy boundary at any
    render handoff, but the frame budget stops paying for inference/embedding
    spikes; or (b) **one shared device with a scheduler** — zero-copy potential,
    but burn work must yield to the frame budget (priority/time-slicing/
    submission gating) or it janks the UI, as observed. The repaint coalesce is
    a host-side band-aid over (b)'s absence, not the decision.
  - **What to measure next.** Frame-time impact of a burst embed/generate on the
    shared device (is a yield/priority knob enough?) vs the copy-boundary cost of
    a separate device at the one place tensors meet pixels (the Lane 5 cond-1
    `<orrery>` element, if it lands). Until then no lane hard-codes either; the
    coalesce holds the UI usable on the shared device.
- **D2 wasm model-size ceiling**: the
  [headed-browser probe](../implementation_strategy/2026-08-09_browser_model_ceiling_probe_plan.md)
  owns artifact/storage copies, worker execution, cancellation, UI impact, and
  the configured size sweep. It proceeds independently of Lane 2.
- **D3 burn-remote release timing**: the
  [Burn 0.22 migration plan](../implementation_strategy/2026-08-09_burn_0_22_migration_plan.md)
  waits for stable 0.22 and keeps remote execution outside the dependency
  migration. A disposable prerelease compatibility probe is allowed after the
  M2/M3 resource and reclaim seams exist.
- **D4 position readback shape for Lane 5**: batch readback per tick until
  cond-1; revisit if the `<orrery>` element lands.

## Boundaries

- Orthogonal to the 60fps frame-budget work: the frame cost is paint emission
  and raster (tree walking and encoding), and burn moves neither term. The one
  coupling point is D1: a shared device means inference jobs and the renderer
  share a queue, which the frame budget must eventually account for.
- Lane order is leverage order, and Lanes 2-5 all get cheaper after Lane 1,
  but only Lane 2 has a hard external gate (D3). Lanes can proceed
  independently behind their seams.

## Progress

- **2026-07-04**: brief written from a code-verified footprint pass
  (embed/eidetic-search/aether/gyre) plus the prior burn threads
  (harness brief, mesh prior art, geist, communal compute). Mark endorsed all
  five lanes and the shape-the-app-around-it framing.
- **2026-08-09**: refreshed D2 and D3 after ESP consolidation. D2 is now an
  independent headed-browser evidence plan. Stable Burn 0.22 remains gated;
  migration and Burn Remote execution are separate plans/slices.
