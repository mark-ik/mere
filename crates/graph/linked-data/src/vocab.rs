// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Standard-vocabulary alignment for Mere's recognized relations (petgraph-RDF
//! plan, Phase 2's "standard-vocab mapping as a 3-category table").
//!
//! Mere projects its `Semantic` relations under canonical `mere:rel#` IRIs.
//! That keeps the kernel authoritative and the round trip byte-stable, but an
//! outside consumer has no way to know that `mere:rel#cites` *is* `cito:cites`.
//! This module publishes that knowledge as **alignment quads about the
//! properties themselves** — not by rewriting instance data, which is standard
//! linked-data practice: a vocabulary declares how its terms relate to others,
//! and a reasoner bridges the instances.
//!
//! Each recognized [`SemanticSubKind`] falls in one of three categories (the
//! compliance-audit cut in `2026-05-22_statements_over_schema_stance.md`):
//!
//! - **Exact** — the standard term means the same thing. Emitted as
//!   `mere:rel#x owl:equivalentProperty std:y` (a reasoner infers `std:y`
//!   wherever `mere:rel#x` appears, and vice versa). `Cites -> cito:cites`,
//!   `SameEntityAs -> owl:sameAs`.
//! - **Approximate** — the Mere term is *more specific* than a standard term.
//!   Emitted as `mere:rel#x rdfs:subPropertyOf std:y` (one-way: `mere:rel#x`
//!   entails `std:y`, not the reverse). `Summarizes -> cito:cites`,
//!   `DependsOn -> dcterms:requires`.
//! - **Mere-only** — no standard term is even a superproperty; the `mere:rel#`
//!   IRI stands alone. `Blocks`, `NextStep`, `UserGrouped`.
//!
//! The alignment set is a deterministic constant (a function of the vocabulary,
//! not of any graph's data), emitted into the [`GRAPH_VOCABULARY`] named graph so
//! a consumer can query or ignore it independently of the instance data. Because
//! it is re-derivable, the file-I/O ingest path treats this graph as schema and
//! drops it ([`is_vocabulary_quad`]), keeping a round trip through a shared file
//! lossless.

use kernel::graph::{SemanticSubKind, all_semantic_sub_kinds, predicate_iri};
use oxrdf::{GraphName, NamedNode, Quad};

/// The named graph carrying Mere's vocabulary alignment. Kept out of the default
/// graph and the scope graphs so alignment (schema) never mixes with instance
/// facts in a consumer's default-graph query.
pub const GRAPH_VOCABULARY: &str = "https://mere.computer/ns/graph#vocabulary";

/// `owl:equivalentProperty` — links an *exact* Mere/standard term pair.
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
/// `rdfs:subPropertyOf` — links an *approximate* (Mere-more-specific) term to its
/// standard superproperty.
const RDFS_SUB_PROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";

