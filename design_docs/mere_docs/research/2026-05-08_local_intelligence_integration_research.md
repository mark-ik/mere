# Local Intelligence Integration — Research Report (2026-05-08)

**Status**: Research synthesis

> **Crate-name note (2026-06-09 audit):** `intelligence-embeddings`→`intel/embed` (the §13 / 2026-05-18 notes already track this); the `graph-canvas` field-algebra work now lives in `orrery/aether`; donor `graphshell/...` paths point at the GitHub-archived donor. Dated "shipped"/status receipts below are historical record.
**Purpose**: Translate the inherited Graphshell-era intelligence research into a Mere-aligned plan. Cover what models can mean for Mere given the field-algebra landing, the eidetic crate, and Mere's browser/PWA target shape; identify which prior conclusions still hold, which need updating, and the most pragmatic first-cut work.
**Audience**: Architecture / planning. Implementation plans should descend from this report.

**Inheritance (graphshell-era; status noted per item)**:

- `repos/graphshell/design_docs/graphshell_docs/research/2026-04-02_intelligence_taxonomy.md` — **valid; adopt with renames**
- `repos/graphshell/design_docs/graphshell_docs/research/2026-04-02_intelligence_capability_tiers_and_blockers.md` — **valid; sequencing rule still applies**
- `repos/graphshell/design_docs/verse_docs/research/2026-02-24_local_intelligence_research.md` — **largely valid; technical stack settled, distribution updated**
- `repos/graphshell/design_docs/verse_docs/implementation_strategy/self_hosted_model_spec.md` — **contract framing valid; vocabulary needs Mere alignment**
- `repos/graphshell/design_docs/graphshell_docs/implementation_strategy/aspect_distillery/ASPECT_DISTILLERY.md` — **valid; aspect carries forward**
- `repos/graphshell/design_docs/graphshell_docs/implementation_strategy/aspect_distillery/distillation_request_and_artifact_contract_spec.md` — **valid; artifact classes carry forward**
- `repos/graphshell/design_docs/graphshell_docs/implementation_strategy/aspect_distillery/semantic_scene_scaffolding_note.md` — **valid; informs scene-mode integration with field algebra**

**Mere-side context**:

- `repos/mere/design_docs/graphshell_docs/implementation_strategy/2026-05-07_graph_canvas_field_algebra_plan.md` — Burn 0.21 already wired into `graph-canvas`; field algebra exists with `Sample(FieldId)` indirection that accepts vector outputs.
- `repos/mere/crates/eidetic/eidetic-core/` — private local memory crate and typed-artifact substrate; embeddings and vector indexes persist through it but are not owned by it.
- `repos/mere/design_docs/mere_docs/implementation_strategy/2026-05-07_event_dag_substrate_brief.md` — engram/event-DAG substrate; relevant when distillery output crosses peer boundaries via murm/moothold.

---

## 1. Executive summary

**Yes, Mere can host AI models. The question is sequencing.**

The 2026-02-24 graphshell research already concluded that **Burn (with wgpu)** is the right first engine for consumer-device statistical intelligence, on the grounds of universal hardware support — not Nvidia-only. That conclusion is **stronger now** for embeddings and field-algebra-adjacent tensor work, because Mere has additionally committed to a browser/PWA target where Burn-wgpu (via WebGPU) ships in browsers but Wasmtime / candle / libtorch / llama.cpp do not. It should not be stretched into a blanket commitment that Burn is the LLM-serving or LoRA-training runtime.

The key Mere-specific updates to the older research:

