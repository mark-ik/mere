# scholia Founding Proposal

**Date:** 2026-07-08
**Status:** founded. G4 of the generic graph substrate program (canonical plan:
mere's
`design_docs/mere_docs/technical_architecture/2026-07-08_generic_graph_substrate_plan.md`).
The export half of the RDF projection is built and green (5 tests).

## 1. What scholia is

The RDF projection over a chartulary graph. It reads the graph through the
capability traits and emits linked data:

- `Addressed` node to RDF subject (`@id` = primary address, else a `urn:chart:`
  skolem IRI).
- `Labeled` node to `schema:name` (title) and `schema:keywords` (tags) literals.
- `Predicated` edge to a triple (subject, predicate IRI, object subject). An edge
  whose `predicate()` is `None`, an app-private relation, is not projected.

Output is expanded [`to_jsonld`] and [`to_nquads`], plus the raw [`to_quads`].

## 2. Why this is the right shape

The projection is a pure function of the capability traits, not of any concrete
node type. That is the whole point of chartulary's trait seam: the same projector
serves woodshed's `Container`, isometry's entity, and (at G5) mere's `WebNode`,
because all it needs is `Addressed + Labeled` on nodes and `Predicated` on edges.

And it enforces the two-ring split mechanically. Only the semantic ring carries
predicate IRIs, so only the semantic ring reaches RDF; an app's private families
are invisible to the projection with no extra gate. The G3 consumer (woodshed)
exercised the app-private ring; scholia exercises the semantic ring, and the split
holds on first contact (a test asserts an app-family edge produces no triple).

## 3. What is founded, what is roadmap

Founded (the export half): the trait-driven projection to quads, expanded JSON-LD,
and N-Quads. Curated literals (`schema:name` / `schema:keywords`), skolemization of
address-less nodes, semantic-ring-only edges.

Roadmap (harvested from mere's `linked-data`, 3,450 LOC, as the deeper cut lands):

- **Compact JSON-LD** with an inline `@context` (recognized predicates as short
  terms).
- **Ingest and round-trip**: parse JSON-LD / N-Quads back into graph edits, and
  re-pass linked-data's losslessness gate over the generic substrate (the
  acceptance bar named in the plan).
- **Named-graph scopes** and **RDF 1.2 reified statement metadata** (per-statement
  provenance, assertion time, labels).
- **Standard-vocabulary alignment**: map chartulary's `urn:chart:rel:*` recognized
  core to schema.org / CiTO / SKOS via `owl:equivalentProperty` /
  `rdfs:subPropertyOf`.
- **SPARQL** over the projected dataset, via an `oxrdf` / `spareval` backend.

The founding stays dependency-light (`serde` + `serde_json`, its own small term
model) so the export half is clean and portable; the `oxrdf`/`spareval` stack
enters with the ingest, SPARQL, and gate work, matching mere's library choices at
that point.

## 4. The name

*scholia* are the explanatory annotations copied into a manuscript's margins.
Sitting in the family: muniment keeps the records, codicil appends the amendments,
chartulary binds them into the register, stemma traces the descent of copies, and
scholia writes the commentary that makes the register legible to the outside world,
which is exactly what an RDF projection is: the graph, annotated for others to read.

## Provenance

Grounded in the 2026-07-08 read of mere's `linked-data` (lib, the JSON-LD / N-Quads
/ SPARQL machinery, the CiTO alignment, the losslessness gate) and built on
chartulary's capability traits. Sibling to chartulary (the graph), muniment (the
store), codicil (the log), stemma (the lineage).
