# petgraph-RDF plan — a losslessly RDF-projectable, SPARQL-queryable petgraph kernel

Status: **Phase 1A partial.** Direction decided from the feasibility research + a
perf benchmark: **petgraph stays the truth and the runtime; RDF is a lossless,
on-demand projection plus a SPARQL query adapter, never a second held authority.**
The target is not "the kernel stores all RDF" but a defined **Mere RDF projection
profile**: the content subgraph is losslessly projectable, the experience/runtime
layer stays native. The current landed slices are the semantic statement-bucket
backbone, typed literal fidelity on `NodeProperty`, and graph-scope fields on
semantic statements / node properties with dataset-query visibility. Full
named-graph JSON-LD shaping, statement metadata, and the direct SPARQL adapter
are still ahead. The work is to (1) carry the profile in the content model
(statement records in pair-local edge buckets, typed literals, named-graph scope,
statement provenance), (2) prove losslessness with a round-trip test, (3) run
SPARQL over the kernel via a `QueryableDataset` adapter, and (4) — only if
footprint demands — move storage to an interned slotmap kernel. (Merges the
projection-profile take + the statement-bucket revision, 2026-07-04.)

Cross-refs:

- [rdf_native_kernel_feasibility](../research/2026-06-18_rdf_native_kernel_feasibility.md)
  — the feasibility research this resolves; the three options (status quo /
  RocksDB-sidecar-authority / RDF-native) and the spike.
- [graph_query_layer_plan](2026-06-18_graph_query_layer_plan.md) — shipped
  `node_quads` + ephemeral SPARQL this extends and partly subsumes.
- The RDF migration plan reviewed 2026-06-18 (its RocksDB-sidecar-as-authority
  "flip" is the rejected option); [two_natured_kernel_brief](../research/2026-05-30_two_natured_kernel_brief.md)
  (one authority, one-way); [statements_over_schema_stance](../technical_architecture/2026-05-22_statements_over_schema_stance.md).
- Benchmark probe: `crates/probes/rdf-kernel-bench/` *(historical citation)* <!-- doc-audit: historical-path --> (standalone).

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

- **Statement buckets inside `EdgePayload` (the granularity fix).** Today
  `SemanticData { sub_kinds, label, predicate }` is one *aggregate* payload per
  `(from,to)` pair, which cannot carry distinct graph / provenance / time / label
  per predicate. Keep the petgraph edge as the pair-local adjacency bucket, but
  make the bucket enumerate real content statements:
  `SemanticStatement { statement_id, predicate, graph_scope, provenance
  (agent/persona IRI), asserted_at, label?, recognized_sub_kind? }`. The petgraph
  `EdgeIndex` identifies the bucket; `statement_id` identifies the fact. That id is
  what reification, statement provenance, precise retract, snapshot migration, and
  federation tombstones point at. This is a bigger win than tidiness: petgraph
  `EdgeIndex` is unstable under removal and meaningless across serialization, so a
  separate fact handle is what makes retract, migration, and federated tombstones
  sound at all.
  - **The multigraph is logical, not necessarily petgraph-native.** The data-model
    truth is one statement per fact. The runtime representation can be one
    `EdgePayload` per `(from,to)` carrying a statement list, because pair adjacency
    is still the hot path. Pair-level APIs can keep returning "there is an edge from
    A to B"; RDF projection and exact mutation use statement iterators.
  - **Enumerating current `sub_kinds` is not enough.** The statement list must carry
    per-statement metadata. A `BTreeSet<SemanticSubKind>` plus one `label` and one
    `predicate` is still lossy as soon as two predicates on the same pair need
    different graph scope, provenance, assertion time, label, or lifecycle.
  - **Visual collapse stays the default view.** Orrery/node-representation render
    the pair bucket as one visible relation unless the user asks for the constituent
    statements. Collapse is a view over the statement bucket, not a repair for the
    storage model.
  - **`StatementId` allocation must be endpoint-independent and bucket-independent.**
    Treat it as an opaque fact id minted at assert time, not a petgraph handle and
    not a per-graph counter. The preferred direction is a random or ULID-style id
    stable across snapshot, replay, and device boundaries; dedup still happens on
    statement content, separately from id minting. Content-hash ids only work if
    statement metadata is immutable, which this plan does not assume. The current
    landed slice uses a local timestamp+nonce string as a temporary allocator; Phase
    1 is not fully done until the federation-safe minting story is pinned.
  - **Empty semantic buckets do not linger.** Retracting the last semantic statement
    clears the semantic sidecar, and if that leaves the whole `EdgePayload` empty
    the petgraph edge itself is removed. So pair-level APIs read "there is an edge
    A -> B" only when at least one statement or some other edge-family payload still
    exists on that pair.
  - **Retract-by-id lookup stays bucket-local.** Exact retract scans the bucket's
    statement list linearly to find `statement_id`. That is the intended default:
    buckets are expected to stay small, dedup keeps multiplicity bounded, and a
    global `StatementId -> location` index should wait for evidence rather than
    arrive preemptively.
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

