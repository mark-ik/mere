# Local models and the inference/training harness

**Date**: 2026-06-24
**Status**: Direction brief. The inference seam, decoder, Eidetic loading, cancellation, and native
actor/host receipt now live in `esp::infer`; the headed-browser D2 probe and a real adapter/session
remain open. Picks up the piece the
[geist models brief](2026-05-10_geist_models_brief.md) and the
[local-intelligence research](2026-05-08_local_intelligence_integration_research.md) both defer:
not the geist *architecture* (that brief owns it) nor the compute *marketplace* (the communal-compute
briefs own it), but the concrete **harness** that runs local model inference and LoRA training inside
Mere, across the wasm and native targets.

**Related (the local-models lane, indexed in §1):**

- [geist models brief](2026-05-10_geist_models_brief.md) — the architecture: a geist = open-weight base
  + composable LoRA-adapter engrams, Distillery-as-trainer, governance, the runtime *seam* (left
  unbound). This doc binds the seam.
- [local-intelligence research](2026-05-08_local_intelligence_integration_research.md) — tier-1
  embeddings shipped (`intel/embed`); tier-3+ LLM serving + LoRA training deferred. This is that tier.
- [communal compute tiers](2026-06-10_communal_compute_tiers_brief.md) + the resource-coordination
  brief — the compute *marketplace* (offer/ask/receipt, tessera credit, verification). Not re-solved here.
- [persona model brief](2026-05-14_persona_model_brief.md) — persona-scoped adapters (which corpus an
  adapter trains on). The scope axis the harness reads, does not define.
- eidetic [`models`](../../../crates/eidetic/eidetic-core/src/models/) (`ModelManifest` /
  `ModelComponents` / `ModelLibrary`) — content-addressed model + adapter storage. The harness loads
  from here.
- [browser model ceiling probe](../implementation_strategy/2026-08-09_browser_model_ceiling_probe_plan.md)
  — the headed-browser storage, worker, execution, and measurement half of this brief's wasm claim.
- [ESP consolidation plan](../implementation_strategy/2026-08-08_esp_consolidation_plan.md) — the
  settled seam home: `esp::infer` and `esp::embed` in one published crate.

---

## 1. The lane, and the one gap this doc fills

The local-models lane is already well-designed in pieces: the **geist brief** owns the model
architecture and LoRA stacking; **communal-compute** + resource-coordination own the marketplace;
**persona** owns adapter scoping; **eidetic** owns model storage (`ModelManifest` ships,
hash-verified on load); **`intel/embed`** ships tier-1 embeddings behind a provider seam. What none of
them bind is the **runtime + harness**: which engine actually runs inference and training, how it
wires into Mere's process model, and what the wasm and native targets can each do. The geist brief
deliberately left this a seam (§11 "runtime/provider choice", §14); the tier-1 research deferred it as
tier-3+. This doc is that layer. It does not re-derive the architecture, marketplace, or governance.

---

## 2. The runtime seam (the core call)

**Extend the pattern that already works.** `intel/embed` defines `EmbeddingProvider` (a `Send + Sync`
trait: `dimensions` / `metric` / `embed(&[&str]) -> Result<Vec<Vec<f32>>, EmbedError>`) with two
backends behind it: `hashed` (shipped, deterministic, dependency-light) and a `bert` Burn-wgpu slot
(the real model, future). `EmbedError::Backend` already names "Burn / CPU / wgpu". The harness adds two
sibling seams in the same shape, not a new framework:

- **`InferenceProvider`** — text in, tokens/text out, streaming; `Send + Sync`; a capability descriptor
  (context window, quantization, runtime-loader id) so a caller can match a model to a runtime.
  Backends selected by capability, exactly as the geist brief's `ModelRuntimeCapability` envisions.
- **`AdapterLoader`** — load/stack LoRA-adapter engrams against a base, honouring the geist brief's
  compatibility envelope (base bytes, tokenizer, prompt template, target modules, rank/alpha,
  quantization, loader). Silent best-effort loading is a correctness bug (geist §5); a mismatch is a
  rejection or an explicit converter, never a guess.

**Backends, by target:**

- **Burn-wgpu** is the one backend that reaches the browser/PWA (the load-bearing target;
  `project_browser_pwa_shapes_scripting`: no JIT runtimes in-browser, Wasmtime out, Burn-wgpu the
  inference path). Burn 0.21 already runs in `aether` and `intel`. It carries small-model inference and
  the RAG path everywhere, wasm included.
- **Native heavier runtimes** (`mistral.rs`, `llama.cpp` for hot inference; PEFT/Axolotl for training;
  LoRAX for multi-LoRA serving) sit behind the same `InferenceProvider` seam, native-only, selected by
  capability. The seam keeps them swappable; Mere binds none as universal.
- **LoRA machinery** rides the Burn lane via `bunsen` (geist §12): borrow type-erased param descriptors
  + a group optimizer now; adopt its model bodies only if/when an in-house Burn transformer lands.

This is a binding of *shape*, not of vendor: the provider trait + capability-matched backends + the
adapter compatibility envelope. The vendor choice stays per-capability and per-target.

---

## 3. The harness (how it wires into Mere)

Inference and training are off-UI-thread, long-running, low-priority work, the same shape as fetch,
sync, and the Alembic's Athanor daemon. So they ride **armillary actors**, not the render loop:

- **An inference actor** holds a loaded base + active adapter stack, answers prompt requests, streams
  tokens back as actor messages. The host shows status in Steward (a live op) and never blocks the UI
  on a token.
