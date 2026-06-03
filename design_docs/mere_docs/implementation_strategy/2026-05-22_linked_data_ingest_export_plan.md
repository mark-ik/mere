# Linked-Data Ingest / Export Plan (JSON-LD)

**Date**: 2026-05-22
**Status**: Proposed. Foundational decision agreed (open the Semantic predicate);
phases not yet started.
**Scope**: **Ingest** linked data from the web into the graph, and **export**
Mere's graph as linked data. Federation / identity (VCs, DIDs, ActivityPub) and
RDF canonicalization-for-signing are **out of scope** (the JSON-LD footgun lives
there; revisit only if a federation tier demands it).

---

## Findings

### The core decision: closed families, open predicate

> This plan is the **first instance** of the
> [statements-over-schema stance](../technical_architecture/2026-05-22_statements_over_schema_stance.md).
> See that doc for the general principle (open statement substrate + curated
> behavioral lens) and the guardrail against over-applying it.

Today the kernel relation taxonomy (`graph-kernel/src/graph/edge_taxonomy.rs`)
is **fully closed**:

- `EdgeFamily` — 6 closed variants: `Semantic`, `Traversal`, `Containment`,
  `Arrangement`, `Imported`, `Provenance`.
- Each family's data struct holds a `BTreeSet<*SubKind>` of closed `Copy` enums.
  `SemanticData` additionally has `label: Option<String>` (a *display* label).
- There is **no predicate-IRI field anywhere**. `SemanticSubKind` is 17 curated
  variants (`Cites`, `Quotes`, `Contradicts`, `DependsOn`, `NextStep`, …);
  `Imported` is import-*source* provenance (`BookmarkFolder`, `HistoryImport`,
  `RssMembership`, …), **not** an arbitrary-predicate escape hatch.

The closedness is a curated **behavioral** ontology inherited from graphshell —
it buys exhaustive `match` for dispatch, cheap `Copy`/`BTreeSet` storage, stable
archival, and editorial intent. None of that is a structural requirement. It is
closed because nothing needed it open yet.

Linked data needs it open. RDF/JSON-LD predicates are an *open* IRI space; the
web's relations are unbounded (`schema:author`, `foaf:knows`, `schema:cites`, …).
Folding them into the nearest of 17 closed buckets is lossy and can't round-trip.

The resolution separates two axes the schema currently conflates:

1. **`EdgeFamily` = behavioral category** → stays **closed and tiny**. It is how
   the engine decides *how to treat* an edge (nest in layout? temporal event?
   workbench geometry? "just meaning"?). The web doesn't supply these; we derive
   them. Exhaustive matching here is correct.
2. **The predicate within a family = identity / meaning** → goes **open**.
   `SemanticSubKind` is "what kind of meaning," and JSON-LD predicates are
   exactly that. The closed enum is really a *curated recognition list*.

This is how linked data already works — an open predicate space with a
recognized core (schema.org *is* "open space + recognized vocabulary"). Opening
the Semantic predicate converges Mere with the web's model, which is what makes
ingest/export lossless instead of a lossy fold.

**Concretely:** `SemanticData` gains an interned predicate IRI alongside its
`sub_kinds`. Recognized predicates map to a small **Mere vocabulary** of
canonical IRIs *and* keep their `SemanticSubKind` (so existing behavior
dispatch is unchanged); unrecognized predicates carry only the raw interned IRI.
**`sub_kinds` stays authoritative for behavior; `predicate` carries identity and
round-trip fidelity.**

> **Curation vs noise.** Won't the graph drown if every micro-predicate becomes
> an edge? No — **lossless storage, curated presentation.** Keep every predicate
> in the model; let the orrery default to surfacing only recognized predicates
> and dim/collapse the rest. That's a view-layer filter, not a reason to discard
> data at ingest.

### Why the blast radius is small (grounded in the code)

- **Additive, not a rewrite.** We *add* a `predicate` field; `SemanticSubKind`
  stays. Existing matches keep compiling.
