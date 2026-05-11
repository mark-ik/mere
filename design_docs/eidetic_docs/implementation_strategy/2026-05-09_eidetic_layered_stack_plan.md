# Eidetic Layered Stack — Implementation Plan (2026-05-09)

**Status**: Active
**Source design**: [`../research/2026-05-09_eidetic_design_pass.md`](../research/2026-05-09_eidetic_design_pass.md)
**Crate**: `repos/mere/crates/eidetic/` (currently 186-line blob-store contract)

This plan descends from the eidetic design pass. It commits to phased, additive changes from the existing crate to the four-layer stack (blob → manifest → typed-payload → memory-domain), with the schemas-as-engrams recursion, three-axis classification (privacy / provenance / trust), BLAKE3 / CIDv1 content addressing, async Store trait, and OPFS-not-IDB browser backend.

The phases are sequenced so each step is independently useful and the next does not strand the previous.

---

## Phases

### Phase 1 — Async Store trait

Convert the Layer 1 Store trait from sync to async. This is the only API-breaking change in the plan; doing it before Layer 2/3 land minimizes the migration surface.

**Scope**:

- `Store` trait methods return futures (`async fn` or `impl Future<Output = …>`)
- Native fjall, redb, in-memory backends return ready futures (no actual async work)
- `dispatch` helper updated; existing call sites add `.await`
- Existing `intelligence-embeddings::persistence::save_to_eidetic` updated

**Done conditions**:

- All existing eidetic tests pass
- `intelligence-embeddings::persistence` tests pass
- No new functional behavior; trait shape change only
- Targeted `cargo test -p eidetic` and `cargo test -p intelligence-embeddings` green

**Dependencies**: none. Standalone.

---

### Phase 2 — Layer 2 scaffolding (manifest)

Add the manifest layer. Manifests describe blobs without reading them, carry the three-axis classification, and reference schemas by content hash (not enum tag).

**Scope**:

- `src/manifest.rs` — `BlobManifest`, `ManifestId`, `BlobSource` (six variants including `Embedded` and `LocalOnlyRef`), `manifest_version: u32`
- `src/schema.rs` — `SchemaRef`, `Hash` (BLAKE3-backed CIDv1), `Timestamp`, `PrivacyClass`, `ProvenanceRecord`, `TrustEnvelope`, `TrustLevel`, `ModerationState`, `SignatureRef`
- Layer 2 ops: `save_manifest`, `load_manifest`, `list_manifests`, `resolve_blob`
- JSON manifest serialization with explicit `manifest_version` field; lazy migration on read
- BLAKE3 hashing utility; CIDv1 wrapping
- `LocalOnlyRef` rejected at any transfer-serialization boundary (compile-checked where feasible, runtime-rejected otherwise)

**Done conditions**:

- Round-trip tests for manifest save/load
- `resolve_blob` falls through `Local` → `Iroh` → `Https` source order; verifies `content_hash` after fetch; caches locally on first remote success
- Hash mismatch rejected with explicit error
- `LocalOnlyRef` never serialized for transfer (test that proves this)
- Forward-compat read of an older `manifest_version` still loads (with the missing fields defaulted)

**Dependencies**: Phase 1.

---

### Phase 3 — Layer 3 scaffolding (typed payloads + engrams)

Add typed-payload Rust bindings and the engram envelope. The engram envelope wraps a payload with schema reference, integrity hash, three-axis classification, and time bounds.

**Scope**:

- `src/typed.rs` — `TypedPayload` trait, `save_typed`, `load_typed`, `list_typed`
- `src/engram.rs` — `Engram` envelope, `TimeBounds`, integrity verification
- Each well-known schema gets a Rust struct + serde impls; the schema vocabulary is open
- `save_typed` produces a manifest with the schema reference; `load_typed` resolves the blob and deserializes against the static binding

**Done conditions**:

- Typed save/load round-trip with hash verification
- Engram integrity check rejects payload bytes that do not match the declared `content_hash`
- Multiple schemas coexist in one Store; `list_typed::<T>` returns only manifests matching `T`'s schema reference