Done when: the content graph has statement records inside pair-local `EdgePayload`
buckets plus typed properties, each carrying `graph_scope` + optional provenance,
and the model can represent the full profile construct set.

## Phase 2 — Lossless projection + the round-trip gate

- Extend `node_quads` → `dataset_quads(graph)` emitting **named-graph quads** with
  typed/lang literals and reifier-node statement metadata, replacing the
  default-graph-only, curated-literal-only export.
- **Standard-vocab mapping as a 3-category table** for recognized sub-kinds:
  *exact* standard predicate (cites → `cito:cites`, same-entity → `owl:sameAs`),
  *approximate* via `rdfs:subPropertyOf` to a standard term, and *Mere-only*
  (`mere:ns`) for the genuinely novel (`Blocks`, `NextStep`). Per the compliance
  audit. **Landed 2026-07-06** (`vocab::vocabulary_alignment_quads`): alignment
  quads about the *properties* (not rewritten instances) in a `mere:graph#vocabulary`
  named graph — `owl:equivalentProperty` for exact, `rdfs:subPropertyOf` for
  approximate, nothing for Mere-only.
- **Round-trip test = the losslessness gate.** `RDF → kernel (ingest) →
  dataset_quads → RDF` must be equal as a **normalized dataset compare** (sorted
  N-Quads), not raw byte equality. Skolemized blanks make it exact (no isomorphism
  dance). Full RDFC-1.0 is deferred with federation/signing (`rdf-canon` is unstable
  today).
- Turtle / N-Quads I/O via `oxttl` (cheap interop win). **Landed 2026-07-06**
  (`serialize::{to,from}_{nquads,trig}`): N-Quads (canonical) and TriG (both carry
  named graphs; plain Turtle would flatten the scopes). Each serialization appends
  the vocabulary alignment so a file is self-describing; ingest drops that graph
  (re-derivable schema) so a round trip through a file stays lossless. RDF 1.2
  triple terms (reifier metadata) ride through both via oxttl's `rdf-12` feature.

Done when: the round-trip test is green across the full feature set (typed
literals, lang tags, named graphs, reifier metadata, multiple statements on one
node pair, raw + CiTO predicates).

## Phase 3 — `QueryableDataset` adapter (SPARQL over the kernel, no held quads)

- Implement `spareval::QueryableDataset` over the kernel: `InternalTerm` = an
  **interned term-id** (a u32 into a kernel term dictionary, IRIs/literals interned
  once); `internal_quads_for_pattern(s,p,o,g)` walks the kernel's statement buckets
  / properties / classifications / reifiers as quads matching the pattern;
  `internalize`/`externalize_term` via the dictionary.
