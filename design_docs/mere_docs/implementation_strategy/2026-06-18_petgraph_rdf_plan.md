# petgraph-RDF plan — a losslessly RDF-projectable, SPARQL-queryable petgraph kernel

Status: **planned.** Direction decided from the feasibility research + a perf
benchmark: **petgraph stays the truth and the runtime; RDF is a lossless,
on-demand projection plus a SPARQL query adapter, never a second held authority.**
The target is not "the kernel stores all RDF" but a defined **Mere RDF projection
profile**: the content subgraph is losslessly projectable, the experience/runtime
layer stays native. The work is to (1) carry the profile in the content model
(per-statement edges, typed literals, named-graph scope, statement provenance),
(2) prove losslessness with a round-trip test, (3) run SPARQL over the kernel via a
`QueryableDataset` adapter, and (4) — only if footprint demands — move storage to
an interned slotmap kernel. (Merges the projection-profile take + the
per-statement-edge decision, 2026-06-19.)

Cross-refs:

- [rdf_native_kernel_feasibility](../research/2026-06-18_rdf_native_kernel_feasibility.md)
  — the feasibility research this resolves; the three options (status quo /
  RocksDB-sidecar-authority / RDF-native) and the spike.
- [graph_query_layer_plan](2026-06-18_graph_query_layer_plan.md) — shipped
  `node_quads` + ephemeral SPARQL this extends and partly subsumes.
- The RDF migration plan reviewed 2026-06-18 (its RocksDB-sidecar-as-authority
  "flip" is the rejected option); [two_natured_kernel_brief](../research/2026-05-30_two_natured_kernel_brief.md)
  (one authority, one-way); [statements_over_schema_stance](../technical_architecture/2026-05-22_statements_over_schema_stance.md).
- Benchmark probe: `crates/probes/rdf-kernel-bench/` (standalone).

---

## Why this shape (the decision basis)

- **petgraph and oxrdf compose; they are not substitutes.** petgraph is the
  runtime index (adjacency + typed payloads); oxrdf is the content model. The
  orrery hot path is petgraph in *every* design, so there is no hot-path race.
- **Holding RDF as live truth is expensive for no hot-path gain.** The benchmark
  (50k nodes / 200k edges) measured a held-RDF-truth design vs a petgraph-truth
  design: ~11x live memory, ~19x load, ~18x mutation (a `Vec<Quad>` proxy, so a
  lower bound on build/mutate and a likely over-estimate on memory vs an interned
  store — exact multiples need a follow-up, but the direction is robust). Hot-path
  traversal was identical (~0.6 ns/edge, both). So a held-RDF authority is
  rejected.
- **Losslessness is a data-model property, not a library property.** Any index
  (petgraph / gryf / slotmap) can back a lossless or a lossy kernel; what decides
  it is whether the payloads carry the full RDF construct set.
- **`QueryableDataset` lets the kernel be the SPARQL store directly** (3 methods,
  `InternalTerm` = an interned id, oxrdf+spargebra only, no RocksDB). So SPARQL
  needs no held quads and no oxigraph Store.

Net: the kernel is the single fast typed truth; RDF is projected losslessly and
queried on demand.

## The Mere RDF projection profile

Mere does not claim to store all RDF; it promises the **content subgraph** is
losslessly projectable to RDF under a stated profile, and keeps the experience /
runtime layer native.

- **In the profile (content-world facts):** `Semantic` statements, imported/source
  facts, provenance about content statements, node literal properties, `rdf:type`
  classifications, tags.
- **Out of the profile (native Mere only):** `Traversal`, `Arrangement`, fields /
  couplings, and high-frequency workspace / physics / selection / focus /
  materialization state. Experience, not content; not exported unless a later plan
  explicitly opts a construct in (e.g. `Containment` → `dcterms:hasPart`).
- **Constructs the profile preserves:** IRIs; literals with datatype + language;
  named-graph scope; skolemized blank nodes; resource- and literal-valued
  statements; per-statement metadata (graph, provenance, time, label).
- **Explicitly out of scope:** entailment, OWL/RDFS reasoning, SPARQL UPDATE,
  RDF-as-live-store semantics. (SHACL-as-lint and a published `subPropertyOf`-linked
  vocab are compatible later additions, per the compliance audit.)

The round-trip test (Phase 2) is what makes "lossless under the profile" a checked
property rather than a claim.

## Phase 1 — Carry the projection profile in the content model

The content model must hold every construct the profile preserves, at the right
granularity:

