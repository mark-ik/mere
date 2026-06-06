/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use super::*;
use crate::manifest::NoFetcher;
use crate::schema::{ProvenanceOrigin, ProvenanceRecord, Timestamp};
use async_trait::async_trait;
use std::collections::HashMap;


#[derive(Default)]
struct InMemoryStore {
    blobs: HashMap<String, Vec<u8>>,
}

#[async_trait(?Send)]
impl Store for InMemoryStore {
    async fn load_blob(&mut self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.blobs.get(key).cloned())
    }
    async fn save_blob(&mut self, key: &str, value: &[u8]) -> Result<()> {
        self.blobs.insert(key.to_string(), value.to_vec());
        Ok(())
    }
    async fn iter_keys(&mut self, prefix: &str) -> Result<Vec<String>> {
        Ok(self
            .blobs
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}

fn test_provenance() -> ProvenanceRecord {
    ProvenanceRecord {
        origin: ProvenanceOrigin::Generated,
        upstream: Vec::new(),
        tooling: Some("eidetic-test".to_string()),
        generated_at: Timestamp(0),
    }
}

fn test_trust() -> TrustEnvelope {
    TrustEnvelope {
        level: TrustLevel::SelfAsserted,
        signatures: Vec::new(),
        moderation_state: ModerationState::Unreviewed,
    }
}

// --- Mere-native ------------------------------------------------------

#[test]
fn mere_native_validates_required_field_present() {
    let definition = SchemaDefinition {
        format: SchemaFormat::MereNative,
        schema_id: "test/v1".to_string(),
        body: serde_json::json!({
            "required": ["name"],
            "fields": {
                "name": {"type": "string"},
                "age": {"type": "u64"}
            }
        }),
    };
    let payload = br#"{"name": "alice", "age": 30}"#;
    validate_payload(&definition, payload).unwrap();
}

#[test]
fn mere_native_rejects_missing_required_field() {
    let definition = SchemaDefinition {
        format: SchemaFormat::MereNative,
        schema_id: "test/v1".to_string(),
        body: serde_json::json!({
            "required": ["name"],
            "fields": {"name": {"type": "string"}}
        }),
    };
    let payload = br#"{"age": 30}"#;
    let result = validate_payload(&definition, payload);
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("missing required field"));
}

#[test]
fn mere_native_rejects_wrong_field_type() {
    let definition = SchemaDefinition {
        format: SchemaFormat::MereNative,
        schema_id: "test/v1".to_string(),
        body: serde_json::json!({
            "required": [],
            "fields": {"age": {"type": "u64"}}
        }),
    };
    let payload = br#"{"age": "thirty"}"#;
    let result = validate_payload(&definition, payload);
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("failed type check"));
}

#[test]
fn mere_native_enum_field_validates_against_values() {
    let definition = SchemaDefinition {
        format: SchemaFormat::MereNative,
        schema_id: "test/v1".to_string(),
        body: serde_json::json!({
            "required": ["color"],
            "fields": {
                "color": {"type": "enum", "values": ["red", "green", "blue"]}
            }
        }),
    };
    validate_payload(&definition, br#"{"color": "red"}"#).unwrap();
    let bad = validate_payload(&definition, br#"{"color": "purple"}"#);
    assert!(bad.is_err());
}

// --- JSON Schema ------------------------------------------------------

#[test]
fn json_schema_validates_simple_object() {
    let definition = SchemaDefinition {
        format: SchemaFormat::JsonSchema,
        schema_id: "person/v1".to_string(),
        body: serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer", "minimum": 0}
            },
            "required": ["name"]
        }),
    };
    validate_payload(&definition, br#"{"name": "bob", "age": 42}"#).unwrap();
}

#[test]
fn json_schema_rejects_constraint_violation() {
    let definition = SchemaDefinition {
        format: SchemaFormat::JsonSchema,
        schema_id: "person/v1".to_string(),
        body: serde_json::json!({
            "type": "object",
            "properties": {"age": {"type": "integer", "minimum": 0}},
            "required": ["age"]
        }),
    };
    let result = validate_payload(&definition, br#"{"age": -5}"#);
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("json-schema validation failed"));
}

