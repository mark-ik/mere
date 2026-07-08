/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! chartulary capability-trait impls for the web [`Node`] (graph re-base, G5).
//!
//! mere's `Node` is the canonical *foreign* implementor of the chartulary
//! container capabilities: its `id` is the stable identity, its address claims map
//! to scheme-qualified addresses, and its title and tags are the curated labels.
//! This is the first step of the graph re-base: the real web node satisfies the
//! generic substrate's node traits, so the substrate can hold and project it. The
//! browser-runtime facets (favicon, viewer routing, session restore, lifecycle)
//! stay on `Node` and simply do not participate in the generic capabilities.
//!
//! `ContentBearing` is deliberately not implemented yet: mere addresses content
//! through its own cache, not a muniment blob, so adopting `muniment::Hash` for
//! node content is a later re-base step.

use chartulary::{Address, Addressed, Classified, Identified, Labeled, Predicated, RelationClass};
use uuid::Uuid;

use super::edge_data::predicate_iri;
use super::edge_payload::EdgePayload;
use super::node::Node;

impl Identified for Node {
    type Id = Uuid;

    fn id(&self) -> &Uuid {
        &self.id
    }
}

impl Addressed for Node {
    fn addresses(&self) -> Vec<Address> {
        // Map mere's `AddressClaim`s to chartulary `Address`es, primary first.
        let mut primary = None;
        let mut aliases = Vec::new();
        for claim in &self.addresses {
            let address = Address::new(claim.address.as_url_str());
            if claim.is_primary() {
                primary = Some(address);
            } else {
                aliases.push(address);
            }
        }
        let mut out = Vec::new();
        out.extend(primary);
        out.extend(aliases);
        out
    }
}

impl Labeled for Node {
    fn title(&self) -> Option<&str> {
        // An untitled node seeds `title = url`; an empty title is "no title".
        if self.title.is_empty() {
            None
        } else {
            Some(&self.title)
        }
    }

    fn tags(&self) -> Vec<String> {
        self.tags.iter().cloned().collect()
    }
}

/// The single predicate IRI a semantic edge projects, or `None` if the edge
/// carries no semantic relation (an experience-layer edge: Traversal, Containment,
/// Arrangement, Imported). Precedence matches linked-data: an explicit statement or
/// open predicate wins, else the first recognized sub-kind's canonical IRI.
fn edge_predicate(payload: &EdgePayload) -> Option<&str> {
    let semantic = payload.semantic.as_ref()?;
    if let Some(statement) = semantic.statements.first() {
        Some(statement.predicate.as_str())
    } else if let Some(predicate) = &semantic.predicate {
        Some(predicate.as_str())
    } else if let Some(&sub_kind) = semantic.sub_kinds.iter().next() {
        Some(predicate_iri(sub_kind))
    } else {
        None
    }
}

impl Predicated for EdgePayload {
    fn predicate(&self) -> Option<&str> {
        edge_predicate(self)
    }
}

impl Classified for EdgePayload {
    fn class(&self) -> RelationClass {
        match edge_predicate(self) {
            // A semantic edge joins the shared ring as an open predicate (its IRI),
            // so it projects to RDF.
            Some(iri) => RelationClass::open(iri.to_string()),
            // An experience-layer edge is mere's private family: it does not project.
            None => RelationClass::app("mere", 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::edge_taxonomy::{EdgeAssertion, SemanticSubKind};

    #[test]
    fn web_node_satisfies_the_container_capabilities() {
        // A compile-time proof that mere's Node implements the substrate's node
        // capability traits.
        fn assert_caps<N: Identified<Id = Uuid> + Addressed + Labeled>() {}
        assert_caps::<Node>();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_web_node_projects_its_identity_address_and_label() {
        let mut node = Node::test_stub("https://example.test/paper");
        node.title = "A Paper".to_string();
        node.tags.insert("research".to_string());

        assert_eq!(Identified::id(&node), &node.id);
        assert_eq!(
            Addressed::primary_address(&node).unwrap().as_str(),
            "https://example.test/paper"
        );
        assert_eq!(Labeled::title(&node), Some("A Paper"));
        assert_eq!(Labeled::tags(&node), vec!["research".to_string()]);
    }

    #[test]
    fn semantic_edges_project_experience_edges_do_not() {
        // A semantic edge carries a predicate IRI and joins the RDF-projecting ring.
        let mut cites = EdgePayload::new();
        cites.assert_relation(EdgeAssertion::Semantic {
            sub_kind: SemanticSubKind::Cites,
            label: None,
            decay_progress: None,
        });
        assert_eq!(
            Predicated::predicate(&cites),
            Some(predicate_iri(SemanticSubKind::Cites))
        );
        assert!(
            Classified::class(&cites).predicate().is_some(),
            "a semantic edge projects"
        );

        // An edge with no semantic relation is experience-layer: it does not project.
        let experience = EdgePayload::new();
        assert_eq!(Predicated::predicate(&experience), None);
        assert!(
            Classified::class(&experience).predicate().is_none(),
            "an experience edge stays private"
        );
    }
}
