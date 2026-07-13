# Eidetic Deferred Phases — Implementation Plan (2026-06-09)

**Status**: Active (open tail spun out of the completed layered-stack plan).
**Spun out of**: `archive_docs/.../2026-05-09_eidetic_layered_stack_plan.md` (Phases 1-6
plus all nine sidequests shipped; that plan is archived as complete).
**Source design**: [`../research/2026-05-09_eidetic_design_pass.md`](../research/2026-05-09_eidetic_design_pass.md)
**Crate family**: `repos/mere/crates/eidetic/` (`eidetic-core`, `eidetic-fjall`,
`eidetic-https-fetcher`, `eidetic-iroh-fetcher` shipped).

The four-layer stack (blob → manifest → typed-payload → memory-domain), the
schemas-as-engrams recursion, three-axis classification, BLAKE3/CIDv1 addressing,
async Store trait, and the model library are all built. What remains are the
three trigger-gated phases the layered-stack plan deferred, plus the open
questions that outlive it. Each is consumer-gated: build when a real consumer
pulls.

---

## Phases

### Phase 7 — `eidetic-opfs` (browser Store backend)

Browser-side OPFS-backed `Store`. **Trigger**: a browser-side eidetic consumer
pulls (likely `intel/embed` running embeddings in-browser, or browser-side
persistence of a vector index).

**Scope**:

- New crate `crates/eidetic/eidetic-opfs/`.
- Hand-rolled `Store` over `FileSystemSyncAccessHandle` in a dedicated worker.
- One file per blob keyed by hash; manifest store as a directory under a known prefix.
- wasm-bindgen + `web-sys` for OPFS access.
- `fjall-on-OPFS` feasibility (wasi-fs shim) tracked separately, not gating.

**Done conditions**: browser round-trip through Layer 1/2/3 in a wasm harness;
quota-exceeded returns a clean error; persistence survives reload after
`navigator.storage.persist()`; under the 600-LOC ceiling.

**Dependencies**: Phases 1-2 shipped (the OPFS impl is at Layer 1).

**Trigger now emerging (2026-06-24).** Genet's Nova-on-Memory64 browser engine
lane landed (`genet/docs/2026-06-24_nova_memory64_browser_lane_plan.md`): an
in-browser script engine running in a dedicated Web Worker. That is the browser
runtime a browser-side eidetic consumer needs, and its profile fits Phase 7 hand in
glove — the engine lane is **single-worker with no SharedArrayBuffer/Atomics/COOP/
COEP**, which is exactly what OPFS sync access handles want (worker-only, no
cross-origin isolation, one writer so the default per-file exclusive lock is fine).
The old way to get synchronous storage into wasm needed SAB+Atomics+COOP/COEP; OPFS
removes that. Memory64 sizes the *heap*; OPFS sizes the *disk* — complementary.

**The real gate is the wasm build, not the design** (mirroring Phase 9's
tantivy-on-wasm framing — OPFS only settles where bytes live): (a) **VERIFIED
2026-06-24 — `eidetic-core` compiles clean for `wasm32-unknown-unknown`**, needing
only one target-gated `getrandom = { features = ["wasm_js"] }` (a transitive pull via
jsonschema) plus `--cfg getrandom_backend="wasm_js"`, the same flag the engine lane
sets; the abstract `Store` + Layer 1/2/3 logic is browser-ready today. The `Store`
trait is async + blob-KV, so `eidetic-opfs` wraps synchronous sync-handle I/O in async
fns (no executor inside). (b) **VERIFIED 2026-06-24 — `web-sys` binds the OPFS
sync-access surface** (`FileSystemSyncAccessHandle` + the file/dir handles +
`StorageManager`, with `get_size`/`flush`/`close`/`create_sync_access_handle`/
`get_directory`) under `--cfg web_sys_unstable_apis` (pin the binding surface like the
engine lane pins `wasm-bindgen` 0.2.125). (c) a throughput measurement vs
`eidetic-fjall` native for the blob round-trip, in a worker, Chrome + Firefox. So (a)
and (b) are cleared; the only remaining gate is (c) the bench (which needs the real
`eidetic-opfs` crate + a worker harness).

**Layout choice the measurement decides.** One-file-per-blob (the v0 scope above) is
simplest but pays per-file OPFS overhead at corpus scale; a *packed* container (a
couple of OPFS files as a log+index, or redb / sqlite-wasm as the blob store, both of
which already run on a sync handle) amortizes it. Start file-per-blob, measure, pack
if the per-file overhead bites.

