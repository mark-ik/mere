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

## Graph adoption — the big invasive middle (committed 5918d70)

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

## Edit spine — the codicil-backed journal (this step)

mere's durable mutations already exist as a stream of `CapturedDelta`s (stable-id,
serializable), emitted through the capture hook as each live `GraphDelta` applies.
This step gives that stream its principled home over the substrate: a
`codicil::Codicil<CapturedDelta>`, wrapped as `graph::GraphJournal`.

- **The graph is the replay of the journal.** `GraphJournal::replay` rebuilds the
  graph by folding the log through `apply_graph_delta`; `replay_from(seq, &mut graph)`
  advances a checkpoint (a restored `GraphSnapshot`) by only the newer entries. Live
  editing and replay share `apply_graph_delta`, so they cannot diverge, proven by a
  test that builds a graph by live mutation and asserts a journal replay reconstructs
  it identically.
- **Why mere's own vocabulary, not chartulary's `GraphLog`.** chartulary's `GraphLog`
  is topology-only (insert/remove/connect/disconnect) with a read-only `graph()`, so
  it cannot express mere's in-place content edits (title, tags, body, navigation,
  traversal). mere keeps its rich `CapturedDelta` vocabulary and takes only codicil,
  the append-only log primitive, beneath it. This is the two-layer split at the log
  level: codicil is the portable primitive, mere's journal is the orrery's edit
  language over it.
- **Ordered, forkable, persistable.** A journal stamps a monotonic `Seq` (a durable
  cursor for replication and tail-replay), forks with provenance (the log-level mirror
  of a graph fork), and saves/loads whole through a muniment slot.
  `journal_capture_hook` installs a hook feeding a shared journal, the host's
  codicil-backed persistence path.
- **Proof.** Four tests green (record+replay, fork+diverge, muniment round-trip, and
  the live-vs-replay invariant); all 288 kernel tests pass. codicil and muniment were
  already in the tree via chartulary, so the direct deps add no new crates.

## What this proves

The generic substrate can hold mere's real web node. scholia's projector, being a
pure function of the traits, can already emit RDF for a `Graph<Node, ...>` with no
new projection code. The two-ring split, the fork lineage, and the RDF projection
all apply to mere's node the moment mere adopts `chartulary::Graph`.

## What remains (each its own step)

1. **Retire the bespoke capture persistence.** The journal primitive is landed
   (`graph::GraphJournal`, a `codicil::Codicil<CapturedDelta>`). The remaining work is
   host-side: point the app's persistence at a `GraphJournal` (via
   `journal_capture_hook`) and retire the ad-hoc capture-hook plumbing, so codicil is
   the one durable history. `graph/capture.rs`'s `CapturedDelta` vocabulary stays (it
   is mere's edit language); only its bespoke storage goes. Note the discovered
   constraint: chartulary's `GraphLog` is topology-only, so mere's spine is codicil
   under mere's own vocabulary, not `GraphLog<Node, EdgePayload>` as first sketched.
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
