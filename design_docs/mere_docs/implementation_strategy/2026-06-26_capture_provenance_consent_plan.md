# Capture, Provenance, and Consent Plan — one live record: where you went, what you chose among, where it came from, what may leave

**Date**: 2026-06-26
**Status**: C1 (live recorder) + C2 (candidate-context) **built + runtime-verified**
(2026-06-26); the relational-browse V1 **materializer trigger** (`>materialize`)
that lights C2 up is shipped + verified. C5 (page text into the index)
is **built + verified** (`>recall`, `8b8b039`); C3's materialize / crawl half
(harvested links record `ExtractedFrom` provenance) is **built** (`cdd2130`),
and the web-clip case now writes `ClippedFrom` provenance from `>clip`.
C4's membrane is **live**: the **consent gate** (`>capture`), **retention**
(`390d74a`), **forget** (`>forget`, traces + index, `00a5331`), and
federatability (the existing `PrivacyClass`) are built + verified. Remaining:
forget's provenance-edge cleanup, C3's excerpt / summarize / generated-node
provenance cases, and Phase 9
federation promotion/consumption. Created from the 2026-06-26 cross-cutting state
audit (crawl / engram / knot / federation / models / graph / documentscript),
which found that the left half of the browsing-data vision (browse, crawl,
extract, local index) is largely **built**, the right half (distill, federate,
tessera) is mostly **designed**, and the connective tissue between them, the live
capture record, is owned by no plan and written by no running code.
**Lane / conflict posture**: cross-cutting. The live recorder tap is **meerkat**
(navigation + crawl path, Mark's hot files, land in clean files); the record
shape extends **eidetic-core** engrams; the provenance writers are **graph-kernel**
edges asserted by meerkat/inker gestures; consent/retention is a **settings** +
eidetic concern. It does **not** re-derive the eidetic stack or the front-end.
**Relationship to existing plans** (this plan composes and connects them):
- [Eidetic Browsing Derivation](../../eidetic_docs/implementation_strategy/2026-06-12_eidetic_browsing_derivation_plan.md)
  — **built** (E1-E4): `BrowsingTrace` / `BrowsingMemory`, the `project_lineage`
  bridge, the `eidetic-search` tantivy index, hybrid recall. This plan supplies
  the **live caller** that E1 left to a consumer, answers E1's open question on
  trace granularity, and routes page text into E3's index (the parked
  "genet-side text-extraction seam" trigger, now satisfied by `genet-extract`).
- [Relational Browse Graphlet](2026-06-23_relational_browse_graphlet_plan.md)
  — V1 (single-hop materializer) + V2 (crawl actor) are **built**. Its **V3**
  (relational capture into eidetic) is **moved here** and generalized: V3 was the
  candidate-set enrichment alone; this plan unifies it with the live recorder,
  provenance, and consent so all four ride one record written from the first
  traversal.
- [Eidetic Deferred Phases](../../eidetic_docs/implementation_strategy/2026-06-09_eidetic_deferred_phases_plan.md)
  — Phase 9 **consume half** (federated index merge) reads the federatability
  class this plan's C4 sets.
- [Graph Projections Research](../research/2026-06-22_graph_projections_research.md)
  — the **Provenance-trail** projection consumes the edges C3 writes.
- [petgraph / RDF](2026-06-18_petgraph_rdf_plan.md) — RDF-star edge provenance is
  the *RDF projection* of the kernel provenance edges C3 writes; distinct work.
- [Communal Compute Tiers](../research/2026-06-10_communal_compute_tiers_brief.md)
  + [Geist Models Brief](../research/2026-05-10_geist_models_brief.md) — tessera
  contribution-pricing and the flora (federated-LoRA) lane both consume the
  provenance + federatability this plan produces.
- Privacy axis source: [Eidetic Design Pass](../../eidetic_docs/research/2026-05-09_eidetic_design_pass.md)
  (§8 privacy/quota, the three-axis classification).

---

## Thesis (why one record, and why now)

The audit's sharpest finding: the architecture is being built at both ends of the
pipe (relational browsing on the left, flora / tessera / constitution on the
right) while the membrane both ends depend on has no crate and no plan. Pull on
the loose threads and they are one layer:

