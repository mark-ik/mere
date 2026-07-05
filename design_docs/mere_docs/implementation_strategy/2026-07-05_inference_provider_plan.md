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

### P1 — Burn model body (own body, reference-vendored — decided 2026-07-05)

Not an adaptation of llama-burn's crate: an **own, HF-config-driven
llama-family decoder** in embed's `bert/` pattern, built from burn-nn's own
primitives — `RmsNorm`, `RotaryEncoding`, `SwiGlu`, and the autoregressive
KV cache all ship in burn-nn 0.21, so the framework already carries most of
what llama-burn hand-rolls against its older pre-release pin. Mark's call:
fit the model to our framework and conventions, not the framework to the
example.

- **Config**: read HF `config.json` (hidden/layers/heads/kv-heads/rope
  theta + scaling/norm eps/vocab) into our own decoder config, so one body
  runs the whole open llama-family class (TinyLlama, Llama 3.2, SmolLM)
  from the HF layout directly. TinyLlama first (no license gate).
- **Tokenizer**: the `tokenizers` crate only (HF `tokenizer.json`),
  dropping llama-burn's tiktoken/sentencepiece duality — one tokenizer
  stack across embed + infer, and one shared onig/wasm wall instead of
  three.
- **Loader**: safetensors → burn weight injection exactly as
  `embed::bert::loader` does it (HF name map, transpose-at-boundary,
  validation), feeding P2's eidetic byte path with no filesystem
  convention.
- **What llama-burn remains**: a reference implementation to read and diff
  against (borrow technique, not structure) — chiefly the GQA attention
  wiring and generation/sampling glue, which are the two parts burn-nn
  does not hand us. Credit it in module docs where its technique is used.
- **Correctness net**: we own the numerics, so mirror
  `embed::bert::validation` — a fixture test against reference outputs for
  a real checkpoint, plus the cross-backend parity pattern from the flip
  plan.
- **Owned module names** are a feature, not a cost: Lane 4's LoRA adapter
  envelope needs stable target-module naming, which we control only in our
  own body.

Done when a 1-3B model streams tokens through the seam natively with
recorded tokens/sec on CPU vs GPU, validated against reference outputs.

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
- 2026-07-05, **P1 borrow assessment**: tracel-ai/models (`llama-burn`,
  MIT OR Apache-2.0, unpublished workspace crate) currently pins
  `burn = "0.21.0-pre.4"` — a pre-release that does not unify with the
  workspace's 0.21.0 final, so a git dependency would drag a second
  incompatible burn tree in and its `Llama<B>` would be generic over the
  wrong `Backend` trait. Route: vendor + adapt behind a feature (their
  pre.4 API is essentially 0.21 final, so the adaptation is small), or
  wait for their post-0.21 bump and re-check. Either way P1 is its own
  focused pass (~2-3k lines across model/transformer/cache/rope/sampling/
  tokenizer/pretrained, plus tokenizer deps: tiktoken-rs for llama3,
  tokenizers for tiny). Preserve attribution when vendoring.
- 2026-07-05, **P1 route decided (own body)**: burn-nn 0.21 ships
  `RmsNorm`, `RotaryEncoding`, `SwiGlu`, and an autoregressive KV cache —
  verified in the pinned crate source — so vendoring llama-burn's whole
  structure would mean adopting hand-rolled copies of things our framework
  already provides, shaped by their pre-release pin and their example
  needs (Meta-checkpoint import, pretrained download, dual tokenizers).
  Decision (Mark): own HF-native decoder body from burn-nn primitives;
  llama-burn demoted from vendor-source to reference implementation. P1
  section rewritten accordingly.