- **Keep the shipped ephemeral-Store path as the baseline** until the adapter
  passes (a) row-parity tests (same `SELECT`/`ASK` results) and (b) a perf
  comparison on representative queries. Then `>sparql` rewires onto the adapter and
  the copy-into-a-Store path retires. The adapter's win is no copying quads into an
  oxigraph Store per query.
- The term dictionary introduced here is a derived index over the kernel — and is
  exactly the structure the Phase 4 endgame would make canonical, so this is also
  the on-ramp to compact memory. **Deferred** (2026-07-06): the shipped
  spareval-over-`dataset_quads` path already satisfies the done-condition below,
  and the footprint measurement (Phase 4 gate) shows the term dictionary reclaims
  ~1% of live memory, so building the kernel-owned dictionary now would be an
  on-ramp to a gate that is not close. Reconsider once image blobs are
  externalized and RDF strings become the dominant remaining cost.

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
- **Gate measured 2026-07-06 — Phase 4 is not close, and its premise is wrong.**
  `crates/probes/rdf-kernel-footprint/` *(historical citation)* <!-- doc-audit: historical-path --> (a counting global allocator over a
  realistic enriched-kernel graph; the old `rdf-kernel-bench` measured the
  rejected held-RDF-truth axis, not this one) decomposes live heap at 50k nodes /
  100k statements / 2 props: **images 352 MiB (64%)** (`thumbnail_png` +
  `favicon_rgba`, held inline in every Node), **RDF content 154 MiB (28%)**
  (dominated by `EdgePayload` / statement *struct* overhead + unique strings —
  ids, values — not repeated IRIs), **structure/adjacency 48 MiB (8%)**, total
  553 MiB. A term dictionary — Phase 4's whole mechanism — reclaims the repeated
  predicate IRIs only: **6 MiB, ~1% of total**. The 46-byte per-statement ids are
  unique and intrinsically un-internable. So interning targets the wrong whale.
  The footprint levers in ROI order are: (1) **externalize image blobs**
  (content-addressed store, keep a hash in the node) — reclaims ~64% and is a
  bigger, simpler win than Phase 4, now scoped in
  [node_image_externalization_plan](2026-07-06_node_image_externalization_plan.md); (2) **slim `EdgePayload`** (box the rare edge
  families / a compact statement record) — chips the 28%; (3) the term dictionary
  / slotmap kernel — last, ~1%, and only meaningful *after* (1) makes strings the
  dominant remaining cost. Phase 4 stays gated, and its Phase 3 on-ramp (a
  kernel-owned term dictionary) is correspondingly deferred: the shipped
  spareval-over-`dataset_quads` path already meets the Phase 3 done-condition.
- Lower-risk default is Phases 1-3 on petgraph, which already deliver lossless RDF
  + SPARQL. Phase 4 is the "kernel is a lean RDF store" endgame, not a prerequisite.

## Findings (research backing)

- **Benchmark (held-RDF-truth axis)**: held-RDF-truth ≈ 11x memory / 19x load /
  18x mutate vs petgraph-truth (lower-bound proxy); hot path identical. The
  original `crates/probes/rdf-kernel-bench/` *(historical citation)* <!-- doc-audit: historical-path --> (now gone from the tree) measured
  this — the *rejected* design's cost, not the enriched kernel's footprint.
- **Footprint decomposition (Phase 4 axis, 2026-07-06)**:
  `crates/probes/rdf-kernel-footprint/` *(historical citation)* <!-- doc-audit: historical-path --> measures where the enriched kernel's live
  heap goes — images 64% / RDF content 28% (mostly struct, not strings) /
  structure 8% at 50k nodes; a term dictionary reclaims ~1%. See the Phase 4
  gate note. Re-runnable (`cargo run --release`).
- **`QueryableDataset`** (spareval): `internal_quads_for_pattern` +
  `internalize_term` + `externalize_term`, `InternalTerm: Clone+Eq+Hash`
  (interned-id friendly), `Error`; oxrdf + spargebra only, no RocksDB. SPARQL over
  a custom backend is supported.
