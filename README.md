# scholia

The RDF projection over a [chartulary](https://github.com/mark-ik/chartulary)
graph's semantic ring. Scholia are the annotations in a manuscript's margins; this
crate is that commentary layer for a graph, projecting it into linked data.

The projection is driven by chartulary's capability traits:

- an `Addressed` node becomes an RDF subject (its primary address is the `@id`, or a
  `urn:chart:` skolem IRI if it has none),
- a `Labeled` node contributes `schema:name` and `schema:keywords` literals,
- a `Predicated` edge (one carrying a predicate IRI) becomes a triple; an
  app-private relation is **not** projected.

Only the shared semantic ring reaches RDF. An app's private relation families stay
private, by construction.

```rust
use scholia::{to_jsonld, to_nquads};

let json = to_jsonld(graph);   // expanded JSON-LD
let doc = to_nquads(graph);    // N-Quads
```

This is the export half, harvested from mere's `linked-data` and re-seamed onto the
traits. Compact JSON-LD with `@context`, ingest and round-trip, named-graph scopes,
RDF 1.2 reified statement metadata, standard-vocabulary alignment, and SPARQL (via
an `oxrdf`/`spareval` backend) are the roadmap. See [`design_docs/`](design_docs/).

License: dual MIT OR Apache-2.0, at your option.