**Dependencies**: Phase 2.

---

### Phase 4 — Meta-schema for schema engrams

Pick the meta-schema (the schema that describes schemas) and implement the recursion. A schema engram is just an engram whose payload is a schema definition; its own `schema` field points to the meta-schema.

**Scope**:

- Decision: lean toward a Mere-native JSON shape (simplest viable); JSON Schema is the alternative if ecosystem reuse outweighs implementation cost
- Meta-schema is itself an engram with a known content hash (bootstrap); its `schema` field points to itself or is a special `META_SCHEMA` sentinel
- Schema validator: given an engram and its declared schema, validates the payload conforms
- Layer-3 binding registry: maps known `SchemaRef` values to Rust deserializers; unknown schemas tolerated and ignored unless `required_for_application`

**Done conditions**:

- A schema engram can be saved, loaded, and used to validate a typed payload
- Recursive resolution: load a payload → fetch its schema engram (also from the Store) → validate
- Unknown-schema payload loads with a deferred error (consumer handles), not a fatal load failure

**Dependencies**: Phase 3.

---

### Phase 5 — Layer 4: `eidetic::models`

First concrete consumer of the layered stack. `ModelManifest` schema + library API; refactor `intelligence-embeddings::BertEmbeddingProvider` to load from buffers.

**Scope**:

- `src/models/mod.rs`, `src/models/library.rs`
- `ModelManifest` schema engram (well-known SchemaRef)
- API: `ModelLibrary::save_model(...)`, `ModelLibrary::load_model(model_id)`, `ModelLibrary::list_models()`
- `intelligence-embeddings::BertEmbeddingProvider::from_bytes(config, weights, tokenizer, device)` constructor
- Existing `BertEmbeddingProvider::load(model_dir)` either delegates to `from_bytes` after reading files, or is removed if no callers remain
- Sources list demonstrates Local + Iroh + HTTPS for HF-hosted models

**Done conditions**:

- MiniLM weights round-trip through eidetic (save → load → embedding inference matches reference)
- Sources list resolves correctly when `Local` is missing (forces Iroh / HTTPS path)
- Hash verification rejects tampered bytes

**Dependencies**: Phase 3 (Phase 4 is not strictly required for this — `ModelManifest` can be a known schema even before the meta-schema work formalizes the recursion).

---

### Phase 6 — Migrate `intelligence-embeddings::persistence`

Convert vector-index persistence from raw `save_blob` to `save_typed::<VectorIndex<K>>`. Backwards-compatible read path for existing data; gradual upgrade as indices are re-saved.

**Scope**:

