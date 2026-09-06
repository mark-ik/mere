# Eidetic Design Pass — Research Report (2026-05-09)

**Status**: Research synthesis with recommendations

> **Crate-name note (2026-06-09 audit):** the four-layer stack this pass proposes is largely **built**; the remaining Phases 7-9 live in the [eidetic deferred-phases plan](../implementation_strategy/2026-06-09_eidetic_deferred_phases_plan.md). `intelligence-embeddings`→`intel/embed`; donor `graphshell/...` paths point at the GitHub-archived donor (local clone deleted 2026-05-27). Dated receipts below are historical record.
**Purpose**: Eidetic shipped 2026-05-06 as a 186-line blob-store contract. It needs to grow into Mere's actual private-memory layer — the substrate this research has been describing for months under various names (mnem, STM/LTM, EngramMemory, browsing memory). This pass proposes how, grounded in the inherited graphshell-era research and the immediate concrete pulls (eidetic-backed model storage, vector-index persistence, browsing-memory accumulation).
**Audience**: Architecture / planning. Implementation plans should descend from this report.

**Inheritance and prior context**:

- `repos/graphshell/design_docs/verse_docs/implementation_strategy/2026-02-26_intelligence_memory_architecture_stm_ltm_engrams_plan.md` *(historical citation)* <!-- doc-audit: historical-path --> — STM/LTM/EngramMemory/Ectoplasm vocabulary; **valid, adopt selectively for vocabulary**
- `repos/graphshell/design_docs/verse_docs/implementation_strategy/engram_spec.md` *(historical citation)* <!-- doc-audit: historical-path --> (1100+ lines) — canonical engram spec; **example schema vocabulary going forward** (memory-kind enum reframed as open schema references — see §3 carry-forward / §7.4)
- `repos/graphshell/design_docs/graphshell_docs/implementation_strategy/aspect_distillery/distillation_request_and_artifact_contract_spec.md` *(historical citation)* <!-- doc-audit: historical-path --> — typed artifact classes; **valid, eidetic typed-payload schema should align with this vocabulary**
- `repos/mere/crates/eidetic/src/lib.rs` *(historical citation)* <!-- doc-audit: historical-path --> — current 186-line implementation
- `repos/mere/crates/eidetic/src/lib.rs` *(historical citation)* <!-- doc-audit: historical-path --> — current 186-line implementation
- TERMINOLOGY.md memory: *eidetic* = "private local accumulated browsing memory; the substrate engrams are distilled from"

---

## 1. Executive summary

**Eidetic today is a blob store.** Request::{LoadBlob, SaveBlob}, Response::{BlobLoaded, BlobSaved}, Store trait, dispatch helper. Storage backends (fjall, redb, OPFS, in-memory) implement Store. That's it. It works as a contract layer; it does not yet embody the "private local accumulated browsing memory" the product needs.

**Eidetic should grow into a four-layer stack**, with the existing crate becoming Layer 1:

1. **Blob layer** (current) — byte-addressable storage. Backends. Stays mostly unchanged. The Store trait will need to go async to accommodate browser backends (see §4.5).
2. **Manifest layer** — typed metadata about blobs: content hash, size, schema reference, source list, timestamps, and a three-axis classification (privacy / provenance / trust). Schema references are content-addressed pointers to schema definitions, not enum tags.
3. **Typed-payload layer** — Rust-side implementations of schema-conformant data. The schema vocabulary is open and aligns with the inherited Distillery artifact classes (`StructuredFact`, `RetrievalMemory`, `BehaviorProfile`, `ModelManifest`, `AdapterWeights`, etc.); communities can mint new schemas without coordinating.
4. **Memory-domain layer** — the user-facing concept: traversal logs, browsing memory, settings, indices, distilled summaries. Composed from typed payloads.

**The first three concrete applications** that pull on this layering:

- **Eidetic-backed model storage** (immediate, blocks on Layer 2 manifest): MiniLM weights as a blob, identified by hash, with manifest tracking source URLs / iroh tickets / provenance.
- **Vector-index persistence** (already shipped at Layer 1 via `intelligence-embeddings::persistence`; benefits from Layer 2 manifest for hash-pinning and freshness).
- **Browsing memory accumulation** (the product purpose; needs all four layers): traversal events, page snapshots, derived clip nodes, etc.

**The engram bridge**: eidetic does not own engrams. Engrams are the portable contribution payload for moothold/murm transfer (per `engram_spec.md`, treated here as an example schema vocabulary, not a closed enum). An engram is a **schema reference + schematized data + integrity envelope**: immutable, content-addressable, time-bounded snapshot. Eidetic owns the *private* substrate from which engrams are *distilled* via the Distillery aspect when policy allows. Schemas are themselves engrams — same storage, same addressing, same trust model — and consumers fetch a schema engram to validate a payload against. This recursion is what lets new schemas circulate without a central registry.

---

## 2. What eidetic is today

```rust
pub enum Request {
    LoadBlob { key: String },
    SaveBlob { key: String, value: Vec<u8> },
}
pub enum Response {
    BlobLoaded { key: String, value: Option<Vec<u8>> },
    BlobSaved { key: String },
}
pub trait Store {
    fn load_blob(&mut self, key: &str) -> Result<Option<Vec<u8>>>;
    fn save_blob(&mut self, key: &str, value: &[u8]) -> Result<()>;
}
pub fn dispatch(store: &mut dyn Store, request: &Request) -> Result<Response> { ... }
```

**Properties**: backend-agnostic, key-value, no metadata, no schema, no privacy class, no provenance, no timestamps. Strings are the only key type. Errors carry messages but no kind.

