# Geist Models — Personal and Moot-Trained Inference Adapters

**Date**: 2026-05-10
**Status**: Proposal (design probe)
**Scope**: Defines the architectural shape for community-trained and personally-trained inference models in mere. A *geist model* is an open-weight base model plus one or more LoRA-style adapters trained on a defined corpus (a person's flora, a moot's flora, or both, composed at inference). Treats personal-geist (orrery-scope) and moot-geist (moot-and-up scope) as one architecture with shared substrate (eidetic engrams, persona-derived scopes, Distillery, tessera). Distinct from but adjacent to the existing local-intelligence-integration research, which scopes tier-1 embeddings and defers tier-3+ LLM serving — this brief is the tier-3+ shape that picks up where that defer leaves off.

**Implementation correction (2026-08-31):** this remains useful background,
but its key vocabulary and aggregation account are superseded. **FLORA** means
federated LoRA, specifically the exact FLoRA stack: participant A factors
concatenate vertically, B factors concatenate horizontally, scaling is applied
once to B, heterogeneous ranks sum to the global rank, and a signed explicit
budget bounds that rank. The lower-case *flora* corpus sense in this brief was
a capitalization-driven misunderstanding. **Codicil** replaces Engram,
**Standing** replaces Tessera, and a community-adopted adapter can become a
**Tulpa** through Gemot's frozen-electorate recognition lane. The former
memorial Tulpa meaning is now **Hagiograph**. Generic inference-time summing of
independent adapters is a separate composition technique and must not be cited
as the implemented FLORA protocol.
**Related**:

- [`../research/2026-05-08_local_intelligence_integration_research.md`](2026-05-08_local_intelligence_integration_research.md) — tier-1 embeddings landed; tier-3+ LLM serving deferred behind Distillation Boundary + AWAL prerequisites. This brief is the architectural sketch for that deferred tier.
- [`../implementation_strategy/2026-05-07_event_dag_substrate_brief.md`](../implementation_strategy/2026-05-07_event_dag_substrate_brief.md) — substrate; particularly §8.7 (persona keypair derivation) and the engram envelope shape.
- [`../implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md`](../implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md) — tier framework (orrery → moot → moothold → coalition) and voluntary-hosting-with-stakes pattern that compute hosting extends.
- [`../implementation_strategy/2026-05-10_graph_cluster_namespaces_brief.md`](../implementation_strategy/2026-05-10_graph_cluster_namespaces_brief.md) — graph-cluster namespaces; relevant for scoping which engrams a tier-specific adapter trains on.
- [`../../eidetic_docs/implementation_strategy/2026-05-09_eidetic_layered_stack_plan.md`](../../archive_docs/2026-06-09_completed_plans/2026-05-09_eidetic_layered_stack_plan.md) — Phase 5 already stores model weights as content-addressed engrams; this brief uses that as substrate.

---

## 0. Why this brief exists

Two questions surfaced during the 2026-05-10 conversation:

1. *Can a moot host and train a model on its accumulated knowledge — its geist — and serve it as a computation utility for members, with optional networked compute and tessera-based compensation?*
2. *Can individuals do the same for themselves, training against their own dataset?*

The answer to both is **yes, with shared architecture.** This brief sketches that architecture. The substrate is mostly already in place (eidetic Phase 5 stores model weights; tessera exists as a compensation primitive; persona keypair derivation gives scoped identity; the Distillery framing already nominates a moment of "compress local memory into a portable payload"). What's missing is the layer that turns these pieces into an inference-and-training system.

**The unifying concept: a geist model is a base open-weight model plus one or more LoRA adapters, each adapter trained on a defined-scope corpus (personal, moot, moothold, coalition), each adapter an engram, composed at inference time by the user.** Personal and community models are one mechanism applied at different scopes.

---

## 1. The geist concept across tiers

The tier framework already structures mere's data model:

| Tier | Scope | Geist model interpretation |
|---|---|---|
| **t1 — Orrery** | A single user's graph view (`SpaceId::Personal(master_pubkey)`) | A model that thinks in the user's voice over their own annotated content |
| **t2 — Moot** | A themed federatable graph community (`SpaceId::Moot(MootId)`) | A model that thinks in the moot's idiom over the moot's flora |
| **t3 — Moothold** | A federation of moots | A model trained across multiple moots' floras (cross-moot synthesis) |
| **t4 — Coalition** | A sovereign coalition of mootholds | Very rare, very large; speculative |

**The geist of a thing is its accumulated, schema-typed knowledge plus an inference primitive that thinks in its idiom.** Engrams are the memory; the geist model is the thinking. Distillation is the moment knowledge becomes shareable; training a geist adapter is the moment knowledge becomes inference-ready. Two products of the same operation.

Each tier's geist model is composable at inference: a user can apply their personal adapter, a moot adapter for the moot they're currently in, and another moot adapter for a co-membership all at the same time. The model becomes "you, thinking about this moot's topic." This is the architectural keystone (§5).

---

## 2. The substrate that's already there

Inventory of pieces in place or in flight:

