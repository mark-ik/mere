# RDF-native kernel — feasibility + spike plan

Status: **research, decision-pending.** Scopes whether to make the kernel's
*content* storage RDF-native (option 3 below) rather than today's typed-petgraph
truth with a lossy RDF export (option 1), or a separate RocksDB Oxigraph store as
the authority (option 2, rejected). The decision gates on a perf spike, scoped
here. No code written.

Cross-refs:

- [two_natured_kernel_brief](2026-05-30_two_natured_kernel_brief.md) — content
  authoritative / experience derived; one authority, one-way projections.
- [statements_over_schema_stance](../technical_architecture/2026-05-22_statements_over_schema_stance.md)
  — the open-predicate model; position 1/2/3; canonicalization deferred with
  federation.
- [graph_query_layer_plan](../implementation_strategy/2026-06-18_graph_query_layer_plan.md)
  — the shipped node_quads projection + ephemeral SPARQL this would subsume.
- [interaction_model_spine](../technical_architecture/2026-06-18_interaction_model_spine.md)
  §6 (made-semantic).
- Prompted by an external RDF-migration plan (reviewed 2026-06-18): its
  "RDF-as-durable-authority via a sidecar" was the over-reach; this is the
  reframe that keeps one authority while making truth RDF-native.

---

## The three options

1. **Typed petgraph truth + RDF as a lossy derived export (today).** Conservative.
   RDF is second-class; `apply_contribution` ingest drops everything but curated
   literals; federation/provenance ride a re-derived projection.
2. **A separate RocksDB Oxigraph store as the content authority (the migration
   plan's flip). Rejected.** Second authority, RocksDB (native-only), weak wasm
   durability, pins truth to a pre-Rec standard mid-library-migration, and
   contradicts the one-authority discipline.
3. **RDF-native kernel.** Content truth *is* an in-memory RDF dataset held inside
   the kernel (pure Rust, no RocksDB), with derived typed indices for the orrery
   hot path; SPARQL runs over the kernel directly; persistence is the kernel's own
   snapshot; the experience layer stays in typed side-tables. Gets RDF-as-truth's
   interop / federation / query / provenance benefits natively, without a separate
   store, uniform across native and wasm. **This doc scopes option 3.**

## Feasibility findings (research 2026-06-18)

- **`spareval` exposes a `QueryableDataset` trait; a consumer implements it over
  their own data and runs SPARQL with no Oxigraph Store and no RocksDB.** This is
  the enabler for "the kernel is the SPARQL-queryable store." `spareval`
  (+ `spargebra`, `sparopt`) is a standalone pure-Rust crate, positioned as a
  building block for SPARQL implementations. Source: docs.rs/spareval.
- **Pure-Rust in-memory RDF storage exists either way.** `oxrdf::Dataset`, or
  oxigraph's in-memory `Store` (`quads_for_pattern`, named-graph methods,
  `bulk_loader`, `query`, `dump_to_writer`; the RocksDB methods are guarded out on
  wasm). So if implementing `QueryableDataset` directly is more than we want, the
  in-memory `Store` is a ready, indexed, pure-Rust content store with zero RocksDB.
  Source: docs.rs/oxigraph store.
- **The refactor is contained behind the kernel's stable public API.** Consumers
  (orrery, meerkat) use the `Graph` public methods (`out_neighbors`, `get_node`,
  `assert_relation`, `cull_aabb`, `positions`); direct petgraph coupling *outside*
graph-kernel is ~5 occurrences, all in `crates/orrery/arrangements` *(historical citation)* <!-- doc-audit: historical-path --> adapters.
  So changing the kernel's storage behind those methods is graph-kernel-internal,
  not a workspace rewrite. (grep: `.inner`/`petgraph::`/`NodeIndex`/`StableGraph`
  = 5 hits across 3 adapter files in orrery.)
- **RDF 1.2 statement metadata = reifier-node form.** oxrdf supports triple terms
  (rdf-12), but JSON-LD can't carry them and Oxigraph's triple-term serialization
  is still settling, so use the reifier-node encoding (`reifier rdf:reifies …` +
  PROV-O on the reifier). Pace adoption by pinning oxrdf.
- **RDFC-1.0 canonicalization in pure Rust (`rdf-canon`) exists but is explicitly
  unstable / not-for-production.** Defer canonicalization and signing to the
  federation phase; do not block the core on it. Source: github zkp-ld/rdf-canon.
- **Persistence stays the kernel's own** (rkyv of the structures, or an
  N-Quads/JSON-LD snapshot), uniform native + wasm. No RocksDB anywhere.
- **SHACL/ShEx validation is available pure-Rust via `rudof`** as an optional lint,
  later.

## Design sketch (the RDF-native kernel)

- **Content store** = an in-memory RDF dataset (oxrdf quads), partitioned into
  named graphs (`source/user/agent/moot`): nodes are subjects (URL or skolem IRI),
  edges are predicate triples (open IRIs; recognized sub-kinds become a
  recognized-IRI set for *behavior*, not storage), properties/classifications are
  literal / `rdf:type` triples, statement metadata is reifier-node triples + PROV-O.
