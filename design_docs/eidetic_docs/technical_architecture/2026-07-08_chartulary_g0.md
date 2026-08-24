# chartulary G0

**Date:** 2026-07-08
**Status:** G0 landed. The canonical program plan lives in mere:
`design_docs/mere_docs/technical_architecture/2026-07-08_generic_graph_substrate_plan.md`
(phases, decisions, the wider stack). This doc records what the G0 cut is and the
choices made in it.

## What G0 is

The skeleton of the substrate: the generic graph, the capability traits, the
default payloads, and the two-ring taxonomy. Done-condition met: a toy graph
builds, queries, filters, and serde round-trips on the default payloads (9 tests).

- **`Graph<N, E>`** on petgraph `StableGraph` (stable keys). One required bound,
  `N: Identified`, backing an id-to-key index so nodes are found by stable
  identity, not only by position. Multigraph (parallel edges allowed).
- **Capability traits** (`caps.rs`): `Identified` (required); `Addressed`,
  `ContentBearing` (a muniment `Hash` + media type), `Labeled` on nodes;
  `Classified`, `Predicated` on edges. Each unlocks a query or a projection.
  Capability-bounded methods (`nodes_tagged`, `out_edges_of_class`) demonstrate
  the pattern.
- **Default payloads** (`container.rs`): `Container` (string id, addresses,
  content hash, media type, title, tags) and `Relation` (class + label),
  implementing every trait. An app instantiates `Graph<Container, Relation>` to
  start, or supplies its own payload.
- **Two-ring taxonomy** (`taxonomy.rs`): `RelationClass::Semantic` (a `Recognized`
  core with canonical IRIs, plus `Open` predicate IRIs) projects to RDF;
  `RelationClass::App { family, kind }` does not. This is the plan's section-4
  option (a): a fixed core plus an open `(family, kind)` app namespace.

## Decisions made in G0

- **Family registry: recognized enum + `(family: String, kind: u16)`.** The open
  app namespace is string-keyed on the family and `u16`-keyed on the kind. The
  compact `u32` transport tag from mere's precedent is deferred until a
  render/hit-test layer needs it (plan open question 1); the string family is fine
  until then and simpler.
- **Recognized-core IRIs are `urn:chart:rel:*`.** A `urn:` namespace so the
  substrate claims no domain. Alignment to standard vocabularies (schema.org,
  CiTO, SKOS) is `scholia`'s job at G4, not baked in here.
- **Default `Container` identity is `String`.** The caller decides what identity
  means (URN, slug, content-hash hex). mere's `Uuid`-bearing web node is a
  separate payload, not the default.
- **Content is out-of-line.** A node holds only the `muniment::Hash` of its body,
  not the bytes. muniment is a `default-features = false` dep (the `Hash` type
  only, no codecs).
- **The id index is never serialized.** `Serialize` delegates to the inner graph;
  `Deserialize` rebuilds the index from the nodes, so it cannot drift.
- **Multigraph semantics** (plan open question 2): parallel edges allowed, matching
  petgraph. mere's one-edge-per-pair-with-many-statements model, if wanted, layers
  on top rather than constraining the core.

## Not in G0 (later phases)

The edit spine over codicil (G1), stemma lineage (G2), a first real consumer (G3),
the scholia RDF projection (G4), and mere's re-base plus analytics retarget (G5).
Position/velocity, rendering, physics, and web-runtime facets stay out of the core
by design; they are a consumer's payload concern.