- **Per-statement edges (the granularity fix).** Today `SemanticData { sub_kinds,
  label, predicate }` is one *aggregate* payload per `(from,to)` pair, which cannot
  carry distinct graph / provenance / time / label per predicate. Make the content
  graph a true **multigraph: one petgraph edge per statement** (per predicate),
  payload `SemanticStatement { predicate, graph_scope, provenance (agent/persona
  IRI), asserted_at, label?, recognized sub_kind? }`. The petgraph `EdgeIndex` is
  the statement handle (what reification needs), so no separate statement arena;
  recognized sub-kinds become behavior flags on the statement, not the storage unit.
  - **Multi-edge is the data-model truth; visual collapse is experience-layer.** The
    orrery collapsing parallel edges into one drawn edge is a zoom/domain-dependent
    LOD setting (like node clustering) that reveals the constituent statements on
    hover/select. That logic is owned by node-representation / orrery, not here; the
    kernel always stores the statements separately.
- **Typed literals.** `NodeProperty` gains `datatype: Option<String>` (IRI),
  `lang: Option<String>`, `graph_scope`, and `provenance` (literals are statements
  too). `xsd:string` default, `rdf:langString` when lang set. Ingest captures
  datatype/lang from `oxjsonld` instead of flattening. Curated `title`/`tags` keep
  their `xsd:string` fast-path but project as literals.
- **Named graphs.** A `graph_scope` on every statement/property:
  `Default | Source | User | Agent | Moot | Custom(IRI)`, with a small registry → IRI.
- **Statement metadata = one reifier encoding.** Project annotated statements as a
  reifier node + RDF 1.2 `rdf:reifies` with an *object-position* triple term, plus
  PROV-O / `rdfs:label` on the reifier. One encoding for the native quad / SPARQL /
  Turtle path; JSON-LD-of-metadata is a known deferred gap (JSON-LD can't carry
  triple terms), the classic-reification compat added only when a consumer needs it.
  Confidence stays deferred.
- **`rdf:type`** continues via `NodeClassification` (its provenance/status ride as
  statement metadata where they map). **Blank nodes** keep skolemization (makes
  round-trip exact). **Set semantics**: dedup identical statements on assert.

Keep the common case cheap: `provenance` is `Option` and `graph_scope` defaults to
`Default`, so a human-asserted, now, default-graph statement carries nothing beyond
its predicate.

Done when: the content graph is per-statement multigraph edges + typed properties,
each carrying `graph_scope` + optional provenance, and the model can represent the
full profile construct set.

## Phase 2 — Lossless projection + the round-trip gate

- Extend `node_quads` → `dataset_quads(graph)` emitting **named-graph quads** with
  typed/lang literals and reifier-node statement metadata, replacing the
  default-graph-only, curated-literal-only export.
- **Standard-vocab mapping as a 3-category table** for recognized sub-kinds:
  *exact* standard predicate (cites → `cito:cites`, same-entity → `owl:sameAs`),
  *approximate* via `rdfs:subPropertyOf` to a standard term, and *Mere-only*
  (`mere:ns`) for the genuinely novel (`Blocks`, `NextStep`). Per the compliance
  audit.
- **Round-trip test = the losslessness gate.** `RDF → kernel (ingest) →
  dataset_quads → RDF` must be equal as a **normalized dataset compare** (sorted
  N-Quads), not raw byte equality. Skolemized blanks make it exact (no isomorphism
  dance). Full RDFC-1.0 is deferred with federation/signing (`rdf-canon` is unstable
  today).
- Turtle / N-Quads I/O via `oxttl` (cheap interop win).

Done when: the round-trip test is green across the full feature set (typed
literals, lang tags, named graphs, reifier metadata, multi-predicate, raw + CiTO
predicates).

## Phase 3 — `QueryableDataset` adapter (SPARQL over the kernel, no held quads)

- Implement `spareval::QueryableDataset` over the kernel: `InternalTerm` = an
  **interned term-id** (a u32 into a kernel term dictionary, IRIs/literals interned
  once); `internal_quads_for_pattern(s,p,o,g)` walks the kernel's edges /
  properties / classifications / reifiers as quads matching the pattern;
  `internalize`/`externalize_term` via the dictionary.
- **Keep the shipped ephemeral-Store path as the baseline** until the adapter
  passes (a) row-parity tests (same `SELECT`/`ASK` results) and (b) a perf
  comparison on representative queries. Then `>sparql` rewires onto the adapter and
  the copy-into-a-Store path retires. The adapter's win is no copying quads into an
  oxigraph Store per query.
