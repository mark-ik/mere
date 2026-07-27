// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Eidetic-backed validation for chartulary facets.
//!
//! [`chartulary::FacetValidator`] is synchronous while schema resolution from
//! an eidetic store is asynchronous. The host therefore resolves schema
//! engrams once, registers the parsed [`eidetic::SchemaDefinition`]s here, and
//! uses this immutable in-memory adapter on every write. Unknown facet ids
//! remain forward-compatible and pass through unchanged.

use std::collections::BTreeMap;

use chartulary::{FacetError, FacetId, FacetValidator};
use eidetic::SchemaDefinition;
use serde_json::Value;

/// A preloaded facet-schema registry implementing chartulary's synchronous
/// validation seam.
#[derive(Clone, Debug, Default)]
pub struct SchemaFacetValidator {
    schemas: BTreeMap<FacetId, SchemaDefinition>,
}

impl SchemaFacetValidator {
    /// An empty validator. Unknown facets are accepted.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace the schema for one facet id.
    pub fn register(
        &mut self,
        facet: FacetId,
        definition: SchemaDefinition,
    ) -> Option<SchemaDefinition> {
        self.schemas.insert(facet, definition)
    }

    /// The preloaded definition for a facet.
    pub fn get(&self, facet: &FacetId) -> Option<&SchemaDefinition> {
        self.schemas.get(facet)
    }

    /// Iterate the definitions a host should persist as schema engrams.
    pub fn definitions(&self) -> impl Iterator<Item = (&FacetId, &SchemaDefinition)> {
        self.schemas.iter()
    }
}

impl FacetValidator for SchemaFacetValidator {
    fn validate(&self, facet_id: &FacetId, value: &Value) -> Result<(), FacetError> {
        let Some(definition) = self.schemas.get(facet_id) else {
            return Ok(());
        };
        let bytes = serde_json::to_vec(value).map_err(|error| FacetError {
            facet: facet_id.clone(),
            reason: format!("payload serialization failed: {error}"),
        })?;
        eidetic::validate_payload(definition, &bytes).map_err(|error| FacetError {
            facet: facet_id.clone(),
            reason: error.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eidetic::{MereNativeFieldSpec, MereNativeSchemaBuilder};
    use serde_json::json;

    fn validator() -> SchemaFacetValidator {
        let mut validator = SchemaFacetValidator::new();
        validator.register(
            FacetId::new("note.document"),
            MereNativeSchemaBuilder::new("note.document/v1")
                .field("version", MereNativeFieldSpec::U64, true)
                .field("format", MereNativeFieldSpec::String, true)
                .build(),
        );
        validator
    }

    #[test]
    fn a_preloaded_schema_accepts_valid_and_rejects_invalid_payloads() {
        let validator = validator();
        let facet = FacetId::new("note.document");
        assert!(
            validator
                .validate(&facet, &json!({"version": 1, "format": "djot"}))
                .is_ok()
        );
        assert!(validator.validate(&facet, &json!({"version": "one"})).is_err());
    }

    #[test]
    fn an_unknown_facet_remains_forward_compatible() {
        assert!(
            validator()
                .validate(&FacetId::new("future.exotic"), &json!([1, 2, 3]))
                .is_ok()
        );
    }
}