**Second consumer.** Genet's web-platform storage for scripted pages
(`localStorage` / `sessionStorage` / IndexedDB / Cache) can ride this same eidetic
`OpfsStore` in the worker rather than stand up a parallel store — one OPFS-backed
durable layer, two faces. (DocumentScript's deferred `persistent-storage` profile
world is the third: one host-agnostic WIT contract, `OpfsStore` browser /
`FjallStore` native.)

### Phase 8 — `eidetic::browsing` (Layer-4 browsing memory)

**ACTIVATED 2026-06-12** → built under the
[browsing derivation plan](2026-06-12_eidetic_browsing_derivation_plan.md)
(slices E1/E2; the pull is Mark directly — the single-consumer rule).

Browsing-memory accumulation composing `BrowsingTrace` and related typed payloads
into a user-facing `BrowsingMemory` API. **Trigger**: a UI surface pulls, e.g.
meerkat's omnibar/command surface persisting query history, or the gloss
Navigator recalling a recent corridor.

**Scope (detail when triggered)**: `BrowsingTrace`, `ClipNode`, `SettingBundle`
schema engrams; `BrowsingMemory` API (`record_traversal`, `recent_corridor`,
`nodes_visited_in_window`, `co_occurrence`); quota policy per design pass §8.

**Dependencies**: Phase 3 shipped. Note the node-navigation lineage work already
gives nodes a durable nav history; browsing memory should read from that rather
than duplicate a visited-set.

### Phase 9 — `SearchIndex` schema + tantivy

**Producer half ACTIVATED 2026-06-12** → built native-first under the
[browsing derivation plan](2026-06-12_eidetic_browsing_derivation_plan.md)
(slices E3/E4: `eidetic-search`, the format-versioned `SearchIndex` engram,
BM25 recall + fast-field reports, the engine-agnostic hybrid-fusion seam).
The **consume half stays here** (EngramDirectory over iroh-blobs ranges,
per-moot merge policy, defensive ingestion, the wasm probe) — trigger
unchanged: a moot-side consumer.

First-class lexical search; the federated-contribution counterpart to the vector
index. **Trigger**: a moot-side consumer pulls lexical search, or graph/semantic
search demands exact-phrase recall.

**The frame (2026-06-11): one index, two configurations.** eidetic is the
**producer** (read-write, `IndexWriter`, OPFS/native Directory, indexing your
own trails and notes); moot is the **consumer** (the `SearchIndex` engram is an
immutable, schema-and-format-tagged, CID-addressed slice, shared read-only,
range-fetched on demand, merged under per-moot policy, tessera-gated). The
engram is the hand-off: eidetic mints it, moot federates it. tantivy's relevance
to both is the same relevance seen from the two ends.

**Scope (detailed 2026-06-11; verified against tantivy 0.26.0 — the source
checkout at `Code/.tantivy-probe` — and the design pass §7.5):**

- `SearchIndex` schema engram: corpus shape, tokenizer, ranking config, field
  set, **and reserved fast-field columns** — the aggregations framework over
  columnar fast fields (terms / histogram / stats / facets, composing across
  merged segments) is what makes a moot's flora *browsable* (reports: counts
  over time, top domains, topic facets per contributor), not just searchable.
  Reserve the columns in the schema now so reports need no re-index later.
- **The engram contract carries a tantivy format version** beside the schema:
  tantivy does not promise index compatibility across releases, so recipients
  reject-or-re-mint on mismatch (the workspace-pins doctrine applied to a wire
  format). Re-minting is cheap in principle because the source corpus is itself
  engrams — "re-index the flora at format N+1" is an indexing **bounty**
  (communal-compute brief). Caveat for that bounty's verification: tantivy
  indexing is **not byte-deterministic** (segment layout varies with threading
  and merge policy), so index bounties verify by query-equivalence spot-checks
  (claimed-task shaped), never first-valid-hash.
- **Two Directory backends, not one** — the produce and consume paths have
  different requirements and the consume path is strictly simpler:
  - *Produce* (eidetic, read-write): `tantivy::Directory` over OPFS (browser)
    and native fs; this is the only path that needs atomic writes, lock files,
    and `ManagedDirectory` GC.
  - *Consume* (moot, read-only): an **`EngramDirectory`** that lazily
    range-fetches. The seam is `FileHandle::read_bytes(Range) -> OwnedBytes`
    (+ `read_bytes_async`, `common/src/file_slice.rs:22-29`), and it maps
    almost one-to-one onto **iroh-blobs range requests** — bao-tree streaming
    already serves verified byte ranges of a blob, so per-range integrity
    comes from the transport. Plus a small hot cache (the Quickwit
    split/hotcache **technique** — borrow the idea; Quickwit is AGPL, tantivy
    itself is MIT, keep the line clean). Payoff: a member browses a large
    community index without downloading it first, reading only the sstable
    blocks and posting ranges a query touches. No `IndexWriter`, no locks.