// --- JSON-LD ----------------------------------------------------------

#[test]
fn json_ld_validates_payload_with_context_and_type() {
    let definition = SchemaDefinition {
        format: SchemaFormat::JsonLd,
        schema_id: "schema-org/Article/v1".to_string(),
        body: serde_json::json!({
            "@context": "https://schema.org/",
            "@type": "Article"
        }),
    };
    let payload = br#"{
        "@context": "https://schema.org/",
        "@type": "Article",
        "headline": "Hello",
        "author": "alice"
    }"#;
    validate_payload(&definition, payload).unwrap();
}

#[test]
fn json_ld_rejects_payload_without_context_or_type() {
    let definition = SchemaDefinition {
        format: SchemaFormat::JsonLd,
        schema_id: "schema-org/Article/v1".to_string(),
        body: serde_json::json!({
            "@context": "https://schema.org/"
        }),
    };
    let result = validate_payload(&definition, br#"{"headline": "no jsonld marker"}"#);
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("missing both"));
}

#[test]
fn json_ld_accepts_array_type_subsumption() {
    // Schema requires @type "Article"; payload declares
    // ["Article", "BlogPosting"] — should pass.
    let definition = SchemaDefinition {
        format: SchemaFormat::JsonLd,
        schema_id: "schema-org/Article/v1".to_string(),
        body: serde_json::json!({
            "@context": "https://schema.org/",
            "@type": "Article"
        }),
    };
    let payload = br#"{
        "@context": "https://schema.org/",
        "@type": ["Article", "BlogPosting"],
        "headline": "Hi"
    }"#;
    validate_payload(&definition, payload).unwrap();
}

#[test]
fn json_ld_accepts_array_schema_with_multi_type_payload() {
    // Schema requires both Article and CreativeWork; payload has both.
    let definition = SchemaDefinition {
        format: SchemaFormat::JsonLd,
        schema_id: "creative/v1".to_string(),
        body: serde_json::json!({
            "@context": "https://schema.org/",
            "@type": ["Article", "CreativeWork"]
        }),
    };
    let payload = br#"{
        "@context": "https://schema.org/",
        "@type": ["Article", "CreativeWork", "BlogPosting"]
    }"#;
    validate_payload(&definition, payload).unwrap();

    // Missing one of the required types -> error.
    let bad = br#"{
        "@context": "https://schema.org/",
        "@type": "Article"
    }"#;
    let result = validate_payload(&definition, bad);
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("CreativeWork"));
}

#[test]
fn json_ld_required_props_rejects_missing_property() {
    let definition = SchemaDefinition {
        format: SchemaFormat::JsonLd,
        schema_id: "schema-org/Article-strict/v1".to_string(),
        body: serde_json::json!({
            "@context": "https://schema.org/",
            "@type": "Article",
            "required_props": ["headline", "author"]
        }),
    };
    let ok = br#"{
        "@context": "https://schema.org/",
        "@type": "Article",
        "headline": "Hi",
        "author": "alice"
    }"#;
    validate_payload(&definition, ok).unwrap();

    let missing_author = br#"{
        "@context": "https://schema.org/",
        "@type": "Article",
        "headline": "Hi"
    }"#;
    let result = validate_payload(&definition, missing_author);
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("author"));

    // Null counts as missing for the required check.
    let null_author = br#"{
        "@context": "https://schema.org/",
        "@type": "Article",
        "headline": "Hi",
        "author": null
    }"#;
    let result = validate_payload(&definition, null_author);
    assert!(result.is_err());
}

#[test]
fn json_ld_rejects_type_mismatch() {
    let definition = SchemaDefinition {
        format: SchemaFormat::JsonLd,
        schema_id: "schema-org/Article/v1".to_string(),
        body: serde_json::json!({
            "@context": "https://schema.org/",
            "@type": "Article"
        }),
    };
    let payload = br#"{"@context": "https://schema.org/", "@type": "Recipe"}"#;
    let result = validate_payload(&definition, payload);
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("@type"));
}

