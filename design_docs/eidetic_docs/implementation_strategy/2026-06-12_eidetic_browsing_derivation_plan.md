# Eidetic Browsing Derivation Plan — your own trail, made useful

**Date**: 2026-06-12
**Status**: Active. This plan **activates** two of the
[deferred phases](2026-06-09_eidetic_deferred_phases_plan.md) — Phase 8
(browsing memory) in full, and Phase 9's **producer half** native-first — and
sequences them into the user-value arc Mark named: *a user derives useful
information from their own browsing, for themselves*. The deferred-phases plan
stays the umbrella for what this plan does not pull in (Phase 7 / OPFS, the
wasm probe, and Phase 9's moot-consume half: `EngramDirectory`, merge policy,
defensive ingestion).
**Source design**: [eidetic design pass](../research/2026-05-09_eidetic_design_pass.md)
(§Layer 4, §6.2 browsing memory, §7.5 search-index engrams, §8 privacy/quota).
**Conflict posture**: pure mere lane — eidetic crates, `import`, and a dev
bin; no serval-layout, no meerkat render/input/frame_ops, no pelt. Shell
surfacing is the explicitly **gated last slice**, adopted after the
window-composition reshape settles (the mesh-P6 pattern).

---

## The user story (what "useful" means here)

Everything below is **LocalOnly by default** (the design pass's privacy axis);
sharing any of it into a moot is a separate, explicit, later act.

- **Recall** — "where did I read about X?" answered from your own trail, not
  a search engine: BM25 over what you visited, fused with the vector index
  that already exists in `intel/embed`.
- **Corridors** — "what was I just doing?" — recent paths, resumable; the
  thing the gloss strip wants to show.
- **Reports** — "what does my month look like?" — visits over time, top
  domains, topic facets; tantivy's fast-field aggregations exist for exactly
  this (reserved in the Phase 9 schema).
- **The geist's factual side** — the same recall plumbing is the BM25 half of
  hybrid retrieval for any local assistant query (the resource brief's RAG
  conclusion); building recall for the user builds it for the geist.

What already exists, so this plan composes rather than invents: live
navigation memory in `node-lineage` (visits, transitions, owner branches,
edge co-occurrence views — the capture substrate); real-history ingest in
`import` (Chrome/Netscape bookmarks + history → `ImportedPageSeed`, parser
only, mapping left to the call site); the vector half in `intel/embed`
(`EmbeddingProvider`, persisted `VectorIndex`); eidetic Layers 1-3 + the
model library. The missing middle is trace engrams, the lexical index, and
the derivation API over them.

## Slices (each independently landable; done conditions, not dates)

### E1 — `BrowsingTrace` + `BrowsingMemory` (Phase 8 core)

The Layer-4 memory domain the crate was named for.

- `BrowsingTrace` schema engram (schema-as-engram, like the model library):
  visit/traversal records — node identity (canonical URL + title), transition
  kind, timestamp, optional dwell. Deliberately *coarser* than node-lineage's
  full per-owner branch detail: lineage is the live, graph-shaped working
  memory; a trace is the durable, portable impression. ClipNode / clip
  library is **out** of E1 (user-curated clips arrive with a capture UI).
- `eidetic::browsing` module (eidetic-core; serde-only, no heavy deps):
  `BrowsingMemory` with `record_traversal`, `recent_corridor(n)`,
  `nodes_visited_in_window(start, end)`, `co_occurrence(a, b)` — the design
  pass §6.2 surface.
- **The lineage bridge reads, never duplicates**: a projection from
  `node-lineage` snapshots (`GraphMemorySnapshot` / visit + transition
  records) into trace engrams, so live browsing becomes durable memory by
  projection, not by a second visited-set. The projection lives eidetic-side
  (lineage stays dependency-free of eidetic).
- Quota per design pass §8: keep-N-recent with age-out, **configurable**, no
  silent deletion of anything pinned.

**Done when**: trace engrams round-trip through Layers 2/3 with the three-axis
classification defaulted LocalOnly; a lineage snapshot projects into traces
and the four reads answer over them; quota ages out beyond N in a test; every
file under the 600-LOC ceiling.

### E2 — real history in (`import` bridge + the recall bin)

- A small mapper (call-site style, per import's own doc): history-visit /
  session payloads → `BrowsingTrace` engrams, provenance carried from
  `BrowserImportRun`.
- `examples/eidetic-recall` dev bin (the `mesh-peer` pattern: a rehearsal
  artifact, not a product surface): point it at an exported history file,
  build a browsing memory, answer corridor/window queries from the terminal.

**Done when**: a real exported history file (Mark's) lands as traces and
`recent_corridor` / `nodes_visited_in_window` answer over it from the bin.

### E3 — the lexical index (Phase 9 producer half, native-first)

New crate `crates/eidetic/eidetic-search` (tantivy is a heavy dep; it does
not enter eidetic-core). Scope per the deferred plan's verified Phase 9
detail, producer side only:

- `SearchIndex` schema engram: field set, tokenizer, ranking config,
  **tantivy format version** (reject-or-re-mint on mismatch), and the
  **reserved fast-field columns** for E4's reports.
- Native-fs `tantivy::Directory` produce path (`IndexWriter`, atomic writes).
  OPFS/wasm is Phase 7 + the wasm probe, deferred.
- Index the trail: titles, canonical URLs, domains, timestamps from
  `BrowsingTrace` (+ clip text when clips exist). **Full page text is not in
  this slice**: content capture needs a text-extraction seam on the serval
  side (the laid-out document already has it; pulling it crosses the
  conflict line today). Named below as the content-capture trigger.
- Re-index from engrams (the re-mint path): the index is derived state; the
  trace corpus is the source of truth.
- BM25 recall queries with ranked hits.

**Done when**: the recall bin answers "where did I read about X?" with ranked
hits over an indexed real-history trail; a format-version mismatch refuses
the index and re-mints from traces; `cargo test -p eidetic-search` green.

### E4 — hybrid recall + reports

- **Fusion seam is engine-agnostic**: `eidetic-search` fuses
  `(bm25_hits, vector_hits) -> ranked` (reciprocal-rank fusion default,
  weights **configurable**); it does not depend on `embed`. The bin wires
  embed's vector index beside the lexical one — the dependency-light shape,
  and the geist later wires the same seam.
- Reports over fast-field aggregations: visits-over-time histogram, top
  domains, facet counts — printed by the bin first, shell surfaces later.

**Done when**: one fused query returns a ranking that uses both halves
(test: a vector-only and a lexical-only hit both surface); the bin prints a
month report from real history; fusion weights are a setting, not a constant.

### E5 — shell surfacing (gated; do not start)

Omnibar recall, gloss corridor strip, apparatus reports. **Gated on the
window-composition reshape settling** (same adoption pattern as mesh → P6).
The bin is the proof surface until then.

## Out of scope (named, with triggers)

- **Phase 7 / OPFS + the wasm probe** — browser-side produce; stays in the
  deferred plan. Trigger unchanged.
- **Phase 9 consume half** (`EngramDirectory` over iroh-blobs ranges,
  per-moot merge, defensive ingestion) — federation; stays in the deferred
  plan. Trigger: a moot-side consumer.
- **Full page-text capture** — trigger: a serval-side text-extraction seam
  (post-V4 conversation; the engine owns layout-ordered text). Until then
  recall ranks on titles/URLs/clips, which already beats nothing by a lot.
- **ClipLibrary capture UI** — trigger: E5 territory (a capture verb needs a
  surface).
- **Settings-as-payloads domain** — unrelated to this arc.

## Open questions

- Trace granularity vs. lineage detail: the projection collapses per-owner
  branches into a flat chronological trail; is per-owner (per-persona)
  partitioning of *traces* wanted from day one? (Privacy axis suggests yes —
  cheap to carry, expensive to retrofit.)
- Where co-occurrence should live long-term: lineage already has
  `AggregatedEntryEdgeView`; E1 answers from traces for portability, but the
  two should not drift into disagreeing definitions.
- The geist wiring point for hybrid retrieval (which crate owns the fused
  query API once a real assistant consumer exists).

## Progress

- **2026-06-12** — Plan written; activates Phase 8 + Phase 9 (producer half,
  native-first) out of the deferred-phases plan, sequenced E1→E4 with E5
  gated on the reshape. Survey grounding it: `node-lineage` already records
  visits/transitions/co-occurrence (capture exists; the bridge projects),
  `import` already parses real Chrome/Netscape history (the E2 demo is
  Mark's own trail), `embed` already persists a vector index to eidetic
  (E4's other half), eidetic-core has no `browsing` module yet (Phase 8
  genuinely unbuilt). tantivy enters via a new `eidetic-search` crate, never
  eidetic-core. One correction from the survey: `import` *models* history
  visits (`ImportedHistoryVisitItem`, serde) but only *parses* bookmark
  files — history lives in browser SQLite with no interchange format, so E2
  ingests history as a JSONL of the import crate's own type (a documented
  one-line sqlite3 export produces it) and bookmarks via the real parser.
- **2026-06-12** — **E1 + E2 landed; 76 tests green
  (`cargo test -p eidetic --all-features`).** E1: `Store::delete_blob`
  (default-error method, fjall impl) + `manifest::delete_manifest`
  (manifest-only, blob bytes await GC — design pass §8);
  `eidetic::browsing` — the `BrowsingTrace` schema engram (mere-native
  payload + `bootstrap_browsing_schema`, the models pattern), `PageRef` /
  `TraceTransition` / `TraceEvent` as portable wire types,
  `save_trace` (always LocalOnly / SelfAsserted), and `BrowsingMemory`
  (per-owner open segments, flush-at-N, `recent_corridor`,
  `nodes_visited_in_window`, `co_occurrence` — defined narrowly as direct
  traversals either direction so it cannot drift from lineage's edge view —
  and `apply_quota` keep-N age-out, proven in tests); the lineage bridge as
  feature-gated `browsing::lineage::project_lineage` (snapshot + two
  extractor closures → per-owner chronological traces; reads, never
  duplicates). E2: workspace pin for `import`; the `eidetic-recall` example
  (embedded tests via `test = true`): `ingest-bookmarks` (real parser) /
  `ingest-history` (JSONL) / `corridor` / `window` / `co` / `stats` over a
  fjall store. Rehearsed end-to-end: both formats ingested, store durable
  across processes, corridor shows transitions + referrers chronologically.
  **Remaining for E2's done condition**: Mark's run over a real export
  (`cargo run -p eidetic --example eidetic-recall -- --db <dir> --owner
  <tag> ingest-bookmarks <file>`; the Firefox history one-liner is in the
  bin's doc header). Next slice: E3 (`eidetic-search`); the recall bin
  moves there so one bin carries the whole arc.
- **2026-06-12** — **E3 + E4 landed: `eidetic-search` with 15 tests green
  (11 lib + 4 example); whole family green (eidetic 72, fjall 7, import
  3).** E3: the `SearchIndexSpec` engram (typed payload + bootstrap +
  mintable via `save_spec`; carries tantivy version, fields version,
  tokenizer) doubling as a JSON **sidecar** in the index directory so
  `open` refuses before tantivy touches segments; `TrailIndex` (native-fs
  produce path, one document per traversal event; reserved fast columns
  domain/owner/at_ms/transition); BM25 `search` over title/url/domain; the
  re-mint contract proven in a test (drifted sidecar → `FormatMismatch` →
  `rebuild` from the corpus recovers). One tantivy 0.26 API drift from the
  plan's verified-against-0.26.0 notes: `TopDocs::with_limit` is now a
  builder finished by `.order_by_score()` — caught by compile. E4:
  `fuse` (reciprocal-rank fusion; engine-agnostic — the caller brings the
  vector ranking, so no embed dependency; weights a setting, proven
  steering in a test) and the fast-column reports (`top_domains`,
  `visits_histogram`) via tantivy's aggregations. The recall bin moved to
  `eidetic-search/examples/` and grew `index` / `search` / `report`;
  `search` auto-re-mints on format mismatch. Full-arc rehearsal: ingest
  (both formats) → mint (7 documents) → recall ("vello scene" and
  "tantivy" rank correctly across both sources) → report (correct domain
  counts + day histogram). Side fix in `import`: the Netscape parser
  dropped `ADD_DATE`/`LAST_MODIFIED` (hardcoded `None`) — now parsed, so
  bookmark events carry real times into the histogram. **Open for E4's
  full done-condition**: wiring a real vector ranking (embed) into a fused
  query happens at the first consumer (geist/shell), per the
  engine-agnostic seam rule. E5 (shell surfacing) stays gated on the
  window-composition reshape.
- **2026-06-12** — **E4 closed for real: the recall bin is the first fused
  consumer, both engines live.** Survey first corrected two stale beliefs:
  embed's BERT provider is *not* scaffold (full forward pass with pooling +
  L2, reference fixtures validated against the very model the repo carries
  at `models/all-MiniLM-L6-v2` — the Cargo.toml feature comment lags) and
  the design pass §6.1 migration is already done (`VectorIndex` persists
  through the typed layer with its own schema engram). So the bin grew
  `embed-index` (embed every distinct page on burn's CPU backend, batch 16,
  mint the `VectorIndex<String>` engram LocalOnly) and
  `recall <query>` (BM25 ranking + vector nearest → `fuse` → ranks shown
  per engine), with `--model-dir` defaulting to the repo's model checkout.
  Rehearsed: `"graphics rendering library"` — **zero keyword overlap with
  the corpus** — ranked vello then wgpu by the vector half alone (the
  semantic-recall payoff), while `"tantivy"` showed both engines agreeing
  on the same top two. 384-dim MiniLM, engram-persisted, 15 tests still
  green. (Ops note: a concurrent `cargo clean` in mere mid-build read as
  artifact corruption and ate `target/smoke/` — smoke fixtures are
  clean-vulnerable by design; re-made.) The geist/shell wiring of the same
  seam remains the consumer-side step it always was.
