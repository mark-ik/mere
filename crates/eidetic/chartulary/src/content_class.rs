// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Content classes: node typing defined as data.
//!
//! A **content class** is the node-side analog of the edge
//! [`taxonomy`](crate::taxonomy): it names a kind of node (a web page, a note, a
//! denizen, a media item) by the [facets](crate::facet) its members carry. A
//! class is *data*, not code: `class_id` + the facets a member must carry +
//! each facet's schema reference. So a modder ships a new content class the way
//! they ship anything else (a pack carrying the class document plus its facet
//! schemas), and the browser's own "web page" is just the first such class.
//!
//! Validation reuses the facet [`FacetValidator`](crate::facet::FacetValidator)
//! seam: a class [`admits`](ContentClass::admits) a node when every required
//! facet is present and each validates. chartulary stays schema-agnostic; the
//! schema references a class declares are for distribution and for configuring
//! the host's validator (eidetic mere-side). An **unknown class is inert**: a
//! node tagged with a class this build has no definition for reads back as
//! [`ClassMembership::Unknown`], never an error.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::facet::{FacetError, FacetId, FacetValidator, NodeFacets};

/// The reserved facet id a node uses to declare its content class. Its value is
/// the class id string.
pub const CLASS_FACET: &str = "chartulary.class";

/// The stable id of a content class, e.g. `mere.web-page`, `mere.note`,
/// `servitor.denizen`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClassId(pub String);

impl ClassId {
    /// Name a class.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A content class, defined as data: which facets a member must carry, and the
/// schema reference for each (an opaque string; a host resolves it, e.g. to an
/// Eidetic schema-codicil id).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentClass {
    /// The class id.
    pub class_id: ClassId,
    /// Human-facing label.
    #[serde(default)]
    pub label: Option<String>,
    /// The facets a member must carry, each with its schema reference.
    #[serde(default)]
    pub required_facets: BTreeMap<FacetId, String>,
}

/// Why a node is not a member of a class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClassError {
    /// A required facet is absent.
    MissingFacet(FacetId),
    /// A required facet is present but failed validation.
    InvalidFacet(FacetError),
}

impl ContentClass {
    /// A class requiring the named facets (schema refs paired). `label` unset.
    pub fn new(
        class_id: impl Into<String>,
        required_facets: impl IntoIterator<Item = (FacetId, String)>,
    ) -> Self {
        Self {
            class_id: ClassId::new(class_id),
            label: None,
            required_facets: required_facets.into_iter().collect(),
        }
    }

    /// Attach a human-facing label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Whether `facets` satisfy this class: every required facet present and
    /// valid under `validator`. The membership check, using the same seam the
    /// facet store validates writes with.
    pub fn admits(
        &self,
        facets: &NodeFacets,
        validator: &impl FacetValidator,
    ) -> Result<(), ClassError> {
        for facet_id in self.required_facets.keys() {
            let value = facets
                .get(facet_id)
                .ok_or_else(|| ClassError::MissingFacet(facet_id.clone()))?;
            validator
                .validate(facet_id, value)
                .map_err(ClassError::InvalidFacet)?;
        }
        Ok(())
    }
}

/// A registry of known content classes. Classes not present read as unknown
/// (inert), never an error, so a graph carrying a class this build does not
/// define still loads and the class is discoverable by id.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassRegistry {
    classes: BTreeMap<ClassId, ContentClass>,
}

/// The result of reading a node's declared class against a registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClassMembership<'a> {
    /// The node declares no class.
    Unclassified,
    /// The node declares a class this registry knows.
    Known(&'a ContentClass),
    /// The node declares a class this build has no definition for (inert).
    Unknown(ClassId),
}

