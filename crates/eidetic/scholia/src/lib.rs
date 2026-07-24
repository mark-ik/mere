//! scholia — the RDF projection over a chartulary graph.
//!
//! Scholia are the annotations written in a manuscript's margins: the commentary
//! layer over the text. This crate is that layer for a graph, projecting its
//! **semantic ring** into RDF so it can be shared and queried as linked data.
//!
//! The projection is driven entirely by chartulary's capability traits:
//!
//! - a node that is [`Addressed`](chartulary::Addressed) becomes an RDF subject
//!   (its primary address is the `@id`; a node without one gets a `urn:chart:`
//!   skolem IRI),
//! - a node that is [`Labeled`](chartulary::Labeled) contributes `schema:name`
//!   (title) and `schema:keywords` (tags) literals,
//! - an edge that is [`Predicated`](chartulary::Predicated) with a predicate IRI
//!   becomes a triple; an app-private relation (no predicate) is **not** projected.
//!
//! So only the shared semantic ring reaches RDF; an app's private families stay
//! private. Output is expanded [`to_jsonld`] and [`to_nquads`].
//!
//! This is the export half, harvested from mere's `linked-data` and re-seamed onto
//! the traits. The deeper harvest (compact JSON-LD with an `@context`, ingest and
//! round-trip, named-graph scopes, RDF 1.2 reified statement metadata, standard-
//! vocabulary alignment, and SPARQL via an `oxrdf`/`spareval` backend, with
//! linked-data's losslessness gate re-passed) is the roadmap in `design_docs/`.

pub mod project;

pub use project::{Quad, SCHEMA_KEYWORDS, SCHEMA_NAME, Term, to_jsonld, to_nquads, to_quads};