/// How a recognized Mere relation aligns to a standard-vocabulary term.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    /// Same meaning as the standard term (`owl:equivalentProperty`).
    Exact(&'static str),
    /// More specific than the standard term (`rdfs:subPropertyOf`).
    Approximate(&'static str),
    /// No standard superproperty; the `mere:rel#` IRI stands alone.
    MereOnly,
}

/// The standard-vocabulary alignment for a recognized semantic sub-kind.
///
/// The exact/CiTO/OWL/PROV anchors are the compliance audit's
/// (`statements_over_schema_stance`); the remainder are categorized
/// conservatively — a relation is only mapped when a standard term genuinely is
/// the same as, or a superproperty of, the Mere term, and is left `MereOnly`
/// otherwise rather than forced onto an ill-fitting IRI. The `match` is
/// exhaustive, so a newly added [`SemanticSubKind`] must be categorized here
/// before it compiles.
pub fn alignment(sub_kind: SemanticSubKind) -> Alignment {
    use Alignment::{Approximate, Exact, MereOnly};
    use SemanticSubKind::*;

    // CiTO — the Citation Typing Ontology (`http://purl.org/spar/cito/`).
    const CITO_CITES: &str = "http://purl.org/spar/cito/cites";
    const CITO_INCLUDES_QUOTATION_FROM: &str = "http://purl.org/spar/cito/includesQuotationFrom";
    const CITO_AGREES_WITH: &str = "http://purl.org/spar/cito/agreesWith";
    const CITO_DISAGREES_WITH: &str = "http://purl.org/spar/cito/disagreesWith";
    // OWL / RDFS / Dublin Core Terms.
    const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
    const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";
    const DCTERMS_REQUIRES: &str = "http://purl.org/dc/terms/requires";

    match sub_kind {
        // Exact CiTO / OWL correspondences (stance-doc anchored).
        Cites => Exact(CITO_CITES),
        Quotes => Exact(CITO_INCLUDES_QUOTATION_FROM),
        Supports => Exact(CITO_AGREES_WITH),
        Contradicts => Exact(CITO_DISAGREES_WITH),
        SameEntityAs => Exact(OWL_SAME_AS),

        // Approximate: the Mere relation is a narrower case of a standard term.
        // Summarizing / elaborating / questioning a work all entail referencing
        // it, so each is a subproperty of `cito:cites`.
        Summarizes | Elaborates | Questions => Approximate(CITO_CITES),
        // A hyperlink is a (structural) "see also".
        Hyperlink => Approximate(RDFS_SEE_ALSO),
        // A dependency is a narrower "requires".
        DependsOn => Approximate(DCTERMS_REQUIRES),

        // Mere-only: no standard term is even a superproperty. Grouping and
        // agent-derivation are experience/provenance flavored; example-of and
        // the near-identity pair (duplicate / canonical-mirror) have no safe
        // standard superproperty (subPropertyOf owl:sameAs would over-claim
        // logical identity); Blocks / NextStep are workflow relations.
        UserGrouped | AgentDerived | ExampleOf | DuplicateOf | CanonicalMirrorOf | Blocks
        | NextStep => MereOnly,
    }
}

/// The vocabulary-alignment quads: for every recognized semantic relation with a
/// standard correspondence, one quad linking its canonical `mere:rel#` IRI to a
/// standard term (`owl:equivalentProperty` for [`Alignment::Exact`],
/// `rdfs:subPropertyOf` for [`Alignment::Approximate`]), in the
/// [`GRAPH_VOCABULARY`] named graph. [`Alignment::MereOnly`] relations contribute
/// nothing. A deterministic constant, independent of any graph's data.
pub fn vocabulary_alignment_quads() -> Vec<Quad> {
    let Ok(graph) = NamedNode::new(GRAPH_VOCABULARY) else {
        return Vec::new();
    };
    let graph = GraphName::from(graph);
    let mut quads = Vec::new();
    for sub_kind in all_semantic_sub_kinds() {
        let (alignment_predicate, standard) = match alignment(sub_kind) {
            Alignment::Exact(iri) => (OWL_EQUIVALENT_PROPERTY, iri),
            Alignment::Approximate(iri) => (RDFS_SUB_PROPERTY_OF, iri),
            Alignment::MereOnly => continue,
        };
        if let (Ok(subject), Ok(predicate), Ok(object)) = (
            NamedNode::new(predicate_iri(sub_kind)),
            NamedNode::new(alignment_predicate),
            NamedNode::new(standard),
        ) {
            quads.push(Quad::new(subject, predicate, object, graph.clone()));
        }
    }
    quads
}

/// Whether a quad belongs to the vocabulary-alignment graph. The file-I/O ingest
/// path drops these (they are re-derivable schema, not instance content) so a
/// round trip through a shared N-Quads / TriG file stays lossless.
pub fn is_vocabulary_quad(quad: &Quad) -> bool {
    matches!(&quad.graph_name, GraphName::NamedNode(g) if g.as_str() == GRAPH_VOCABULARY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sub_kind_is_categorized_and_maps_to_a_valid_iri() {
        for sub_kind in all_semantic_sub_kinds() {
            match alignment(sub_kind) {
                Alignment::Exact(iri) | Alignment::Approximate(iri) => {
                    assert!(
                        NamedNode::new(iri).is_ok(),
                        "{sub_kind:?} maps to a valid standard IRI"
                    );
                }
                Alignment::MereOnly => {}
            }
        }
    }

    #[test]
    fn alignment_quads_use_the_right_predicate_per_category() {
        let quads = vocabulary_alignment_quads();

        let cites = predicate_iri(SemanticSubKind::Cites);
        let cites_quad = quads
            .iter()
            .find(|q| q.subject.to_string().contains(cites))
            .expect("cites has an alignment quad");
        assert_eq!(cites_quad.predicate.as_str(), OWL_EQUIVALENT_PROPERTY);
        assert_eq!(
            cites_quad.object.to_string(),
            "<http://purl.org/spar/cito/cites>"
        );
        assert!(
            is_vocabulary_quad(cites_quad),
            "alignment lives in the vocab graph"
        );

        let hyperlink = predicate_iri(SemanticSubKind::Hyperlink);
        let hyperlink_quad = quads
            .iter()
            .find(|q| q.subject.to_string().contains(hyperlink))
            .expect("hyperlink has an alignment quad");
        assert_eq!(hyperlink_quad.predicate.as_str(), RDFS_SUB_PROPERTY_OF);

        // A Mere-only relation contributes no alignment quad.
        let blocks = predicate_iri(SemanticSubKind::Blocks);
        assert!(
            !quads.iter().any(|q| q.subject.to_string().contains(blocks)),
            "Blocks is Mere-only and must not be aligned"
        );
    }

    #[test]
    fn alignment_is_deterministic() {
        assert_eq!(vocabulary_alignment_quads(), vocabulary_alignment_quads());
    }
}
