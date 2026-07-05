# Burn Wgpu Flip Plan (burn brief, Lane 1)

**Date**: 2026-07-04
**Status**: P0-P3 landed and measured (2026-07-05). P4 (wasm receipt) blocked on the getrandom `wasm_js` porting tax; see Findings.
**Related**: [burn_utilization_brief](../research/2026-07-04_burn_utilization_brief.md) (Lane 1 + decision D1; this plan is its first spin-out), [local_models_harness_brief](../research/2026-06-24_local_models_harness_brief.md) (D2, the wasm ceiling, starts after this), `crates/intel/embed`, `crates/orrery/aether`.

## Scope

Turn the burn wgpu backend on and measure it. Three deliverables:

1. `embed` gains a wgpu feature and `BertEmbeddingProvider<Wgpu>` runs.
2. `aether`'s declared `field-burn-wgpu` actually builds, with a parity test
   against the ndarray backend.
3. Recorded CPU-vs-GPU numbers for both workloads, plus a verified answer on
   burn 0.21's existing-device init seam (the D1 input).

Out of scope: model downloads/marketplace (eidetic owns artifacts), inference
(`InferenceProvider`, Lane 3), any netrender integration (D1 is decided from
receipts here, executed wherever the first shared-device consumer lands).

## Seam audit (code-verified 2026-07-04)

- `BertEmbeddingProvider<B: Backend>` is already backend-generic end to end
  (`bert/provider.rs`); tests alias `type B = NdArray<f32>`.
- `aether::lower_burn` is generic over `B: Backend`; tests alias the same way.
- Neither crate names a concrete backend outside test aliases, so the flip is
  manifests + tests + measurement, not surgery.

## Phases

### P0 — wgpu feature builds

`bert-wgpu = ["bert", "burn/wgpu"]` in embed (mirrors aether's
`field-burn-wgpu` naming); `cargo check` both crates with the wgpu features on.
First build pulls the cubecl tree; expect it to be slow once. Done when both
feature combinations compile on Windows.

### P1 — parity tests

- aether: lower a representative scalar + vector field program on
  `Wgpu` and `NdArray` at the same positions; assert values match within
  tolerance. Gated on `field-burn-wgpu`.
- embed: run a small randomly-initialized BERT forward on both backends;
  assert embeddings match within tolerance (random weights are fine for
  parity and timing; real MiniLM via `MERE_MINILM_DIR` stays the ignored
  full-pipeline path). Gated on `bert-wgpu`.

Done when both parity tests pass on this machine's GPU.

### P2 — measurement

Ignored, feature-gated timing tests (run explicitly, printing µs):

- embed: batch of N texts through the provider, CPU vs GPU, including a
  warmup pass so kernel compilation is not billed to the steady number.
- aether: a field program evaluated over large position batches (1k / 10k /
  100k), CPU vs GPU.

Done when the Progress log records the numbers with batch sizes, and the
brief's Lane 1 knows whether GPU wins and from what batch size up.

### P3 — existing-device init seam (D1 input)

Read the fetched burn-wgpu 0.21 source and record: can the wgpu backend be
initialized from an existing `wgpu::Device`/`Queue` (netrender's), and on what
API. No integration here; the receipt + a recommendation go to the brief's D1.

### P4 — wasm build receipt

`cargo check --target wasm32-unknown-unknown` for aether (`field-burn-wgpu`)
and embed (`bert-wgpu`). Build-only receipt; a runtime WebGPU pass rides the
serval web-smoke harness later and D2 (model-size ceiling) stays with the
harness brief.

## Findings

- 2026-07-04: seam audit above. Tests use `burn::tensor::backend::BackendTypes`
  for device types on 0.21; mirror that idiom in new tests.
- 2026-07-05, **field-eval timing** (release, Windows laptop, default wgpu
  adapter, includes device→host readback, GPU warmed up; the Lane-1 scalar
  program: gaussian + 0.25·(x·linear)):

  | N positions | ndarray CPU | wgpu GPU |
  | --- | --- | --- |
  | 1,000 | 129µs | 963µs |
  | 10,000 | 184µs | 708µs |
  | 100,000 | 941µs | 1,170µs |

  CPU wins through 100k for this cheap elementwise program; the gap closes
  with N (7.5x at 1k → 1.24x at 100k). GPU pays off for fields only with
  much heavier programs, many fields batched per dispatch, or resident
  positions (no per-call readback, the D1 shared-device shape). Verdict:
  keep ndarray the default field-eval backend for now.
- 2026-07-05, **BERT timing** (release, same machine, MiniLM-L6 dims with
  deterministic synthetic weights, readback included, GPU warmed up):

  | batch × seq | ndarray CPU | wgpu GPU | speedup |
  | --- | --- | --- | --- |
  | 1 × 32 | 29.9ms | 9.3ms | 3.2x |
  | 8 × 64 | 221ms | 12.8ms | 17x |
  | 32 × 128 | 2,006ms | 53ms | 38x |

  Decisive: embeddings (and by extension Lane-3 inference) belong on
  burn-wgpu on any machine with a GPU. Corpus embedding at CPU speed is
  not viable (2s per 32-doc batch).
- 2026-07-05, **D1 receipt** (read from burn-wgpu 0.21 / cubecl-wgpu 0.10
  source): `WgpuSetup { instance, adapter, device, queue, backend }` +
  `init_device(setup, options) -> WgpuDevice` registers an existing wgpu
  device as `WgpuDevice::Existing(id)`. burn's cubecl-wgpu pins `wgpu = "29"`,
  the same major the mere workspace pins, so cargo unifies the crate and
  netrender's device/queue can be handed to burn directly. Constraint noted
  in their source: one burn device per adapter. Shared-device interop is
  mechanically possible today; whether to share stays a scheduling question
  (queue contention vs the frame budget), revisit when the first resident-
  data consumer lands.
- 2026-07-05, **P4 wall**: `cargo check -p aether --features field-burn-wgpu
  --target wasm32-unknown-unknown` fails in `getrandom 0.4` ("enable the
  wasm_js feature") — the known wasm porting tax the
  cross_platform_parallelism_strategy brief catalogued. Needs a target-gated
  getrandom feature (and burn's fusion feature may need disabling on wasm
  per burn-wgpu's own docs). Deferred to its own pass; not a Lane-1 gate.

## Progress

- 2026-07-04 — plan written (Lane 1 spin-out of the burn utilization brief);
  seam audit done; implementation starting same session.
- 2026-07-05 — P0-P3 landed. `bert-wgpu` feature added to embed;
  `aether/src/lower_burn/tests_wgpu.rs` (scalar + vector ndarray↔wgpu parity,
  both green on the real GPU, plus the ignored timing test) and
  `embed/src/bert/wgpu_parity.rs` (deterministic synthetic-weight BERT parity,
  green; ignored MiniLM-dims timing test) added. Timing + D1 receipts recorded
  in Findings. Note: `cargo test -p embed --features bert-wgpu` currently
  needs `--lib` — the pre-existing `tests/bert_full_pipeline.rs` is broken
  against eidetic's changed `ResolvedModel` API (concurrent work, not this
  plan's scope).