impl ClassRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) a class, returning any prior definition.
    pub fn register(&mut self, class: ContentClass) -> Option<ContentClass> {
        self.classes.insert(class.class_id.clone(), class)
    }

    /// Look up a class by id.
    pub fn get(&self, class_id: &ClassId) -> Option<&ContentClass> {
        self.classes.get(class_id)
    }

    /// The class a node declares (via the reserved [`CLASS_FACET`]), resolved
    /// against this registry. Absent facet is [`Unclassified`](ClassMembership::Unclassified);
    /// an undefined class is [`Unknown`](ClassMembership::Unknown), never an error.
    pub fn membership<'a>(&'a self, facets: &NodeFacets) -> ClassMembership<'a> {
        let Some(value) = facets.get(&FacetId::new(CLASS_FACET)) else {
            return ClassMembership::Unclassified;
        };
        let Some(id) = value.as_str() else {
            return ClassMembership::Unclassified;
        };
        let class_id = ClassId::new(id);
        match self.classes.get(&class_id) {
            Some(class) => ClassMembership::Known(class),
            None => ClassMembership::Unknown(class_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facet::{AcceptAll, FacetStore};
    use serde_json::{Value, json};

    /// A validator that requires a facet's value to be an object with a `mode`
    /// string — enough to exercise "validates member instances."
    struct RequiresMode;
    impl FacetValidator for RequiresMode {
        fn validate(&self, facet_id: &FacetId, value: &Value) -> Result<(), FacetError> {
            if value.get("mode").and_then(Value::as_str).is_some() {
                Ok(())
            } else {
                Err(FacetError {
                    facet: facet_id.clone(),
                    reason: "expected an object with a string `mode`".into(),
                })
            }
        }
    }

    fn web_page_class() -> ContentClass {
        ContentClass::new(
            "mere.web-page",
            [(
                FacetId::new("web.viewer"),
                "schema:web-viewer-v1".to_string(),
            )],
        )
        .with_label("Web page")
    }

    fn facets_with(class: &str, pairs: &[(&str, Value)]) -> NodeFacets {
        let mut store: FacetStore<String> = FacetStore::new();
        store
            .set(
                "n".into(),
                FacetId::new(CLASS_FACET),
                json!(class),
                &AcceptAll,
            )
            .unwrap();
        for (id, value) in pairs {
            store
                .set("n".into(), FacetId::new(*id), value.clone(), &AcceptAll)
                .unwrap();
        }
        store.facets_of(&"n".to_string()).unwrap().clone()
    }

    #[test]
    fn a_class_admits_a_valid_member_and_rejects_a_missing_or_invalid_facet() {
        let class = web_page_class();

        let ok = facets_with(
            "mere.web-page",
            &[("web.viewer", json!({ "mode": "reader" }))],
        );
        assert!(class.admits(&ok, &RequiresMode).is_ok());

        let missing = facets_with("mere.web-page", &[]);
        assert_eq!(
            class.admits(&missing, &RequiresMode),
            Err(ClassError::MissingFacet(FacetId::new("web.viewer")))
        );

        let invalid = facets_with("mere.web-page", &[("web.viewer", json!("not-an-object"))]);
        assert!(matches!(
            class.admits(&invalid, &RequiresMode),
            Err(ClassError::InvalidFacet(_))
        ));
    }

    #[test]
    fn a_nodes_class_is_queryable() {
        let mut registry = ClassRegistry::new();
        registry.register(web_page_class());

        let facets = facets_with(
            "mere.web-page",
            &[("web.viewer", json!({ "mode": "reader" }))],
        );
        match registry.membership(&facets) {
            ClassMembership::Known(class) => {
                assert_eq!(class.class_id, ClassId::new("mere.web-page"));
                assert_eq!(class.label.as_deref(), Some("Web page"));
            }
            other => panic!("expected Known, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_class_is_inert_not_an_error() {
        let registry = ClassRegistry::new(); // knows nothing
        let facets = facets_with("some-mod.exotic", &[]);
        assert_eq!(
            registry.membership(&facets),
            ClassMembership::Unknown(ClassId::new("some-mod.exotic")),
            "an undefined class reads as Unknown, never a load failure"
        );

        let bare = NodeFacets::new();
        assert_eq!(registry.membership(&bare), ClassMembership::Unclassified);
    }

    #[test]
    fn a_class_definition_round_trips_as_data() {
        let class = web_page_class();
        let json = serde_json::to_string(&class).unwrap();
        let back: ContentClass = serde_json::from_str(&json).unwrap();
        assert_eq!(back, class, "a class is data: it serializes and reloads");
    }
}