- **eidetic Phase 5** stores model weights as content-addressed engrams: `ModelManifest` schema engram referencing `weight_blob` + `tokenizer_blob`. Sources list resolves Local + Iroh + HTTPS. Hash-verified on load. Already shipped (2026-05-09 progress note in the eidetic plan). LoRA adapters are just smaller engrams of the same shape.
- **Engram envelope three-axis classification** (privacy / provenance / trust) carries directly to "this adapter was trained by these provenance peers from these source engrams under this trust envelope." No envelope changes needed.
- **Tessera** is the existing tokenized contribution-and-trust receipt (per `project_tessera_trust_token` memory). Extends naturally to "tessera as compute credit" — askers spend tessera, providers earn it, governance follows the existing chain-rooted reputation model.
- **Persona keypair derivation** (substrate brief §8.7) gives each persona its own derived identity. A persona's adapter is trained on that persona's scoped engrams; private to that persona; composable with that persona's master at inference.
- **The Distillery** is already conceptually present in the inherited STM/LTM/Engrams plan as "compress local memory into a portable payload." This brief proposes a sibling operation: **Distillery-as-trainer** — same source engrams, output is a LoRA adapter rather than a published engram payload.
- **Voluntary hosting with stakes** (moot-tiers brief) is the governance shape that compute hosting extends. Members volunteer GPU cycles or storage; reputational stakes are the accountability primitive; lapse-and-revive is normal.
- **Burn-backed statistical stack** (per the local-intelligence-integration research) is the Rust-native first choice for embeddings, field-algebra-adjacent tensor work, and browser/PWA-reachable small-model inference. It is **not** yet the committed LoRA training or hot LLM-serving runtime. Tier-3+ should keep a runtime seam so `mistral.rs`, `llama.cpp`, PEFT/Axolotl, LoRAX, Burn, or a future native runtime can be swapped by capability.

What's NOT in place:

- The training pipeline itself (Distillery → LoRA adapter)
- The adapter-composition logic at inference time
- The runtime/provider seam for LLM inference and adapter loading
- The strict adapter-compatibility contract (base model, tokenizer, quantization, target modules, prompt template)
- The compute marketplace primitives (tessera-as-credit accounting, peer GPU offer/ask)
- The governance for moot-trained adapters (who trains, who approves, how to roll back)

This brief sketches all of those.

---

## 3. Open vs closed weights — only open viable

Closed-weight models (Anthropic Claude, OpenAI GPT, Google Gemini) cannot be moot-hosted or personally fine-tuned because the weights aren't accessible. They can be queried with moot-context (RAG-style) via a member's API key, which is a valid parallel pattern but not a *geist model* in the sense of this brief.

Open-weight families viable as geist bases (May 2026):

- **Llama 3** family (Meta) — 1B, 3B, 8B, 70B; permissive license with usage limits.
- **Mistral / Mixtral** — strong sparse-mixture-of-experts; commercial-friendly licenses on smaller models.
- **Qwen** (Alibaba) — multilingual; competitive at smaller sizes.
- **DeepSeek** — strong reasoning; recent versions liberal-licensed.
- **Phi** (Microsoft) — small, efficient; 1B and 3B variants run on CPU.
- **Gemma** (Google) — small open variants of Gemini lineage.

A moot's constitution declares its base model choice — version, size, quantization. Members must be able to obtain and run that base. Base-model engram (model weights as content-addressed blob) is shipped via the same eidetic Phase 5 mechanism that already exists for embedding models.

**Configurability is non-negotiable.** Per workspace `feedback_configurability_over_opinionated_defaults`, the base-model choice is a per-space setting; no hardcoded default. The brief recommends starting with Phi-3 mini (small, CPU-runnable) for personal experiments and a Llama 3 8B for moot experiments, but neither is prescriptive.

---

## 4. Three operations (RAG, LoRA, full fine-tune)

Geist models compose three orthogonal operations. The user/moot picks any combination.

### 4.1 RAG (retrieval-augmented generation)

Index the corpus's engrams; at inference time, retrieve relevant context and prepend to the prompt. **No model training.** Works with any base model, including closed-API models.

- **Index:** vector embeddings over engram payloads. Tier-1 embeddings (already shipped in `intel/embed`) cover this directly.
- **Retrieval:** k-nearest-neighbor over the embedding index, scoped to the appropriate corpus (orrery, moot, etc.).
- **Inference:** retrieved context is prompt-prepended; model answers grounded in retrieved engrams.
- **Cost:** index storage (MB-to-GB range depending on corpus size); inference is base-model cost.
- **Best for:** answering "what does my flora say about X" — knowledge index queries.

### 4.2 LoRA / QLoRA fine-tuning

Train low-rank adapters on the corpus's content. Base weights are frozen; the adapter is a small (~10-100MB) additive delta.

- **Cost:** single GPU, hours-to-days for 7B base model. QLoRA (4-bit base + LoRA) reduces VRAM further; runs on a 16GB-VRAM laptop GPU.
- **Output:** a LoRA adapter engram with `ModelAdapterManifest` schema (sketched in §6) referencing base model + training corpus.
- **Best for:** capturing the corpus's style, vocabulary, idiom — making the model *sound* like the corpus.
- **Not best for:** factual knowledge from the corpus (RAG does this better; fine-tuning blurs facts into statistical patterns).

### 4.3 Full fine-tuning / continued pretraining

Update all weights. Multi-GPU days-to-weeks for non-trivial models.

- **Cost:** prohibitive for individuals and most moots; only justifiable for very mature mootholds with deep accumulated content and committed compute resources.
- **Best for:** deep adaptation of a base model to a domain — e.g. a moothold focused on a specialist field whose vocabulary and reasoning patterns aren't well-represented in the base.
- **Defer:** out of scope for v1. Possibly out of scope permanently if LoRA + RAG cover the same value at 100x less cost.

### 4.4 Recommended composition