- 2026-07-05, **P3 ordering note**: the actor landed before P1 — it is
  provider-agnostic (runs today on `CannedProvider`, the real body drops
  in unchanged) and it fixes the seam's consumer shape early. In-flight
  cancellation is deferred until the provider callback grows a
  `ControlFlow` return alongside P1 (noted in `actor.rs`).

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
- 2026-07-05 — P1 slice 1 landed: `infer::decoder` behind the `decoder` /
  `decoder-wgpu` features (burn optional dep mirroring embed's bert split).
  `config.rs` parses real HF `config.json` (TinyLlama's actual config in
  the test, unknown fields ignored, GQA fallback + divisibility
  validation), `attention.rs` is the GQA + RoPE + causal-mask block built
  on burn-nn's `RotaryEncoding`/mask helpers with `linear_no_bias`
  injection, `layer.rs` the pre-norm residual block on `RmsNorm` + `SwiGlu`
  with the HF weight-name mapping documented for the loader slice. Rope is
  model-owned and passed into forwards (no per-layer duplication). Tests
  19/19: config parse/validate, shape/determinism/finite, the causality
  probe (perturbing the last token leaves earlier positions bit-stable —
  catches a missing or inverted mask), GQA-equals-duplicated-kv-MHA
  equivalence (locks the HF grouping convention), and ndarray↔wgpu layer
  parity on the real GPU. Next slices: model stack + safetensors loader,
  KV-cached generation loop (brings the `ControlFlow` callback change),
  provider impl + validation fixture.
- 2026-07-05 — P1 slice 2 landed: the model stack and the HF loader.
  `model.rs` (embedding → layer stack → final RmsNorm → LM head, one
  model-owned rope shared by all layers; tied-embeddings path uses the
  transposed embedding matrix and is proven equal to an explicit
  transposed head), `tensors.rs` (safetensors extraction widened with
  BF16/F16 → f32 decode via `half` — TinyLlama ships bfloat16; quantized
  dtypes rejected explicitly, not auto-cast), `loader.rs`
  (`load_decoder_from_bytes`: full HF llama-family name map with
  transpose-at-boundary; missing `lm_head.weight` accepted only when the
  config says tied, rejected otherwise; byte-buffer API so eidetic's
  `ModelComponents.weight_bytes` feeds it directly for P2). Loader tests
  build synthetic safetensors checkpoints in-memory: f32 load → finite
  logits, bf16 load stays within 0.05 of f32, tied/untied both paths,
  missing tensors named in errors. Suite: 33/33 with
  `--features actor,decoder-wgpu`. Remaining P1: KV-cached generation
  loop + sampling (+ `ControlFlow` cancellation), `InferenceProvider`
  impl, TinyLlama validation fixture + tokens/sec numbers.
- 2026-07-05 — P1 slice 3 landed: generation, cancellation, and the
  provider. The seam's callback now returns `ControlFlow` (Break = stop
  after the delivered fragment); `CannedProvider` honors it and the
  **actor gained real cancellation**: `InferCommand::Cancel` is drained
  from inside the streaming callback via `try_recv` (mid-stream cancel
  stops the provider and suppresses the in-flight fragment; queued
  cancels drop the request before it starts; `Generate`s seen mid-stream
  queue rather than vanish; `InferUpdate::Cancelled` reports it — one
  actor-loop bug found by the tests: a channel disconnect during the
  drain must not discard pending work). `decoder::attention` grew the
  KV-cached path (`LayerKvCache` stored pre-GQA-expansion; rectangular
  tril mask for prefill, maskless single-token decode; the uncached
  forward is now a delegation, so all prior tests re-validate the cached
  code). `generate.rs` is the greedy prefill-then-decode loop whose
  correctness lock is **cached-equals-full-recompute** over the same
  synthetic model, plus eos-stop and Break-stop tests. `provider.rs` is
  `DecoderProvider`: tokenizers-crate encode, streaming detokenization by
  prefix-delta with held-back non-prefix decodes (BPE boundary safety),
  stop-string truncation before emission, `PromptTooLong` against the
  real context window, greedy-only with explicit rejection of nonzero
  temperature until sampling lands, and `from_bytes` over the artifact
  triple — the eidetic P2 constructor. `eos_token_id` parses HF's
  int-or-list form. Suite: 46/46 with `--features actor,decoder-wgpu`
  (incl. GPU parity). Remaining P1: the TinyLlama real-checkpoint
  validation fixture + tokens/sec CPU-vs-GPU numbers; temperature/top-p
  sampling as its own follow-up.
- 2026-07-05 — **P1 validated on the real checkpoint.** TinyLlama-1.1B-
  Chat-v1.0 downloaded to `C:\t\models\TinyLlama-1.1B-Chat-v1.0` (outside
  the repo, deliberately — 2.2GB must not ride a working-tree commit
  sweep); its real `config.json` parses with our field set unchanged.
  `tests/tinyllama_real.rs` (all `#[ignore]`d behind `MERE_TINYLLAMA_DIR`)
  loads through `DecoderProvider::from_bytes` — the full chain: bf16
  safetensors decode, HF name map, GQA, RoPE, KV cache, real BPE
  tokenizer. The semantic fixture passed first try: greedy continuation
  of "The capital of France is" → `"Paris, which is the capital of
  France"` (release, burn-ndarray; model load 1.5s; ~13s/token CPU —
  single-threaded ndarray, the number the wgpu lane exists to beat).
  Streaming-equals-collected and the CPU-vs-GPU tokens/sec receipts in
  the same file; numbers recorded below as they land.
- 2026-07-05 — P3 landed ahead of P1 (see Findings for why). `infer::actor`
  behind the `actor` feature (armillary optional dep, so the seam core
  stays wasm-clean — `cargo check -p infer --target wasm32-unknown-unknown`
  passes): `spawn_inference_actor` builds the provider on the actor thread,
  emits `Ready { capability }` at startup, then
  `Started`/`Fragment`.../`Finished`-or-`Failed` per request with
  correlation ids. `cargo test -p infer --features actor`: 14/14 green
  (streaming order, error path, id correlation). Remaining phases: P1
  (vendored burn model body), P2 (eidetic `ManifestId` loading — corridor
  already proven byte-faithful by the fixed round-trip test), P4
  (measurements). The host wiring (meerkat kernel inbox + omnibar consumer)
  rides P1/P2, since a canned-echo omnibar serves no one.