1. **Burn is no longer hypothetical** — it shipped in `graph-canvas` for the field-algebra evaluator at version 0.21 with ndarray and optional wgpu backends. Burn-backed BERT embeddings are mechanically wired in `intelligence-embeddings`; real MiniLM fixture validation is the remaining empirical gate.
2. **The first-class integration site is now the field algebra**, not the agent runtime. An embedding model produces `Vec<f32>` per node; that's a vector field that plugs into the existing `Sample(FieldId)` indirection. Force-directed-by-meaning, semantic neighbor search, and content clustering all fall out of one wiring.
3. **Eidetic is the right persistence substrate, not the embedding owner** — `intelligence-embeddings` owns providers, vector index, search facade, and field bridge; eidetic owns typed persistence, model manifests, content-addressed blobs, and future memory-domain APIs.
4. **Distribution is no longer "Verse" — it's moothold + iroh blobs.** The model-as-engram framing carries forward; the transport layer changed.
5. **Agentic intelligence is genuinely deferred** — Distillation Boundary, AWAL, typed artifact promotion, and supervised-agent runtime are real prerequisites the older research correctly identified, and Mere has not built them yet. Generation-first chat-with-graph features will be premature.

**The strongest first slice is structural intelligence powered by Burn-evaluated embeddings**: a small embedding model (MiniLM-class), a vector index persisted through eidetic, and an embedding-as-vector-field coupling so semantic similarity becomes an attractor in the existing layout system. **No LLM needed for the first horizon.**

---

## 2. The intelligence taxonomy (Mere-aligned)

The graphshell taxonomy stands. Five mechanism categories, with Mere-natural names where useful:

| Mechanism | What it is | Mere current state |
| --- | --- | --- |
| **Structural** | Graph thinking about itself. Graph topology, traversal history, frame affinity, graphlets, force-directed layout | Strong substrate already in `graph-canvas` + `graph-tree` |
| **Semantic** | Symbolic — UDC, tags, edge families, classifications | Migrated; tag/UDC systems live in graphshell donor docs and must come over |
| **Statistical** | Embeddings, similarity, learned ranking | **Mechanically shipped through Tier 2; real MiniLM empirical validation still pending** |
| **Generative** | Summaries, extraction, synthesis text | Deferred (LLM cost + trust gates) |
| **Agentic** | Orchestration over time — observe, plan, act, evaluate | Deferred (needs Distillery + AWAL) |
| **Collective / transfer** | Engrams, FLora, federated/shared artifacts | Carries through `moothold` + `mooting` once distillery exists |

The four orthogonal axes — mechanism, output, scope, autonomy — should be preserved in design discussions. The most-used Mere shorthand:

> *Structural intelligence is the graph thinking about itself. Statistical intelligence is the graph noticing latent similarity beyond explicit labels. Generative intelligence is the graph speaking. Agentic intelligence is the graph working over time. Collective intelligence is what gets shared across moots.*

---

## 3. Capability tiers (the planning ladder)

Adopting the older 6-tier ladder, with Mere-current readiness:

- **Tier 0 — Descriptive structural**: explain this graphlet, graph diff, current-thread reconstruction. *No new dependencies.* **Ready when graphlet/explanation surfaces are migrated.**
- **Tier 1 — Semantic assistance**: tag/UDC suggestions with provenance, accept/reject flows. *Existing dependencies.* **Blocked on durable semantic-record migration from graphshell.**
- **Tier 2 — Statistical retrieval**: embeddings, vector index, semantic search, semantic clustering. **First model-assisted target.**
- **Tier 3 — Generative local**: bounded summaries, extraction, "explain this selection". *Adds small LLMs behind a provider seam (`mistral.rs`, `llama.cpp`, candle, Burn, or future runtime depending on the feature).* **Deferred until Tier 2 is empirically validated and trust surfaces exist.**
- **Tier 4 — Supervised agentic**: AWAL, Distillery, typed artifacts, background agents. **Needs all of §6's prerequisites.**
- **Tier 5 — Transfer / collective**: engrams shared via moothold, community-fine-tuned LoRAs. **Needs Tier 4 plus federation.**

The graphshell research's sequencing rule still applies: **structural and semantic mature first, statistical is the best first model-assisted jump, generative comes after, agentic last**.

---