- The term dictionary introduced here is a derived index over the kernel — and is
  exactly the structure the Phase 4 endgame would make canonical, so this is also
  the on-ramp to compact memory.

Done when: `>sparql` and the seed-graph SPARQL tests run via the adapter over the
kernel directly, with no oxigraph Store in the path.

## Phase 4 (gated, endgame) — interned slotmap kernel, only if footprint demands

- Trigger: the enriched petgraph kernel's memory (typed literals + graph-ids +
  reifier nodes + the term dictionary) becomes a real concern, or compact interned
  storage is wanted as truth.
- Shape: a term dictionary (intern IRIs/literals → ids), triples as compact
  `(id, id, id, graph-id)` in slotmap arenas (generational stable keys =
  safe handles + safe removal), plus a derived adjacency index for the orrery hot
  path. RDF-lossless **and** compact (interning) **and** petgraph-speed.
- Gate: re-run the benchmark probe with interning and the real oxigraph in-memory
  Store for comparison; decide on the measured footprint, not on vibes.
- Lower-risk default is Phases 1-3 on petgraph, which already deliver lossless RDF
  + SPARQL. Phase 4 is the "kernel is a lean RDF store" endgame, not a prerequisite.

## Findings (research backing)

- **Benchmark**: held-RDF-truth ≈ 11x memory / 19x load / 18x mutate vs
  petgraph-truth (lower-bound proxy); hot path identical. Probe at
  `crates/probes/rdf-kernel-bench/`.
- **`QueryableDataset`** (spareval): `internal_quads_for_pattern` +
  `internalize_term` + `externalize_term`, `InternalTerm: Clone+Eq+Hash`
  (interned-id friendly), `Error`; oxrdf + spargebra only, no RocksDB. SPARQL over
  a custom backend is supported.
- **Current lossy fields**: `NodeProperty { predicate, value }` (no datatype/lang,
  types.rs); no named-graph tagging (default graph only); no statement metadata;
  edge `label` dropped on export. `NodeClassification` already carries rich
  provenance/status and maps to `rdf:type`.
- **Granularity**: `SemanticData` is one aggregate payload per `(from,to)` edge, so
  it cannot hold per-predicate graph/provenance/time/label. Per-statement multigraph
  edges (one edge per predicate) are the fix and the correct RDF granularity.
- **`rdf-canon` is unstable** → skolemized deterministic compare for the round-trip
  now; full RDFC-1.0 deferred with federation.
- **Library is a swappable index**; losslessness is the data model (petgraph now,
  slotmap-interned as the gated endgame).

## Risks

- **Persistence migration**: the enriched model changes `GraphSnapshot` / the
  `Persisted*` rkyv types; version it, round-trip it, and back up `graph.json`
  before any storage change.
- **Adapter query perf**: SPARQL over a kernel-walking `QueryableDataset` may trail
  an indexed store on complex queries; fine for ad-hoc/interop (not the hot path),
  measure if it bites.
- **Write ergonomics/invariants**: enriching `SemanticData` + graph-tagging must
  keep the typed assert API ergonomic and referentially integral (endpoints exist).
- **Scope discipline**: Phase 4 is evidence-gated, not default; do not build the
  slotmap kernel unless the footprint demands it.
- **Common-case bloat**: per-statement `provenance` + `graph_scope` on every
  statement/property must not tax the 99% case (human-asserted, now, default graph).
  Keep `provenance` `Option` and the default graph_scope free, and keep a
  snapshot-size before/after test as a gate.

## Progress

- **2026-06-18** — Plan from the feasibility research + the perf benchmark (probe
  `crates/probes/rdf-kernel-bench/`, held-RDF-truth ≈ 11x/19x/18x overhead, hot
  path identical) + the `QueryableDataset` and data-model research. Direction
  decided: petgraph truth, RDF as a lossless on-demand projection + a
  `QueryableDataset` SPARQL adapter; held-RDF-truth (the migration plan's flip)
  rejected on the benchmark. No code written.
- **2026-06-19** — Merged the projection-profile take. Reframed to a stated **Mere
  RDF projection profile** (content in, experience out), and adopted **per-statement
  petgraph edges** (multigraph, one edge per predicate) over enriching aggregate
  `SemanticData` — the correct RDF granularity for per-statement provenance / graph /
  time. Multi-edge is the data-model truth; visual edge-collapse is an
  experience-layer LOD setting (reveal on hover/select), owned by
  node-representation / orrery. One reifier encoding (defer JSON-LD-metadata compat);
  ephemeral Store stays the SPARQL baseline until the adapter proves parity + perf;
  common-case-bloat guard added. No code written.