- `VectorIndex` schema engram
- `save_to_eidetic` calls `save_typed`; `load_from_eidetic` calls `load_typed`
- One-shot migration helper for legacy keys (read old, save new, delete old) — gated on a feature flag or run-once on first load
- rkyv as the per-schema serializer for `VectorIndex` (per the design pass's per-schema choice)

**Done conditions**:

- New saves carry full Layer 2 manifest metadata
- Existing legacy data still loads; migration helper successfully upgrades a populated test store
- No behavioral change for callers of `intelligence-embeddings::persistence`

**Dependencies**: Phase 3, Phase 5 (proves the pattern with a concrete consumer first).

---

### Phase 7 — `eidetic-opfs` crate

Browser-side OPFS-backed Store implementation. Lands when a browser-side eidetic consumer pulls on it (likely the embedding provider in browser context, or browser-side persistence of a vector index).

**Scope**:

- New crate `repos/mere/crates/eidetic-opfs/`
- Hand-rolled `Store` impl over `FileSystemSyncAccessHandle` (in a dedicated worker)
- One file per blob keyed by hash; manifest store as a directory under a known prefix
- wasm-bindgen wiring; `web-sys` for OPFS access
- Concurrent feasibility check on `fjall-on-OPFS` via a wasi-fs shim — track in a separate research note; do not block this phase on it

**Done conditions**:

- Browser-side eidetic round-trips through Layer 1/2/3 in a wasm test harness
- Quota-exceeded returns a clean error (not a panic)
- Persistence survives page reload (after `navigator.storage.persist()`)
- LOC under 600 (~400 estimated)

**Dependencies**: Phase 1, Phase 2 (ideally Phase 3, but Phase 7 is not strictly blocked by Layer 3 — the OPFS impl is at Layer 1).

---

### Phase 8 — Layer 4: `eidetic::browsing` (deferred)

Browsing memory accumulation. Layer 4 module composing `BrowsingTrace` and related typed payloads into the user-facing `BrowsingMemory` API.

**Trigger**: a UI surface pulls on it. Likely candidates: graphshell command palette → semantic search persists query history; Navigator surface → recent-corridor recall.

**Scope** (to be detailed when triggered):

- `BrowsingTrace`, `ClipNode`, `SettingBundle` schema engrams
- `BrowsingMemory` Layer 4 API: `record_traversal`, `recent_corridor`, `nodes_visited_in_window`, `co_occurrence`
- Quota policy per the design pass §8

**Dependencies**: Phase 3.

---

### Phase 9 — `SearchIndex` schema + tantivy integration (deferred)

First-class lexical search. Tantivy `Directory` impl over OPFS for browser; native filesystem for native. `SearchIndex[corpus-shape]` schema engram for federated contribution.

**Trigger**: a moot-side consumer pulls on lexical search, or graphshell semantic-search demands exact-phrase recall.

**Scope** (to be detailed when triggered):

- `SearchIndex` schema engram with corpus shape, tokenizer, ranking config, field set declared
- `tantivy::Directory` impl over OPFS (browser) and native fs (native)
- Index merge plumbing for moot-side composability (per design pass §7.5)
- Segment-separable composition default

**Dependencies**: Phase 3 (typed payloads), Phase 7 (OPFS for browser case).

---

## Findings

(Conclusions from the design pass that guide this plan; recorded so they survive without re-deriving.)

- **BLAKE3 / CIDv1** chosen over SHA-256 — iroh-native, tree-mode incremental verification, consistent with event-DAG substrate brief.
- **OPFS over IndexedDB** for browser — file-shaped (tantivy compatibility), 5–10× faster for blob-heavy workloads, large-blob streaming. Settles a question the design-pass draft had originally answered with IDB.
- **Async Store trait** is collateral to the OPFS choice (no `block_on` in browser wasm).
- **Three-axis classification** (privacy / provenance / trust) — kept separate, never bundled. Each axis evolves on its own clock.
- **Schemas-as-engrams** — recursive, content-addressed, no central registry. The meta-schema is itself an engram.
- **Engrams are immutable, time-bounded snapshots** — edits do not exist; merges produce new engrams.
- **Manifest serialization is JSON** with explicit `manifest_version: u32` for forward-compatible migration; typed-payload serialization is per-schema (rkyv for VectorIndex, raw bytes for ModelWeights, JSON-LD for schema.org-shaped, etc.).
- **One Store per identity** — defer instantiation to `mere-identity`, but lock the API shape now (Store ops do not take an identity argument).
- **`engram_spec.md` is an example schema vocabulary, not a closed enum** — its 17 memory kinds become well-known schemas at known content hashes.

## Open questions

These may surface during implementation; the design pass lists them in §11 "Remaining":

- Meta-schema choice (Phase 4 will settle)
- Federated identity for engram signatures — waits on `mere-identity`
- fjall-on-OPFS feasibility — Phase 7 tracks; not gating
- Moot accepted-schema-set discovery API — Phase 9 territory
- Schema-engram GC semantics — surfaces when GC is implemented (no current phase)

## Sidequests (post-Phase-6, ordered by leverage)

These are out-of-scope for the numbered phases but worth tracking; surfaced during the 2026-05-09 review.

1. **`eidetic-https-fetcher`** — first concrete `BlobFetcher`. ~150 LOC of `reqwest`/`ureq` in a companion crate. Lights up Phase 5's HF Hub story. Land first because that's where HF actually lives.
2. **`eidetic-iroh-fetcher`** — natural follow-up; smaller than the HTTPS fetcher because `iroh-blobs` already does BLAKE3-verified fetch (just wrap, no rehash). The Mere-shaped p2p answer.
3. **`eidetic-fjall` backend** — production default per workspace memory. No design questions left, just implementation. Required before eidetic ships in any real configuration.
4. **`bootstrap()` helper** that seeds the meta-schema engram on first init. Prevents subtle "schema not found / silently tolerant" failures when consumers forget the init step.
5. **Engram bundle / `TransferProfile` schema** — composite-bundle pattern as a concrete schema, with a helper for "engram with a list of memory references." Lets `ModelManifest` (weight + tokenizer) be polyglot without a custom struct; unblocks future moot-snapshot engrams.
6. **Schema authoring ergonomics + search + registry.** A small `schema!` builder/macro reduces friction. Search-by-`schema_id`/description is a Layer-4 indexing concern (depends on sidequest #7). A schema registry is plausibly a moot itself ("schema-coop") rather than a centralized service — fits the federation story. Worth its own plan when a second schema author shows up.
7. **`iter_keys` on the Store trait.** Deferred at Phases 2 and 3; the deferral is starting to compound. Unblocks `list_manifests`, `list_typed`, and any "show me everything" UX. Cost is real (cascading test-impl updates) but bounded.
8. **JSON-LD vocabulary checking.** Current validator is parse-only; full SHACL/vocab-checking lands when a schema.org-shaped payload pulls (e.g. clip-of-Article engrams). Pulls in an RDF crate.
9. **BLAKE3 streaming verification.** Current `resolve_blob` loads full bytes then verifies. For 200MB+ model weights, streaming via BLAKE3's tree mode enables progress reporting and partial-failure handling.

## Engram lifecycle policy

Engrams are content-addressed downloads owned by the user. The substrate (eidetic) does **not** auto-delete engrams under quota pressure or any implicit policy. This is the substrate-level analogue of "don't auto-delete your music library."

- **Default: durable.** Engrams persist until the user removes them.
- **Health-check, not destruction.** A diagnostic surface that probes each source and reports dead links is appropriate; it does not trigger deletion.
- **Opt-in expiry policies.** A user (or a Layer-4 module on their behalf) can set a TTL or "all-sources-dead → candidate-for-removal" policy. Removal still confirmed, not silent.
- **STM is a Layer-4 / UX concept**, not a substrate behavior. Ephemeral memory is engrams *marked* ephemeral with opt-in TTL — built on top of eidetic, not below.
- **Storage growth is addressed via UX tools** (browse, search, prune) — not via implicit GC.

## Risks / things to watch for

- **Async migration friction.** Async traits in stable Rust need care (associated `Future` types, `Send` bounds). Phase 1 may need `async-trait` macro until the trait shape stabilizes.
- **Hash interop with HF Hub.** HF publishes SHA-256; eidetic uses BLAKE3. HTTPS source resolution rehashes locally on first fetch. Watch for cases where users expect to verify against HF's published hash directly — surface BLAKE3 as the canonical hash but optionally record HF's SHA-256 as a side-check in `schema_metadata`.
- **OPFS sync-API worker-only constraint.** `FileSystemSyncAccessHandle` is only available in dedicated workers. Main-thread browser consumers need a worker shim. Phase 7 absorbs this complexity.
- **Schema-engram bootstrap.** The meta-schema is itself an engram, but the Store needs to load *something* before any engram is parseable. Phase 4 must define a bootstrap rule (e.g. the meta-schema CID is hardcoded; the meta-schema engram is shipped as a known artifact in the eidetic crate).
- **Forward-compat tolerance vs strictness.** "Unknown schemas ignored unless `required_for_application`" is the rule from the engram_spec. Layer 3 needs an explicit knob — defaulting to tolerant on read, strict on `required_for_application`-marked memories.

## Progress

- 2026-05-09 — Plan drafted from design pass §12 recommended sequence + conversation conclusions. No code changes yet.
- 2026-05-09 — **Phase 1 complete.** `Store` trait converted to async via `async-trait` macro with `?Send` bound (cross-target safe — works on native multi-threaded runtimes and single-threaded wasm). `dispatch` is now async. `pollster` added as dev-dep for sync-test entry points. Migrations: `intelligence-embeddings::persistence` (5/5 tests green), `graphshell::app_state::persistence::consume_persistence_effects` and `graphshell::app_state::services::dispatch_pending_effects` (50/50 graphshell tests green). Full workspace builds clean. Decision noted: `?Send` chosen because (a) browser wasm is single-threaded so `Send` is fictional there, (b) Store consumers `.await` in place rather than spawning to background tasks. Native callers needing `Send` can wrap at the call site if ever required.
- 2026-05-09 — **Phase 2 complete.** Layer 2 manifest scaffolding landed in `eidetic` (no other crate impact — purely additive). New modules: `src/schema.rs` (Hash/BLAKE3, ManifestId, SchemaRef, Timestamp, PrivacyClass, ProvenanceRecord/Origin, TrustEnvelope/Level, ModerationState, SignatureRef) and `src/manifest.rs` (BlobManifest with `manifest_version: u32`, BlobSource six-variant enum including Embedded with inline bytes and LocalOnlyRef as an explicit safety rail, ops `save_manifest` / `load_manifest` / `resolve_blob`, plus a `BlobFetcher` trait for the Iroh/HTTPS/file delegation path so eidetic stays dep-light, and a `NoFetcher` no-op default). Manifest serialization is JSON via serde_json. Hash verification (BLAKE3) on every successful fetch in `resolve_blob`, with mismatched bytes rejected and the next source tried. Non-Local fetched bytes auto-cache locally under `blob:<hex-hash>`. `to_transferable()` strips `LocalOnlyRef` for outbound transfer (errors if no transferable sources remain). 21/21 eidetic tests green (3 existing dispatch + 6 schema + 12 manifest); workspace builds clean. `list_manifests` deferred — requires `iter_keys` on Store, which adds churn to all impls; will land when a concrete consumer pulls on it. Embedded variant retained per the engram_spec carry-forward; small inline metadata blobs use it.
- 2026-05-09 — **Phase 3 complete.** Layer 3 typed payloads + engram envelope landed in `eidetic` (no other crate impact). New modules: `src/engram.rs` (`Engram` envelope with `schema` reference, `payload`, `content_hash`, three-axis classification, `TimeBounds`, `envelope_version: u32`, `verify_integrity()` rejecting tampered payloads and invalid bounds, `id()` returning `ManifestId::from_hash(content_hash)`) and `src/typed.rs` (`TypedPayload` trait with `schema_ref()` + serializer/deserializer hooks defaulting to JSON, plus `save_typed` / `load_typed` ops). `save_typed` writes blob under `"blob:<hex-hash>"` and manifest under `"manifest:<hex-hash>"`. `load_typed` enforces a schema-mismatch guard — loading a saved Greeting as a Counter is an explicit error, not silent garbage. End-to-end integrity: `load_typed` calls `resolve_blob` which BLAKE3-verifies bytes against the manifest's `content_hash` before deserialization. 32/32 eidetic tests green (3 dispatch + 6 schema + 12 manifest + 6 engram + 5 typed); workspace builds clean. `list_typed` deferred along with `list_manifests` (both want `iter_keys` on Store).
- 2026-05-09 — **Phase 4 complete.** Polyglot meta-schema architecture landed in `eidetic` (no other crate impact). New module `src/schema_def.rs` adds `SchemaFormat` enum (`MereNative` / `JsonSchema` / `JsonLd`), `SchemaDefinition` payload, `SchemaValidator` trait + dispatch via `validate_payload`, and three validators: **Mere-native** (full structural validation — required fields, field types via `MereNativeFieldSpec`, enum values), **JSON Schema** (thin wrapper around the `jsonschema` crate, full Draft 7+ compliance), **JSON-LD** (parse-only — verifies `@context` or `@type` presence and matches declared `@type` if present; full SHACL/vocab-checking deferred to first concrete schema.org payload). Bootstrap: `META_SCHEMA_PAYLOAD` is a const byte string describing the SchemaDefinition shape itself in Mere-native; `META_SCHEMA_REF` is a `LazyLock<SchemaRef>` computed from its BLAKE3 hash; `meta_schema_engram()` builds the bootstrap engram. Recursion holds: meta-schema describes itself, schema engrams point to META_SCHEMA_REF, regular engrams point to schema engrams. Recursive resolution via `validate_against_schema(store, fetcher, schema_ref, payload_bytes)` loads the schema engram and runs the matching validator; tolerant on unknown schema (returns `Ok(())` rather than erroring) per the design pass's "tolerant on read" rule. 46/46 eidetic tests green (32 prior + 14 schema_def: 4 Mere-native + 2 JSON Schema + 3 JSON-LD + 2 round-trip/recursion + 1 tolerance + 2 meta-schema). Workspace builds clean. Validators are pluggable — adding a new `SchemaFormat` variant + `SchemaValidator` impl + dispatch arm extends polyglot support without touching existing format code.
- 2026-05-09 — **Phase 5 complete.** First concrete Layer-4 consumer landed. New `eidetic::models` module (`src/models/mod.rs` + `src/models/library.rs`): `ModelManifest` typed payload (model_id, architecture, license, inline config, content-addressed `weight_blob` + `tokenizer_blob`), `ModelLibrary` API (`save_model`, `save_model_with_components`, `load_model`, `resolve_components`), and `ModelComponents` returning resolved bytes for a downloaded model. Inline `MODEL_MANIFEST_SCHEMA_PAYLOAD` const + `MODEL_MANIFEST_SCHEMA_REF` `LazyLock`; `OPAQUE_BLOB_SCHEMA_REF` for the weight/tokenizer sub-blobs. Cross-crate work in `intelligence-embeddings`: factored `BertTokenizer::configure` shared helper, added `BertTokenizer::from_bytes`, refactored `load_artifacts` to delegate to new `artifacts_from_bytes`, added `validate_weights_from_bytes` + `extract_all_tensors_from_bytes` + `load_into_model_from_bytes`, and the public `BertEmbeddingProvider::from_bytes(config_bytes, tokenizer_bytes, weights_bytes, device)` constructor — the buffer-based path eidetic-resolved model artifacts flow through. 52/52 eidetic tests green (6 new in models: round-trip, resolve, hash-mismatch on weight blob, unknown id, dedupe via content-addressing, schema-ref stability); 4 ignored bert tests still need `MERE_MINILM_DIR`; full workspace builds clean.
- 2026-05-09 — **Phase 6 complete.** Migrated `intelligence-embeddings::persistence` from raw `Store::save_blob` / `load_blob` to Layer 3 `save_typed::<VectorIndex<K>>` / `load_typed`. New `VECTOR_INDEX_SCHEMA_PAYLOAD` const + `VECTOR_INDEX_SCHEMA_REF` `LazyLock`; one schema covers all `VectorIndex<K>` regardless of `K` (key type is data-level, not schema-level). `TypedPayload` impl for `VectorIndex<K>` where `K: Hash + Eq + Clone + Serialize + DeserializeOwned`. New `save_to_eidetic` returns `ManifestId` instead of taking a string key (callers persist the id externally); `load_from_eidetic` takes `ManifestId` + a `BlobFetcher`. No legacy migration step (crate at 0.0.1, no real deployments). 6/6 persistence tests green (round-trip, missing, content-addressed overwrite-by-id, empty, schema stability, corrupted blob); workspace builds clean.
- 2026-05-10 — **Sidequests 1–9 complete.**
  - **#1 `eidetic-https-fetcher` crate** — synchronous HTTPS via `ureq` (rustls TLS), `BlobFetcher` impl returning `Ok(None)` for non-HTTPS sources, configurable max-response-bytes guard, mockito-based tests. 4/4 tests green.
  - **#2 `eidetic-iroh-fetcher` crate** — wraps `mere-transport`'s `BlobStore` + `IrohTransport` for `BlobSource::Iroh { ticket }` resolution. Ticket format `"<node-id-hex>/<blob-hash-hex>"` (64+1+64 chars). Provides `build_ticket()` / `parse_ticket()`. Includes a real two-node iroh end-to-end test that does a p2p blob transfer through the BlobFetcher trait surface. 5/5 tests green.
  - **#3 `eidetic-fjall` crate** — production-default native `Store` impl over fjall LSM. `FjallStore::open(path)` for the simple case, `open_partition(path, name)` for hosting multiple identities under one keyspace. Implements `iter_keys` via fjall's prefix-scan. Tests cover round-trip, missing, overwrite, manifest round-trip, persistence-across-reopen, partition isolation, and prefix iteration. 7/7 tests green.
  - **#4 `bootstrap_meta_schema(store)` + top-level `eidetic::bootstrap(store)`** — idempotent first-init helper that seeds the meta-schema engram (writes `META_SCHEMA_PAYLOAD` bytes verbatim — re-serialization could reorder JSON keys and break the BLAKE3 anchor). Returns immediately if already present. Two new tests: idempotent-seed and bootstrap-enables-strict-validation.
  - **#5 `Bundle` schema (composite engram)** — `Bundle { bundle_id, bundle_kind, description, members: Vec<BundleMember> }` with `BundleMember { kind, manifest, required, note }`. `Bundle::member(kind)` and `required_members()` helpers. `verify_required_members(store, fetcher, bundle)` walks required members, loads each manifest, resolves blob (with hash check), and lists missing on first failure (non-required missing is tolerated). The `engram_spec.md` `TransferProfile` pattern survives as one well-known schema among many. 5 new tests.
  - **#6 schema authoring + search** — `MereNativeSchemaBuilder` for ergonomic Mere-native schema construction (description, version, fields with required/type). `find_schema_by_id(store, fetcher, schema_id)` walks every schema engram and returns the first match by human-readable id. The schema-registry concern is left for its own plan; eidetic now has the primitives a registry would use.
  - **#7 `iter_keys` on the Store trait** — added with a default impl that returns "unsupported." `list_manifests(store, schema_filter)` and `list_typed::<T>(store)` ride on top. `FjallStore` and the eidetic-internal test stores override; other test impls (graphshell, intelligence-embeddings) keep the default — list functionality is opt-in per backend.
  - **#8 JSON-LD validator enhancements** — array `@type` support in both schema and payload (subsumption: payload `["Article", "BlogPosting"]` satisfies schema `"Article"`); multi-required-type schemas (`@type: ["Article", "CreativeWork"]`); `required_props: [...]` field for structural property requirements. Full SHACL/RDF-vocab checking still deferred (no RDF dep). 3 new tests.
  - **#9 BLAKE3 streaming verification** — `Hash::from_chunks(impl IntoIterator<Item = &[u8]>)` and `Hash::from_reader(impl Read)` factories using `blake3::Hasher` incrementally, 64 KiB chunks for the reader path. Architectural prep for future streaming `BlobFetcher` impls — large-blob verification no longer requires holding the full payload as a single contiguous slice. 2 new tests prove streaming-equivalence.

  **Totals across all the work above:** eidetic 67/67, eidetic-fjall 7/7, eidetic-https-fetcher 4/4, eidetic-iroh-fetcher 5/5, intelligence-embeddings 11/11 (incl. 6 persistence), graphshell 50/50. Full workspace builds clean.