## 4. Burn — what's already wired and what's needed next

### 4.1 Already in the workspace

- `burn = "0.21"` (optional, behind `field-burn` feature) at `repos/mere/crates/graphshell/graph/graph-canvas/Cargo.toml`
- `field-burn-wgpu` feature flag for downstream consumers wanting wgpu acceleration
- Lowering walks the field algebra AST and emits Burn tensor programs — full forward eval for Const, CoordX/Y, Time, Add, Mul, Scale, Negate, Gaussian, Linear, Disk (4 falloffs), Dot, Sample, plus closed-form gradients
- 23 backend tests passing on the ndarray backend; lowering is generic over `B: Backend`

### 4.2 What hosting a model adds

| Component | Where | Already present? |
| --- | --- | --- |
| `burn-tensor` | transitive | ✅ via burn |
| `burn-nn` (Linear, attention, embeddings, transformer blocks) | transitive | ✅ via burn |
| `burn-autodiff` | transitive | ✅ via burn (we don't use it for inference) |
| `burn-import` (PyTorch / safetensors / ONNX → Burn module) | feature flag | ⚠️ not used yet; current BERT path manually maps safetensors |
| `tokenizers` (HF Rust) | optional dep | ✅ behind `intelligence-embeddings/bert` |
| Vector index | `intelligence-embeddings` | ✅ flat index; HNSW deferred |
| Model manifest + weight cache | `eidetic::models` | ✅ content-addressed `ModelManifest` + component resolution |

### 4.3 Burn vs the alternatives, revisited

The graphshell research preferred Burn over candle on hardware-support grounds (universal wgpu vs Nvidia-CUDA-required). That holds for the statistical tier. Three caveats are now load-bearing:

1. **Burn's model zoo is smaller than candle's.** For Llama-class models, candle has plug-and-play implementations; Burn requires manual architecture definition. For embedding models (BERT) the gap is small; for LLMs it's real.
2. **For LLM-class hot inference, llama.cpp / mistral.rs are stronger.** If a future tier-3 generative feature lands, evaluating `mistral.rs` (or candle for that one path) alongside Burn is reasonable. Burn doesn't have to win every axis.
3. **For LoRA training and multi-adapter serving, Burn is not the default assumption.** PEFT/Axolotl, `mistral.rs`, `llama.cpp`/GGUF, and LoRAX have more direct adapter affordances today. Mere should define a provider/capability contract, not embed the LLM runtime choice into the substrate.

For Mere's first-target tier (embeddings), Burn is still the right path. For Tier 3+, the decision is **provider seam first, runtime later**.

---

## 5. The first-cut integration: embeddings, eidetic, and the field algebra

This is the single most leveraged first slice. Concrete shape:

### 5.1 Crate layout

```
repos/mere/crates/eidetic/
├── src/
│   ├── lib.rs                    (existing)
│   ├── embedding.rs              ← NEW: trait + provider impls
│   ├── vector_index.rs           ← NEW: HNSW or flat index over eidetic blobs
│   └── ...
```

Or, if `eidetic` should stay narrow (Request/Response/Store), a sibling crate:

```
repos/mere/crates/intelligence/
├── intelligence-embeddings/      ← burn-bert + tokenizer + vector index
├── intelligence-models/          ← shared model manifest + weight cache
└── intelligence-llm/             ← deferred (tier-3)
```

**Recommendation**: keep `eidetic` narrow (private memory blob storage) and put embedding work in `intelligence-embeddings` as a sibling. The vector index is *over* eidetic blobs, but the embedding-model machinery has its own dependency surface (burn-import, tokenizers) that shouldn't bloat eidetic.

### 5.2 The embedding API

```rust
// illustrative
pub trait EmbeddingProvider: Send + Sync {
    fn dimensions(&self) -> usize;
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError>;
}

pub struct BertEmbeddings<B: Backend> { /* model, tokenizer */ }

impl<B: Backend> EmbeddingProvider for BertEmbeddings<B> { /* ... */ }
```

`BertEmbeddings::load(weights_path, tokenizer_path)` deserialises a HuggingFace safetensors model into burn-nn modules. The first model is **MiniLM-L6** (Apache 2.0, ~22 MB, dimensions=384) — small enough to bundle, fast on CPU, and the standard reference embedding for sentence-similarity tasks.

### 5.3 Field-algebra integration — and the spatial/embedding bridge

The field algebra evaluates at 2D positions `(x, y)`; embeddings are high-dimensional. There is a translation problem worth being honest about. Three bridges, each useful for a different feature:

**Bridge A — query similarity → scalar field.** Pick a query embedding (a search bar entry, the focused node's embedding, etc.). For each node, compute cosine similarity to the query → one scalar per node. Build a piecewise-linear or kd-tree-interpolated scalar field over canvas coordinates whose value at any point is the interpolated node-similarity. Register as a `ScalarField::Sample`-able. **Drives**: heatmap backgrounds (TwoDPreset::Heatmap), focus-attraction couplings, terrain renderings.

**Bridge B — node-pair springs.** For each pair of nodes (or each node and its k-nearest in embedding space), compute a spring constant from cosine similarity. Apply as inter-node forces in the existing scene_physics pass. **This is not a field-algebra integration** — it's a new pair-force pass alongside `compute_node_separation`. **Drives**: force-directed-by-meaning layout. The right home is `scene_physics/embedding_springs.rs` (a follow-on file in the split scene_physics module).

**Bridge C — pre-projected 2D targets.** Reduce the N-dim embedding space to 2D via UMAP/t-SNE/PaCMAP (small Rust crates exist) → per-node target position. Apply a soft spring toward each node's target. **Drives**: semantic-projected layouts where the canvas IS the embedding space (good for thumbnails / overviews; potentially disorienting for primary editing).

Bridges A and B are the immediate first-cut targets; Bridge C is more of a special view mode. Semantic search (the simplest demo) needs only the vector index — it doesn't need any of these bridges, just `index.nearest(query_embedding, k)` returning node keys.

**Slice ordering**:

1. Embeddings + vector index alone → semantic search (command-palette → top-k nodes). No field algebra, no scene_physics touch.
2. Bridge A → focus-mode heatmap + attraction-toward-similar. Field-algebra wiring.
3. Bridge B → semantic-physics layout. New coupling pass in scene_physics.
4. Bridge C → "embedding view" projection mode. Likely a new `TwoDPreset` or its own ViewDimension variant.

Slice 1 is the deliverable for this first commit; the rest is sequenced behind real UX decisions.

### 5.4 Vector index

For the first horizon, a flat index (cosine similarity over `Vec<Vec<f32>>`) is fine for a few thousand nodes. Past that, an HNSW index becomes necessary. Candidate crates:

- `hnsw_rs` — pure Rust, no GPU needed
- `instant-distance` — pure Rust, simpler API
- `usearch` — C bindings, fast, larger surface

Pure-Rust matters for the wasm32 / PWA target. Recommend `hnsw_rs` or `instant-distance`.

### 5.5 Weight distribution

Per the Mere `2026-05-07_event_dag_substrate_brief.md`, blobs distribute via iroh. Model weights are exactly the right shape for iroh blobs. The `ModelManifest` from the graphshell research carries forward but the source list updates:

```json
{
  "model_id": "minilm-l6-v2",
  "architecture": "bert",
  "hash": "blake3:...",
  "sources": [
    { "type": "iroh", "ticket": "..." },
    { "type": "https", "uri": "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/..." }
  ]
}
```

Bundling the 22 MB MiniLM in the installer remains the recommended default; iroh-distributed alternatives (LoRA adapters, larger models) are opt-in.

### 5.6 Engrams come in schema-typed classes (not one bag)

Important framing that ties §5 and §8 together: **engrams are schema-typed**, not a single undifferentiated payload kind. The graphshell distillery contract enumerates the artifact-class vocabulary (`StructuredFact`, `DerivedSummary`, `RetrievalMemory`, `BehaviorProfile`, `AdapterWeights`, `AdapterDatasetSlice`, `EvalReceipt`, `ArrangementScaffold`, `SceneSuggestion`, `SpatialHintSignal`, etc.); an engram is the transfer envelope wrapping one or more of those typed payloads.

For Mere this matters concretely:

- A **model engram** (the "geist" of a model — fine-tuned weights, LoRA adapter, behavior profile, evaluation receipt) is a totally different schema than a **memory engram** (curated subset of someone's eidetic memory, structured facts, retrieval memories, derived summaries) or a **scene engram** (arrangement scaffolds, scene suggestions, spatial hint signals).
- The schema is part of the engram's manifest, not just a free-text label.
- **Moots subscribe to schema-classes they care about.** A "Rust learners" moot accepts model-engrams typed as `AdapterWeights[base=qwen, domain=rust-lang]` and memory-engrams typed as `RetrievalMemory[topic=rust]`; it ignores `BehaviorProfile` engrams or unrelated-domain adapters.
- Federation/distribution machinery in moothold should filter on schema before delivery, not after — saves bandwidth, saves trust review, and gives moots a clean discovery story ("this moot is the place for `AdapterWeights[domain=rust]`").

This pushes back on a temptation to treat "engram" as a universal blob type. The transfer envelope is uniform; the schema-typed payload inside is what makes one engram useful to a moot and another irrelevant.

---

## 6. The distillation boundary, revisited for Mere

The graphshell `ASPECT_DISTILLERY.md` and `distillation_request_and_artifact_contract_spec.md` define a typed-artifact pipeline that the older research correctly identified as a prerequisite for trustworthy intelligence. **For Mere, this stays load-bearing but is deferred for the first tier.** Statistical (embedding) intelligence does not produce typed artifacts that need promotion or transfer — it produces vectors that drive the field algebra in real time.

When Mere reaches Tier 3 (generative) or Tier 4 (agentic), the Distillery contract becomes essential:

- Every distillation begins with an explicit `DistillationRequest` (feature_id, source_classes, transform_family, output_kind, provider_trust_class).
- Transforms are a fixed vocabulary: `Summarize`, `ExtractFacts`, `BuildRetrievalMemory`, `BuildArrangementScaffold`, `BuildSceneSuggestion`, etc.
- Outputs are typed artifact classes: `DerivedSummary`, `StructuredFact`, `RetrievalMemory`, `ArrangementScaffold`, `SceneSuggestion`, `SpatialHintSignal`, etc.
- Every artifact carries provenance, privacy class, exportability class.
- Promotion to STM/LTM/transfer is a separate decision from distillation.

The Mere-shaped owner of this contract is plausibly **a new `distillery` crate sibling to `eidetic`**, or an `intelligence-distillery` sub-crate of an `intelligence/` family. The aspect_distillery doc carries forward verbatim; the implementation is a separate plan.

**For the first horizon, Mere does not need this.** Embedding inference is structural enough not to require typed-artifact gating.

### 6.1 Semantic scene scaffolding — connection to fields

The graphshell `semantic_scene_scaffolding_note.md` defines `ArrangementScaffold` and `SceneSuggestion` as typed artifacts. With the new field algebra in place, scene scaffolding becomes specifically:

- **Arrangement scaffold** = a generated `FieldProjection` (named fields, couplings, edge-path rules).
- **Scene suggestion** = a `ViewDimension` choice (TwoD preset, TwoPointFiveProjection variant, z_field selection).

This aligns the older artifact framing with current Mere primitives. It also means scene scaffolding will, when it lands, naturally ride on top of the field-algebra Rhai surface — a `BuildArrangementScaffold` transform produces a Rhai script (or a structured `FieldProjection` value) that the host previews and accepts.

---

## 7. Browser / PWA path

Per the Mere memory `project_browser_pwa_shapes_scripting`, browser/PWA delivery is a real target. This shapes the intelligence stack:

- **Burn-wgpu via WebGPU works in browsers** (Chromium has WebGPU; Safari recently added it). Embedding inference in-browser is feasible for small models.
- **MiniLM-L6 in-browser** is realistic — ~22 MB weight, ~384 dims, sub-100ms per batch on consumer GPUs.
- **Tokenizers**: the `tokenizers` crate compiles to wasm32 with some friction (regex backends matter); `tokenizers-wasm` already exists.
- **HNSW indexes**: pure-Rust crates compile to wasm fine; in-memory indexes are limited by browser memory budgets but a few thousand nodes is comfortable.
- **LLM-class models in-browser**: infeasible for the product-grade path today. Even if tiny demos work, ~600 MB weights, slow wgpu inference, and browser memory limits make this a poor default UX. Defer to native paths.
- **Wasmtime / mistral.rs / llama.cpp / ONNX Runtime**: none ship in browser. If any of these become a hot path, native-only is the constraint.

**Implication**: Tier 2 (embeddings) is browser-deliverable. Tier 3+ (LLMs, agentic) is native-only for the foreseeable future. Plan accordingly.

---

## 8. Federation: model distribution via moothold

The graphshell research described "Verse Intelligence" — model distribution and community fine-tuning over the P2P substrate. With Mere's reshape:

- **Verse → Mere/moothold + iroh blobs**: model manifests live as engrams (specifically `AdapterWeights` / `ModelManifest`-class engrams; see §5.6), weights live as iroh blobs.
- **LoRA adapters as engrams**: the engram framing fits adapters perfectly — small, signed, peer-distributable, optionally signed by validators. The schema makes adapter-engrams distinguishable from memory-engrams or behavior-profile-engrams at the moothold level, so subscription routing and trust review can branch on type before payload inspection.
- **Community fine-tuning**: a moot can curate a dataset (their flora) and produce an adapter targeting MiniLM/Qwen/etc.; the adapter is a schema-typed engram exchanged via murm or moothold.
- **Capability contracts**: the `CapabilityContract` from `self_hosted_model_spec.md` carries forward — features declare requirements, the local runtime checks satisfiability, providers (local or peer-served via murm) bid. For Tier 3+, the capability contract is the boundary that keeps Burn, `mistral.rs`, `llama.cpp`, PEFT/Axolotl, and LoRAX swappable.

**For the first horizon, federation is out of scope.** Bundle one MiniLM, ship a flat-index in eidetic, prove the field-algebra integration, then layer adapters and federation later.

---

## 9. Recommended sequencing

1. **Tier 0 / 1 prep — migrate the explanation surfaces** from graphshell donor docs (graphlet model, semantic tagging, UDC, enrichment lane). These are blockers for any intelligence to feel trustworthy. They are not Burn-dependent.
2. **Tier 2 first slice — embeddings via Burn**:
   - New `intelligence-embeddings` crate (or `eidetic::embedding` if Mark prefers narrow scope).
   - Bundle MiniLM-L6 (Apache 2.0, ~22 MB).
   - Add `burn-import` feature for safetensors loading.
   - Add `tokenizers` dep.
   - Define `EmbeddingProvider` trait + `BertEmbeddings<B>`.
   - Wire embeddings as a `VectorField::Sample` source feeding the field-algebra registry.
3. **Tier 2 second slice — vector index**:
   - Pure-Rust HNSW (`hnsw_rs` or `instant-distance`) inside eidetic or intelligence-embeddings.
   - `nearest(k, vector) -> Vec<NodeKey>` API.
   - Persistence integrated with eidetic's blob store.
4. **Tier 2 third slice — UI surfaces**:
   - Semantic neighbor search (command palette → vector index → highlighted nodes).
   - Semantic clustering layout (force-directed-by-embedding via field coupling).
   - Heatmap background (TwoDPreset::Heatmap with embedding-similarity field).
5. **Tier 3 entry point — bounded generation behind a provider seam**:
   - This is where Distillery becomes load-bearing. Plan + scope before any LLM ships.
   - Define `GenerateProvider` / `LocalModelRuntime` capability surface before binding to Burn, `mistral.rs`, `llama.cpp`, candle, or an API provider.
   - Likely first feature: graphlet digest with explicit `BuildArrangementScaffold` artifact.
6. **Tier 4 — agentic**: Defer. Real prerequisite list: Distillation Boundary in runtime, AWAL durability, typed artifact promotion, supervised-agent execution with capability contracts. Each is its own plan.
7. **Tier 5 — federation / collective**: Defer. Maps onto moothold + iroh blob distribution; LoRA adapters as engrams.

---

## 10. Open questions

1. **Crate boundary**: does embedding work go in `eidetic::embedding` (narrow), a sibling `intelligence-embeddings` (clean separation), or an `intelligence/` family of sub-crates? Lean **sibling family** to keep eidetic's surface clean.
2. **Tokenizer dep**: `tokenizers` (HF Rust crate) is the standard but pulls in regex/onig backends. wasm32 compatibility needs verification before committing.
3. **First model bundled vs downloaded**: bundling 22 MB MiniLM is cheap; for installer-size discipline a download-on-first-launch flow is also reasonable. Decide based on first-light renderer's overall installer story.
4. **Weight format on-disk**: safetensors is the cross-framework standard. eidetic's blob store should treat them as opaque blobs with manifest metadata.
5. **Vector index persistence**: HNSW indexes are large per-node; rebuilding from raw embeddings on cold start is acceptable for small graphs (<10k nodes), painful for larger. Decide when scale becomes real.
6. **Capability-contract layer**: the `self_hosted_model_spec.md` contract framing is sound but the implementation cost is real. Defer until at least three capability classes exist (embedding + summarization + classification) — until then a flat provider trait is enough.
7. **Browser model fetching**: in-browser, blobs come from CDN or peer (iroh-over-wasm is a research probe). Bundling weights into the wasm package bloats the package; on-demand fetch with OPFS cache is the natural path if eidetic-opfs lands, with IndexedDB only as a fallback.
8. **Hash agility vs current code.** The substrate brief's §13 says digest fields should be multihash-aware. Current `eidetic::schema::Hash` is raw BLAKE3 `[u8; 32]`. Either migrate it early to a multihash-aware type or explicitly document that hash agility is a design target not yet reflected in the implementation.

---

## 11. What to read next

- For the *technical stack*: `2026-02-24_local_intelligence_research.md` §1, §2, §9 (model selection still valid; Tier 1 stack of MiniLM + small LLM + Florence + Whisper is a sensible long-term ecology).
- For the *taxonomy and sequencing*: `2026-04-02_intelligence_taxonomy.md` §3-7 and `2026-04-02_intelligence_capability_tiers_and_blockers.md` §3-8.
- For the *typed-artifact contract* (relevant when Tier 3+ work begins): `aspect_distillery/distillation_request_and_artifact_contract_spec.md`.
- For the *runtime contract framing*: `self_hosted_model_spec.md` §4-9.
- For *Mere's current substrate*: `2026-05-07_graph_canvas_field_algebra_plan.md` §3 and §5 (the field algebra and the lossless ladder are where embeddings plug in).

---

## 12. Bottom line

Burn is wired. Eidetic exists. The field algebra has a `Sample` indirection waiting for vector inputs. **The first integration is structurally simple**: load MiniLM via `burn-import`, embed each node's text, register the embeddings as a vector field, attach a coupling, and force-directed-by-meaning works.

Everything else — generative chat, agentic background workers, federation of fine-tuned adapters — is real and worth pursuing, but is genuinely deferred behind prerequisites the older research correctly identified (Distillation Boundary, AWAL, typed-artifact promotion, capability-contract runtime).

The strongest near-term move is **embeddings + vector index + field-coupling** as one slice. That single integration unlocks semantic clustering, semantic search, terrain backgrounds, and force-directed-by-meaning all from one wiring — and it ships in browsers.

---

## 13. Implementation status (2026-05-09 update)

The intelligence-embeddings crate has shipped through Tier-2 (statistical retrieval) end-to-end. Status by section:

- **§5 first-cut integration** — DONE. Crate at `crates/intel/embed/` (formerly `crates/intelligence-embeddings/`). Trait + flat vector index + hashed test provider + Bridge-A field-algebra integration + eidetic persistence + `SemanticSearch` facade. ~108 active tests.
- **§5 BERT-via-Burn** — DONE mechanically. Full layer stack (`BertEmbeddings`/`BertSelfAttention`/`BertSelfOutput`/`BertAttention`/`BertIntermediate`/`BertOutput`/`BertLayer`/`BertEncoder`/`BertModel`) implemented in Burn 0.21 with `from_loaded` constructors. `BertEmbeddingProvider::<B>::load(model_dir, device)` is the one-shot entry point. PyTorch `[out, in]` → Burn `[in, out]` Linear-weight transpose handled at the safetensors-extraction boundary. HF `LayerNorm.weight/bias` → Burn `gamma/beta` mapped at the construct boundary.
- **§5 BERT validation** — TIERED, AWAITING EMPIRICAL RUN. Tier-1 (cheap fixture comparison) runnable as soon as fixtures populate; helper at `scripts/capture_minilm_fixtures.py`. Tier-2 (continuous candle-or-ort comparison) gated behind `bert-validation` feature, structurally ready. First empirical run reveals whether numerical adjustments are needed at three known sites: Linear transpose direction, GELU variant, attention scale factor.
- **§6 Distillery** — UNCHANGED. Genuinely deferred until Tier-3+ work begins.
- **§9 sequencing** — Steps 1–4 (Tier-2 first/second/third slices) all landed. Step 5 (Tier-3 entry / generative bounded summaries) blocked behind Distillery and the provider-runtime contract as planned.

The remaining BERT work is environmental — a developer with `MERE_MINILM_DIR` and either `sentence-transformers` (Python) or `candle-transformers` (Rust) populates `FIXTURES`, runs the tier-1 test, and either confirms or fixes the three known empirical sites. The mechanical pipeline is done.

### 2026-05-11 audit adjustment

- Burn remains the Tier-2 embedding / tensor path, but the document no longer treats it as the Tier-3+ LLM or LoRA runtime by default.
- `eidetic` has moved beyond the early blob-only description: manifest, typed-payload, schema-engram, engram, bundle, and model-storage layers are present in code. Future updates should treat eidetic as the persistence substrate for typed intelligence artifacts, while keeping model/provider logic in sibling intelligence crates.
- Hash-agility is now called out as a current code/design mismatch: the design wants multihash discipline, while the implementation still uses raw BLAKE3 hashes.

### 2026-05-18 topology adjustment

- Target crate family: `crates/intel/embed/`. The current
  `intelligence-embeddings` crate becomes the `embed` crate in the `intel`
  family, not an `eidetic` submodule and not a graphshell crate.
- `intel/embed` owns embedding providers, vector indexes, semantic search,
  and typed intelligence-signal production. It may persist indexes and model
  artifacts through eidetic, but eidetic remains the storage substrate rather
  than the model/signal owner.
- The current graph-canvas bridge (`canvas_search` / `field_bridge`) is a
  transitional integration surface. Long-term, graph-canvas-specific field
  adapters belong beside `graphshell/graph/graph-canvas` or as an explicit
  adapter crate; `intel/embed` should retain a graph-agnostic provider/index
  core.
