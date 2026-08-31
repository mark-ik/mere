// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The relation taxonomy: two rings.
//!
//! Relations come in two kinds, per the substrate plan:
//!
//! - **The shared semantic ring** ([`RelationClass::Semantic`]): interoperable
//!   knowledge relations. A [`Recognized`] core with canonical IRIs, plus open
//!   predicates (any raw IRI, passed through verbatim). Only this ring projects
//!   to RDF (via [`Predicated`](crate::Predicated)).
//! - **App-private families** ([`RelationClass::App`]): an app's own typed
//!   relations, namespaced by family. The experience layer: never projected.
//!
//! The family registry is the "compact core + open app namespace" shape (plan
//! section 4, option a): a fixed recognized enum, plus a `(family, kind)` pair
//! for app relations. Standard-vocabulary alignment (mapping [`Recognized`] to
//! schema.org / CiTO / SKOS) belongs to the [`rdf`](crate::rdf) projection,
//! not the substrate taxonomy.

use serde::{Deserialize, Serialize};

/// The chartulary namespace for recognized-core predicate IRIs. `urn:`-scoped so
/// the substrate claims no domain; [`rdf`](crate::rdf) can align them to standard
/// vocabularies downstream.
pub const REL_NS: &str = "urn:chart:rel:";

/// The class of a relation: which ring it belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationClass {
    /// The shared, RDF-projecting semantic ring.
    Semantic(Semantic),
    /// An app-private family: `family` namespaces the app, `kind` selects the
    /// relation within it. Not projected to RDF.
    App { family: String, kind: u16 },
}

impl RelationClass {
    /// A recognized-core semantic relation.
    pub fn recognized(rel: Recognized) -> Self {
        RelationClass::Semantic(Semantic::Recognized(rel))
    }

    /// An open semantic relation with a raw predicate IRI.
    pub fn open(iri: impl Into<String>) -> Self {
        RelationClass::Semantic(Semantic::Open(iri.into()))
    }

    /// An app-private relation family.
    pub fn app(family: impl Into<String>, kind: u16) -> Self {
        RelationClass::App {
            family: family.into(),
            kind,
        }
    }

    /// The predicate IRI this relation projects to, or `None` for an app family.
    pub fn predicate(&self) -> Option<&str> {
        match self {
            RelationClass::Semantic(Semantic::Recognized(rel)) => Some(rel.iri()),
            RelationClass::Semantic(Semantic::Open(iri)) => Some(iri.as_str()),
            RelationClass::App { .. } => None,
        }
    }
}

/// A semantic-ring relation: recognized or open.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Semantic {
    /// A relation in the recognized core, with a canonical IRI.
    Recognized(Recognized),
    /// An open predicate: any IRI, verbatim.
    Open(String),
}

/// The recognized-core relations. A small, stable starter set of general knowledge
/// relations, each with a canonical IRI. Apps add their own via open predicates or
/// app families; this set grows deliberately, not per-app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Recognized {
    /// The subject cites the object.
    Cites,
    /// The subject supports the object's claim.
    Supports,
    /// The subject contradicts the object's claim.
    Contradicts,
    /// The subject elaborates on the object.
    Elaborates,
    /// The subject is an example of the object.
    ExampleOf,
    /// The subject summarizes the object.
    Summarizes,
    /// The subject and object denote the same entity.
    SameEntityAs,
    /// The subject depends on the object.
    DependsOn,
}

impl Recognized {
    /// The canonical IRI for this relation.
    pub fn iri(self) -> &'static str {
        match self {
            Recognized::Cites => "urn:chart:rel:cites",
            Recognized::Supports => "urn:chart:rel:supports",
            Recognized::Contradicts => "urn:chart:rel:contradicts",
            Recognized::Elaborates => "urn:chart:rel:elaborates",
            Recognized::ExampleOf => "urn:chart:rel:exampleOf",
            Recognized::Summarizes => "urn:chart:rel:summarizes",
            Recognized::SameEntityAs => "urn:chart:rel:sameEntityAs",
            Recognized::DependsOn => "urn:chart:rel:dependsOn",
        }
    }

    /// Recognize a canonical IRI back into a core relation, or `None` if it is an
    /// open predicate outside the recognized core.
    pub fn from_iri(iri: &str) -> Option<Self> {
        Some(match iri {
            "urn:chart:rel:cites" => Recognized::Cites,
            "urn:chart:rel:supports" => Recognized::Supports,
            "urn:chart:rel:contradicts" => Recognized::Contradicts,
            "urn:chart:rel:elaborates" => Recognized::Elaborates,
            "urn:chart:rel:exampleOf" => Recognized::ExampleOf,
            "urn:chart:rel:summarizes" => Recognized::Summarizes,
            "urn:chart:rel:sameEntityAs" => Recognized::SameEntityAs,
            "urn:chart:rel:dependsOn" => Recognized::DependsOn,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognized_iri_round_trips() {
        for rel in [
            Recognized::Cites,
            Recognized::Supports,
            Recognized::Contradicts,
            Recognized::Elaborates,
            Recognized::ExampleOf,
            Recognized::Summarizes,
            Recognized::SameEntityAs,
            Recognized::DependsOn,
        ] {
            assert_eq!(Recognized::from_iri(rel.iri()), Some(rel));
            assert!(rel.iri().starts_with(REL_NS));
        }
    }

    #[test]
    fn semantic_ring_projects_app_families_do_not() {
        assert_eq!(
            RelationClass::recognized(Recognized::Cites).predicate(),
            Some("urn:chart:rel:cites")
        );
        assert_eq!(
            RelationClass::open("https://schema.org/citation").predicate(),
            Some("https://schema.org/citation")
        );
        assert_eq!(RelationClass::app("isometry", 3).predicate(), None);
    }
}