- Index-merge plumbing for moot composability (design pass §7.5):
  segment-separable composition default (per-contributor partitions; merge is
  per-moot policy via `IndexMerger`), schema + format verified before merge.
- **Defensive ingestion on the consume path**: transport integrity (bao) gates
  tamper, tessera gates who, but parsing a peer's term dict / postings is
  still an attack surface. Validate peer segments in a constellation-style
  worker (panic isolation + respawn already exist there); a panic rejects the
  engram, never the kernel. wasm caveat: panic-catching is unavailable under
  panic=abort, so validation is Result-shaped there or runs native-side at
  the moot boundary.
- **The real browser gate is tantivy-on-wasm, not storage** (OPFS settles
  where bytes live). One-command probe from the checkout:
  `cargo check --target wasm32-unknown-unknown --no-default-features` (mmap is
  a feature to drop; threads live mostly in `IndexWriter`, and the consumer
  read path can run a single-thread executor). Fallback if the engine is too
  heavy there: `ownedbytes` / `sstable` / `columnar` are individually MIT and
  borrowable as building blocks — a big retreat, so probe first.
- **Hybrid retrieval**: tantivy is also the BM25 half of geist RAG beside
  `intel/embed`'s vector index (BM25 + vector fusion is the standard quality
  move, and the resource-coordination brief already concluded RAG carries the
  geist's factual side). Phase 9 is retrieval quality for every geist query,
  not just a moot search box.

**Dependencies**: Phase 3 (typed payloads), Phase 7 (OPFS for the browser
*produce* case; the consume path depends on iroh-blobs ranges instead).

---

## Open questions (carried forward)

- **Federated identity for engram signatures** — waits on the persona/identity
  vault (`crates/persona/identity`, now built); wire signature verification once
  signed engrams cross a peer boundary.
- **Schema-engram GC semantics** — surfaces when GC is implemented; no current
  phase. The engram lifecycle policy (durable by default, no implicit deletion)
  stands.
- **Moot accepted-schema-set discovery API** — Phase 9 territory; how a moot
  advertises which schema classes it ingests (ties to the moothold federation
  filtering in the local-intelligence research §5.6).
- **fjall-on-OPFS feasibility** — Phase 7 tracks; not gating.

---

## Progress

