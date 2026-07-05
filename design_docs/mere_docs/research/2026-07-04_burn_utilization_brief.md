# Burn Utilization Brief

**Date**: 2026-07-04
**Status**: direction brief. All five lanes endorsed (Mark, 2026-07-04): burn is a crucial component and the app gets shaped around it. Each lane spins out to its own `implementation_strategy/` plan when picked up.
**Related**: [local_intelligence_integration_research](2026-05-08_local_intelligence_integration_research.md) (the original burn-first stance; this brief extends it ecosystem-wide), [local_models_harness_brief](2026-06-24_local_models_harness_brief.md) (the provider seams + backend-by-target table), [geist_models_brief](2026-05-10_geist_models_brief.md) (LoRA adapters as engrams), [communal_compute_tiers_brief](2026-06-10_communal_compute_tiers_brief.md), [mesh_lease_scheduler_plan](../implementation_strategy/2026-06-30_mesh_lease_scheduler_plan.md) (burn-remote-over-iroh prior art), [data_oriented_doctrine_brief](../../2026-07-02_data_oriented_doctrine_brief.md), [dependency_footprint_brief](../../2026-07-04_dependency_footprint_brief.md).

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

**Status 2026-07-05**: landed and measured except the wasm receipt; see the
[burn_wgpu_flip_plan](../implementation_strategy/2026-07-04_burn_wgpu_flip_plan.md).
Headline: BERT is decisively GPU (3.2x at batch 1 up to 38x at 32×128);
field eval stays CPU-default (ndarray wins through 100k positions on a
cheap program; GPU needs resident data or heavier programs to pay).

## Lane 2: burn-remote over iroh as the fleet compute protocol

The mesh lease scheduler's compute half, ready-made (prior art recorded in the
[mesh_lease_scheduler_plan](../implementation_strategy/2026-06-30_mesh_lease_scheduler_plan.md)):
burn mounts as an ALPN on murm's existing `iroh::protocol::Router`, so it
rides the same endpoint/identity/relay policy with no second connection pool.
`RemoteTicket` / `PeerAuthorizer` is exactly the tessera/kith authorization
seam. Lease semantics (owner-reclaim, heartbeat) stay ours. Their wasm-client
remote-compute example is the shape of the no-JIT browser inference lane,
with the caveat that browser-side transport is websocket for now.

Gated on the post-0.21 release (PR #5111 is on main, unreleased; expect API
churn). **Done when** a second machine executes tensor ops for the first over
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

## Commitments (what "shaped around burn" means)

1. **Hot data stays columnar.** The
   [data_oriented_doctrine_brief](../../2026-07-02_data_oriented_doctrine_brief.md)
   and tensorization are the same discipline: data already kept in columns is
   data burn consumes without a marshalling layer. Already policy; this brief
   adds the second reason to hold it.
2. **Burn stays behind the seams.** `EmbeddingProvider`, `InferenceProvider`,
   and aether's field registry are the only burn boundaries. gyre, serval,
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
- **D2 wasm model-size ceiling**: inherited from the harness brief; empirical;
  sets the in-browser inference tier.
- **D3 burn-remote release timing**: Lane 2 waits on the post-0.21 release;
  re-check the API against the plan's seam notes when it ships.
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