- **Current lossy fields**: full named-graph export/import is still incomplete;
  `NodeClassification`, curated `title`/`tags`, and JSON-LD shaping are not yet
  graph-scope-aware, so the dataset/SPARQL path is ahead of the JSON-LD shapers.
  Statement metadata now round-trips in kernel snapshots and projects into the
  dataset/SPARQL path as RDF 1.2 reifier nodes, but JSON-LD export still omits
  that metadata and ingest still ignores triple-term reifier input. The older
  literal-flattening gap is now partially retired:
  `NodeProperty` carries datatype/lang, linked-data ingest keeps them, and
  export/query projection emits them. `NodeClassification` already carries rich
  provenance/status and maps to `rdf:type`.
- **Granularity**: `SemanticData` is one aggregate payload per `(from,to)` edge, so
  it cannot hold per-predicate graph/provenance/time/label. Statement records are
  the fix. They can live inside the pair-local `EdgePayload` bucket; they do not
  require one petgraph edge per statement as long as statement identity is explicit.
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
- **Write ergonomics/invariants**: replacing aggregate `SemanticData` with a
  statement bucket must keep the typed assert API ergonomic and referentially
  integral (endpoints exist).
- **Statement identity**: `EdgeIndex` is only the pair-bucket handle in this model.
  `StatementId` must be stable enough for reification, retract, snapshot round-trip,
  and federation tombstones. A local allocator is fine for the landed slice, but
  Phase 1 completion needs a device-safe minting story that does not depend on edge
  position or serialization order.
- **Scope discipline**: Phase 4 is evidence-gated, not default; do not build the
  slotmap kernel unless the footprint demands it.
- **Common-case bloat**: per-statement `provenance` + `graph_scope` on every
  statement/property must not tax the 99% case (human-asserted, now, default graph).
  Keep `provenance` `Option` and the default graph_scope free, and keep a
  snapshot-size before/after test as a gate. **Gate landed 2026-07-06**
  (`kernel graph::tests::snapshot_size`): a common-case property is pinned at
  ~229 B in the compact-JSON snapshot (ceiling 280), and populating the metadata
  must cost strictly more (pay-per-use). **Finding**: the metadata fields are
  `Option`-typed but not `#[serde(skip_serializing_if)]`, so a common-case
  property still emits ~90 B of `null`/default keys — bounded, but not literally
  "free". A follow-up could add `skip_serializing_if` to
  `PersistedSemanticStatement` / `PersistedNodeProperty` (backward-compatible via
  the existing `#[serde(default)]`) to reclaim it; the gate would then lock in the
  cheaper baseline.

## Progress