- **Derived indices** over the content: an adjacency index (node → out/in
  neighbors) for the orrery hot path, plus the existing by-address / by-id maps.
  Caches rebuilt from the content, not authority.
- **Experience layer** = typed side-tables keyed by node id (positions, velocities,
  selection, focus, materialization, gyre bodies). Not in the RDF dataset
  (experience, derived); persisted in the runtime snapshot, not RDF.
- **Public API unchanged.** `out_neighbors`/`get_node`/`assert_relation`/
  `cull_aabb`/`positions` map onto (content store + adjacency index) or (experience
  side-tables). Consumers do not change. The ~5 arrangement-adapter petgraph
  accesses get re-expressed via the public API or a thin typed graph-view shim.
- **SPARQL** = implement `QueryableDataset` over the content store; `spareval` runs
  queries on the kernel directly. The current ephemeral `query.rs` path is
  subsumed.
- **Federation/provenance** = native: content is canonicalizable RDF named graphs;
  sign and exchange slices once `rdf-canon` matures.

## The spike (decides option 3 on evidence)

1. **`spareval`-over-kernel probe.** Implement a minimal `QueryableDataset` over a
   toy kernel-shaped store (or over `oxrdf::Dataset`), run a SPARQL `SELECT`,
   confirm it works with no Oxigraph Store / no RocksDB, on native and wasm.
   *Fallback if the trait-over-custom-backend is impractical:* wrap oxigraph's
   in-memory `Store` (still pure Rust, no RocksDB).
2. **Hot-path perf benchmark — the decision gate.** The orrery's per-frame queries
   (adjacency / `out_neighbors` + node-attribute lookup + cull) on (a) current
   petgraph vs (b) an RDF-dataset / in-memory-`Store`-backed graph **with a derived
   adjacency index**, at 1k / 10k / 50k nodes+edges. Hypothesis: the adjacency
   index decouples the frame loop from RDF pattern-lookup speed, so (b) ≈ (a).
   **Gate: if (b) with the index cannot match petgraph for the frame loop at 50k,
   option 3 is not viable for the orrery — hold option 1 and keep RDF a derived
   projection.**
3. **Kernel-shape sketch + change-surface.** A concrete struct sketch (content
   store + adjacency index + experience side-tables + public-API mapping +
   persistence) and the exact list of sites to change (kernel-internal + the ~5
   adapter accesses), confirming containment.

## Risks / what kills it

- **Perf-gate failure** (the decisive one). Mitigated by the adjacency-index
  hypothesis, but must be measured, not assumed.
- **Mutation ergonomics + invariants.** petgraph plus the borrow checker give
  ergonomic, referentially-integral typed mutation; the RDF-backed write API
  (`assert_relation`, etc.) must preserve endpoint-existence and stay ergonomic.
  Design the write path to keep the invariants the type system gives today.
- **`rdf-canon` immaturity** → defer federation/signing; do not depend on it for
  core.
- **RDF 1.2 churn** → pin oxrdf, use the reifier-node form.
- **Persistence migration** → `graph.json` / `GraphSnapshot` becomes a content +
  experience snapshot; keep a one-version backup and a round-trip equivalence test
  (`graph.json` → RDF-native → `graph.json`).

## Recommendation

Run the spike (probes 1-3). If the perf gate passes, option 3 is the right target:
reachable in pure Rust + wasm, contained behind the public API, and strictly
better than options 1 and 2 (RDF-as-truth's interop/federation/query/provenance
without a second store, RocksDB, or the one-authority violation). If the gate
fails, hold option 1 with RDF as a derived projection (the shipped query-layer
direction). Either way the spike is small and decision-bounded.

## Progress

- **2026-06-18** — feasibility research: `spareval`'s `QueryableDataset` trait makes
  the kernel-as-SPARQL-backend feasible with no RocksDB; pure-Rust in-memory store
  available as a fallback; change-surface contained (~5 direct petgraph sites
  outside graph-kernel, all arrangement adapters); `rdf-canon` immature (defer
  canon/signing); reifier-node form for RDF 1.2 metadata. Spike plan scoped; the
  hot-path perf benchmark is the decision gate. No code written.
- **2026-06-18 (resolved)** — benchmark ran (`crates/probes/rdf-kernel-bench/` *(historical citation)* <!-- doc-audit: historical-path -->).
  Reframed by Mark: petgraph (runtime index) and oxrdf (content model) *compose*,
  so the hot path is petgraph in every design and there was no speed race. The
  real axis is the cost of *holding* RDF as live truth: ≈11x memory, ≈19x load,
  ≈18x mutate vs petgraph-truth (lower-bound `Vec<Quad>` proxy), hot path
  identical (~0.6 ns/edge). Verdict: keep petgraph as truth, RDF as a lossless
  on-demand projection + a `QueryableDataset` SPARQL adapter (no held quads), with
  an interned slotmap kernel as a gated endgame. Continued in
  [petgraph_rdf_plan](../implementation_strategy/2026-06-18_petgraph_rdf_plan.md).
