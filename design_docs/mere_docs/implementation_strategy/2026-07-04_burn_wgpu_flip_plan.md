# Burn Wgpu Flip Plan (burn brief, Lane 1)

**Date**: 2026-07-04
**Status**: in progress.
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

## Progress

- 2026-07-04 — plan written (Lane 1 spin-out of the burn utilization brief);
  seam audit done; implementation starting same session.
