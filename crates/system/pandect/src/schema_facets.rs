// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Eidetic-backed validation for chartulary facets.
//!
//! [`chartulary::FacetValidator`] is synchronous while schema resolution from
//! an eidetic store is asynchronous. The host therefore resolves schema
//! codicils once, registers the parsed [`eidetic::SchemaDefinition`]s here, and
//! uses this immutable in-memory adapter on every write. Unknown facet ids
//! remain forward-compatible and pass through unchanged.

use std::collections::BTreeMap;

use chartulary::{ContentClass, FacetError, FacetId, FacetValidator};
use eidetic::{
    BlobFetcher, BlobSource, Hash, ManifestId, MereNativeFieldSpec, MereNativeSchemaBuilder,
    PrivacyClass, ProvenanceRecord, SchemaDefinition, SchemaRef, Store, Timestamp, TrustEnvelope,
    TypedPayload, bootstrap_meta_schema, load_typed, save_schema, save_typed,
};
use serde::{Deserialize, Serialize};
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

    /// Iterate the definitions a host should persist as schema codicils.
    pub fn definitions(&self) -> impl Iterator<Item = (&FacetId, &SchemaDefinition)> {
        self.schemas.iter()
    }

    /// Persist every registered facet schema as an Eidetic schema codicil.
    ///
    /// Returns the content-addressed manifest id for each facet. Repeating the
    /// call is idempotent because schema identity is its serialized content.
    pub async fn persist_definitions(
        &self,
        store: &mut dyn Store,
        privacy: PrivacyClass,
        provenance: ProvenanceRecord,
        trust: TrustEnvelope,
        created_at: Timestamp,
    ) -> eidetic::Result<BTreeMap<FacetId, ManifestId>> {
        bootstrap_meta_schema(store).await?;
        let mut persisted = BTreeMap::new();
        for (facet, definition) in &self.schemas {
            let id = save_schema(
                store,
                definition,
                privacy,
                provenance.clone(),
                trust.clone(),
                created_at,
            )
            .await?;
            persisted.insert(facet.clone(), id);
        }
        Ok(persisted)
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

/// Schema for a content-class definition codicil.
///
/// A class is ordinary data and points at the schema codicils for its required
/// facets. This schema makes the class document itself an eidetic typed
/// payload, so built-ins and pack-defined classes use the same persistence
/// path.
pub fn content_class_schema_definition() -> SchemaDefinition {
    MereNativeSchemaBuilder::new("chartulary.ContentClass/v1")
        .description("A content class id, label, and required facet schema references")
        .field("class_id", MereNativeFieldSpec::String, true)
        .field("label", MereNativeFieldSpec::String, false)
        .field("required_facets", MereNativeFieldSpec::Object, true)
        .build()
}

/// Content-addressed reference to [`content_class_schema_definition`].
pub fn content_class_schema_ref() -> SchemaRef {
    let bytes =
        serde_json::to_vec(&content_class_schema_definition()).expect("class schema serializes");
    SchemaRef::from_id(ManifestId::from_hash(Hash::of(&bytes)))
}

/// Eidetic typed binding for chartulary's data-defined content class.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentClassCodicil(pub ContentClass);

impl TypedPayload for ContentClassCodicil {
    fn schema_ref() -> SchemaRef {
        content_class_schema_ref()
    }
}

/// Persist one content-class definition and its schema codicil.
#[allow(clippy::too_many_arguments)]
pub async fn save_content_class(
    store: &mut dyn Store,
    class: &ContentClass,
    privacy: PrivacyClass,
    provenance: ProvenanceRecord,
    trust: TrustEnvelope,
    created_at: Timestamp,
) -> eidetic::Result<ManifestId> {
    bootstrap_meta_schema(store).await?;
    let schema = content_class_schema_definition();
    let schema_id = save_schema(
        store,
        &schema,
        privacy,
        provenance.clone(),
        trust.clone(),
        created_at,
    )
    .await?;
    debug_assert_eq!(SchemaRef::from_id(schema_id), content_class_schema_ref());
    save_typed(
        store,
        &ContentClassCodicil(class.clone()),
        Vec::<BlobSource>::new(),
        privacy,
        provenance,
        trust,
        created_at,
    )
    .await
}

/// Load one persisted content-class definition.
pub async fn load_content_class(
    store: &mut dyn Store,
    fetcher: &mut dyn BlobFetcher,
    id: ManifestId,
) -> eidetic::Result<Option<ContentClass>> {
    Ok(load_typed::<ContentClassCodicil>(store, fetcher, id)
        .await?
        .map(|codicil| codicil.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eidetic::{NoFetcher, ProvenanceOrigin};
    use muniment::MemoryBackend;
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
        assert!(
            validator
                .validate(&facet, &json!({"version": "one"}))
                .is_err()
        );
    }

    #[test]
    fn an_unknown_facet_remains_forward_compatible() {
        assert!(
            validator()
                .validate(&FacetId::new("future.exotic"), &json!([1, 2, 3]))
                .is_ok()
        );
    }

    fn classifications() -> (PrivacyClass, ProvenanceRecord, TrustEnvelope, Timestamp) {
        let created_at = Timestamp(42);
        (
            PrivacyClass::PublicPortable,
            ProvenanceRecord {
                origin: ProvenanceOrigin::Generated,
                upstream: Vec::new(),
                tooling: Some("pandect-test".into()),
                generated_at: created_at,
            },
            TrustEnvelope::self_asserted(),
            created_at,
        )
    }

    #[test]
    fn registered_schemas_and_content_classes_round_trip_as_codicils() {
        pollster::block_on(async {
            let mut store = MemoryBackend::new();
            let validator = validator();
            let (privacy, provenance, trust, created_at) = classifications();
            let ids = validator
                .persist_definitions(
                    &mut store,
                    privacy,
                    provenance.clone(),
                    trust.clone(),
                    created_at,
                )
                .await
                .unwrap();
            let schema_id = ids[&FacetId::new("note.document")];
            let loaded_schema = eidetic::load_schema(&mut store, &mut NoFetcher, schema_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                loaded_schema,
                validator
                    .get(&FacetId::new("note.document"))
                    .unwrap()
                    .clone()
            );

            let class = ContentClass::new(
                "mere.note",
                [(FacetId::new("note.document"), schema_id.to_string())],
            )
            .with_label("Note");
            let class_id =
                save_content_class(&mut store, &class, privacy, provenance, trust, created_at)
                    .await
                    .unwrap();
            let loaded = load_content_class(&mut store, &mut NoFetcher, class_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(loaded, class);
        });
    }
}
