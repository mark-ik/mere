# G5: mere re-base onto chartulary — progress

**Date:** 2026-07-08
**Status:** graph adoption landed. mere's `Graph` now *is* a
`chartulary::Graph<Node, EdgePayload>` under the hood. Three steps are done: the node
capabilities, the edge capabilities, and the graph swap itself (all 284 kernel tests
green). What remains is history-over-spine and the analytics retarget. Canonical plan:
`2026-07-08_generic_graph_substrate_plan.md`.

## What landed (committed under 1fe6a82)

- **Step 0, the trait-API refinement (chartulary).** Re-basing a real foreign node
  surfaced that `Addressed::addresses` and `Labeled::tags` returned *borrowed slices
  of chartulary's own types* (`&[Address]`, `&[String]`). That fits the default
  `Container` (which stores exactly those) but not mere's `Node`, which holds
  `Vec<AddressClaim>` and `HashSet<String>`, neither of which can produce those
  slices. The traits now return **owned values** (`Vec<Address>`, `Vec<String>`,
  `Option<Address>`), so a node maps from its own storage. chartulary (15 tests) and
  scholia (5) stay green. This is the classic "the abstraction looked fine against
  its own default impl; the first foreign implementor found the leak" lesson.
- **mere's `Node` implements the capabilities (kernel).** A new
  `graph/chart.rs` implements `Identified` (id = `Uuid`), `Addressed` (address
  claims mapped to scheme-qualified `Address`es, primary first), and `Labeled`
  (title with the empty-is-none rule, tags from the set). Verified by a compile-time
  bound check and a runtime test (`cargo test -p kernel chart`, 2 tests). The
  browser-runtime facets (favicon, viewer routing, session restore, lifecycle) stay
  on `Node` and simply do not participate in the generic capabilities.
- **`ContentBearing` deliberately deferred.** mere addresses content through its own
  cache, not a `muniment` blob, so adopting `muniment::Hash` for node content is a
  later re-base step, not this one.

## Edge capabilities (committed under fc5b8db)

- **mere's `EdgePayload` implements `Classified` / `Predicated` (kernel).** The
  single-predicate helper picks an explicit statement or open predicate first, else
  the first recognized `SemanticSubKind`'s canonical `urn:chart:rel:*` IRI. A
  semantic edge joins the shared ring as an open predicate (so it projects to RDF); an
  experience-layer edge (Traversal, Containment, Arrangement, Imported) reports
  `RelationClass::app("mere", 0)` and stays private. This is the two-ring split
  realized on mere's real edge type.

## Graph adoption — the big invasive middle (this step)

mere's `Graph.inner` is no longer a bare `petgraph::StableGraph`; it is a
`chartulary::Graph<Node, EdgePayload>`. The change:

- **The substrate owns topology and identity.** mere's hand-rolled `id_to_node`
  index is retired; chartulary's built-in `by_id` is the single identity index. Node
  add/remove route through `insert` / `remove` (which maintain the index); edge
  add/remove through `connect` / `disconnect`. Weight mutation uses `node_mut` /
  `edge_mut`.
- **Algorithms read through one seam.** chartulary grew a read-only
  [`inner()`](../../../chartulary/src/graph.rs) accessor returning the underlying
  `&StableGraph`, plus `contains_node` and `node_weights_mut`. mere's graph queries
  (shortest path, SCC, connectivity, structural iteration) run petgraph's own
  functions over `inner()`. Topology mutation cannot go through it (the borrow is
  immutable by design), so the identity index cannot drift.
- **Persistence untouched.** mere's rkyv `Archive`/`Serialize`/`Deserialize` for
  `Graph` delegate through `to_snapshot` / `from_snapshot` (`GraphSnapshot`), which
  never serialize `inner` directly. Swapping the field type left the on-disk format
  and the whole persistence path unchanged.
- **Proof.** All 284 kernel unit tests pass unchanged — snapshot round-trips,
  derivation, cross-graph copy, node history, fields/couplings, and the chart
  capability tests. The single-write-path revision boundary still holds (every
  mutator still bumps `revision`).

This is the load-bearing move of the two-layer vision: chartulary is the portable
topology-plus-identity engine; mere's `Graph` is the orrery wrapper that keeps its
app-specific sidecars (nav history, fields, couplings, import records, url index,
revision, session) around the substrate.

## What this proves

The generic substrate can hold mere's real web node. scholia's projector, being a
pure function of the traits, can already emit RDF for a `Graph<Node, ...>` with no
new projection code. The two-ring split, the fork lineage, and the RDF projection
all apply to mere's node the moment mere adopts `chartulary::Graph`.

## What remains (each its own step)

1. **Edit spine over the substrate.** Route mere's durable mutations through a
   `GraphLog<Node, EdgePayload>` (chartulary's spine over codicil) so the graph
   becomes the replay of an append-only log, then retire `graph/capture.rs`'s
   bespoke delta-capture. The single-write-path funnel (`apply::apply_graph_delta`)
   is already the one place to instrument.
2. **History over the spine + stemma.** Re-derive mere's `graph/history.rs`
   snapshots over codicil and retire the in-tree `node-lineage` copy in favour of
   `stemma` (node-level lineage) alongside the graph-level log.
3. **Content over muniment.** Implement `ContentBearing` on `Node` by moving node
   content behind a `muniment` blob (`muniment::Hash`), retiring the bespoke cache
   path for stored bodies.
4. **Analytics retarget.** Point aether (fields) and signals (centrality,
   community) at the generic graph, at which point they become promotable, closing
   the loop opened by the 2026-07-08 survey.
5. **Done-condition.** meerkat runs on the substrate graph with no behaviour change.
   The graph swap already meets this at the kernel level (284 tests unchanged); the
   remaining steps deepen the adoption (log, lineage, content) rather than gate it.

The graph swap (the big invasive middle) is done. The remaining steps are additive
deepenings, each its own focused increment.
