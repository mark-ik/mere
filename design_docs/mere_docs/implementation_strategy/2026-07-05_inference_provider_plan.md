# Inference Provider Plan (burn brief, Lane 3)

**Date**: 2026-07-05
**Status**: in progress. P0 (the seam + deterministic stub) this session; model body, eidetic wiring, and the actor follow.
**Related**: [burn_utilization_brief](../research/2026-07-04_burn_utilization_brief.md) (Lane 3), [local_models_harness_brief](../research/2026-06-24_local_models_harness_brief.md) (§2 defines this seam; §3 the actor harness; §4 the wasm/native split), [geist_models_brief](../research/2026-05-10_geist_models_brief.md) (adapter envelope, deferred to Lane 4), [burn_wgpu_flip_plan](2026-07-04_burn_wgpu_flip_plan.md) (the GPU receipts motivating burn-first).

## Scope

The `InferenceProvider` seam the harness brief specifies, burn-first per the
brief's Lane 3: trait + capability descriptor + deterministic stub now, a
burn model body and the armillary actor behind it next. Native heavy
runtimes (mistral.rs, llama.cpp) remain future backends behind the same
trait, selected by capability; nothing here binds a vendor.

Out of scope: `AdapterLoader` (the geist compatibility envelope, Lane 4's
entry point), the Distillery trainer, marketplace/governance.

## Phases

### P0 — The seam crate

`crates/intel/infer` (sibling of `embed`, same shape):

- `InferenceProvider`: `Send + Sync`; `capability()` descriptor;
  `generate_streaming(&request, on_token)` as the primary call (tokens
  stream through a callback the actor layer will forward as messages);
  `generate` as the provided convenience wrapper.
- `ModelCapability`: model id, context window, quantization, runtime-loader
  id, streaming flag — the matching surface the harness brief names, serde
  so it can travel to UI/status surfaces. A small `CapabilityQuery` for
  callers that pick a provider by requirement.
- `InferError` mirroring `EmbedError`'s vocabulary.
- `CannedProvider`: the deterministic, dependency-light stub (exact-match
  responses + echo fallback), the `hashed`-equivalent that lets the whole
  pipeline be tested on CI machines with no GPU and no model.

Done when the crate builds in the workspace with focused tests for
streaming order, max-token/stop handling, capability matching, and trait
object safety.

### P1 — Burn model body

A small instruct model on burn-wgpu behind a feature (embed's `bert`
pattern): borrow the model body from tracel-ai/models rather than writing
fresh; capability advertises the real context window and `burn-wgpu` loader.
Opt-in GPU smoke mirroring the flip plan's timing tests. Done when a 1-3B
model streams tokens through the seam natively with recorded tokens/sec on
CPU vs GPU.

### P2 — Eidetic model loading

Bases resolve by `ManifestId` through `ModelLibrary::resolve_components`
(the corridor the fixed bert_full_pipeline test now proves end to end); no
new storage, no filesystem convention. Done when P1's model loads from an
eidetic store instead of a directory path.

### P3 — The inference actor

An armillary actor holding a loaded provider, answering prompt requests,
streaming tokens back as actor messages (harness brief §3); real status, no
placebo progress. Done when a host-side dev consumer (omnibar `>`-lane or a
test harness) shows streamed tokens from the actor.

### P4 — Measurements that gate the defaults

The harness brief's two open questions, answerable once P1 exists: the wasm
model-size ceiling (D2) and whether burn-wgpu inference is competitive
enough to be the default native path. Recorded numbers, then the default
gets bound.

## Findings

- 2026-07-05: seam shape lifted from `embed::provider` (trait +
  error + vocabulary in one module, stub backend beside it, real backend
  feature-gated) — the pattern the harness brief says to extend.

## Progress

- 2026-07-05 — plan written; P0 implementation same session.
- 2026-07-05 — P0 landed. `crates/intel/infer` in the workspace:
  `provider.rs` (`InferenceProvider` streaming-primary trait,
  `ModelCapability` + `CapabilityQuery` matching, `InferError`), `canned.rs`
  (`CannedProvider`: exact-match + echo, word-fragment streaming honouring
  `max_tokens`/`stop`, `PromptTooLong` on a shrinkable context window).
  `cargo test -p infer`: 11/11 green, including trait object safety and
  Send + Sync. No consumers yet by design; the actor (P3) is the intended
  first one.