- **Consumers iterate `sub_kinds`.** The behavior-dispatch sites
  (`facet_projection`, `graph-layout/adapters/radial`, `platen/canvas_scene`,
  `session-runtime/switcher_thumbnail`) iterate the sub-kind set. An unrecognized
  raw-IRI edge has an **empty** `sub_kinds` → it is naturally skipped / rendered
  generically. So lossless storage + curated presentation falls out *for free*;
  near-zero forced consumer changes.
- **Persistence is a DTO layer, already serde-default.** Edges persist via
  `Persisted*` DTOs (`graph/snapshot/{to,from}.rs`), and
  `PersistedSemanticEdgeData` already marks **every** field `#[serde(default)]`
  (precedent: `agent_decay_progress: Option<f32>`). The live store is
  `graph.json` via `serde_json` (`session_graph_store`). So adding
  `#[serde(default)] predicate: Option<String>` to the DTO is **backward
  compatible** — old graphs load with `predicate = None`. Store the IRI *string*
  in the persisted form (session-local intern symbols aren't stable); re-intern
  on load.

### Model mismatches to decide up front

- **Node identity.** JSON-LD `@id` (an IRI) → a node's URL/address when
  dereferenceable. Blank nodes (`_:b0`) are document-scoped → **skolemize**
  (mint a stable Mere IRI) rather than emit blank nodes on export.
- **Literals have nowhere general to go.** `Node` has `title` / `tags` /
  `classifications` / … but **no key→value property bag**. So JSON-LD literal
  properties (`name`, `datePublished`, …) map to a **curated few** existing
  fields (`schema:name` → `title`, `schema:keywords` → `tags`) and otherwise
  drop. A general property bag is a separate kernel change — **deferred**.
- **No remote contexts.** Ship a **bundled local `@context` cache** for the
  vocabularies we support (schema.org, Dublin Core, ActivityStreams) and refuse
  remote fetches. Kills both the wasm-weight problem and the surprise-network /
  trust problem. Expansion-with-fixed-local-contexts is a small slice of a full
  JSON-LD processor.

### Placement (proposed, open for review)

- **Kernel owns** the open predicate field + interning + the canonical-IRI
  vocabulary for recognized kinds (it's a kernel schema change and kernel owns
  the relation vocabulary).
- **A new portable crate `linked-data`** owns JSON-LD I/O (expansion, bundled
  contexts, ingest → graph contribution, export ← graph). Deps `kernel` + a
  JSON-LD expansion lib; must stay wasm-light. *Alternative considered:* a module
  in `inker` (ingestion controller) — rejected for now because linked-data
  enrichment is a graph output, not document rendering, and export has no inker
  role. Flag for Mark: dedicated crate vs inker module.

### JSON-LD processor: prior art + choice (resolves open question 1)

Ingest (Phase 2) needs JSON-LD **expansion**: apply `@context`, resolve
terms/CURIEs to full IRIs, yield triples. Surveyed against Mere's constraints —
wasm-light, **no surprise network**, synchronous, output that maps onto the graph
kernel:

- **`oxjsonld`** (Oxigraph's JSON-LD parser; 0.2.5, MSRV 1.87) — **recommended.**
  Synchronous; pure-Rust with minimal deps (`json-event-parser`, `oxiri`,
  `oxrdf`, `ryu-js`, `thiserror`; `tokio` optional, stays off). Emits `oxrdf`
  triples. Remote-context loading is **opt-in** through a
  `JsonLdLoadDocumentCallback`: supply one that serves the bundled contexts
  (schema.org, Dublin Core, ActivityStreams) and refuses everything else, and the
  "bundled local cache, no remote fetch" requirement falls straight out of the
  seam with no HTTP dependency in the tree. wasm-clean.
- **`json-ld`** (Haudebourg; ~0.5) — full JSON-LD 1.1 (expansion / compaction /
  flattening) via the `JsonLdProcessor` trait and an async `Loader`. Most
  correct, but **async** (extra wasm ceremony) and a large crate family
  (`json-ld-syntax` / `-core` / `-context-processing`). Overkill for
  expand-with-fixed-contexts; revisit only if compaction or flattening is needed.
- **Hand-rolled expander** — for the bundled-context subset, expansion is
  term→IRI mapping; smallest footprint, but reinvents the CURIE / `@base` /
  `@vocab` / value-object edge cases `oxjsonld` already handles. Not worth it now
  that a lean sync parser exists.

**Prior art (the NextGraph thread).** NextGraph — "convergence of P2P + Semantic
Web on CRDTs" — built its RDF stack on **Oxigraph**, forked as `ng-oxigraph`
(adds CRDTs to RDF/SPARQL plus an encrypted RocksDB backend). That independently
validates the Oxigraph family as the Rust RDF substrate. Mere takes only the
**parser** (`oxjsonld` + `oxrdf`), not the triplestore — the kernel is the store.
The deeper NextGraph convergence (RDF-CRDT, SPARQL over the graph) is prior art
for the **federation tiers** (murm / moot), not this effort; SPARQL stays
"buildable on the predicate substrate," deferred. `sophia` (general RDF toolkit)
is the other option but adds no advantage over the Oxigraph parser here.

---

## Plan

Phases are ordered to put the riskiest/most-foundational kernel change first,
then the cheaper direction (export) before the parsing-heavy one (ingest). Each
has a done-condition, not a date.

### Phase 0 — Open the Semantic predicate (kernel)
- Add an interned `predicate` to `SemanticData` (symbol → IRI; symbol table
  lives with the graph/session). Recognized `SemanticSubKind` ↔ canonical Mere
  vocabulary IRI, **both directions**.
- Add `#[serde(default)] predicate: Option<String>` to
  `PersistedSemanticEdgeData`; update `snapshot/{to,from}.rs`. Persist the IRI
  string; re-intern on load.
- **Done:** old `graph.json` loads unchanged (`predicate = None`); a recognized
  sub-kind round-trips kind ↔ IRI ↔ kind; a raw IRI survives a save/load;
  existing kernel + consumer tests stay green.
- **Landed 2026-05-31.** `SemanticData.predicate: Option<String>` (raw IRI string,
  **not yet interned** — interning stays open question 2); `predicate_iri` /
  `sub_kind_from_iri` canonical-vocabulary helpers (`REL_VOCAB`; recognized
  sub-kind ↔ IRI both ways, tested for all 17 sub-kinds); `EdgePayload::set_semantic_predicate`;
  `PersistedSemanticEdgeData.predicate` (`#[serde(default)]`) with
  `snapshot/{to,from}.rs` round-trip (restored via `find_edge_key` /
  `get_edge_mut`, so no `EdgeAssertion` sweep). Old graphs load with `None`;
  kernel suite **228/228** green. **Open-predicate gap closed 2026-06-01:** a raw
  predicate-*only* edge (empty `sub_kinds`) now asserts via
  `Graph::assert_semantic_predicate`, reports the `Semantic` family (`has_family`
  reads the predicate, not just sub-kinds), and round-trips through
  `snapshot/{to,from}.rs`. Kernel **234** green.
- **First consumer (2026-06-01):** the knot statements ingest (`inker::statements`;
  knot design §10.5 Phase 4). A djot link's `rel`
  (`[Topic](mere://node/topic){rel=cites}`) becomes a `Semantic` edge: `resolve_rel`
  recognizes a bare slug *or* full Mere IRI, `apply_link_statements` asserts the
  sub-kind and stamps the canonical predicate via `set_semantic_predicate`. It
  edges only *existing* targets (node creation is host/wasm-gated) and returns a
  raw / CURIE `rel` as `unrecognized` — exercising, and confirming the user-facing
  need for, the raw-predicate-edge path this plan defers to Phase 2.

### Phase 1 — Export (graph → JSON-LD)
- New `linked-data` crate. `to_jsonld(subgraph) -> JSON-LD`: nodes → objects
  with `@id` (URL, or skolemized), `@type` from tags/classification; edges →
  predicates (recognized → canonical IRI, raw → verbatim IRI); curated literals
  (`title` → `schema:name`, …).
- **Done:** a seeded graph emits valid expanded JSON-LD; a golden test pins the
  output; recognized + raw predicates both appear correctly.
- **Landed 2026-06-01.** New `linked-data` crate
  (`crates/graphshell/graph/linked-data`, deps `kernel` + `serde_json`).
  `to_jsonld(&Graph) -> serde_json::Value` emits expanded JSON-LD (array of node
  objects, full IRIs, no `@context`): `@id` is the primary-address URL or a
  skolemized `urn:uuid:` IRI; a node's `Semantic` edges become predicate IRIs —
  an explicit open predicate verbatim, else each sub-kind via `predicate_iri`;
  curated literals `title` → `schema:name`, `tags` → `schema:keywords`. Reads the
  graph through the public surface (`nodes` / `out_neighbors` / `find_edge_key` /
  `get_edge` / `semantic_data`), so the kernel was untouched. Deterministic
  (sorted tags + targets, `BTreeMap` predicate keys); a golden test pins the
  recognized-IRI + raw-IRI + literals output, plus an empty-graph case.
  **Deferred from this slice:** `@type` from classifications (needs a class-IRI
  scheme, like `REL_VOCAB` for types — avoided inventing vocabulary now); only the
  `Semantic` family is exported (the other families are the experience layer).

### Phase 2 — Ingest (standalone `application/ld+json` → graph)
- `from_jsonld(bytes) -> GraphContribution { nodes, edges }` using **bundled
  local contexts**, no remote fetch. `@id` → node identity (skolemize blanks);
  predicates → `Semantic` edges (recognized → subkind **+** IRI; else raw IRI);
  literals → curated fields/tags or drop.
- Wire as an inker-routed input for `application/ld+json` (a graph-contribution
  output, **separate from** `EngineDocument`/render).
- **Tooling:** parse with `oxjsonld` (sync, wasm-light) behind a bundled-context
  load callback; map the resulting `oxrdf` triples to `GraphContribution` (see the
  JSON-LD-processor finding).
- **Kernel prerequisite — done 2026-06-01.** A raw predicate lands as a
  `Semantic` edge with **empty `sub_kinds`** via
  `Graph::assert_semantic_predicate`; such an edge reports the `Semantic` family
  and round-trips through snapshots. Ingest can now assert recognized rels
  (sub-kind + canonical IRI) *and* raw/CURIE rels (predicate only). The matching
  consumer wiring — pointing `inker::statements`' `unrecognized` path at
  `assert_semantic_predicate`, plus CURIE→IRI expansion — is the first ingest
  task.
- **Done:** a sample JSON-LD doc yields the expected nodes + typed edges; an
  unknown predicate lands as a raw-IRI `Semantic` edge with empty `sub_kinds`; a
  recognized one also sets its sub-kind; a literal maps to `title`/`tags`.
- **Core landed 2026-06-01** (`linked-data::ingest`). `from_jsonld(&[u8]) ->
  Result<GraphContribution, IngestError>` parses with `oxjsonld` (sync): a
  resource predicate → `EdgeContribution`, `rdf:type` → a node type,
  `schema:name` / `schema:keywords` → title / tags (other literals dropped),
  blank nodes skolemized to `urn:mere:bnode:`. `apply_contribution(&mut Graph,
  …)` (native; `add_node` is wasm-gated) creates a node per subject/object and
  asserts each edge — recognized predicate → typed sub-kind + canonical IRI,
  unrecognized → `assert_semantic_predicate` (the open-predicate kernel path).
  Done condition met for expanded / inline-`@context` documents; 2 tests (parse +
  materialize). Deps `oxjsonld = "0.2"`, `oxrdf = "0.3"`.
- **Loader mechanism landed 2026-06-01.** `ContextCache` (a URL → context-bytes
  map) + `from_jsonld_with_contexts(bytes, cache)` wire oxjsonld's
  `for_slice(..).with_load_document_callback(..)`: a registered remote `@context`
  is served from the cache and anything else is refused (no network). Tested both
  ways (resolve + refuse). **Still open there:** the context *assets* — embed the
  real schema.org / Dublin Core / ActivityStreams context JSON (a bundle-size /
  which-vocabularies call) versus a minimal curated context. The cache takes
  either; populating it is the decision. **Consumer note:** the curated
  kernel-vocab context has no consumer yet. Its first would be **compacted
  JSON-LD export** (the ergonomic dual of expanded `to_jsonld`, using the context
  to shorten output); the same context is also the **interop seam** that aliases
  Mere's `rel#` IRIs onto standard vocabularies (CiTO / schema.org) when Mere
  publishes linked data. Both are downstream, so the context stays deferred.
- **inker routing landed 2026-06-01.** `application/ld+json` routes to
  `ENGINE_LINKED_DATA_INGEST` (`"linked-data.ingest"`), a **Headless marker route
  target** (not a render engine), with `is_graph_contribution_route()` to
  recognize it — the same host-handled pattern as `ENGINE_EXTERNAL_PROTOCOL`. A
  host special-cases this decision to feed the body to `from_jsonld` instead of
  dispatching an engine. 1 routing test; inker **73**.
- **Host consumer landed 2026-06-01** (`mere-app`). `navigation::resolve(address,
  &mut Graph)` classifies via the *unfiltered* policy (the marker is not a
  registered engine, so a registry-filtered route would skip it), and on a
  graph-contribution route parses the body with `from_jsonld` + merges it with
  `apply_contribution`; `navigate` / `back` / `forward` then rebuild the orrery
  scene and persist. A seeded `mere://demo.jsonld` (plus local `.jsonld` files)
  makes it exercisable: opening it merges two papers and a `cites` edge into the
  live graph. mere-app **19**. linked-data is no longer inert — the fetch → route
  → ingest → merge loop runs end to end.
  - **Gated decisions made (light path):** standalone `application/ld+json` only;
    native `apply_contribution` (a wasm host materializes via `add_node_with_id`);
    inline / expanded `@context` only (remote-context bundling deferred);
    re-ingest idempotent by URL; ingested nodes seed at the origin and the
    orrery's layout spreads them.
  - **Gated decisions resolved 2026-06-02:** `@type` → `rdf:type` classifications
    (`8190220`); a kernel literal **property bag** `Node.properties` so
    non-curated literals survive ingest/export round-trip (`1f47b4d`); **HTML
    harvest** — `linked-data::from_html` + the host harvesting embedded JSON-LD
    while rendering (`c957164`); blank-node skolemization scoped per document
    (oxjsonld already assigns unique labels, so collision was a non-issue; full
    idempotency needs canonicalization, deferred) (`8190220`); **context presets**
    `ContextCache::full` / `minimal` / `new` + a curated Mere context (`082de79`).
  - **Still open:** vendoring the standard-vocabulary context assets (schema.org
    ≈ 207 KB, CC-BY-SA into an MPL repo — a size / licensing call) so `full()` is
    truly full; the wasm materialization path.

### Phase 3 — Round-trip + coverage
- Ingest → export → compare, modulo the by-design drops (uncurated literals).
- Recognition-table coverage test (every `SemanticSubKind` ↔ a canonical IRI).
- **Done:** round-trip is stable for the recognized vocabulary + raw predicates.
- **Landed 2026-06-01.** `to_jsonld_compact(&Graph)` emits compacted JSON-LD
  (`@graph` under an inline `@context`): a recognized relation + the curated
  literals become short terms backed by the context (term → IRI, built from
  `predicate_iri` / `sub_kind_from_iri`, so it cannot drift); a raw predicate
  keeps its full IRI as the key (the open tail stays explicit). This is the
  curated kernel-vocabulary context's **first consumer**. Round-trip tested both
  ways: a seeded graph → `to_jsonld` *and* `to_jsonld_compact` → `from_jsonld`
  yields the same logical content (A's literals, the `cites` edge at its canonical
  IRI, the raw `schema:citation` edge). linked-data **9**. The full recognition
  table (all 17 `SemanticSubKind` ↔ IRI) is covered at the kernel level (Phase 0);
  linked-data round-trips a representative recognized + raw pair.

### Deferred (explicit non-goals for this effort)
- HTML `<script type="application/ld+json">` / microdata extraction — needs
  Serval/HTML parsing; do after standalone-resource ingest proves the seam.
- General node **property bag** for full literal fidelity (separate kernel
  change).
- JSON-LD framing, RDF canonicalization (URDNA2015/RDFC-1.0), signing,
  federation / VCs / ActivityPub.

---

## Open questions

1. **JSON-LD lib:** **Resolved 2026-06-01 → `oxjsonld`** (Oxigraph's sync,
   wasm-light JSON-LD parser; opt-in bundled-context loader; the
   NextGraph-validated Oxigraph family). See the JSON-LD-processor finding above
   for the alternatives weighed (`json-ld`/Haudebourg, `sophia`, hand-rolled).
2. **Interning home:** per-graph vs per-session symbol table for predicate IRIs.
3. **Crate placement:** **Resolved 2026-06-01 → dedicated `linked-data` crate**
   (landed at `crates/graphshell/graph/linked-data`). The knot statements ingest
   is the document-derived exception and lives in `inker::statements`.
4. **rkyv snapshot path:** the live path is `graph.json` (serde, trivially
   migratable). The compact rkyv/redb snapshot path (`persistence.rs`) needs a
   field-addition check if/when it's the active store — confirm before relying on
   it for linked-data graphs.

---

## Progress

- **2026-05-22** — Plan created. Verified against the code: taxonomy is fully
  closed (no predicate slot); `SemanticData = { sub_kinds: BTreeSet<…>, label }`;
  persistence via `Persisted*` DTOs with all-`#[serde(default)]` fields and a
  `graph.json`/`serde_json` live path (→ additive, backward-compatible
  migration); `Node` has no general property bag; `SemanticSubKind` consumers
  iterate the sub-kind set (→ unrecognized predicates render generically, ~zero
  forced consumer churn). No code written yet.
- **2026-06-01** — Phase 1 (export) landed in the new `linked-data` crate (see
  the Phase 1 note). Phase 2 researched: open question 1 resolved to `oxjsonld`
  (sync, wasm-light, opt-in bundled-context loader), corroborated by NextGraph's
  Oxigraph-based RDF stack; written up in the JSON-LD-processor finding. Surfaced
  the Phase 2 **kernel prerequisite** — raw-predicate-only `Semantic` edges plus
  their persistence round-trip. Open questions 1 and 3 closed; 2 (interning) and
  4 (rkyv path) remain.
- **2026-06-01 (later)** — Phase 2 **kernel prerequisite done**:
  `EdgePayload::has_family(Semantic)` now counts a predicate-only sidecar;
  `Graph::assert_semantic_predicate(from, to, iri)` is the open-predicate write
  path; `snapshot/from.rs` recreates predicate-only edges (create-if-missing,
  mirroring the traversal block). 2 kernel tests (assert + snapshot round-trip);
  kernel **234**, plus graph-layout / platen / session-runtime / inker /
  linked-data green. Ingest (Phase 2 proper) is now unblocked.
- **2026-06-01 (later still)** — Phase 2 **core ingest landed** in
  `linked-data::ingest`: `from_jsonld` (oxjsonld parse → `GraphContribution`) +
  `apply_contribution` (materialize, recognized → sub-kind, raw →
  `assert_semantic_predicate`). linked-data **4** (2 export + 2 ingest). API was
  read from the oxjsonld/oxrdf source (not guessed). Remaining: bundled-context
  loader + inker `application/ld+json` routing (see Phase 2).
- **2026-06-01 (loader)** — bundled-context **mechanism** landed: `ContextCache`
  plus `from_jsonld_with_contexts` over oxjsonld's `with_load_document_callback`
  (serve registered contexts, refuse the rest, no network). linked-data **6**.
  Open: embed real context assets (schema.org/DC/AS vs minimal) + inker routing.