- **The Distillery-as-trainer** is a native-only, rare, heavy armillary job: read the scoped corpus
  (persona/moot engrams), train a LoRA, emit a `ModelAdapterManifest` + `TrainingCorpus` + `EvalReport`
  engram triple (geist §6) for the host to adopt. It proposes; adoption is a separate gated act, like
  Athanor's proposal/apply split.
- **Model loading** is eidetic's job, already built: `ModelManifest` resolves weights from Local + Iroh
  + HTTPS sources, hash-verified. The harness loads a base or adapter by `ManifestId`; no new storage.

**The dev/test harness** matters because CI and most contributor machines have no GPU: a `hashed`-style
deterministic stub `InferenceProvider` (canned/echo outputs) lets the whole pipeline (seam, actor,
adapter manifest load, RAG plumbing) be tested without a model, exactly as `intel/embed`'s `hashed`
provider tests the embedding path. A real-model smoke is an opt-in, GPU-gated, native-only test.

---

## 4. wasm vs native (the split that decides scope)

The PWA target is load-bearing, so the harness is honest about the boundary:

| Capability | In-browser (wasm, Burn-wgpu) | Native desktop |
| --- | --- | --- |
| RAG over eidetic embeddings | Yes (tier-1 embeddings already wasm-clean) | Yes |
| Small-model inference (1-3B) | Yes, Burn-wgpu | Yes |
| 7B+ inference | Practical only native (memory/throughput) | Yes, via the native seam |
| LoRA training | No (native-only) | Yes, GPU-gated |
| Heavy runtimes (mistral.rs / llama.cpp / PEFT) | No (native crates) | Yes |

So the in-browser geist is "RAG + small-model inference"; training and large models are a native
capability the seam exposes when present. This is the same conclusion the geist brief reaches for
compute access (§7.4), stated as a build-target contract.

---

## 5. First implementation slice (grounded, no training)

Mirrors the geist brief's experiments 1-2, scoped to what exists:

1. **`InferenceProvider` seam + a stub backend**, beside `EmbeddingProvider` in the `intel` lane. A
   `hashed`-style deterministic stub so the seam, the actor, and the RAG shim are testable now.
2. **A RAG shim** over the live tier-1 embeddings (`intel/embed` + the vector index): retrieve scoped
   engrams, build a prompt context, hand to the provider. Confirms the loop personal-engrams → context
   → output with the stub, no model required.
3. **`ModelAdapterManifest` save/load over eidetic** (the geist §6 schema as a `TypedPayload`, like the
   Alembic's `GraphEngram`): name, hash, compatibility-envelope, save, reject a mismatched load. Proves
   adapters can be addressed and validated before any training exists.

Net-new here: the `InferenceProvider` / `AdapterLoader` traits, the inference actor, the adapter
manifest binding. Reused: `EmbeddingProvider`, the vector index, eidetic `ModelManifest` storage, the
armillary actor framework, the eidetic typed-payload layer.

**Current disposition (2026-08-09):** `InferenceProvider`, its deterministic stub, the Burn decoder,
Eidetic model loading, streaming actor, cancellation, and a headed native host are landed in
`esp::infer`. `AdapterLoader` and its manifest/session binding remain gated on one real adapter; the
browser execution claim moved to the dedicated D2 probe.

---

## 6. Owned elsewhere (not re-solved here)

- **Marketplace** (offer/ask/receipt, tessera credit, peer GPU, verification): communal-compute +
  resource-coordination briefs.
- **Privacy / consent of training data** (per-engram training posture, forward-only deletion, the
  right-to-be-forgotten incompatibility): geist §10.2, the hardest question, owed its own pass.
- **Adapter governance** (trainer/evaluator roles, adoption review, revocation): geist §8.
- **Architecture** (LoRA stacking, the schemas, base-model choice): geist brief, canonical.

---

## 7. Open questions specific to the harness

- **Where the seam crate lives (answered).** `esp` is published from Mere with independent
  `esp::infer` and `esp::embed` namespaces and feature gates.
- **Streaming shape across the actor boundary.** Token-by-token messages vs chunked; backpressure when
  the UI is slow. Mirror the fetch actor's delivery model.
- **Adapter format canon.** PEFT/safetensors as the training artifact, runtime-specific converted
  adapter engrams at the edges (geist §11) — confirm once a real runtime is wired.
- **wasm model size ceiling.** What actually runs in a browser tab's memory budget via Burn-wgpu;
  empirical, sets the in-browser inference tier.
- **Whether Burn-wgpu inference is competitive enough** to be the default native small-model path too,
  or native always reaches for `mistral.rs` / `llama.cpp`. Measure before binding.

---

## Progress

- 2026-06-24: Brief drafted as the spin-off the Alembic plan's open decision #4 calls for (the
  local-models + harness lane). Grounded in the live seam (`intel/embed`'s `EmbeddingProvider` + the
  `hashed` / `bert`-Burn-wgpu backends), eidetic `ModelManifest` storage, Burn 0.21 in `aether`/`intel`,
  and the armillary actor framework. Frames the gap as the runtime + harness layer the geist brief
  (§11/§14) and the tier-1 research both defer; does not re-derive architecture, marketplace, or
  governance (cross-referenced in §1/§6). Corrected a homonym during scoping: `offgrid_lora_transports`
  is LoRa *radio*, not LoRA adapters, so it is not part of this lane.
- 2026-08-09: Refreshed after ESP E0-E4. Marked the implemented inference/actor/Eidetic/native-host
  slice as landed, recorded ESP as the seam home, linked the D2 headed-browser plan, and kept
  `AdapterLoader` gated on a real immutable model-session implementation rather than an abstract
  manifest-only trait.
