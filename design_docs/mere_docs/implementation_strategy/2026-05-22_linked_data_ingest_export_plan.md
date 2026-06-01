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
  kernel suite **228/228** green. **Deferred:** raw predicate-*only* edges (empty
  `sub_kinds`) are not yet recreated on load — that pairs with JSON-LD ingest
  (Phase 2).
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
- **Done:** a sample JSON-LD doc yields the expected nodes + typed edges; an
  unknown predicate lands as a raw-IRI `Semantic` edge with empty `sub_kinds`; a
  recognized one also sets its sub-kind; a literal maps to `title`/`tags`.

### Phase 3 — Round-trip + coverage
- Ingest → export → compare, modulo the by-design drops (uncurated literals).
- Recognition-table coverage test (every `SemanticSubKind` ↔ a canonical IRI).
- **Done:** round-trip is stable for the recognized vocabulary + raw predicates.

### Deferred (explicit non-goals for this effort)
- HTML `<script type="application/ld+json">` / microdata extraction — needs
  Serval/HTML parsing; do after standalone-resource ingest proves the seam.
- General node **property bag** for full literal fidelity (separate kernel
  change).
- JSON-LD framing, RDF canonicalization (URDNA2015/RDFC-1.0), signing,
  federation / VCs / ActivityPub.

---

## Open questions

1. **JSON-LD lib:** the `json-ld` crate (Haudebourg) + `sophia`/`rdf-types`, vs a
   minimal hand-rolled expander for the bundled-context subset. Decide on
   wasm-weight grounds; full processors are heavy.
2. **Interning home:** per-graph vs per-session symbol table for predicate IRIs.
3. **Crate placement:** dedicated `linked-data` crate (proposed) vs `inker`
   module.
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