**For personal-geist v1: RAG + (optionally) LoRA.** RAG carries the knowledge work; LoRA is opt-in for users who want voice/style mirroring. Both train against the user's eidetic content scoped to the persona.

**For moot-geist v1: RAG + LoRA.** RAG over the moot's flora; LoRA adapter trained periodically by the designated trainer-role member.

**Full fine-tuning: deferred.** Revisit when a moothold demonstrates the need.

---

## 5. LoRA stacking — the architectural keystone

LoRA adapters compose mathematically: each is an additive low-rank delta to the base weights. Multiple adapters apply simultaneously by summing their deltas.

```text
inference-time weights = base_weights
                       + alpha_personal * personal_adapter_delta
                       + alpha_moot_a   * moot_a_adapter_delta
                       + alpha_moot_b   * moot_b_adapter_delta
                       + ...
```

Each `alpha_*` weight is a per-adapter scaling factor (typical range 0.5-1.5). The user composes at inference time: which adapters are active, with which weights.

**This is the architectural keystone.** It unifies the personal and moot cases under one mechanism:

- **Each adapter is an engram.** Carries its own provenance, trust envelope, schema reference. Adapters from a moot you don't trust can be filtered out via the trust axis.
- **Each adapter is scoped.** A personal adapter is `PrivacyClass::Private`, scope `SpaceId::Personal(master_pubkey)`. A moot adapter is `PrivacyClass::MootMember` (or whatever the eidetic envelope ends up calling it), scope `SpaceId::Moot(moot_id)`.
- **Composition is the user's authority.** The user (or their automation) picks which adapters to activate. Mere never composes adapters automatically without user configuration. This matches the existing "you choose what to subscribe to" pattern.
- **Composition mirrors tier nesting.** A user in moot M (under moothold H) can compose: personal + M-moot + H-moothold adapters. Each tier's geist contributes to the inference.

**Existing tooling:** the HuggingFace PEFT library + LoRAX support multi-LoRA inference. `mistral.rs` and `llama.cpp` are stronger near-term candidates for local hot LLM inference than Burn; PEFT/Axolotl remain stronger near-term candidates for adapter training. Burn remains the right first-class Rust tensor stack for embeddings and field algebra, but geist must not depend on Burn becoming the adapter runtime.

Operationally, "LoRA stacking" is not just summing arbitrary deltas. Adapters compose cleanly only when their compatibility envelope matches:

- same base model bytes or an explicitly compatible base lineage
- same tokenizer bytes and chat/prompt template
- same adapter format and format version
- same target-module names and layer mapping
- compatible rank / alpha semantics
- compatible quantization assumptions and runtime loader

If any of these differ, the adapter is either rejected or loaded through an explicit converter that produces a new adapter engram. Silent best-effort adapter loading is a correctness bug.

---

## 6. Distillery-as-trainer — the sibling operation

The Distillery's existing role: compress local memory into a portable engram. Add a sibling: **train a LoRA adapter from local memory.**

```text
                   Local memory (eidetic engrams)
                              │
                              │ Distillery
                              │
              ┌───────────────┼─────────────────────┐
              │                                     │
              ▼                                     ▼
      Published engram                    LoRA adapter engram
      (declarative content;               (procedural style/idiom;
       schema-typed payload)               base-model-ref + delta weights)

      Same source corpus.                 Same trust envelope.
      Different output type.              Same provenance chain.
```

Both crystallize at the engram boundary. Both produce content-addressed engrams. Both carry three-axis classification. The user (or moot governance) picks which output(s) to produce per Distillery invocation.

### Schema sketch

```rust
// Illustrative signature only — not implementation-ready (per
// feedback_spec_code_samples_illustrative_vs_implementation_ready).

struct ModelAdapterManifest {
    base_model_ref: ManifestId,        // points to ModelManifest engram
    adapter_blob: BlobHash,             // BLAKE3 of LoRA weights
    adapter_format: AdapterFormat,      // LoRA, QLoRA, prefix-tune, etc.
    adapter_format_version: String,
    runtime_compat: AdapterRuntimeCompat,
    rank: u16,                          // LoRA rank
    alpha: f32,
    target_modules: Vec<String>,        // which base layers were adapted
    tokenizer_ref: ManifestId,          // tokenizer this adapter was trained against
    prompt_template_hash: BlobHash,      // prompt/chat template used for training/eval
    quantization_assumption: QuantizationAssumption,
    training_corpus_root: ManifestId,   // points to a TrainingCorpus engram
    training_method: TrainingMethod,    // hyperparameters as a structured payload
    trainer_provenance: ProvenanceRecord,  // who trained this; what GPUs; what wall-clock
    eval_results: Option<ManifestId>,   // optional; points to EvalReport engram
}

struct AdapterRuntimeCompat {
    minimum_capabilities: Vec<ModelRuntimeCapability>, // e.g. peft-lora, gguf-lora, x-lora
    known_loaders: Vec<RuntimeLoaderId>,               // runtimes this adapter was tested in
    converter_lineage: Vec<ManifestId>,                // source adapters if converted
}

struct TrainingCorpus {
    source_engrams: Vec<ManifestId>,    // every engram included in training
    sampling_policy: SamplingPolicy,    // how engrams were weighted/filtered
    privacy_posture: TrainingPrivacy,   // see §10.2
    consent_policy_ref: ManifestId,     // constitution / per-engram training posture snapshot
    excluded_engrams_hash: BlobHash,    // audit hook without exposing every excluded item
    snapshot_taken_at: Timestamp,
}

struct EvalReport {
    base_model_ref: ManifestId,
    adapter_ref: ManifestId,
    benchmarks: Vec<BenchmarkResult>,   // generic + corpus-specific
    memorization_canaries: Vec<CanaryResult>,
    poisoning_checks: Vec<PoisoningCheckResult>,
    sample_outputs: Vec<SampleOutput>,  // human-readable; for adoption review
}
```

