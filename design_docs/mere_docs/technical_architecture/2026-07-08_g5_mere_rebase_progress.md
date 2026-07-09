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

## Host adoption + lineage on stemma (this step)

- **Host adoption of the journal.** meerkat's `graph_delta_log.rs` (the env-gated
  session delta logger, the one host consumer of the capture hook) now maintains and
  replays through a `GraphJournal` (the kernel spine type), not a raw `CapturedDelta`
  vec. The streaming `.postcardlog` file stays as the crash-safe transport: codicil's
  founding persistence rewrites the whole log per save, so per-delta streaming stays
  the durable path until codicil grows append-friendly, per-entry persistence (its own
  roadmap). The production snapshot-plus-persisted-tail model (crash recovery via
  `GraphSnapshot` checkpoint + journal tail) is the larger follow-on, a deliberate
  design touching session save/restore.
- **Lineage on stemma; node-lineage retired.** The kernel's nav-history authority
  (`graph/history.rs`'s `SharedNavigationMemory`) now sits on `stemma::Stemma`
  (`StemmaSnapshot`), not the in-tree MPL `node-lineage`. Because stemma is
  node-lineage promoted near-verbatim (`GraphMemory` → `Stemma`), the swap is aliased
  imports and the nav model, snapshot format, and every history test are unchanged
  (288 kernel tests pass). eidetic's `browsing::lineage` bridge migrated the same way,
  so `node-lineage` has zero consumers and the crate is removed from the workspace.

## Content-bearing nodes (this step)

mere's `Node` now implements `ContentBearing`, completing the node capability set
(Identified + Addressed + Labeled + ContentBearing; the edge has Predicated +
Classified). `content()` is the blake3 `muniment::Hash` of the node's inline authored
body (`Node::body`, a knot note's djot), the address that body would have in a muniment
`BlobStore`, computed with `Hash::of` (sync, no store needed). So a node names its
content's identity while the bytes still live inline. `media_type()` is the node's
`mime_hint`. A bare web tab with no authored body has no graph-owned content (its
fetched page lives in mere's cache, off the node). 289 kernel tests pass. This unlocks
scholia projecting content identity and the cross-vault content-addressing story.

## What remains

1. **Content bytes out-of-line (follow-on).** Move `Node::body` bytes into an actual
   `muniment::BlobStore` (store only the `Hash` on the node) and content-address fetched
   web pages that live in mere's cache. `ContentBearing` already names the identity; this
   moves the storage. A data-model migration, its own deliberate step.
2. **Analytics retarget.** Point aether (fields) and signals (centrality, community) at
   the generic `chartulary::Graph<N, E>` rather than mere's concrete `Graph`, at which
   point they become promotable, closing the loop opened by the 2026-07-08 survey. A real
   genericization (each structural algorithm re-bounded on the substrate's node/edge
   capabilities), possibly growing chartulary's read surface.
3. **Production journal persistence (follow-on).** A persisted tail journal alongside the
   `GraphSnapshot` checkpoint for crash recovery, once codicil grows append-friendly
   persistence. A deliberate design, not a rush.
4. **Done-condition.** meerkat runs on the substrate graph with no behaviour change. The
   graph swap already meets this at the kernel level (289 tests unchanged); the remaining
   steps deepen the adoption rather than gate it.

The graph swap (the big invasive middle), the edit spine, host adoption, the lineage
migration, and content-bearing nodes are done. The remaining steps are additive
deepenings.
