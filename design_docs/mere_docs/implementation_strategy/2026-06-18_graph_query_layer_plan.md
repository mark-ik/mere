# Graph query layer plan (RDF / SPARQL / Oxigraph)

Status: **slices 1+2 shipped and verified.** SPARQL query over the focused graph,
kernel-sourced and one-way (the kernel stays truth; this is a derived, read-only
view for interop and exploration). A residual backlog follows; none of it blocks.

Parent / cross-refs:

- [interaction_model_spine](../technical_architecture/2026-06-18_interaction_model_spine.md)
  §6 (made-semantic): `to_jsonld` export is the kernel-authoritative *broadcast*;
  this plan is the *query* facet of the same stage.
- [unified_document_host_plan](2026-06-17_unified_document_host_plan.md) — its Phase 2
  "semantic surface" payoff (emit kernel-sourced JSON-LD into the orrery DOM) is the
  view-legibility sibling of this query work; both build on `node_quads`.
- The JSON-LD ingest/export bridge this builds on is the (archived, completed)
  `2026-05-22_linked_data_ingest_export_plan` in
  [`archive_docs/2026-06-09_completed_plans/`](../../archive_docs/2026-06-09_completed_plans/).

---

## What shipped

### node_quads — the single kernel→RDF projection (substrate)

`pub fn node_quads(graph, key, node) -> Vec<oxrdf::Quad>` in
[`crates/graph/linked-data/src/lib.rs`]. The expanded and compacted JSON-LD
shapers (`node_object` / `compact_node_object`) now render from it instead of each
walking the graph, retiring the duplicated walk. Verified by the existing
linked-data goldens (21/21). `oxrdf 0.3` was already a direct dep; no new
dependency.

### Slice 1 — the query capability (library)

`linked-data/src/query.rs`, behind an optional **`query` feature**:
`pub fn sparql(graph, &str) -> Result<QueryRows, String>` projects the focused
graph into a fresh in-memory Oxigraph store via `node_quads` (converted across the
oxrdf-version gap by `to_ox_quad`), runs the query, returns the solution rows.
Ephemeral (store built per call, dropped after). `oxigraph = { version = "0.5",
default-features = false }` so `Store::new()` is in-memory with **no RocksDB** —
keeps the wasm/PWA target viable. Verified: `sparql_selects_a_literal_and_an_edge_over_node_quads`
(literal + edge SELECT over the seed graph), 22/22 lib tests.

### Slice 2 — the `>sparql` omnibar verb (host)

The command shell ([`crates/meerkat/src/shell_eval.rs` *(historical citation)* <!-- doc-audit: historical-path -->]) gained a `sparql_query`
field on `ShellOutcome` and an arg-bearing `sparql("…")` binding (mirroring the
`relate("…")` precedent: record into the outcome, don't mutate inline, since the
shell snapshot has no RDF graph); `complete()` ghosts the verb. The host drain
([`crates/meerkat/src/command_drain.rs` *(historical citation)* <!-- doc-audit: historical-path -->] `submit_omnibar_command` → `run_sparql_query`
→ `format_sparql_rows`) runs it over the focused graph and echoes a one-line
result. meerkat enables `linked-data/query`. Verified: full meerkat suite green
(64 lib + 94 bin), `shell_eval::tests::sparql_records_the_query_for_the_host_to_run`.

Live form:

```
>sparql("SELECT ?s ?o WHERE { ?s <https://schema.org/name> ?o }")
```

---

## Residual backlog (ranked by leverage; none blocking)

1. **Shared `mapping` module (cleanup).** Export (`node_quads`) and ingest
   (`ingest.rs`, already on `oxrdf` quads) should meet at one set of decisions,
   retiring `ingest.rs`'s duplicate `RDF_TYPE` const and its two pre-existing
   unused-import warnings. Small, tidy, low-risk.
2. **RDF-star edge metadata (fidelity — the substantive next RDF step).** Today
   only `Semantic` edges + curated literals project; edge provenance / durability
   (the `predicate` + `edge_data` fields) are dropped. RDF-star (Oxigraph's
   `rdf-12` feature) is the standards-correct home: `node_quads` emits quoted
   triples for the metadata, so export and query become lossless for edges. The
   one genuinely new RDF capability worth doing.
3. **`CONSTRUCT` / `DESCRIBE` → graphlet.** `query.rs` currently returns an error
   for `QueryResults::Graph`. Wiring `CONSTRUCT` output into a derived subgraph
   ties SPARQL to graphlet-derivation (reveal latent structure as a real graphlet).
4. **Results pane.** Slice 2 echoes one line on the status bar; a tabular
   `ListPane`-style results surface is the richer UX (variables as columns).
5. **Turtle / N-Quads I/O.** Cheap interop win via `oxttl` (same ox* family),
   widening import/export beyond JSON-LD. A lot of linked data in the wild is
   Turtle.
6. **Semantic-surface JSON-LD-in-view (Path A payoff).** Emit kernel-sourced
   `<script type="application/ld+json">` per card once orrery nodes are DOM. Gated
   on unified-document-host Phase 2 (orrery-as-element), not pure RDF work.
7. **Synced-mirror store (perf).** The ephemeral per-query rebuild is fine for
   occasional queries; a long-lived store kept in sync on mutation is the
   follow-on only if query frequency demands it. One-way (kernel → store) per the
   two-natured rule.
8. **Federation / UPDATE / HDT (future tiers).** SPARQL `UPDATE` is deliberately
   out (the kernel is the one-way authority). Federated `SERVICE` query across
   moots and HDT (compact binary graph distribution) are federation-tier items.

PROV-O (for the edge provenance in #2) and SKOS (for tag hierarchies) are the
vocabularies to reach for if/when those land; Atomic Data is a design reference,
not a task.

## Progress

- **2026-06-18** — node_quads projection + slices 1 (linked-data `query` feature)
  and 2 (`>sparql` omnibar verb) shipped and verified green; slice-2 files
  warning-clean. Not committed (working tree). The build detour was a stale local
  `Cargo.lock`: piecemeal `cargo update -p {genet-layout, netrender}` left an
  incoherent partial state (a `windows` 0.61/0.62 split); a full fresh resolve
  (`rm Cargo.lock`) to the current owned-fork mains (genet `69431717`, netrender
  `c5e6400c`) restored a coherent set. Lesson, matching the workspace convention:
  for gitignored, branch-tracked owned forks, re-resolve fresh rather than bump
  pins one at a time.