**Why these are separate engrams:** the corpus must be reproducible (which engrams went in), the eval must be transparent (which benchmarks, which sample outputs), the adapter must be verifiable (training inputs traceable). Each engram is content-addressed and can be sourced independently.

**The adapter is auditable, not bit-reproducible by default.** Given the same base model, corpus, training method, and seed, a third party can re-run training and compare behavior, eval scores, and rough weight statistics, but GPU nondeterminism and library drift make byte-identical adapters unrealistic. The trust contract should say "re-trainable evidence" rather than "same bytes."

---

## 7. Personal-geist — the simpler case

The orrery-scoped case. Single user, single dataset, single device (or a user's multi-device personal namespace).

### 7.1 Substrate readiness

Already in place:

- Persona keypair derivation gives scoped identity per persona.
- Eidetic stores everything — AWAL browsing trails, graph annotations, notes, murmurings authored by the user.
- Phase 5 stores model weights and (with this brief) adapter weights.
- Tier-1 embeddings already index a portion of personal content (with more to come per the local-intel research).

A personal-geist v1 needs:

- A **base-model selection step** at first run (defaults: Phi-3 mini for CPU users, Llama 3 8B for GPU users; configurable).
- A **personal RAG index** scoped to the user's eidetic content (extension of existing tier-1 embeddings).
- An optional **personal LoRA training pipeline** (Distillery-as-trainer invocation, scoped to the user's persona engrams, output is a personal adapter engram).
- An **inference shim** that composes base + personal adapter (if present) and answers user prompts with personal RAG context.

### 7.2 What it actually delivers (honest)

| Promise | Delivered by | Realistic |
|---|---|---|
| "Knows my content" | RAG over personal engrams | **Yes** |
| "Sounds like me" | Personal LoRA adapter | **Yes (style/vocab)** |
| "Reasons like me" | Aspirational | **Mostly no** — current LoRA captures style not reasoning patterns |
| "Doesn't hallucinate about my content" | RAG with strict grounding mode | **Mostly yes** with care |
| "Stays current as I add new content" | Periodic Distillery re-train | **Yes** but cadence-bounded |

Don't promise the third row. Personal AI hype overstates "thinks like you"; the honest deliverable is "writes like you, knows what you know."

### 7.3 Persona scoping

Each persona has its own derived adapter:

- `mark.master` — full adapter trained on all personal engrams.
- `mark.work_persona` — adapter trained on work-context engrams only (`PrivacyClass::WorkContext`).
- `mark.private_persona` — adapter trained on private-context engrams only.

The user composes at inference time which persona-adapter is active. This gives clean separation: you can ask the work-persona model a work question without it reaching into private context, even though both train on the same base model.

This is the t1 analog of the moot-and-moothold composition pattern — same mechanism at smaller scope.

### 7.4 Compute access for users without GPUs

Three escape hatches:

1. **CPU-runnable small models.** Phi-3 mini, Qwen 0.5B, similar — usable for inference on any modern laptop. Training is harder on CPU but feasible for very small adapters.
2. **Cloud-rent for training spikes.** User pays for a GPU instance for a few hours, runs the Distillery training, downloads the adapter engram, future inference is local. Fits "training is rare; inference is constant."
3. **Peer compute marketplace** (§9). Another mere user with GPU does the training in exchange for tessera. Same primitive that serves moot-geist.

Configurability per workspace memory: the user picks the compute path; defaults cover the common case but never lock in.

---

## 8. Moot-geist — the multi-author case

Generalizes the personal case to multi-author content with governance.

### 8.1 The trainer role

A moot designates one or more **trainer-role** members. The trainer:

- Receives a Distillery-training cap (meadowcap-style) from moot governance scoping the corpus they may train on.
- Periodically (event-triggered or scheduled) runs the training pipeline against the moot's flora.
- Publishes the resulting `ModelAdapterManifest` + `TrainingCorpus` + `EvalReport` engrams into the moot.
- Earns tessera for the compute work performed.

The trainer is a privileged role — they have access to the full corpus and produce artifacts that other members consume. Trainer selection is a governance decision; the moot's constitution declares the rule (designated member, rotating slot, election, hire-from-marketplace).

Trainer and evaluator should be separate roles for adopted moot adapters. A trainer may publish an `EvalReport`, but adoption should require either independent evaluator signatures or quorum review of a reproducible eval harness. A trainer self-certifying their own adapter is acceptable for personal-geist and experiments, not for a moot default.

### 8.2 Adoption review

A new adapter engram is *proposed*, not automatically active. Moot members (or designated reviewers) review the EvalReport, look at sample outputs, decide whether to adopt. Adoption is itself a moot event:

- `AdapterAdoption(adapter_engram_id)` — a quorum-gated event that promotes the adapter to "active" for the moot.
- Members default to using the active adapter; can manually override.

Bad adapters can be rolled back by an `AdapterRevocation` event. The trust envelope on each adapter engram tracks the history.

Adoption review must include at least three gates: compatibility validation (the adapter loads only against the declared base/tokenizer/runtime envelope), quality validation (corpus-specific + general evals), and safety validation (memorization canaries + poisoning checks). This makes "bad adapter" concrete rather than a vibes-based review category.

### 8.3 Cross-moot composition

A user in multiple moots can compose multiple moot adapters at inference. Each adapter contributes its idiom; the result is "you thinking about this conversation, weighted by the moots you're a member of."

The user authority principle holds: mere never composes moot adapters without explicit user opt-in. A user choosing not to load moot M's adapter doesn't violate moot M's membership; the moot adapter is a *tool*, not a *requirement*.

### 8.4 Privacy and consent for moot training

This is the hardest design question (covered in §10.2 below). Briefly: the moot's constitution declares the training-consent rule. Three viable shapes:

- **Default-in.** Members consent to inclusion at moot-join; new members joining after an adapter is trained are not retroactively included until next training.
- **Default-out.** Members must explicitly opt in per-engram or per-flora-set.
- **Hybrid.** Some engram classes (chat, public posts) default-in; some (annotations, sensitive engrams) default-out.

The hybrid is probably right; opinionated defaults are wrong per the configurability memory.

### 8.5 Federation across mootholds

A moothold (t3) can train a federation-shared adapter from member moots' floras. The same trainer-role + adoption-review pattern applies, scoped to moothold governance. Moothold adapters are larger-scope, slower-cadence, higher-eval-bar.

A user in mootholds H1 and H2 can compose H1's adapter, H2's adapter, plus moots within each, plus their personal — composition is unbounded in principle. In practice, each adapter adds inference latency proportional to its rank; users will probably stack 2-5 adapters max for real-time use.

---

## 9. Compute layering and tessera-as-credit

Inference and training are different compute profiles; both layer over the same marketplace primitive.

### 9.1 Inference tiers

| Model size | Inference compute | Latency profile |
|---|---|---|
| 1-3B (Phi, Qwen mini) | CPU laptop or any phone | Real-time |
| 7-8B (Llama 3 8B, Mistral 7B) | Single GPU laptop / M-series Mac | Real-time |
| 13-70B (Llama 3 70B, Mixtral) | Single high-end GPU or single host | Near-real-time |
| 70B+ (frontier open) | Multiple peers (Petals-style sharding) | Conversational; per-token network hops |

Smaller models cover most personal-geist use cases. Moot-geist ranges into 13-70B as moot floras grow.

### 9.2 Training tiers

| Adapter scale | Training compute |
|---|---|
| Personal LoRA over 7B base | Single laptop GPU, hours |
| Moot LoRA over 7B base | Same; trainer role's machine |
| Moot LoRA over 70B base | Single high-end GPU or rented cloud, days |
| Moothold LoRA over 70B base | Multi-GPU; cloud rental or dedicated trainer-collective |
| Full fine-tune anything | Multi-GPU days-to-weeks; out of v1 scope |

### 9.3 Tessera as compute credit

Extends the existing tessera primitive. The shape:

- A **compute offer** is a signed event from a member: "I offer N GPU-hours of class C at price T tessera per hour, until time X."
- A **compute ask** is a signed event from another member: "I need to train adapter A (or run inference query Q); I'll pay up to T tessera."
- Match-making is moothold-internal (probably an `intelligence-marketplace` module or a Distillery-side concern).
- On completion, a **compute receipt** event is signed by both parties; tessera moves from asker to provider.
- Per-task split: a Petals-style sharded inference involves N providers; the asker's tessera splits N ways proportional to compute contributed.

This puts compute on the same chain-rooted reputation footing as content contribution. Bad providers (unreliable, slow, incorrect outputs) lose tessera reputation; good providers earn it.

**Configurability:** every member can decide their offer policy (no offer / offer to my moots / offer to anyone). Tessera prices are user-settable, not protocol-fixed.

### 9.4 Closed-API fallback

A parallel path: a member uses their own paid API key (Anthropic, OpenAI) to query a closed model with moot-context prepended. Different from a geist model — no community training, just RAG over a paid third-party endpoint. Worth supporting because some users want frontier-model quality and accept the closed-trust trade-off.

This pattern doesn't conflict with geist models; the user picks per-query which mode to use. Both paths share the RAG + adapter-context plumbing.

---

## 10. The hard parts

### 10.1 Catastrophic forgetting

Fine-tuning on a narrow corpus degrades the model's general capabilities — math, coding, factual recall about non-corpus topics. LoRA mitigates by being additive (you can disable the adapter to recover base behavior) but doesn't eliminate within-adapter degradation.

Mitigations:

- **Mix general data into training corpus.** Standard practice; reduces forgetting.
- **Adapter alpha scaling.** Lower alpha at inference reduces adapter influence; users can dial it.
- **Eval against general benchmarks.** EvalReport includes both corpus-specific and general benchmarks; a regression in general benchmarks is a flag for adoption review.

This is well-trodden in the LoRA literature; no novel design required.

### 10.2 Privacy of training data — the hardest question

A model fine-tuned on content memorizes some of it. Members who later want to delete their contributions face difficulty: the model has already incorporated them, and "forgetting" trained-in content is an active research area without clean solutions.

The realistic answer is **contributions are durable in trained adapters**, and the privacy posture must be set up-front:

- **Training-corpus consent is per-engram.** When a member contributes an engram, they declare whether it's trainable (`TrainingPosture::Allowed | Forbidden | AskMe`). Default is `Forbidden` for new members; users opt in over time as they build trust.
- **Deletion is forward-only.** If a member opts out of training inclusion, future adapters won't include their content. Past adapters that included it remain — unchanged, signed, content-addressed.
- **Adapter expiry / regeneration cadence.** Moots can declare an adapter-regeneration cadence (every N months); old adapters become deprecated; adapters trained after a member's opt-out won't include their content. Eventually-correct, not immediately-correct.
- **Differential privacy in training.** DP-LoRA techniques exist; they reduce memorization at a quality cost. Worth offering as an opt-in for sensitive moots.
- **Right-to-be-forgotten incompatibility.** Mere is incompatible with strict GDPR-style "delete all traces of me" if the user has consented to training inclusion — past adapters can't be unilaterally rewritten without breaking content-addressing. Document this honestly; let users opt out of training to preserve forgettability.

This is the hardest single design question in this brief. It deserves a follow-up brief or a careful section in the moothold governance design.

### 10.3 Continuous learning cadence

Re-training on every new engram is wasteful. Never re-training stales the adapter. The realistic cadence is event-triggered batch:

- Periodic (e.g. weekly) Distillery training runs.
- Event-triggered runs when N new engrams have accumulated since last training, or when a significant content shift is detected (e.g. a new topic cluster forms per the namespace brief §3.2).
- User/moot governance picks the cadence rule.

Continuous adapter updates (one-engram-at-a-time gradient updates) are an active research area but probably not v1.

### 10.4 Quality control

A bad LoRA adapter (overfit, misaligned, inadvertently biased by training-data skew) can pollute responses. Mitigations:

- **EvalReport with sample outputs.** Adoption review sees concrete examples before voting.
- **Shadow mode.** A new adapter runs in parallel with the active one for a probation period; members can compare outputs without it being default-active.
- **Roll-back via AdapterRevocation event.** Reverses adoption; falls back to prior adapter or base.
- **Trust envelope decay.** Adapters from low-tessera trainers carry weaker trust; members can filter by trust.

### 10.5 Storage cost of model engrams

7B model at int4 ≈ 3.5GB. 70B at int4 ≈ 35GB. Pinning a moot's base + adapters means tens-of-GB per moot. Voluntary-hosting-with-stakes from the moot tiers brief covers this — model storage is one of the things hosts stake on.

LoRA adapters themselves are small (10-100MB each), so adapter-only sync is cheap; the heavy cost is the base model. A common base model shared across many moots reduces aggregate cost.

### 10.6 Provenance leakage in the adapter itself

A moot adapter's training corpus is recorded in the `TrainingCorpus` engram — listing which engrams (and therefore which authors) contributed. This is honest provenance but leaks membership-graph structure. Sensitive moots may want training corpora that are **opaque-but-attested** (a hash of the corpus list rather than the list itself, signed by a quorum of trusted reviewers).

The trade-off is auditability vs membership privacy; per-moot configurable.

### 10.7 Model poisoning and memorization canaries

Multi-author training turns every trainable engram into a possible attack surface. A malicious contributor can try to poison the adapter ("when asked about X, answer Y"), leak secrets through memorization, or skew the moot's voice by flooding low-quality training text.

Mitigations should be part of the adapter contract rather than an afterthought:

- **Canary strings.** Sensitive or synthetic strings inserted into private / forbidden engrams must not appear in generated output. EvalReport records pass/fail without exposing the canary itself.
- **Influence caps.** SamplingPolicy caps per-author and per-cluster contribution weight so one high-volume author cannot dominate a moot adapter.
- **Poison probes.** EvalReport includes prompts designed to catch known injected claims, hidden instructions, and identity/policy spoofing.
- **Segment-separable corpora.** Keep per-contributor dataset segments long enough to ablate suspicious contributors and re-run evals.
- **Independent evaluator signatures.** Adoption review trusts evaluator evidence more than trainer evidence.

---

## 11. Open questions

- **Runtime/provider choice.** Burn is the Rust-native statistical stack, but geist needs an `intelligence-llm`-style provider seam. Near-term candidates: `mistral.rs` or `llama.cpp` for local inference, PEFT/Axolotl for training, LoRAX for production multi-LoRA serving, Burn only where its model/runtime support is actually competitive. See §12 for `bunsen`, the Rust-native prior art on the Burn path (param descriptors + group optimizer).
- **Inference-time adapter composition limits.** How many adapters can stack before quality degrades or latency explodes? Empirical question; needs measurement.
- **Adapter format interoperability.** PEFT (HuggingFace) format vs GGUF/llama.cpp vs `mistral.rs`-loadable adapters vs MLX vs Burn-native. Probably PEFT/safetensors as the canonical training artifact and runtime-specific converted adapter engrams at the edges.
- **Trainer reproducibility.** Floating-point nondeterminism in GPU training means re-runs don't bit-match. How strict is the reproducibility claim, and what does third-party verification actually establish?
- **Multi-device personal training.** A user with a phone, laptop, and desktop — does each device train its own adapter, or is there a designated training device? Probably designated; smaller devices contribute via federated-learning-style gradient pooling at higher complexity.
- **Federated learning (vs trainer-role) for moot adapters.** An alternative to the trainer-role pattern: members each train locally on their own contributed engrams, share gradient updates, the moot aggregates. Heavier protocol, better privacy, much higher complexity. Consider as v2.
- **Adapter governance at moothold scale.** A moothold-shared adapter is trained on multiple moots' floras. Does each moot pre-approve inclusion of its flora? Or is moothold-membership implicit consent? Governance question without a settled answer.
- **Compensation for indirect contributors.** A model trained on engrams I authored generates revenue for the moot via tessera-paid inference queries. Do I get a share? Or is contribution implicitly already compensated by the tessera I earned at engram-publication time? Probably the latter; revisit if it feels unjust at scale.
- **Closed-model RAG pricing pass-through.** When a member uses a paid Claude/GPT API key with moot context, who pays for the API call? Just the asker? Or moot-funded? Governance question — probably asker-pays by default with optional moot-subsidy.
- **Adapter-as-IP question.** Does an adopted moot adapter "belong" to the moot? Members? Trainer? This question matters at moothold dissolution. Probably "belongs to the moot constitution" — moot can grant export licenses but the adapter stays moot-owned by default.

---

## 12. Prior art: bunsen (Rust-native Burn extensions)

[`zspacelabs/bunsen`](https://github.com/zspacelabs/bunsen) (Apache-2.0 / MIT, on
crates.io + docs.rs) is a "batteries-included" community standard library
extending [burn](https://burn.dev). It pins **burn 0.21** (the version Mere
already runs in `aether` and `intel/embed`), and its stated mission is to track
burn's release cycle and absorb the extension churn. Evaluated 2026-06-06. It
matters here because it is the Rust-native prior art for the LoRA / model-surgery
machinery this brief's tier-3+ path needs, and it sits inside the Burn lane the
provider seam already permits (§5, §11).

**Verdict: borrow technique now; adopt wholesale only when geist runs a real
in-house Burn transformer.** Today `intel/embed` is a hash-based provider with
only a *future* Burn-BERT slot, so bunsen's model body (blocks, kits, dataloader,
PyTorch import) has no consumer yet, while the small high-value pieces are cleanly
liftable.

### 12.1 Borrow list (technique, liftable at burn 0.21)

- **Type-erased parameter descriptors** (`burner::descriptors::TensorParamDesc` /
  `TensorDesc` / `TensorKindDesc`). Capture a `Param<Tensor<B,R,K>>`'s ParamId,
  shape, rank, dtype, kind, and size estimate via `From<&Tensor<..>>`, dropping the
  const-generic rank and kind. This is the escape hatch from Burn's generics that
  makes generic parameter manipulation possible. The keystone lift: one small
  module, no heavy deps. Serves §5 adapter-target selection (find the rank-2
  attention / MLP weights to attach a LoRA to).
- **Group optimizer** (`burner::optim::GroupOptimizerAdaptorN`). Partitions params
  into disjoint `ParamId` groups, mounts a separate `SimpleOptimizer` plus a
  per-group learning-rate selector on each, and implements Burn's own
  `Optimizer<M,B>` with duplicate-ParamId detection. This is the §4.2 LoRA
  training shape directly: one optimizer / LR for adapter params, another (or
  frozen) for the base. Their example drives Muon for matrix params and AdamW for
  the rest.
- **The reflection *pattern*, not their realization.** bunsen walks a `Module`
  into a queryable tree (`burner::module::reflection::XmlModuleTree`) and selects
  param groups by XPath. The idea (visitor to addressable param list to query to
  groups) is exactly the §5 model-surgery shape. Their realization is an XML
  document plus an XPath engine (`xot` + `xee-xpath`), which is heavy and
  wasm-hostile. Borrow the idea; back it with predicate closures or a path-glob
  over ParamId paths, not an XML stack.
- **Shape contracts** (`shape_contract![]`, the `contracts` module). Runtime
  tensor-shape assertions. Optional, useful if the intel tensor code grows.

### 12.2 What the whole dependency would buy later

If geist / intel commits to an in-house Burn transformer, bunsen's `blocks`
(attention with KV-cache, rotary embeddings, SDPA, patch-embed, drop-path /
drop-block, Swin and transformer families), `kits` (whole models),
`bunsen-firehose` (columnar dataloader plus a Burn batcher), and PyTorch
checkpoint import (Whisper / ResNet via `burn-store`) become a broad, burn-aligned
foundation, and bunsen's churn-buffering answers the workspace-pins doctrine
directly. That is the moment to re-evaluate the "use the whole buffalo" question.

### 12.3 Adoption caveat (wasm)

The browser / PWA target is load-bearing, and bunsen's heavy parts are
native-only: `reflection` (XML), `cache` (downloader + TLS), `store` (PyTorch
import), and the preview chat dataloader (arrow / parquet). All are feature-gated,
but `default` enables reflection + train + store, so any adoption means
`default-features = false` and cherry-picking. The portable core (blocks,
descriptors, ops, contracts, group-optimizer) extends Burn's modules and tensors
and should ride burn-wgpu to wasm.

---

## 13. First experiments

In rough order of leverage:

1. **Personal RAG over eidetic, served by a small local model.** Tier-1 embeddings + `mistral.rs` or `llama.cpp` inference behind a thin provider seam. Confirms the integration loop (personal engrams → RAG context → model output). Deliverable: a CLI that answers questions about personal flora.
2. **Adapter manifest without training.** `ModelAdapterManifest` + compatibility envelope + mock adapter blob save/load via eidetic. Demonstrates that adapter artifacts can be named, checked, and rejected before any training work exists.
3. **LoRA loading against a known runtime.** Use an existing public PEFT/GGUF-compatible adapter and load it through one runtime. Deliverable: compatibility failure cases are explicit, not mysterious runtime errors.
4. **Personal LoRA training on a 7B-or-smaller base.** Single-GPU Axolotl/PEFT training run on a user's personal flora. Burn can be evaluated here, but it should not be the assumed path. Output: personal adapter engram plus eval/canary report.
5. **LoRA stacking inference shim.** Compose base + personal-adapter + mock moot-adapter at inference. Measure latency overhead per stacked adapter and quality changes under different alpha weights.
6. **Single-moot trainer/evaluator/adoption loop.** A test moot with three users; one designated trainer; independent evaluator signs an EvalReport; members adopt; query the adapted moot model. Smallest trustworthy end-to-end demonstration.
7. **Tessera-as-credit primitive.** Compute-offer / compute-ask / compute-receipt event types. Implement matching for inference jobs (lower-stakes than training). Real test moots opt in; observe whether the marketplace converges.
8. **Differential-privacy LoRA opt-in.** Implement DP-LoRA training as an opt-in mode; measure quality cost; document trade-off for sensitive moots.
9. **Multi-moot adapter composition by a real user.** A user in two test moots loads both moot adapters + their personal; inference-time composition; user assesses whether the result feels right.

Each experiment is independently useful; later experiments don't strand earlier ones.

---

## 14. What this brief does not decide

- **Specific base-model defaults.** The brief recommends starting points (Phi-3 mini for personal CPU, Llama 3 8B for personal GPU, Llama 3 70B for moot) but defers binding selection to per-space configuration.
- **LLM runtime and training backend.** The brief commits to a provider seam and adapter artifact contract, not to Burn, PEFT, `mistral.rs`, `llama.cpp`, LoRAX, or MLX as the universal runtime.
- **Federated learning vs trainer-role adoption.** Trainer-role for v1; federated learning as v2 if privacy demands it.
- **Differential privacy default.** DP-LoRA as opt-in; not protocol-default.
- **Tessera price discovery.** Pricing is per-member-set; protocol provides the rails, not the prices.
- **Adapter rollback semantics.** Adoption-revocation is sketched but the precise quorum/governance rule is moothold-design territory.
- **Closed-model API integration shape.** Worth supporting as a parallel path; specific shape (per-user keys, moot-shared keys, moot-funded keys) deferred.
- **Multi-device personal training coordination.** Single designated training device for v1; multi-device pooling deferred.

---

## Findings

(Captured during the 2026-05-10 brief-drafting session.)

- The personal and moot cases share the same architecture; treating them as one brief saves duplication and makes LoRA stacking the unifying mechanism (§5).
- Most of the substrate is already in place — eidetic Phase 5 stores model weights, persona keypair derivation gives scopes, tessera is the compensation primitive, the Distillery framing nominates the moment. The missing pieces are the training pipeline, the adapter composition shim, the compute marketplace, and the moot governance for adapter adoption.
- The honest deliverable for personal-geist is "writes like you, knows what you know" — not "thinks like you." Don't promise the third row.
- Privacy of training data is the hardest design question (§10.2). Forward-only deletion is the realistic answer; document the right-to-be-forgotten incompatibility honestly.
- Burn is not the committed geist runtime. Use Burn where it is strong (embeddings / tensor work / field algebra) and keep tier-3+ behind a provider seam.
- `bunsen` (zspacelabs, Apache-2.0 / MIT, burn 0.21) is the Rust-native prior art for the tier-3+ LoRA path. Borrow now: type-erased param descriptors plus a group optimizer; borrow the reflection idea without its XML / XPath deps. Adopt wholesale only when geist runs an in-house Burn transformer (blocks / kits / firehose / PyTorch import); wasm adoption needs `default-features = false`. See §12.
- LoRA stacking is the architectural keystone — it unifies personal + moot + moothold under one composition mechanism, mirrors the tier framework, and respects the user-authority principle.
- LoRA stacking needs a strict compatibility envelope. Base bytes, tokenizer, prompt template, target modules, adapter format, quantization, and runtime loader are part of adapter identity.
- Distillery-as-trainer is the natural sibling operation to Distillery-as-engram-publisher. Same source corpus, two output types, picked per invocation.
- Moot adapters require trainer/evaluator separation, canary checks, poisoning probes, and adoption gates before they become defaults.
- Tessera-as-compute-credit is a clean extension of the existing tessera primitive; per-task splits map onto chain-rooted reputation; both training and inference compute layer over the same marketplace.
- Closed-model RAG is a valid parallel path, not a competitor to geist models; users pick per-query.
- Configurability per workspace memory is non-negotiable: base model, training method, training cadence, compute source, adapter composition, privacy posture — all per-space settings, not protocol-default.

---

## Progress

### 2026-05-10

- Brief drafted from the 2026-05-10 conversation.
- DOC_README index updated.

### 2026-05-11

- Hardened after repo/upstream audit: Burn narrowed to statistical/tensor stack rather than universal geist runtime; tier-3+ moved behind a provider seam.
- Added strict adapter compatibility envelope, audit-not-bit-reproducibility wording, trainer/evaluator separation, canary/poisoning gates, and revised first experiments around manifest/load proof before training.

### 2026-06-06

- Evaluated `zspacelabs/bunsen` (Rust-native burn-extension suite, burn 0.21) as prior art for the tier-3+ path. Added §12 (prior art) with a borrow list (type-erased param descriptors, group optimizer, the reflection pattern minus its XML / XPath deps), the whole-dependency case, and the wasm adoption caveat. Cross-linked from §11 (runtime/provider choice) and Findings. Renumbered the trailing sections (First experiments → §13, What this brief does not decide → §14). Confirmed Mere is on burn 0.21 (`aether`, `intel/embed`).