**Current consumer**: `intelligence-embeddings::persistence` serializes a `VectorIndex` as JSON via serde, calls `Store::save_blob` / `load_blob`. Works fine for now; misses everything Layer 2+ would add.

**Crate-level intent** (from lib.rs docs): "private local-memory lane for the Mere browser. Owns the vocabulary for owner-scoped local blobs, caches, and accumulated browsing memory — the lane that 'keeps the impressions over time' in Mere's printing-press metaphor."

The implementation does not yet match the intent. The intent is the right one.

---

## 3. Inherited research — what to keep, what to update

The graphshell-era memory architecture plan (`2026-02-26_intelligence_memory_architecture_stm_ltm_engrams_plan.md`) defines a richer vocabulary that maps cleanly onto eidetic's growth path:

| Inherited concept | Status for Mere/eidetic |
| --- | --- |
| **STM** (short-term memory; high-churn, editable, TTL'd) | **Adopt** — Layer 4 has STM-shaped payloads (session notes, scratchpads, recent retrievals). |
| **LTM** (long-term memory; durable, indexed, versioned) | **Adopt** — Layer 4 has LTM-shaped payloads (curated browsing memory, eval receipts, adapter manifests). |
| **EngramMemory** (persisted, portable, indexed unit) | **Reframe** — engrams are *outputs* that may be distilled from eidetic, not eidetic's primary type. Layer 3 typed-payloads share the schema vocabulary but eidetic owns the local copy. |
| **Ectoplasm** (ephemeral runtime signal stream) | **Defer** — relevant later when agentic intelligence lands; not first-cut. |
| **MemoryExtractor** (outbound conversion) | **Reframe as Distillery aspect** — extraction lives in a Distillery layer above eidetic, gated on privacy policy. Eidetic exposes typed-payload reads; Distillery transforms into engrams. |
| **MemoryIngestor** (inbound import) | **Reframe** — ingestion comes from murm (bilateral) and moothold (community) via the engram contract, then optionally lands in eidetic as typed payloads. |
| **MemoryPromotionPolicy** (STM → LTM rules) | **Adopt** — Layer 4 includes promotion rules per schema. |

**What's new/different in Mere** (vs the graphshell plan):

- Eidetic is its own crate with a Store trait — not coupled to a specific persistence backend
- Schemas (formerly "schema-class") are content-addressed engrams, not a closed enum; the Distillery artifact-class list survives as an example vocabulary
- Engrams flow through murm + moothold (federation tiers), not "Verse"
- Browser/PWA target shapes backend choices, but the answer is **OPFS, not IndexedDB** (see §4.5)

### Carry-forward from `engram_spec.md`

The graphshell-era TransferProfile design (`engram_spec.md`, ~1100 lines) is stale on terminology and on its closed-enum approach to memory kinds, but several patterns are load-bearing and survive intact:

- **CIDv1 with BLAKE3** for content addressing — already settled there, confirms direction here
- **Three-way `MemoryLocation` enum** (`Embedded` / `ContentAddressed` / `LocalOnlyRef`) — semantic safety rail; `LocalOnlyRef` literally cannot leave the host
- **Composite engram-as-bundle pattern** (`TransferProfile`) — a bundle is itself a schema, not a built-in concept
- **"Sparse is valid"** — partial engrams are first-class; consumers use only the memories they need
- **`MergeLineage`** — original engram IDs immutable; merges create new engrams referencing inputs by hash
- **`PolicyDiff` artifacts** — delta engrams between snapshot engrams; closes the loop on snapshots-not-subscriptions
- **Three orthogonal axes**: privacy (who sees) / provenance (origin) / trust (confidence) — keep them split, do not collapse
- **Forward-compatibility rule** — unknown schemas tolerated, ignored gracefully unless `required_for_application`
- **Redaction modes** (`None` / `MetadataOnly` / `SummaryOnly` / `PointerOnly`) — vocabulary for extraction slicing
- **Adopted standards stack**: UUID v4 stable IDs, CIDv1 BLAKE3, DID:key for contributor identity, VC 2.0 for governance receipts

### Stale and reframed

- **Verse / FLora → moot / moothold / murm.** Mechanical rename.
- **`EngramMemoryKind` as a closed 17-variant enum** → open schema references. The spec's kinds become well-known schemas at known content hashes; the closed enum disappears.
- **Specific ref fields in `TransferProfile`** (`ranking_policy_ref`, `embedding_profile_ref`, …) → generic schema-typed reference list keyed by schema.
- **`EngramValidationClass` and bundled `PrivacyClass`** → the three orthogonal axes (privacy / provenance / trust) make validation-class a derivable view rather than a separate enum. Collapse.
- **UDC profile and the `RankingFeature` enum** → domain content for specific schemas, not envelope concerns. UDC becomes one allowed schema (`UdcClassification`); ranking features live inside the `RankingPolicy` schema.

---

## 4. The four-layer proposal

### Layer 1 — Blob store (current crate)

**Status**: shipped. Stays mostly unchanged.

The Store trait is a clean contract. Backends implement it. The only growth here is small: an additional method or two for batch operations and existence-checks if the higher layers want them.

**Possible additions**:

- `delete_blob(key: &str) -> Result<bool>` — return whether anything was removed. Currently no removal API.
- `exists(key: &str) -> Result<bool>` — without paying the cost of a full load. Some backends (fjall, redb) can answer this cheaply.
- `iter_keys(prefix: &str) -> Box<dyn Iterator<Item = Result<String>>>` — for the manifest layer to enumerate stored manifests. Optional (could live as a higher-layer helper that walks the store).

These are small extensions; defer until a concrete need pulls each in.

### Layer 2 — Manifest

**Purpose**: describe a blob without reading it. Content hash, size, schema reference, sources, timestamps, three-axis classification (privacy / provenance / trust).

```rust
// illustrative — signature-only, not implementation-ready
pub struct BlobManifest {
    /// Stable identifier — content hash of the blob (BLAKE3, CIDv1).
    pub id: ManifestId,
    /// Schema reference — content-addressed pointer to a schema definition.
    /// Schemas are themselves engrams; this is just another ManifestId.
    pub schema: SchemaRef,
    /// Hash of the blob content for verification (BLAKE3, CIDv1).
    pub content_hash: Hash,
    pub byte_size: u64,
    pub created_at: Timestamp,
    pub last_accessed: Option<Timestamp>,
    /// Where this content can be obtained. Multiple sources tried in order.
    pub sources: Vec<BlobSource>,
    /// Three-axis classification — kept separate, never bundled.
    pub privacy: PrivacyClass,
    pub provenance: ProvenanceRecord,
    pub trust: TrustEnvelope,
    /// Free-form metadata interpreted per-schema.
    pub schema_metadata: serde_json::Value,
    /// For forward-compatibility migrations.
    pub manifest_version: u32,
}

/// Where a blob can be obtained. Carries semantic meaning beyond mere routing:
/// `LocalOnlyRef` literally cannot leave the host (safety rail for slicing).
pub enum BlobSource {
    /// Embedded directly in the manifest (small metadata blobs only).
    Embedded { media_type: String, byte_len: u64 },
    /// Cached locally in the Store.
    Local { key: String },
    /// Iroh blob ticket — peer-distributed, BLAKE3-verified by iroh natively.
    Iroh { ticket: String },
    /// HTTPS URL (e.g. HuggingFace Hub).
    Https { url: String },
    /// User-provided file path on the host.
    LocalFile { path: String },
    /// Local-only reference; never serialized for transfer.
    LocalOnlyRef { local_ref: String },
}

/// Privacy — who is allowed to see this artifact.
pub enum PrivacyClass {
    LocalOnly,
    TrustedPeersOnly,
    MootScoped,
    PublicPortable,
}

/// Provenance — where this came from and what produced it.
pub struct ProvenanceRecord {
    pub origin: ProvenanceOrigin,
    pub upstream: Vec<ManifestId>,    // ancestor engrams (for derivations / merges)
    pub tooling: Option<String>,
    pub generated_at: Timestamp,
}

/// Trust — how confident the local node is in the contents.
pub struct TrustEnvelope {
    pub level: TrustLevel,            // SelfAsserted / PeerAttested / CommunityReviewed / CheckpointAccepted
    pub signatures: Vec<SignatureRef>,
    pub moderation_state: ModerationState,
}
```

**The three axes are orthogonal and independently checked.** Privacy gates *"may I show this to that recipient?"* Provenance answers *"where did this come from, and is the lineage acceptable for this use?"* Trust answers *"how confident am I that the contents are what they claim?"* Bundling them into a single enum loses information — a payload may be `LocalOnly` (privacy) but `CheckpointAccepted` (trust), or `PublicPortable` (privacy) but `SelfAsserted` (trust). Each axis evolves on its own clock and is revoked independently.

**Storage shape**: manifests are themselves serialized to the blob layer under a known prefix (`manifest:<id>`). The blob layer doesn't know they're manifests; the manifest layer is just a typed view of certain blobs.

**Operations**:

- `save_manifest(store, manifest) -> Result<()>`
- `load_manifest(store, id) -> Result<Option<BlobManifest>>`
- `list_manifests(store, schema_filter) -> Result<Vec<BlobManifest>>`
- `resolve_blob(store, manifest) -> Result<Vec<u8>>` — try sources in order, fetch + verify hash, cache locally on first success

The `resolve_blob` step is the load-bearing user-facing one — it's how "model weights live somewhere" becomes "give me the bytes." Sources are tried in order; first success caches locally so subsequent calls are fast.

### Layer 3 — Typed payloads

**Purpose**: encode the schema-specific structure of each payload type as Rust types. Layer 3 is the **Rust-side implementation** of schema-conformant data — Rust structs that deserialize bytes from a blob into typed values, given a known schema. The schema itself lives one level up as a content-addressed artifact (see §7.2). Layer 3 is not the schema; it is the language-specific binding to a schema. Other implementations in other languages would bind the same schemas to their own native types.

The schema vocabulary is open. The following echoes the Distillery artifact classes from graphshell as an initial set of well-known schemas, but it is not a closed enum — communities can mint new schemas without coordinating:

- `StructuredFact` — extracted assertion with provenance
- `DerivedSummary` — text summary over approved sources
- `RetrievalMemory` — indexed memory unit for later retrieval
- `BehaviorProfile` — agent / workflow behavior description
- `EvalReceipt` — evidence of evaluation
- `ModelManifest` — pointer to a model with hash + sources (immediate use case)
- `AdapterWeights` — LoRA / fine-tune adapter (later)
- `VectorIndex` — vector index over embeddings (already shipped via `intelligence-embeddings`)
- `BrowsingTrace` — recent traversal events (the "actual browsing memory")
- `ClipNode` — captured clip / snippet / page extract
- `SettingBundle` — typed settings snapshot

Each well-known schema has its own Rust struct + Serialize/Deserialize. The manifest's `schema` field is a content-addressed reference to a schema engram; the consumer picks the matching Rust binding (or fetches and parses the schema dynamically if no static binding is registered).

```rust
// illustrative — signature-only, not implementation-ready
pub trait TypedPayload: Serialize + DeserializeOwned + Send + Sync {
    fn schema_ref() -> SchemaRef;
    fn serialize_to_bytes(&self) -> Result<Vec<u8>>;
    fn deserialize_from_bytes(bytes: &[u8]) -> Result<Self> where Self: Sized;
}

// Example: model manifest as a typed payload
pub struct ModelManifest {
    pub model_id: String,        // "minilm-l6-v2"
    pub architecture: String,    // "bert"
    pub config: BertConfig,
    pub weight_blob: ManifestId, // ID of a separate ModelWeights blob
    pub tokenizer_blob: ManifestId,
}
```

**Operations**:

- `save_typed<T: TypedPayload>(store, value, sources, privacy) -> Result<ManifestId>`
- `load_typed<T: TypedPayload>(store, id) -> Result<T>` — loads manifest + resolves blob + deserializes
- `list_typed<T: TypedPayload>(store) -> Result<Vec<(ManifestId, BlobManifest)>>` — enumerate manifests for one schema

### Layer 4 — Memory-domain

**Purpose**: the user-facing memory concept. Composes typed payloads into the things Mere actually does — accumulated browsing memory, distilled summaries over time, settings.

Examples:

- **`BrowsingMemory`** — a chronological log of visited nodes / paths, indexed by recency, traversal corridor. Composed from many `BrowsingTrace` typed payloads.
- **`ClipLibrary`** — user-clipped page extracts. Composed from `ClipNode` payloads.
- **`SettingStore`** — typed settings, with version history. Composed from `SettingBundle` payloads.
- **`ModelLibrary`** — locally available models. Composed from `ModelManifest` payloads.
- **`VectorIndexLibrary`** — persisted indices per graph view. Composed from `VectorIndex` payloads.

Layer 4 provides domain-shaped APIs (`add_traversal`, `recent_corridor`, `clip_at_url`, etc.) that don't expose the manifest/blob primitives directly. The plumbing is below.

### Backend choice and async Store trait

The Layer 1 Store trait abstracts over backend storage. Backend choice has two axes:

**Native:** fjall (LSM, default), redb (B-tree, simpler-semantics use cases), in-memory (testing). All use blocking file I/O.

**Browser:** the choice is non-obvious; the doc's earlier draft assumed IndexedDB, but the right answer is OPFS. Two real candidates:

- **IndexedDB** — async key-value with secondary indexes, cross-store transactions, mature Rust crates (`idb`, `indexed_db_futures`, `rexie`). The reflexive default for "persistent browser storage."
- **OPFS (Origin Private File System)** — sync file I/O via `FileSystemSyncAccessHandle` in dedicated workers; reportedly 5–10× faster than IDB for blob-heavy workloads (SQLite-on-OPFS benchmarks); shipped in all major browsers (Chrome 102+, Firefox 111+, Safari 15.2+).

**Recommendation: OPFS for browser, not IDB.** Three reasons:

1. **Tantivy compatibility.** Tantivy's `Directory` trait expects file-shaped semantics (`Seek + Read`, atomic writes, lock files). OPFS provides these directly via `FileSystemSyncAccessHandle`; a `tantivy::Directory` impl over OPFS is a few hundred LOC of glue. IDB requires chunked-storage workarounds — same wall the SQLite community hit (their OPFS VFS replaced their IDB VFS for exactly this reason). Since `SearchIndex` is a first-class engram class (§7.5), tantivy compatibility is load-bearing.

2. **Composable index sharing.** Tantivy indices are *files*; shipping an index between peers means shipping a directory of files. With OPFS this is a directory copy. With IDB it is a chunked-value reassembly.

3. **Large-blob workload.** Model weights (50–500MB safetensors) are *one file* in OPFS. In IDB they are a single value loaded whole or chunked manually. Streaming/seek primitives matter once payload sizes grow past a few MB.

Both backends share the same per-origin quota pool and `navigator.storage.persist()` machinery, so quota and persistence behavior are not differentiators.

**The collateral decision: the Store trait must be async.** OPFS is fundamentally event-driven and there is no `block_on` available in browser wasm. Native fjall/redb implementations resolve immediately (return ready futures); browser implementations actually await. Async-ifying the trait *after* Layer 2/3 ops compose across multiple consumers is a painful migration; commit to it now.

**Crate layout:**

- `eidetic` — Layer 1 trait (async), fjall + redb + in-memory native backends, Layer 2/3 logic
- `eidetic-opfs` — OPFS-backed Store implementation (browser-only; isolates wasm-bindgen as a transitive dep)

A potential path worth tracking: **fjall-on-OPFS via a wasi-fs shim**. If feasible, this collapses the backend story to one engine instead of two. Treat as a feasibility check, not a commitment — the hand-rolled OPFS backend is the safer default until the shim path is validated.

---

## 5. The first concrete application — model storage

### Pull

`BertEmbeddingProvider::load(model_dir)` currently expects a directory on disk with three files. The research report §5.5 calls for moving toward iroh-blob distribution + hash-pinned manifests so model files are first-class Mere data, not external file paths.

### Shape with the new layering

```rust
// illustrative
pub struct ModelManifest {
    pub model_id: String,
    pub architecture: String,
    pub config: serde_json::Value,  // schema-flexible across architectures
    pub weight_blob: ManifestId,
    pub tokenizer_blob: ManifestId,
    pub license: String,            // "Apache-2.0", "MIT", etc. — open-source first
}

// Save a model into eidetic
let weight_id = save_blob_with_manifest(
    &mut store,
    &fs::read("model.safetensors")?,
    schemas::MODEL_WEIGHTS,            // SchemaRef — content-addressed pointer
    sources_with_iroh_and_https,
    PrivacyClass::LocalOnly,
    ProvenanceRecord::self_imported("hf:sentence-transformers/all-MiniLM-L6-v2"),
    TrustEnvelope::self_asserted(),
).await?;
let tokenizer_id = save_blob_with_manifest(
    &mut store,
    &fs::read("tokenizer.json")?,
    schemas::TOKENIZER,
    sources,
    PrivacyClass::LocalOnly,
    ProvenanceRecord::self_imported("hf:sentence-transformers/all-MiniLM-L6-v2"),
    TrustEnvelope::self_asserted(),
).await?;
let manifest = ModelManifest {
    model_id: "minilm-l6-v2".into(),
    architecture: "bert".into(),
    config: parsed_config,
    weight_blob: weight_id,
    tokenizer_blob: tokenizer_id,
    license: "Apache-2.0".into(),
};
let model_manifest_id = save_typed(&mut store, &manifest, ...).await?;

// Load a model from eidetic
let manifest: ModelManifest = load_typed(&mut store, model_manifest_id).await?;
let weight_bytes = resolve_blob(&store, manifest.weight_blob).await?;
let tokenizer_bytes = resolve_blob(&store, manifest.tokenizer_blob).await?;
let provider = BertEmbeddingProvider::<B>::from_bytes(
    manifest.config,
    weight_bytes,
    tokenizer_bytes,
    device,
)?;
```

**This requires** a small extension to `BertEmbeddingProvider`: a `from_bytes` constructor that takes config + weights + tokenizer as buffers rather than reading from disk. That's a one-screen refactor of `BertEmbeddingProvider::load`.

### Distribution

Sources list lets a single manifest carry multiple delivery paths:

```rust
sources: vec![
    BlobSource::Local { key: "blob:minilm-weights-...".into() },
    BlobSource::Iroh { ticket: "blob1abc...".into() },
    BlobSource::Https { url: "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/model.safetensors".into() },
],
```

Resolution tries Local first (cached), then Iroh (peer-distributed via `mere-transport`), then HTTPS (HF Hub). All hash-verified against the manifest's `content_hash`. Open-source-options-first because hash + license stay local even if Hugging Face goes away.

---

## 6. Other near-term applications

### 6.1 Vector-index persistence (already shipped at Layer 1; tightens at Layer 2)

`intelligence-embeddings::persistence::save_to_eidetic` currently calls `Store::save_blob` directly with a hand-picked key. Migration to Layer 2 means: same call, but it goes through `save_typed::<VectorIndex<K>>(...)` and gets a manifest with content hash + schema reference + three-axis classification. Backwards-compatible at the data layer (still JSON-in-blob); adds metadata layer above.

### 6.2 Browsing memory (eidetic's actual product purpose)

`BrowsingTrace` typed payloads accumulated chronologically. The Layer-4 `BrowsingMemory` provides:

- `record_traversal(from_node, to_node, edge_kind, timestamp)` — write a trace
- `recent_corridor(focus_node, n) -> Vec<TraversalEvent>` — read recent path
- `nodes_visited_in_window(start, end) -> Vec<NodeKey>` — temporal slice
- `co_occurrence(node_pair) -> u32` — how often were these visited together
- ... etc

This is what eidetic was *named* for. The blob/manifest/typed-payload layers underneath are just the substrate.

### 6.3 Settings as typed payloads

Currently scattered through graphshell-app-state. Migration: each setting bundle becomes a typed payload with version history. Reverting to a previous setting becomes loading an older manifest revision. Doesn't have to happen day one but the layer supports it cleanly.

---

## 7. The engram bridge

**Eidetic owns the private substrate; engrams are immutable, schema-typed snapshots derived from it.** The two are not the same thing, and the relationship needs to be precise.

### 7.1 What an engram is

```rust
// illustrative — signature-only, not implementation-ready
pub struct Engram {
    pub schema: SchemaRef,             // content-addressed pointer to a schema definition
    pub payload: Bytes,                // schematized data conforming to the schema
    pub content_hash: Hash,            // BLAKE3 of payload, verifies integrity
    pub privacy: PrivacyClass,
    pub provenance: ProvenanceRecord,
    pub trust: TrustEnvelope,
    pub bounds: TimeBounds,            // declared temporal range of source data
}
```

Properties:

- **Immutable.** Once produced, an engram's bytes never change. Editing means producing a new engram. The hash verifies that the bytes still match what was declared.
- **Snapshot, not subscription.** An engram captures source data within declared time bounds. "Refreshing" produces a new engram, not an update to an old one. This makes engrams cleanly content-hashable (no mutable state to track) and audit-friendly.
- **Schema-typed.** The schema reference declares "this is an X, and X is defined here." A consumer with the schema definition can parse, validate, and reason about the payload.
- **Composable.** An engram's payload may itself be a bundle of typed memories (each a sub-engram by reference). The composite-bundle pattern from `engram_spec.md`'s `TransferProfile` survives as one well-known schema among many — not a built-in concept.

### 7.2 Schemas are themselves engrams

Schemas don't live in a special registry. A schema is an engram whose payload is a schema definition (JSON Schema, JSON-LD context, custom DSL). Its own schema reference points to a **meta-schema** that describes "this is a schema definition."

This recursion buys three things:

- **No central authority.** New schemas circulate the same way new content does — peer-to-peer, content-addressed, signed if desired.
- **Content-addressed identity.** Schemas are referenced by hash; identical schemas have identical references regardless of who minted them.
- **One storage layer.** The eidetic Store does not need to know about schemas as a special case. They are just engrams. A consumer encountering an unknown schema reference fetches the schema engram, parses it, then uses it to parse the original payload.

For interop with the existing web, schema engrams may be wrappers around schema.org / JSON-LD context documents — Mere does not have to mint a parallel vocabulary for things the web already names well. A clip of an article is genuinely a schema.org `Article`; a clip of a recipe is `Recipe`.

### 7.3 Distillery as the boundary

```
                            DISTILLERY ASPECT
                            (privacy-gated transform; on demand)
                                    │
                                    ▼
   eidetic substrate ───→ schema-typed engrams ───→ moothold/murm transfer
   (LocalOnly default)    (immutable snapshots)     (per moot acceptance)
```

A Distillery invocation:

1. Reads from the local eidetic substrate within declared bounds (slice — see §7.5).
2. Extracts/transforms the data per a target schema.
3. Produces an engram with that schema, the extracted payload, declared bounds, and three-axis classification.
4. Returns the engram. The substrate is unchanged.

The engram is the artifact that flows over moothold/murm. The eidetic copy of the source data stays put. The engram pairs naturally with its time bounds — there is no "current" version to chase.

### 7.4 What this design pass commits to

- **Schema is a content-addressed reference, not an enum tag.** (Already in §4.2.)
- **Engrams are immutable; merges produce new engrams.** Edits do not exist. `MergeLineage` records ancestry.
- **Three-axis classification (privacy / provenance / trust)** on every engram and every manifest.
- **Forward-compatibility default**: unknown schemas are tolerated; consumers ignore what they do not understand unless a memory is marked `required_for_application`.
- **The `engram_spec.md` memory-kind enum is treated as an example schema vocabulary**, not a fixed contract. Most of those kinds survive as named schemas, but lose their privileged enum status.

Distillery itself is still deferred (per the local intelligence research §6). What this pass does is make schemas, payloads, immutability, integrity verification, and the three-axis split right *now*, so that when Distillery lands, the bridge needs no architectural change.

---

## 7.5 Composable engram contributions and moot acceptance

Engrams are not just private snapshots — they are **shareable, composable contributions**. Users contribute their schema-typed engrams to moots; moots compose contributions into shared corpora. This adds requirements that reach back into eidetic's design.

### Per-site profiles — two distinct slicing modes

A user's eidetic accumulates many kinds of source data. When producing an engram, the user (or policy) chooses a *slice* of that data to extract. Two slicing modes exist, with different privacy boundaries:

- **Source slicing.** The engram covers only data matching a filter (URLs under arxiv.org, traversal events tagged `research`, etc.). Privacy boundary is clear — the engram literally does not contain non-matching data.
- **Extraction slicing.** Source is full; the schema only surfaces certain fields or projections. Privacy boundary is subtler — metadata leakage is possible (e.g. an `arxiv_papers` engram extracted from a full browsing log can leak co-occurrence info even if URLs are stripped).

Both modes are first-class and **fully configurable per profile**. Source and extraction slicing compose: a profile can declare "extract these fields, and only from URLs matching this filter." The `RedactionProfile` vocabulary from `engram_spec.md` (`None` / `MetadataOnly` / `SummaryOnly` / `PointerOnly`) is the natural extraction-slicing vocabulary.

### Engrams are generated on demand, not maintained

Engrams are *instances in time*, not subscriptions. The user does not maintain N parallel indices over the same source; the Distillery extracts on demand against declared bounds. This trades storage cost (low) for extraction latency (acceptable for federation use cases) and keeps the substrate the single source of truth. Re-extraction with adjusted bounds or schema produces a new engram; old engrams remain valid as audit records.

### Moot-side acceptance — schema sets per moot

Each moot publishes an *accepted schema set*: the engram schemas it will ingest, possibly with version ranges. A contributor's eidetic queries this set and offers matching engrams.

- **Open moots** accept many schemas — catch-all federations.
- **Strict moots** accept one or a few schemas — purpose-built for a particular kind of contribution (e.g. a moot that only accepts `SearchIndex[arxiv-papers-v3]`).

The accepted schema set is itself an engram (recursively), addressable by content hash and versionable. Contributors do not negotiate schemas with moots ad hoc; they read the moot's published schema set.

### Composability default — segment-separable

When a moot ingests contributions from many users, merged outputs are **segment-separable** by default: the moot retains per-contributor segments and can:

- revoke a contribution (drop a segment) without recomputing the whole corpus
- audit per-contributor influence on outputs
- recompute merged views as policy evolves

Tantivy supports this natively — segments stay separable until explicitly merged. Vector indices similarly support per-contributor partitions. Irreversibly-fused merges are an explicit per-moot policy choice, not the default.

### `SearchIndex` as a first-class schema

The `engram_spec.md` lists `EmbeddingProfile` (vector-shaped) but no lexical equivalent. Federated retrieval over browsing memory benefits from both:

- **Vector search** — semantic / concept-similar
- **Lexical search** — exact-phrase, keyword, ranked retrieval (tantivy)

Both should be first-class engram schemas. A `SearchIndex[corpus-shape]` engram declares the schema (corpus shape, tokenizer, ranking config, field set) and ships a tantivy index that conforms to it. Recipients verify schema match before merging; tantivy's `IndexMerger` composes them.

This puts a sharp requirement on the storage layer: tantivy indices are *file-shaped* (segment files, random-access reads, atomic writes). Sharing a tantivy index means shipping its files. This is what makes the Layer 1 backend choice (§4.5) load-bearing for the federation story, not just for native ergonomics.

---

## 8. Privacy, quota, GC

### Privacy, provenance, trust — three orthogonal axes

Every typed payload, every blob manifest, and every engram carries three independent classifications, kept separate to preserve information that bundling would lose:

- **Privacy** (`LocalOnly` / `TrustedPeersOnly` / `MootScoped` / `PublicPortable`) — who is allowed to see this. Default is `LocalOnly`; promotion to a wider audience is always an explicit operation, never automatic.
- **Provenance** (`ProvenanceRecord`) — origin, ancestor engrams, tooling version, generation time. Provenance enables payout/audit and merge-lineage tracking; it is what `MergeLineage` and the engram-spec's provenance machinery preserve across composition.
- **Trust** (`TrustEnvelope`: `SelfAsserted` / `PeerAttested` / `CommunityReviewed` / `CheckpointAccepted`, plus signatures and moderation state) — how confident the local node is in the artifact's contents. Independent of who can see it; an engram can be `LocalOnly` + `CheckpointAccepted` (validated upstream contribution kept private locally) or `PublicPortable` + `SelfAsserted` (something the user shares without external review).

Bundling these into one enum loses information. They evolve on their own clocks, are checked at different layers, and may be revoked independently.

### Quota

Eidetic doesn't enforce quota at Layer 1 (the backend may have its own limits). Layer 4 owns quota policy per memory domain:

- BrowsingMemory: keep N most recent, age out older traces (configurable)
- ModelLibrary: cap at K models, LRU evict (or never evict — model files are large but few)
- ClipLibrary: user-curated, no automatic eviction

### GC

Manifest deletion does not automatically delete blobs (a blob may be referenced by multiple manifests — e.g. a tokenizer.json shared across model presets). A GC pass walks reachable manifests, marks reachable blobs, sweeps unreachable. Run on demand (e.g. user clicks "free space"), not automatically.

---

## 9. Migration path from current eidetic

The Layer 1 trait shape changes (sync → async); existing call sites need an `.await`. Data-layer migration is otherwise additive — new types live alongside the existing blob primitives, and on-disk bytes from the old key-value usage remain readable.

1. **Async-ify the Store trait.** Update existing impls and call sites; native fjall/redb impls return ready futures.
2. **Add Layer 2 types** (`BlobManifest`, `ManifestId`, `BlobSource`, `SchemaRef`, `PrivacyClass`, `ProvenanceRecord`, `TrustEnvelope`, `Hash`, `Timestamp`) and Layer 2 ops (`save_manifest`, `load_manifest`, `list_manifests`, `resolve_blob`).
3. **Add Layer 3 trait + ops** (`TypedPayload`, `save_typed`, `load_typed`, `list_typed`).
4. **First Layer 4 module: `eidetic::models`** — `ModelManifest` schema + library API. Solves the immediate model-storage pull.
5. **Migrate `intelligence-embeddings::persistence`** to use `save_typed::<VectorIndex>` instead of raw `save_blob`. Backwards-compatible if old keys remain readable for one cycle; otherwise, add a one-shot migration step.
6. **Layer 4 modules as use cases land**: `eidetic::browsing` for browsing memory, `eidetic::settings` for settings, etc.
7. **`eidetic-opfs` crate**: lands when browser-side eidetic has its first consumer.

Each step after (1) is bounded and additive; none requires the others.

---

## 10. File-size discipline

Eidetic is one file at 186 LOC today. With the layers it becomes a multi-file crate plus a sibling browser-backend crate. Per the workspace 600 LOC ceiling:

`eidetic` crate:

- `src/blob.rs` (~200 LOC) — async Store trait, Request/Response, plus small additions (`delete`, `exists`, `iter_keys`)
- `src/manifest.rs` (~300 LOC) — Layer 2 types (`BlobManifest`, `BlobSource`, `SchemaRef`, `ManifestId`) and ops
- `src/typed.rs` (~250 LOC) — Layer 3 trait + ops + serde plumbing
- `src/schema.rs` (~250 LOC) — `SchemaRef`, `Hash`, `Timestamp`, `PrivacyClass`, `ProvenanceRecord`, `TrustEnvelope`, `TrustLevel`, `ModerationState`
- `src/engram.rs` (~250 LOC) — engram envelope, integrity verification, time bounds
- `src/models/` (Layer 4) — `mod.rs`, `library.rs` (~250 LOC each)
- `src/browsing/` (Layer 4, deferred) — when browsing memory work lands

`eidetic-opfs` crate (separate, browser-only):

- `src/lib.rs` (~400 LOC) — OPFS-backed Store impl via `wasm-bindgen` + `FileSystemSyncAccessHandle`

All under the ceiling, all crate-internal layers depend downward only. The OPFS crate isolates `wasm-bindgen` as a transitive dep so native consumers do not pay for it.

---

## 11. Open questions — resolutions and remaining

The following questions were raised in the initial pass; resolutions from the 2026-05-09 design conversation are noted below.

### Resolved

1. **Hash function** — *resolved: BLAKE3, with CIDv1/multihash discipline as the intended portable representation.* Iroh's blob protocol uses BLAKE3 natively, so `BlobSource::Iroh { ticket }` gives BLAKE3-verified bytes for free. Tree-mode enables incremental verification of partial weight downloads. HF Hub publishes SHA-256, so HTTPS sources rehash locally on first fetch. Aligned with the event-DAG substrate brief. **Implementation gap as of 2026-05-11:** `eidetic::schema::Hash` currently stores raw BLAKE3 `[u8; 32]`, not a multihash-aware digest. That is acceptable for current integrity checks but should be fixed before digest fields become protocol-level contracts.
2. **Manifest serialization** — *resolved: JSON for manifests; per-schema choice for typed payloads.* Manifests are small and rare-read; the inspectability win exceeds byte-savings, and JSON makes manifest schema evolution nearly free (forward-compatible by default). Typed payloads pick their own serializer per schema (rkyv for `VectorIndex`, raw bytes for `ModelWeights`, JSON-LD for schema.org-shaped payloads, etc.).
3. **Schema vocabulary** — *resolved: schemas are themselves engrams (recursive), addressed by content hash.* The `engram_spec.md` memory-kind list becomes an example schema vocabulary, not a closed enum. Communities mint new schemas without coordinating; consumers fetch a schema engram to validate against; the storage layer needs no special case. See §7.2.
4. **Browser/PWA backend** — *resolved: OPFS, not IndexedDB.* See §4.5. The collateral decision is that the Store trait must be async — native fjall/redb impls return ready futures, browser impls actually await.
5. **Multi-user** — *resolved: one Store per identity.* Lock the API shape now (Store ops do not take an identity argument); defer instantiation until `mere-identity` lands. Identity deletion is a directory drop; backup, encryption-at-rest, and quota all scope naturally. At Mere's expected scale (1–3 identities per user), per-instance overhead is fine.
6. **Upgrade/migration of stored manifests** — *partially resolved by question 3.* Manifests use JSON with a `manifest_version: u32` field for forward-compatible schema changes; semantic changes bump the version and run a lazy migration on read (re-saving the upgraded manifest so the population gradually upgrades as it is touched). Schemas themselves are immutable (content-addressed); evolution means publishing a new schema engram and updating references that point to it.

### Remaining

- **Meta-schema for schema engrams.** What is the meta-schema (the schema that describes schemas)? JSON Schema? JSON-LD context? A custom Mere-native shape? Pick affects consumer parser cost and ecosystem reuse.
- **Federated identity for engram signatures.** `TrustEnvelope::signatures` and `ContributorAttestation` need a stable identity surface; the `engram_spec.md` adopts `did:key`, but `mere-identity` may settle this differently.
- **fjall-on-OPFS feasibility.** Whether the wasi-fs-shim path collapses native and browser backends to one engine, or whether `eidetic-opfs` stays a hand-rolled backend long-term.
- **Moot accepted-schema-set discovery.** Static published manifest, runtime handshake, or registry? Affects the contributor-side query API for §7.5.
- **Schema-engram garbage collection.** A schema engram referenced by any payload manifest is reachable; orphan schemas should be GC-eligible. The recursion does not change GC semantics, but it does mean GC must walk schema references too.
- **Raw BLAKE3 to multihash migration.** Decide whether to migrate `Hash` in place or introduce a separate multihash-aware `ContentDigest` and keep raw BLAKE3 only for local legacy values. This must settle before adapter manifests, capability scopes, and event hashes depend on the type broadly.

---

## 12. Recommended sequence

1. **Async-ify the Store trait** (Layer 1). Native fjall/redb impls return ready futures; the trait shape itself goes async. Smaller migration if done before Layer 2/3 land.
2. **Add Layer 2 + Layer 3 scaffolding** (`manifest.rs`, `typed.rs`, `schema.rs`). Manifest carries schema reference (not enum tag), three-axis classification (privacy / provenance / trust), BLAKE3 content hash. Tests for save/load/resolve at each layer.
3. **Pick the meta-schema for schema engrams.** Simplest viable shape: a JSON document with a known top-level field set. Defer richer meta-schema work until a second schema-author shows up.
4. **Land `eidetic::models`** (Layer 4) — `ModelManifest` schema + library API. First concrete consumer. Ship the `BertEmbeddingProvider::from_bytes` refactor in `intelligence-embeddings` so it accepts buffers instead of paths.
5. **Migrate `intelligence-embeddings::persistence`** to use Layer 3 with the `VectorIndex` schema.
6. **`eidetic-opfs` crate** lands when browser-side eidetic has its first consumer. Hand-rolled OPFS-backed Store impl; concurrently track fjall-on-OPFS feasibility.
7. **Defer browsing-memory Layer 4** until a UI surface pulls on it (graphshell command palette → semantic search persists query history → first concrete browsing-memory use case).
8. **Defer engram bridge implementation** until Distillery aspect lands. Schema framing, immutability, three-axis classification, and forward-compatibility (the prerequisites) are addressed by this design pass.
9. **Defer `SearchIndex` schema and tantivy integration** until a moot-side consumer pulls on lexical search; tantivy `Directory`-on-OPFS is the implementation path.

---

## 13. Bottom line

Eidetic is currently scaffolding for a concept it does not yet embody. Growing it into a four-layer stack (blob → manifest → typed → memory-domain) gives the immediate model-storage application a clean home, gives the existing vector-index persistence a richer substrate, and lays the architecture for browsing memory, settings, federated contribution, and the eventual engram bridge.

**Architectural commitments this pass makes:**

- Schema is a content-addressed reference, not an enum tag. Schemas are themselves engrams. Recursion is the federation story.
- Engrams are immutable, schema-typed, content-hashed snapshots. Edits do not exist; merges produce new engrams. Snapshots, not subscriptions.
- Privacy / provenance / trust are three orthogonal axes, kept separate.
- Layer 1 backend in browser is OPFS, not IndexedDB; the Store trait is async.
- `engram_spec.md` is an example schema vocabulary, not a closed enum.

The first concrete next step is async-ifying the Store trait, then Layer 2 + Layer 3 scaffolding in service of the model-storage pull. Browsing memory, settings, federated `SearchIndex` contribution, and the Distillery bridge come later, when their consumers materialize.