- **2026-06-18** — Plan from the feasibility research + the perf benchmark (probe
  `crates/probes/rdf-kernel-bench/` *(historical citation)* <!-- doc-audit: historical-path -->, held-RDF-truth ≈ 11x/19x/18x overhead, hot
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
  common-case-bloat guard added. Later superseded by the 2026-07-04 statement-bucket
  representation. No code written.
- **2026-07-04** — Revised the granularity decision: keep `EdgePayload` as the
  pair-local adjacency bucket and enumerate statement records inside it. The logical
  multigraph remains one statement per fact, but petgraph no longer needs one edge
  per statement. `EdgeIndex` is the bucket handle; `StatementId` is the statement
  handle for reification / provenance / precise retract / snapshot / federation.
- **2026-07-04 (landed slice)** — Implemented the semantic statement-bucket
  backbone in code. `SemanticData` now carries explicit statement records inside
  the pair-local `EdgePayload`, snapshot persistence round-trips them, and
  kernel/linked-data reads that care about semantic multiplicity now iterate the
  statement list. The current `StatementId` allocator is a local timestamp+nonce
  string; that is good enough for the landed slice but not yet the final
  federation-safe id story.
- **2026-07-04 (landed slice, typed literals)** — Extended `NodeProperty` with
  datatype/lang fidelity, kept old JSON snapshots loadable via serde defaults,
  preserved typed and language-tagged literals on JSON-LD ingest, and projected
  them back out through `node_quads`, expanded/compacted JSON-LD export, and the
  ephemeral SPARQL path. Still missing from Phase 1: named-graph scope, statement
  provenance/time/metadata, and statement-aware write APIs beyond the legacy
  edge-wide predicate stamp.
- **2026-07-04 (landed slice, graph scope)** — Added `graph_scope` to semantic
  statements and node properties, snapshot persistence round-trips it, linked-data
  ingest preserves it for non-curated literal properties and semantic edges, and
  the dataset/SPARQL path now emits named-graph quads from those scopes. The
  JSON-LD shapers still deliberately stay default-graph-only, and curated
  `title`/`tags` plus `rdf:type` classifications are not yet graph-scope-aware.
- **2026-07-06 (Phase 4 gate measured — deferred on evidence)** — Built
  `crates/probes/rdf-kernel-footprint/` *(historical citation)* <!-- doc-audit: historical-path --> (a counting global allocator decomposing the
  enriched kernel's live heap; the cited `rdf-kernel-bench` was gone and had measured
  the rejected held-RDF-truth axis anyway). At 50k nodes / 100k statements: images
  64% / RDF content 28% (mostly `EdgePayload` + statement struct, not repeated
  strings) / structure 8%, total 553 MiB; a term dictionary reclaims ~1%. Verdict:
  Phase 4's interning targets the wrong whale — the footprint levers in ROI order are
  image-blob externalization (~64%), `EdgePayload` slimming, then interning (~1%,
  last). Phase 4 stays gated; the Phase 3 kernel-dictionary on-ramp is deferred (the
  shipped spareval path already meets the done-condition). See the Phase 4 gate note.
- **2026-07-06 (Phase 2 completion: vocab alignment + Turtle/N-Quads I/O + bloat gate)** —
  The three Phase 2 tail items landed. (1) **Standard-vocabulary alignment**
  (`vocab.rs`): each recognized `SemanticSubKind` is categorized exact / approximate /
  Mere-only (CiTO / OWL / RDFS / Dublin Core anchors from the stance doc; the rest
  conservative), published as `owl:equivalentProperty` / `rdfs:subPropertyOf` quads
  *about the properties* in a `mere:graph#vocabulary` named graph. Instance data keeps
  canonical `mere:rel#` IRIs, so the round trip is untouched; a consumer bridges via a
  reasoner. The kernel now owns the relation enumeration (`all_semantic_sub_kinds`); the
  exhaustive `alignment` match forces any new sub-kind to be categorized. (2) **Turtle /
  N-Quads file I/O** (`serialize.rs`, via `oxttl` 0.2, `rdf-12`): `to_nquads` / `to_trig`
  serialize `dataset_quads` + the alignment (self-describing files); `from_nquads` /
  `from_trig` parse back, dropping the re-derivable vocabulary graph so a file round trip
  is lossless (reifier triple terms survive both formats — tested). (3) **Snapshot-size
  gate** (`kernel graph::tests::snapshot_size`): pins the common-case property at ~229 B
  and asserts metadata is pay-per-use; surfaced that the `Option` metadata still emits
  ~90 B of `null` keys (see the common-case-bloat risk for the `skip_serializing_if`
  follow-up). linked-data 36/36 (`--features query`), kernel 281/281. Still ahead in the
  plan: the JSON-LD shaper gaps (deliberately unchanged) and Phase 4 (gated).
- **2026-07-06 (Phase 3 landed: `>sparql` on spareval)** — The query path now evaluates
  directly over the projection: `sparql()` collects `dataset_quads` into an
  `oxrdf::Dataset` and runs `spareval::QueryEvaluator` (spargebra parse, `sparql-12`
  feature for triple terms). spareval ships `QueryableDataset for &Dataset` — interned
  terms + (s,p,o,g) indexes, which is the plan's term-dictionary adapter — and pins
  the same oxrdf 0.3 this crate builds quads with, so the projection feeds the
  evaluator with zero cross-model conversion. Receipts: row parity vs the old
  copy-into-Oxigraph-`Store` path across a 9-query battery (full-dataset scan, GRAPH
  patterns, reifier metadata, lang FILTER, COUNT, ASK true/false) as a green test
  (`spareval_rows_match_store_baseline`), and perf 96.7ms vs 151.3ms (spareval ~36%
  faster, debug, battery x20; tripwire test keeps 3x headroom). The Store path is
  retired from the mainline and survives only as the in-test parity oracle;
  `dep:oxigraph` still sits in the `query` feature solely for that oracle and is the
  one remaining candidate drop. No RocksDB, wasm-viable. A custom kernel-walking
  `QueryableDataset` (skipping quad materialization) stays an option if profiling
  ever demands it; today `dataset_quads` materializes for both paths. Consumer API
  unchanged (`QueryRows`); `EdgeContributionWire` (content-contract) grew the four
  statement-metadata fields with serde defaults so old wire payloads still parse.
- **2026-07-05 (Phase 1 closed + Phase 2 gate green)** — The review-ordered tail landed:
  (1) **StatementId minting is federation-safe**: `{unix_ms}-{process_salt}-{counter}` with a
  64-bit per-process salt (OS randomness native; wasm hosts seed via
  `kernel::types::seed_statement_minter`, per the kernel identity doctrine). Legacy ids stay
  valid opaque handles; dedup remains content-based. (2) **Statement-aware writes**:
  `SemanticStatementSpec` + `Graph::assert_semantic_statement` (content-dedup returns the
  existing handle, metadata updates in place) / `Graph::retract_semantic_statement` (precise
  retract; emptied bucket clears the sidecar, emptied payload removes the petgraph edge) /
  `Graph::assert_persisted_semantic_statement` (id-preserving re-ingest path). (3) **The Phase 2
  round-trip gate is a green test** (`dataset_round_trip_is_lossless_under_the_profile`):
  quad-level ingest (`ingest::from_quads`) now lifts RDF 1.2 reifiers — triple-term `rdf:reifies`
  recognized, reifier subjects excluded from node contributions, `rdfs:label` /
  `prov:wasAttributedTo` / `prov:generatedAtTime` parsed back (xsd:dateTime -> ms) and attached
  to the matching edge (by s/p/o/scope) or node property (by p/value/typing/scope), with
  `urn:mere:statement:<id>` handles preserved through apply — so
  `RDF -> kernel -> dataset_quads -> RDF` compares byte-equal as sorted N-Quads across typed +
  lang literals, named scopes, two differently-scoped statements on one pair, statement
  metadata, recognized + raw predicates, rdf:type, and curated title/tags. "Lossless under the
  profile" is now a checked property. kernel 279/279, linked-data 26/26. Still ahead: Phase 3
  (`QueryableDataset` adapter; the ephemeral-Store `>sparql` stays the baseline until adapter
  parity + perf receipts) and the JSON-LD shaper gaps (deliberately unchanged; N-Quads/Turtle is
  the canonical lossless projection since JSON-LD cannot carry triple terms).
- **2026-07-04 (landed slice, statement metadata)** — Added `provenance_iri` and
  `asserted_at_ms` to semantic statements and node properties, kept older
  snapshots loadable via serde defaults, round-tripped the richer statement
  records through snapshot persistence, and projected statement metadata into the
  dataset/SPARQL path as RDF 1.2 reifier nodes keyed by `StatementId`. The
  JSON-LD shapers still stay on direct default-graph node quads, and ingest still
  treats triple-term reifier metadata as a deferred gap.