- 2026-06-09 — Spun out of the completed layered-stack plan. Phases 1-6 +
  sidequests 1-9 shipped (eidetic 67 tests + the four companion crates green per
  the archived plan's progress log). No new code; this plan holds the deferred
  tail until a consumer triggers each phase.
- 2026-06-11 — **Phase 9 detailed** from a verified tantivy analysis (claims
  checked against the tantivy 0.26.0 source checkout at `Code/.tantivy-probe`
  and the design pass; `FileHandle::read_bytes(_async)` confirmed at
  `common/src/file_slice.rs:22-29`). Folded in: the one-index-two-configurations
  producer/consumer frame (the engram as hand-off), the produce/consume
  Directory split (consume = read-only `EngramDirectory` range-fetching over
  iroh-blobs bao ranges + hotcache, Quickwit technique-only), the
  format-version engram contract (+ re-mint-via-bounty with the
  non-byte-deterministic-indexing verification caveat), reserved fast-field
  columns for the aggregations/reports half, defensive ingestion in a
  constellation-style worker, the wasm probe as the real browser gate, and the
  hybrid-retrieval (BM25 + vector) tie to geist RAG. Still trigger-gated; no
  code.
- 2026-06-12 — **Phase 8 and Phase 9's producer half activated** into the
  [browsing derivation plan](2026-06-12_eidetic_browsing_derivation_plan.md)
  (Mark pulled the user-value arc directly: derive useful information from
  your own browsing). This plan remains the umbrella for Phase 7, the wasm
  probe, and Phase 9's consume/federation half.
- 2026-06-24 — **Phase 7 trigger emerging; measure first.** Genet's
  Nova-on-Memory64 browser engine lane landed (a script engine in a dedicated
  worker), supplying the browser runtime Phase 7's `eidetic-opfs` consumer needs;
  its single-worker, no-SAB/Atomics/COOP/COEP profile is exactly the profile OPFS
  sync access handles want. Folded into Phase 7: the engine-lane trigger + that
  isolation-free fit, the wasm build as the real gate (eidetic-core+opfs on
  wasm32/64, `web-sys` `FileSystemSyncAccessHandle` behind the unstable-apis flag),
  the file-per-blob-then-pack layout call, and genet web-platform storage as a
  second consumer of the same `OpfsStore`. Mark is in; next step is the measurement
  probe (OpfsStore vs FjallStore blob round-trip in a worker). A genet-side draft of
  this was discarded as mis-homed (durable store is eidetic, not genet).
  **Gates (a) + (b) cleared (measured):** `eidetic-core` compiles clean for
  `wasm32-unknown-unknown` (one target-gated `getrandom` `wasm_js` feature + the cfg
  flag; verified then reverted — no code landed), and `web-sys` binds the OPFS
  sync-access surface under `--cfg web_sys_unstable_apis` (a throwaway scratch probe
  compiled clean). **Gate (c) build half done:** a real ~80-line `OpfsStore`
  (`impl Store for OpfsStore` over a `FileSystemDirectoryHandle`, one file per blob,
  `save`/`load`/`delete`) compiled clean for wasm32 on the first try — every web-sys
  call resolved (`get_file_handle`(`_with_options`), `create_sync_access_handle`,
  `get_size`/`read_with_u8_array`/`write_with_u8_array`/`truncate_with_f64`/`flush`/
  `close`). So the **whole stack is proven at compile**; the impl wrote correctly
  against the real API. (Probe lives in the scratchpad, not landed.) What's left for
  the **throughput number**: it inherently needs a browser, and a *worker* (sync
  access handles are worker-only, so it cannot be a main-thread `wasm-bindgen-test`).
  The harness is: install `wasm-bindgen-cli` (none here), generate glue, a worker
  bootstrap + page that runs N save/load round-trips through `OpfsStore` and times
  `performance.now()`, vs a native `FjallStore` baseline. Edge is present; headless
  automation would drive it over CDP from Node, or Mark opens the page directly.
- 2026-06-24 — **Gate (c) MEASURED → "pack small blobs" confirmed.** Ran the OPFS
  substrate doing `OpfsStore`'s exact op sequence (file-per-blob, flush-per-op,
  durable) in a worker via headless Edge (Edg/150, puppeteer-core connected to a
  self-launched Edge over CDP; OPFS needs the `http://localhost` origin), vs
  `eidetic-fjall` native (release, but LSM so save = memtable insert + load = cached,
  i.e. RAM-speed, not flush-per-op — the comparison is durable-OPFS vs in-memory-fjall,
  so the native side is an upper bound). Numbers (save / load ops/s):
  - 1 KB:   OPFS **160 / 146**   vs fjall 155 159 / 2 066 115
  - 64 KB:  OPFS **151 / 187**   vs fjall 4 561 / 163 657
  - 1 MB:   OPFS **92 / 150** (97 / 157 MB/s)  vs fjall 219 / 418 (230 / 439 MB/s)

  **Finding:** OPFS sync-handle *byte* throughput is solid (1 MB ~97 MB/s write /
  ~157 MB/s read, within 2-3x of native), but OPFS ops/s is **flat at ~150 across all
  sizes** — the per-blob cost is dominated by **file-handle + sync-handle creation
  (~6 ms/op)**, not the I/O. So file-per-blob caps the small-blob (engram / manifest)
  corpus at ~150 ops/s while a native LSM does memtable inserts orders of magnitude
  faster. **Verdict (measured, not assumed): pack small blobs.** Phase 7 ships a
  *packed* container for the corpus (one long-lived sync access handle over a log +
  index, so the ~6 ms handle cost amortizes across many blobs), and keeps
  file-per-blob only for large blobs (1 MB+), where it is already fine. This retires
  the "file-per-blob-then-pack" open call: the scope's v0 one-file-per-blob is the
  large-blob path; the corpus path is packed from the start. The exact OpfsStore
  through-wasm delta (wasm-bindgen marshalling, a small constant on top) is the only
  unmeasured piece, deferred (needs `wasm-bindgen-cli`); it does not change the
  pack verdict. All probes (eidetic-opfs crate, the JS bench, the fjall bench) live in
  the scratchpad; no code landed in-tree.
