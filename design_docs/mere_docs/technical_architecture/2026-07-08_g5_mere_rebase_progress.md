# G5: mere re-base onto chartulary — progress

**Date:** 2026-07-08
**Status:** started. The first step landed (mere's `Node` implements the chartulary
capability traits) and it surfaced and fixed a real trait-API friction. The full
re-base (graph adoption, history re-derivation, node-lineage retirement,
aether/signals retarget) is the remaining, larger work. Canonical plan:
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

## What this proves

The generic substrate can hold mere's real web node. scholia's projector, being a
pure function of the traits, can already emit RDF for a `Graph<Node, ...>` with no
new projection code. The two-ring split, the fork lineage, and the RDF projection
all apply to mere's node the moment mere adopts `chartulary::Graph`.

## What remains (the larger re-base, each its own step)

1. **Edge capabilities.** Implement `Classified` / `Predicated` on mere's edge
   payload, mapping its recognized `SemanticSubKind` to `urn:chart:rel:*` (or the
   eventual standard IRIs) and its experience families (Traversal, Arrangement,
   Imported) to app-private relations. This lets mere's semantic edges project and
   keeps its experience edges private, matching linked-data's existing behaviour.
2. **Graph adoption.** Introduce a `chartulary::Graph<Node, WebEdge>` alongside
   mere's concrete `Graph`, or parameterize the concrete one, and route reads
   through the generic surface. This is the big, invasive middle; do it behind a
   seam so meerkat keeps building throughout.
3. **History over the spine.** Re-derive mere's `graph/history.rs` and
   `capture.rs` snapshots over codicil, retiring the bespoke machinery, and retire
   the in-tree `node-lineage` copy in favour of `stemma`.
4. **Analytics retarget.** Point aether (fields) and signals (centrality,
   community) at the generic graph, at which point they become promotable, closing
   the loop opened by the 2026-07-08 survey.
5. **Done-condition.** meerkat runs on the substrate graph with no behaviour change.

Steps 2 through 5 are large and touch mere's live tree; they want their own focused
sessions rather than being rushed. Step 1 (edge capabilities) is the natural next
increment and is additive like step 0.