// --- Recursion / round-trip ------------------------------------------

#[test]
fn schema_engram_round_trips_through_save_and_load() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let mut fetcher = NoFetcher;

        let definition = SchemaDefinition {
            format: SchemaFormat::MereNative,
            schema_id: "Counter/v1".to_string(),
            body: serde_json::json!({
                "required": ["ticks"],
                "fields": {"ticks": {"type": "u64"}}
            }),
        };

        let schema_id = save_schema(
            &mut store,
            &definition,
            PrivacyClass::PublicPortable,
            test_provenance(),
            test_trust(),
            Timestamp(0),
        )
        .await
        .unwrap();

        let loaded = load_schema(&mut store, &mut fetcher, schema_id)
            .await
            .unwrap()
            .expect("schema engram present after save");
        assert_eq!(loaded, definition);
    });
}

#[test]
fn validate_against_schema_resolves_engram_and_runs_validator() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let mut fetcher = NoFetcher;

        let definition = SchemaDefinition {
            format: SchemaFormat::MereNative,
            schema_id: "Counter/v1".to_string(),
            body: serde_json::json!({
                "required": ["ticks"],
                "fields": {"ticks": {"type": "u64"}}
            }),
        };
        let schema_id = save_schema(
            &mut store,
            &definition,
            PrivacyClass::PublicPortable,
            test_provenance(),
            test_trust(),
            Timestamp(0),
        )
        .await
        .unwrap();

        let schema_ref = SchemaRef::from_id(schema_id);

        // Valid payload: should pass.
        validate_against_schema(&mut store, &mut fetcher, schema_ref, br#"{"ticks": 5}"#)
            .await
            .unwrap();

        // Invalid payload: missing required field.
        let result =
            validate_against_schema(&mut store, &mut fetcher, schema_ref, br#"{}"#).await;
        assert!(result.is_err());
    });
}

#[test]
fn validate_against_unknown_schema_is_tolerant() {
    pollster::block_on(async {
        // Per the design pass's "tolerant on read" rule: an unknown
        // schema is not a fatal load failure — payload validation is
        // simply skipped.
        let mut store = InMemoryStore::default();
        let mut fetcher = NoFetcher;
        let unknown_ref = SchemaRef::from_id(ManifestId::of_blob(b"never-saved-schema"));
        validate_against_schema(&mut store, &mut fetcher, unknown_ref, br#"{}"#)
            .await
            .unwrap();
    });
}

#[test]
fn bootstrap_meta_schema_seeds_engram_idempotently() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();

        // Before bootstrap: meta-schema not present.
        assert!(
            crate::manifest::load_manifest(&mut store, META_SCHEMA_REF.0)
                .await
                .unwrap()
                .is_none()
        );

        bootstrap_meta_schema(&mut store).await.unwrap();

        // After bootstrap: manifest present at META_SCHEMA_REF.
        let manifest = crate::manifest::load_manifest(&mut store, META_SCHEMA_REF.0)
            .await
            .unwrap()
            .expect("meta-schema manifest after bootstrap");
        assert_eq!(manifest.id, META_SCHEMA_REF.0);
        assert_eq!(manifest.schema, *META_SCHEMA_REF);

        // Idempotent: calling again is a no-op (no error, same manifest).
        bootstrap_meta_schema(&mut store).await.unwrap();
        let manifest_again = crate::manifest::load_manifest(&mut store, META_SCHEMA_REF.0)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(manifest_again, manifest);
    });
}

#[test]
fn bootstrap_enables_strict_schema_resolution() {
    pollster::block_on(async {
        // Before bootstrap, validate_against_schema is silently
        // tolerant on the meta-schema. After bootstrap, the meta-
        // schema is resolvable, and a real SchemaDefinition payload
        // validates successfully.
        let mut store = InMemoryStore::default();
        bootstrap_meta_schema(&mut store).await.unwrap();

        let mut fetcher = NoFetcher;
        let real_schema = SchemaDefinition {
            format: SchemaFormat::JsonSchema,
            schema_id: "Person/v1".to_string(),
            body: serde_json::json!({"type": "object"}),
        };
        let bytes = serde_json::to_vec(&real_schema).unwrap();
        validate_against_schema(&mut store, &mut fetcher, *META_SCHEMA_REF, &bytes)
            .await
            .unwrap();
    });
}