- **Capture** assumes you may record. There is no consent gate, no incognito
  exclusion, no retention or forget path beyond `apply_quota`'s blunt keep-N.
- **Federation** assumes contributions are attributable (provenance) and legally
  shareable (a federatability policy). Neither exists.
- **Tessera** assumes a contribution can be priced by origin. That needs
  provenance.
- **Governance / legality** assumes the corpus knows where each row came from and
  whether it may leave. It does not.

These are not separate gaps. They are one missing record: *where you went, what
you chose among, where it came from, and what may leave this host.* And the audit
verified the harder truth: **nothing in the running app writes any `BrowsingTrace`
at all** (zero meerkat callers of `record_traversal` / `save_trace` /
`BrowsingMemory` / `project_lineage`). The eidetic sink and schema are built; the
live tap that feeds them is not. So even the plain chronological trail is not
being captured today, let alone the candidate-set, provenance, and consent fields
the right half of the vision depends on.

The ordering consequence is the whole reason this plan exists: **every one of
these fields is cheap at write time and unrecoverable in bulk afterward.** If the
live recorder lands carrying only the chronological traversal, retrofitting
candidate-context, provenance, and consent onto a year of accumulated traces is
the same expensive-later trap the relational plan flagged for candidate-sets
alone. So the record must carry all four from the first traversal, even where a
field starts empty.

This plan does **not** try to build distillation, the index-federation consume
half, or governance. It builds the one record those all read, and the live tap
that writes it.

---

## Phases (each independently landable; done conditions, not dates)

### C1 — The live trail recorder (the foundational tap)

Wire the meerkat-side recorder that persists a `BrowsingTrace` durably on every
navigation (tab-to-tab, link follow, crawl visit). Today the live visit memory
lives in `node-lineage` (in the graph) and `project_lineage`
(`eidetic-core/src/browsing/lineage.rs`) can project a lineage snapshot into
traces, but nothing in the running app calls it, so no durable trace is ever
written. C1 is the live caller plus a direct recorder where lineage does not carry
enough.

- A recorder on the navigation path that calls `record_traversal` / `save_trace`
  (`eidetic-core/src/browsing/mod.rs`), off the UI thread, never blocking
  compositing or input.
- **LocalOnly / SelfAsserted by default** (the existing three-axis classification),
  and it **honors an exclusion flag from day one** (an incognito / do-not-record
  session writes nothing). This is the cheap-now hook for C4.
- The record carries the C2 / C3 / C4 fields as present-but-optional from the
  start (candidate-context, provenance, consent-class), even before their
  producers land, so the schema does not need a migration later.

**Done when**: navigating in the running app produces durable `BrowsingTrace`
engrams in the eidetic store; an excluded session writes none; the recall path
(or a test) reads them back; the recorder runs off the render path. Files under
the 600-LOC ceiling; land in clean meerkat files alongside Mark's churn.

