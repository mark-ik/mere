# Inference Provider Plan (burn brief, Lane 3)

**Date**: 2026-07-05
**Status**: P0 (seam + stub), P1 (own decoder body incl. seeded temperature/top-p sampling; validated on the real TinyLlama checkpoint, 9.95 tok/s on wgpu vs 0.09 on ndarray), P2 (eidetic loading; corridor proven transparent), P3 (actor with cancellation), and the meerkat host wiring (`>ask` omnibar verb) all landed. Remaining: P4's wasm half (gated on the embed-wasm dependency slice).
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
- 2026-07-05 — **tokens/sec receipt (P4's first number).**
  Streaming-equals-collected passed on the real BPE tokenizer. The timing
  run surfaced one genericity bug (argmax read as i64; the wgpu
  instantiation's int element is i32 — fixed with `into_scalar().elem()`),
  then: **CPU 16 tokens in 180.0s (0.09 tok/s) vs wgpu 16 tokens in 1.6s
  (9.95 tok/s) — 110x**, with byte-identical greedy output across
  backends (a strong whole-model numerics check at 1.1B scale). ~10 tok/s
  on this laptop's GPU is interactive-usable; the brief's Lane-3 premise
  (burn-wgpu as the local inference default) now has its number. CPU
  ndarray is confirmed non-viable for generation (it is single-threaded;
  even so, the gap is architectural, not a build artifact — both runs
  release).
- 2026-07-05 — **P2 landed.** `tests/eidetic_corridor.rs` (always-run,
  tiny synthetic checkpoint, no GPU): the artifact triple saved through
  `ModelLibrary::save_model_with_components`, resolved by `ManifestId`,
  fed to `DecoderProvider::from_bytes` — generation is byte-identical to
  a direct load, streaming included, and the resolved weight bytes equal
  the saved ones (consuming the OpaqueBlob raw-bytes fix). Model loading
  has no filesystem convention: a `ManifestId` is the address. eidetic +
  pollster + async-trait joined infer's dev-deps for this test only.
- 2026-07-05 — **sampling landed** (closes P1's last sub-item).
  `decoder/sample.rs`: temperature + optional top-p over the host-side
  logits row, on a dependency-free splitmix64 RNG. The seeding policy is
  deliberate and documented: `GenerationRequest.seed: Some` gives a
  bit-reproducible stream; `None` draws one fresh seed per generation
  from `RandomState` entropy and traces it (`infer` target, debug) so an
  unseeded run is reproducible after the fact. `generate.rs` generalized
  to a `TokenPicker` (Greedy | Sampled) with the greedy wrapper intact,
  so all prior greedy tests keep validating the shared cached path;
  provider maps `temperature == 0.0` → greedy, `> 0` → sampler (invalid
  temperatures rejected), `top_p`/`seed` are new request fields (serde
  defaults, so stored requests stay parseable). Tests: same-seed
  same-stream (sampler and end-to-end through the provider), tiny-top-p
  collapses to argmax, low temperature effectively greedy,
  frequency-ordering over 2000 seeded draws, invalid temps rejected.
  Suite: 53 lib + 1 corridor green with `--features actor,decoder-wgpu`.
  Also re-checked Lane 2's gate while closing out: burn-remote's latest
  crates.io release is still 0.21.0 (2026-05-07); the iroh transport
  remains unreleased, mesh plan status unchanged.
- 2026-07-06 — **meerkat host wiring: the `>ask` omnibar verb.** The
  inference actor now runs inside meerkat, wired the same shape as
  fetch/find/sync: spawned in `shell_new` with a winit-proxy wake, handle
  in `Content`, receiver in `KernelInbox`, drained in `on_user_event`.
  `crates/meerkat/src/infer_host.rs` builds the provider on the actor
  thread — a real `DecoderProvider<Wgpu>` (TinyLlama) under the new
  `local-inference` feature when `MERE_TINYLLAMA_DIR` is set, else infer's
  `CannedProvider` stub, so the shell runs with or without a model.
  Dependency discipline: infer is an `actor`-only dep by default (light —
  armillary + the stub, no burn); `local-inference` adds
  `infer/decoder-wgpu` (the cubecl tree), so ordinary meerkat builds stay
  lean and don't pay the burn compile. meerkat never names `burn` — it
  calls `infer::decoder::load_wgpu_provider`, a new `decoder-wgpu`-gated
  convenience constructor. `>ask <prompt>` (shell_eval verb + bare-line
  sugar, quote-escaped) records an `ask_prompt`; `command_drain::start_ask`
  bumps a correlation id, clears the accumulator, and commands the actor;
  `apply_infer_update` folds streamed fragments into the omnibar location
  echo, dropping stale-id updates from a superseded ask. Builds green both
  ways: default `cargo check` clean, `--features local-inference` builds
  (adds the burn tree, ~5.5min cold). Headed launch with the model
  confirmed burn-wgpu and netrender's wgpu **coexist in one process**
  (app ran + rendered, infer actor started) — the practical D1 check.
  The `>ask` shell_eval unit test is written (mirrors the recall/scene
  tests) and now passes: the lib test target was blocked from compiling by
  an unrelated stale test — `crates/meerkat/src/ingest.rs`'s `#[cfg(test)]`
  asserted `(String, String)` property tuples against the committed
  `NodeProperty` struct refactor — fixed here (match `predicate`/`value`
  fields; `page_extract_enriches…` + `ask_records…` both green). Three
  further lib tests failed, all pre-existing regressions from other
  concurrent work surfaced (not caused) by unblocking the compile; Mark
  authorized fixing them (the owning agent is mid-plan on wallet/persona).
  All three now pass — full meerkat lib suite green, 302 tests:
  - `graph_delta_log_round_trips_and_replays`: two parts. (a) The captured
    count is 53, not 55 — the statement-aware-writes refactor folds each
    semantic-predicate statement into the predicate write, so the two
    predicate deltas capture once each instead of twice; verified by
    tracing every `apply_graph_delta` call to its single capture, and the
    replay reconstructs the graph, so nothing is dropped. (b) The snapshot
    round-trip compared minted `statement_id`s (device + time + sequence)
    and wall-clock `last_visited_ms` (`TouchNodeLastVisited` carries no
    timestamp, so replay re-stamps) — both non-deterministic across a
    replay, so the normalizer now blanks them, exactly as it already zeroes
    `timestamp_secs`. **Open question flagged to the delta owner**: whether
    replay *should* preserve `last_visited_ms` for crash-recovery fidelity
    (else every node reads "visited just now" after recovery) — a
    delta-capture design call, not a test bug.
  - `roster_view::links_tab_lists_relation_families_distinctly`: the test
    helper built three same-endpoint rows all flagged `starts_bundle`,
    which production's `build_link_rows` never does (it sets the flag once
    per `(from, to)` group). Tolerated until the concurrent `Keyed` work
    added `assert_unique_keys`; now a duplicate-key panic. Fixed the test to
    use three distinct endpoint pairs (three real bundles → three sections),
    keeping its assertions with realistic data.
  - `wallet_pairing::minted_offer_round_trips…`: passed once graph_delta_log
    was fixed, untouched — a cascade. The panicking graph_delta_log test
    leaked the process-global delta-capture hook (it panicked before its
    `set_captured_delta_hook(None)` cleanup), and cargo runs tests in
    parallel, so a concurrently-running test that minted graph state saw the
    stale hook. Fixing graph_delta_log's cleanup path removed the pollution.
- 2026-07-06 — **`>ask` proven end-to-end headed on the real model.**
  Release build of meerkat + `local-inference`; drove the real window
  (Ctrl+L to focus the omnibar, SendKeys `>ask The capital of France is`,
  Enter). Receipt from the `meerkat::infer` trace:
  `ask received prompt="The capital of France is"` →
  `ask finished answer=Paris, which is the capital of France.` — the exact
  continuation the standalone real-model test produced, now through the
  real omnibar → classify → shell_eval → `start_ask` → actor → drain →
  chrome path. Model load in-process: read 2.2GB (~1.8s) + burn-wgpu
  device-init & tensor upload (~5s) = **~7s**, alongside netrender's own
  wgpu device, app rendering throughout (the D1 coexistence proof, real
  model resident).
  - **D1 contention, observed and mitigated.** First headed run streamed a
    full chrome repaint per token; with burn's matmuls and vello's raster
    sharing the GPU in one process, per-token cost ~doubled and a
    200-token answer ran >40s. Added a repaint coalesce (omnibar updates
    at most ~every 150ms while streaming; the final answer always paints):
    the same answer now finishes in ~18s. In-app throughput is well below
    the standalone 9.95 tok/s because inference and rendering serialize on
    one queue — the concrete cost of *not* isolating burn's device, a real
    input to the eventual D1 decision.
  - Instrumentation kept (not scaffolding): structured `meerkat::infer`
    tracing for model-load progress and the ask lifecycle (received /
    finished), matching the fetch/content actor startup traces.
  - **Follow-ups** (noted, not blocking): a raw question like "what is the
    capital of France" greedy-decodes to "…France?" because TinyLlama-Chat
    wants its chat template and the provider treats templating as
    above-the-seam — a per-model prompt-template config would make `>ask`
    answer questions well. The omnibar is a cramped surface for a
    multi-sentence answer; a dedicated answer card is the natural home.
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