#[test]
fn mere_native_builder_produces_validating_schema() {
    let definition = MereNativeSchemaBuilder::new("Counter/v1")
        .description("Tick counter")
        .field("ticks", MereNativeFieldSpec::U64, true)
        .field("notes", MereNativeFieldSpec::String, false)
        .build();

    assert_eq!(definition.format, SchemaFormat::MereNative);
    assert_eq!(definition.schema_id, "Counter/v1");

    // Round-trip: serialize, validate against meta-schema, validate
    // a real payload against the built schema.
    let bytes = serde_json::to_vec(&definition).unwrap();
    let meta_definition: SchemaDefinition =
        serde_json::from_slice(META_SCHEMA_PAYLOAD).unwrap();
    validate_payload(&meta_definition, &bytes).unwrap();

    validate_payload(&definition, br#"{"ticks": 7}"#).unwrap();
    let bad = validate_payload(&definition, br#"{"notes": "no ticks"}"#);
    assert!(bad.is_err());
}

#[test]
fn find_schema_by_id_locates_matching_engram() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let mut fetcher = NoFetcher;

        let counter_def = MereNativeSchemaBuilder::new("Counter/v1")
            .field("ticks", MereNativeFieldSpec::U64, true)
            .build();
        let counter_id = save_schema(
            &mut store,
            &counter_def,
            PrivacyClass::PublicPortable,
            test_provenance(),
            test_trust(),
            Timestamp(0),
        )
        .await
        .unwrap();

        let person_def = MereNativeSchemaBuilder::new("Person/v1")
            .field("name", MereNativeFieldSpec::String, true)
            .build();
        let _person_id = save_schema(
            &mut store,
            &person_def,
            PrivacyClass::PublicPortable,
            test_provenance(),
            test_trust(),
            Timestamp(0),
        )
        .await
        .unwrap();

        let found = find_schema_by_id(&mut store, &mut fetcher, "Counter/v1")
            .await
            .unwrap();
        assert!(found.is_some());
        let (id, def) = found.unwrap();
        assert_eq!(id, counter_id);
        assert_eq!(def.schema_id, "Counter/v1");

        let missing = find_schema_by_id(&mut store, &mut fetcher, "Nonexistent/v1")
            .await
            .unwrap();
        assert!(missing.is_none());
    });
}

#[test]
fn meta_schema_engram_id_is_stable_and_self_describing() {
    // The meta-schema engram's id depends only on META_SCHEMA_PAYLOAD,
    // so it's stable across runs and instances.
    let engram = meta_schema_engram();
    assert_eq!(engram.schema, *META_SCHEMA_REF);
    assert_eq!(engram.id(), (*META_SCHEMA_REF).0);
    engram.verify_integrity().unwrap();
}

#[test]
fn meta_schema_payload_validates_a_well_formed_schema_definition() {
    // The meta-schema describes the SchemaDefinition shape itself —
    // verify that a real SchemaDefinition payload conforms to the
    // meta-schema body (the recursion holds).
    let meta_definition: SchemaDefinition =
        serde_json::from_slice(META_SCHEMA_PAYLOAD).unwrap();
    assert_eq!(meta_definition.format, SchemaFormat::MereNative);
    assert_eq!(meta_definition.schema_id, "eidetic.meta-schema/v1");

    // Build a real SchemaDefinition payload and validate it against the
    // meta-schema body.
    let real_schema = SchemaDefinition {
        format: SchemaFormat::JsonSchema,
        schema_id: "Whatever/v1".to_string(),
        body: serde_json::json!({"type": "object"}),
    };
    let real_bytes = serde_json::to_vec(&real_schema).unwrap();
    validate_payload(&meta_definition, &real_bytes).unwrap();
}