**Status: built + runtime-verified 2026-06-26** (commit `ac43edd`). `browse_capture.rs`
writes a single-event `BrowsingTrace` per navigation via `eidetic::browsing::save_trace`,
tapped in `nav_sync.rs` (`sync_orrery` forward navs + `drain_history_step` back/forward),
gated by a `Content.capture_enabled` flag (the C4 hook, default on), schema bootstrapped
at store open. 2 unit tests + a headed run (two navigations each logged "recorded a
browsing trace", no failures). Per-nav trace today; batching into segments + quota is
the C4 retention refinement.

### C2 — Candidate-context (the relational enrichment, absorbed from V3)

Capture the relational decision the bird's-eye view makes observable: the set of
candidates a choice was made against, and the decision (expand / dismiss / pin /
dwell). A non-clicked link emits no event in a tab funnel, which is exactly why
this needs the V1 neighborhood view as its observation point: the materialized
candidate set is the thing a dismiss is measured against.

- Define the candidate-context record explicitly: what a candidate set contains
  (the materialized neighborhood's link set with anchor + source context), and
  which interaction emits which decision. This answers the eidetic plan's open
  E1 question on trace granularity.
- Tap the relational-browse interaction (the V1 materializer / neighborhood
  gestures) to emit the candidate-context onto the C1 record.

Illustrative shape only (not compile-ready):

```rust
// illustrative-signature-only — the fields, not the final type
struct CandidateContext {
    against: Vec<CandidateRef>,   // the set the decision ranged over
    decision: Decision,           // Expand | Dismiss | Pin | Dwell(ms)
    chosen: Vec<CandidateRef>,    // the followed subset (the positives)
}
```

**Done when**: a relational-browse session over a materialized neighborhood
produces traces carrying the candidate set and the decision; the listwise
"real negatives in context" signal is present in the stored record, not
reconstructed; the schema round-trips.

**Status: built + verified 2026-06-26** (schema + capture `223ff4b`;
candidate-source fix + the `>materialize` trigger `831bdcf`). `TraceEvent` gained a
serde-default `candidates: Vec<PageRef>` (schema hash unchanged, old traces
round-trip; eidetic 72 + 15 tests green). The recorder fills it from the **focused
node's `Hyperlink` out-edges** (`candidate_links` by member id, not the stale
from-URL — a navigation has already advanced the node's URL by record time). So it
is populated once the page's neighborhood is **materialized or crawled** and empty
in plain browsing (the honest sparse property). The `Decision` enum
(dismiss / pin / dwell) is **not** built — the first slice records "followed `to`"
against the set; richer decisions need the neighborhood interaction UI. The V1
`>materialize` trigger (relational-browse V1's "thin remaining wire-up") was wired
here so C2 lights up; headed-verified end to end: navigate → `>materialize` →
navigate records candidates 0 → 1.

### C3 — Provenance-family writers (one mechanism, three payoffs)

The `Provenance` family (`ClippedFrom` / `ExcerptedFrom` / `SummarizedFrom` /
`ExtractedFrom` / `GeneratedFrom`, `graph-kernel/.../edge_taxonomy.rs`) was
defined and consumed by nothing live: the audit found only cross-graph
`CopiedFrom` (a node `derivation`) and snapshot replay assert it. Wire the live
gestures to write it.

**Built — materialize / crawl half (`cdd2130`):** `apply_contribution` now
records an `ExtractedFrom` `NodeDerivation` on the target of every harvested
`Hyperlink`, naming the source page. So a materialized / crawled node names the
page it was extracted from — the Provenance family's first live browse writer.
Unit-tested (`a_harvested_hyperlink_records_extracted_from_provenance_on_the_target`)
over the real `apply_contribution` path the materializer feeds.

Implementation note — a deviation from the "writes kernel edges" framing below:
for same-graph harvest we record a node **derivation** (the `CopiedFrom`
mechanism), not a `target -> source` kernel *edge*. An edge would sit in the
target's out-edges and pollute C2's `candidate_links` (a pure `Hyperlink` set); a
derivation is node-local and avoids that. The Provenance-trail projection reads
derivations anyway (it must, for `CopiedFrom`). A kernel *edge* representation is
reserved for the content-derivation gestures below, where no `Hyperlink` doubles
it.

**Built — web clip (`>clip`, 2026-06-29):** the djot/web-clip gesture now writes a
local `knot://clip/<uuid>` node, fills `Node.body` from `build_clip_knot(...)`,
opens the clip as a note tile, and asserts a kernel `ClippedFrom` provenance
relation from the clip node back to the source. Scriptable live surfaces arm a
click picker; non-surface nodes use the loaded/cached document body through the
same fragment -> knot path. The clip fragment can also carry a cropped visual,
stored as the clip node thumbnail/sprite. Unit coverage exercises fragment
parsing, fallback body parsing, cropped visual sizing, clip-node relation +
thumbnail writing, and knot provenance.

**Remaining:**

- Excerpt / summarize / future agent-generate: each gesture asserts the matching
  Provenance relation (`ExcerptedFrom`, `SummarizedFrom`, `GeneratedFrom`, etc.)
  as it produces a node. These are genuine content-derivation, so they warrant a
  kernel *edge* (nothing overlaps).

This is the single mechanism the relational plan's "provenance is one mechanism"
Finding names: it feeds tessera contribution-pricing, training-data legality, and
shared-index source legibility at once.

Boundary: carrying who / when / graph-scope on the provenance record in the *RDF
projection* (RDF-star) is the petgraph-RDF plan's Phase 1-2, not this plan. C3
makes the provenance exist; that plan makes it survive RDF export.

**Done when**: a harvest / clip / summarize gesture asserts a Provenance record a
test (or the Provenance-trail projection) can read; a harvested node can name the
page it came from. **(Met for materialize / crawl and clip; summarize / excerpt /
future generated nodes remain.)**

### C4 — Consent, retention, and forget (the membrane proper)

The privacy / legal layer the whole vision assumes and no plan owns.

- **Consent gate** — **built (`>capture off|corridor|full`).** A persisted
  `CaptureConsent` (off / corridor / full) governs whether the live recorder writes
  traces and at what granularity: `Off` records nothing, `CorridorOnly` keeps the
  navigation corridor but drops the candidate set (C2), `Full` records both. Set at
  runtime by the `>capture` omnibar verb (a bare `>capture` reports the level),
  persisted in `settings.json` (the proven crawl-setting path), enforced in
  `record_browse_nav`. Default is `Full` (preserves the pre-consent behaviour) — the
  opt-in-by-default posture is a one-line change to the default and remains Mark's
  product call (Open questions). Verified at runtime: under `off` a navigation is
  not recorded; under `full` it is, with its candidate set (`candidates=1`).
- **Retention** — **built + verified (`390d74a`).** `BrowsingMemory::apply_quota`
  (keep the N most recent traces, age out the rest; already unit-tested) now runs
  as a per-launch prune at store open, N configurable via `settings.json`
  (`retention_keep_n`, default 10000). Pinning is a node concept, not a trace one,
  so the "never deletes pinned" clause does not apply to traces. Bounding
  intra-session growth (a periodic pass) is a follow-on. Verified: the pass fires at
  startup over the live corpus.
- **Forget** — **built + verified (`00a5331`; `>forget`): traces + index.**
  `BrowsingMemory::forget_url` deletes every trace mentioning a url (from the store,
  the loaded corpus, and any open segment); `>forget` (bare = the focused page, or
  `>forget <url>`) drives it host-side. The `eidetic-search` index is rebuilt from
  the corpus per query, so a forgotten page also leaves the index. Unit-tested
  (`forget_url` removes both ends of an event, idempotent) and headed-verified on a
  scratch profile: browse example.com, `>recall documentation` hits, `>forget`
  reports "removed 1 trace(s)", then `>recall` returns "no trail captured yet".
  **Remaining sub-step**: retract the C3 `ExtractedFrom` provenance derivations on
  the page's node (a narrow, materialized-only case); the trace + index redaction
  (the bulk of "forget my visit") is done.
- **Federatability class** — **met by the existing `PrivacyClass` (no code).** The
  three-axis classification (`PrivacyClass` = who-may-see, `ProvenanceRecord`,
  `TrustEnvelope`/`TrustLevel`) already rides every engram. `save_trace` stamps every
  trace `PrivacyClass::LocalOnly` (+ `SelfAsserted`), asserted by a test
  (`browsing/tests.rs`). `PrivacyClass` (LocalOnly / TrustedPeersOnly / MootScoped /
  PublicPortable) is exactly the "may this leave the host, and to whom" axis the plan
  called for ("riding the existing axes"). Remaining (Phase 9, no consumer yet): a
  promotion path (LocalOnly -> wider) on an explicit share, and the flora lane
  reading the class. This is the hook for the legality policy (Open questions), not
  the policy itself.

**Done when**: a consent setting governs capture **(met — `>capture`)**; a forget
action removes a page's traces, index entries, and derived edges in one pass;
retention ages out beyond configurable N; every record carries a federatability
class defaulting to LocalOnly. **(Built + verified: consent gate, retention
[`390d74a`], and forget [`00a5331`, traces + index]; federatability met by the
existing `PrivacyClass`. C4's membrane is live; only forget's provenance-edge
cleanup, a narrow materialized-only case, remains.)**

### C5 — Page text into the index (the fired trigger)

Close the hop both the eidetic plan and the relational plan describe as the
*other's* trigger. `genet-extract::extract_text` / `main_text` (the producer)
is built; `eidetic-search` indexes only titles / URLs / domains (the consumer);
the connecting call lives in no crate. The trigger ("a genet-side text-extraction
seam") has fired; the consuming work was never re-queued.

- Route `extract_text` / reader-mode `main_text` from the browse / crawl path into
  `eidetic-search` so the index carries page text.
- Index respects the C4 consent + federatability class (do not index what consent
  excludes; mark federatability on the indexed doc).

**Done when**: a browsed or crawled page's main text is indexed and BM25-recallable
through the existing recall path; an excluded page is not indexed; the index lane
(the one the vision says "leads") federates rich text rather than bare titles.

**Status: built + verified 2026-06-28** (commit `8b8b039`). eidetic-search gained a
`text` field (fields v2) + a `rebuild_with_text` variant; meerkat stands up the
trail index live and answers a `>recall <terms>` omnibar verb — it loads the trace
corpus, pulls each page's `main_text` from the durable content cache
(`StaticDocument::parse` → `genet-extract`), re-mints the index with the text, and
echoes the top BM25 hits. Stands up eidetic-search as a live in-app surface (it was
dev-bin only). Headed-verified: `>recall documentation` (a term only in the page
body, not the title or URL) returns the page. Caveats: the index rebuilds per query
(incremental maintenance + a results pane are follow-ons), and **consent-gating of
what is indexed is C4** — the "an excluded page is not indexed" clause lands there.

---

## Boundary with existing plans (what this absorbs vs cross-links)

- **Absorbs** relational-browse **V3** (candidate-set capture) and generalizes it
  into C1-C4. The relational plan's V3 section now points here.
- **Activates** the eidetic plan's parked **full-page-text capture** (C5) and its
  **E1 trace-granularity open question** (C2).
- **Cross-links, does not build**: distillation / LoRA (geist brief), the Phase 9
  consume half, RDF-star edge provenance (petgraph-RDF), the constitution /
  governance primitive (moot constitution brief), tessera pricing. Each reads this
  plan's output; none is in scope here.

---

## Findings (audit-verified, 2026-06-26)

- **RESOLVED (C1, `ac43edd`).** Was: no live trace writer (zero callers of
  `save_trace` etc.). Now `browse_capture.rs` writes a `BrowsingTrace` per
  navigation via `save_trace`, tapped in `nav_sync.rs`; runtime-verified.
- **RESOLVED (C2, `223ff4b`).** Was: `TraceEvent` chronological only. Now it carries
  a serde-default `candidates: Vec<PageRef>` (the listwise set), filled from the
  focused node's out-edges.
- **The `Provenance` edge family has no live writer.** It appears only in the
  taxonomy enum, snapshot round-trip, and a label-formatting match arm; only
  cross-graph copy and snapshot replay assert it. This is C3.
- **No consent / incognito / retention / forget anywhere in eidetic.** Only
  `PrivacyClass` routing and `apply_quota` keep-N exist. This is C4.
- **The text-extraction seam fired.** `genet-extract::extract_text` / `main_text`
  is built and render-free; `eidetic-search` still indexes only titles / URLs.
  This is C5.
- **`net.fetch` is no longer a stub** (relevant because the crawl recorder rides
  the fetch path): `ContentNetFetcher` is a real backend over `fetch_page`,
  origin-gated, SSRF-floored, rate-capped; `document-host` tests green. Earlier
  "stub" framing in the relational and substrate plans was same-day-stale and is
  superseded by the documentscript net-hardening plan.
- **Naming resolved 2026-07-12**: `fauna` is the Moot's catalog of accumulated
  engram CID references (`MootRoster.fauna`); `flora` retains its established
  federated-LoRA meaning. Federatability class C4 and adapter-engram schemas can
  now name the two lanes without date-dependent interpretation.

---

## Open questions (Mark's calls)

- **Consent default.** Off, corridor-only, or full capture by default? The vision
  wants a rich corpus; the privacy posture wants opt-in. This is a product
  decision the membrane must encode, not assume.
- **Legality policy.** Provenance edges say *where a row came from*; they do not
  say *whether you may federate a distillation of it.* C4 provides the
  federatability hook; the policy that sets it (crawled third-party page text:
  local-only, or federatable under what terms?) is unowned and is a product /
  legal call, possibly its own later slice.
- **Candidate-set observability** (partly settled). C2 captures candidates from the
  focused node's graph out-edges, so the signal is present once a page is
  materialized/crawled and empty in plain browsing. Open: is "saw in a normal page,
  did not click" worth capturing too (would need re-extracting the page's links at
  nav time via `genet-extract`, not just reading graph out-edges)? And the richer
  `Decision` (dismiss / pin / dwell, vs the current "followed `to`") needs the
  neighborhood interaction UI.
- **Recorder granularity vs `node-lineage`.** C1 must not let the durable trace
  drift from `node-lineage`'s live edge views or the eidetic `co_occurrence`
  definition; decide what C1 records directly vs projects from lineage.

---

## Progress

- **2026-06-26** — Plan created from the cross-cutting state audit. The audit
  established (against code, not docs) that the live capture record is the keystone
  the larger plan is missing: it sits under capture and under federation
  simultaneously, must be threaded from the first traversal, and is written by no
  running code today. Sequenced C1 (live recorder) first because it is the
  foundation and the free-now / unrecoverable-later piece; C2-C4 ride the same
  record; C5 closes the fired text-extraction trigger. Absorbs relational-browse
  V3. Same session, corrected the stale `net.fetch` "stub" framing and the
  resolved "one missing primitive" framing in the relational-browse plan (V1/V2
  are built; `genet-extract::extract_links` is the primitive). No code yet.
- **2026-06-26 (C1 built + verified, `ac43edd`).** The live recorder:
  `browse_capture.rs` (per-nav `save_trace`, `capture_enabled` gate, schema
  bootstrap at store open), tapped in `nav_sync.rs` (`sync_orrery` +
  `drain_history_step`). 2 unit tests; headed run logged "recorded a browsing
  trace" for two navigations, no failures. Reachable only through the live tap;
  reads (recall/corridor) stay E5.
- **2026-06-26 (C2 built, `223ff4b`).** Candidate-context: serde-default
  `candidates` on the eidetic `TraceEvent` (no schema-hash change, old traces
  round-trip), filled from the focused node's out-edges. eidetic 72 + 15 + meerkat
  2 tests green. Found + noted the sparse property (empty until materialize/crawl).
- **2026-06-26 (materializer trigger + C2 verified, `831bdcf`).** Wired
  `Command::MaterializeFocused` / `>materialize` (relational-browse V1's trigger,
  mirroring `>crawl`) and fixed `candidate_links` to read the focused node by member
  id (the stale-from-URL bug). Headed run proved the loop: navigate → `>materialize`
  → navigate records candidates 0 → 1. **Audit (this entry):** C1 + C2 done +
  verified; relational-browse V1 is now fully reachable. Open lanes unchanged: **C5**
  (route `genet-extract::extract_text` into `eidetic-search` — both ends built, the
  connecting call is the smallest next win), **C3** (Provenance-family edge writers),
  **C4** (consent / retention / forget + federatability), and a richer `Decision`
  model when a neighborhood interaction UI exists.
- **2026-06-28 (C5 built + verified, `8b8b039`).** Page text into the index:
  eidetic-search gained a `text` field + `rebuild_with_text` (fields v2; body-text
  test), and meerkat a `>recall <terms>` omnibar verb that re-mints the trail index
  from the trace corpus + each page's `main_text` (from the durable content cache)
  and echoes BM25 hits — standing up eidetic-search as a live in-app surface (it
  was dev-bin only). Headed-verified a body-only term ("documentation") recalls the
  page. Remaining: **C3** (provenance writers), **C4** (consent / retention /
  forget — also gates what `>recall` indexes), and index optimizations (incremental
  update, a results pane). The membrane's **left half** (capture → relational
  signal → page-text recall) is now live and proven; C3 + C4 are the right half.
- **2026-06-28 (C3 materialize / crawl half built, `cdd2130`).** Harvested links
  carry provenance: `apply_contribution` records an `ExtractedFrom`
  `NodeDerivation` on the target of every `Hyperlink` it asserts, naming the
  source page — the Provenance family's first live browse writer (only cross-graph
  `CopiedFrom` wrote it before). Recorded as a node derivation, not a kernel edge,
  so it stays out of C2's `candidate_links` (kept a pure `Hyperlink` set). New
  `Graph::record_derivation`; unit-tested over the real `apply_contribution` path.
  No headed run — derivations have no UI surface yet; the test covers the exact
  function the materializer feeds, and the materialize → `Hyperlink` half was
  already verified (`831bdcf`). Remaining: **C3** clip / summarize (kernel *edges*,
  waiting on the web-clip gesture) and **C4** (consent / retention / forget).
- **2026-06-28 (C4 consent gate built + verified).** The privacy enforcement point:
  a persisted `CaptureConsent` (off / corridor / full) replaces C1's hardcoded
  `capture_enabled`. `record_browse_nav` honours it — `Off` records nothing,
  `CorridorOnly` keeps the corridor but drops the candidate set, `Full` records both.
  A `>capture` omnibar verb sets it (bare `>capture` reports the level), persisted via
  the proven settings path; `CaptureConsent` lives in the bin while the lib shell
  carries only the string key (the bin/lib module split). Unit test for the enum +
  granularity; headed-verified the effect (one trace recorded for the `full` nav,
  none for the `off` nav, `candidates=1`) and all three verb echoes. Default stays
  `Full` (no behaviour change); opt-in-by-default is a one-line product call.
  Remaining C4: retention, forget, federatability class.
- **2026-06-28 (C4 retention built + verified, `390d74a`; federatability already met).**
  Retention: `BrowsingMemory::apply_quota` (keep-N, already unit-tested) now runs as
  a per-launch pass at store open, N from `settings.json` (`retention_keep_n`,
  default 10000); headed-confirmed the pass fires at startup (`nothing aged out
  keep_n=10000` over the live corpus, a safe no-op below the cap). Pinning is a node
  concept, N/A to traces. Federatability: found already satisfied — `save_trace`
  stamps every trace `PrivacyClass::LocalOnly` (the existing three-axis
  classification; `PrivacyClass` is the who-may-see / may-it-leave-the-host axis),
  asserted by `browsing/tests.rs`. No new code; a promotion path + the flora-lane
  read are Phase 9. Remaining C4: **forget** (traces + index + provenance in one
  pass).
- **2026-06-28 (C4 forget built + verified, `00a5331`: traces + index).** `>forget`
  (bare = the focused page, or `>forget <url>`) deletes every browsing trace
  mentioning the page via `BrowsingMemory::forget_url` (store + corpus + open
  segments); the `eidetic-search` index is rebuilt from the corpus per query, so the
  page leaves the index too. Unit-tested (removes both ends of an event, idempotent)
  and headed-verified on a scratch profile: browse example.com -> `>recall
  documentation` hits -> `>forget` ("removed 1 trace(s)") -> `>recall` returns "no
  trail captured yet". The verb mirrors `>capture` / `>recall` (shell_eval + drain).
  Remaining forget sub-step: retract the C3 `ExtractedFrom` provenance derivations on
  the page's node (a narrow, materialized-only case). With this, C4's membrane
  (consent + retention + forget + federatability) is live.
- **2026-06-28 (C4 membrane live; capture/provenance/consent membrane built end to
  end).** This stretch closed C4: federatability required no code because the
  existing `PrivacyClass` is the leave-the-host axis; retention (`390d74a`) prunes
  per launch from `settings.json`; forget (`00a5331`) removes traces and drops the
  page from `>recall`. Across the plan, C1, C2, the materializer trigger, C5, C3's
  `ExtractedFrom` harvest provenance, and C4 are live. Remaining: forget's
  provenance-edge cleanup, C3 clip / summarize (the web-clip gesture was the next
  C3 slice), and Phase 9 federation (promotion plus the flora consumer).
- **2026-06-29 (C3 web clip built).** `>clip` now creates a local
  `knot://clip/<uuid>` node from a semantic live-surface pick or from a focused
  non-surface document body, writes `Node.body` with `build_clip_knot(...)`, and
  asserts `ProvenanceSubKind::ClippedFrom` from the clip node back to the source.
  Cropped visual clips are stored as thumbnail/sprite data on the clip node. This
  closes the C3 canonical clip case; remaining C3 cases are excerpt / summarize /
  future agent-generated nodes.
