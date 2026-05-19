/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Polyglot schema definitions for schema engrams.
//!
//! A schema engram's payload is a [`SchemaDefinition`]. The definition
//! declares its format (Mere-native / JSON Schema / JSON-LD) and carries the
//! format-specific schema body. [`validate_payload`] dispatches to the
//! matching validator.
//!
//! ## The three formats
//!
//! - **`SchemaFormat::MereNative`** — eidetic's own JSON shape, simplest
//!   viable. Full structural validation: required fields, field types,
//!   allowed-values enums.
//! - **`SchemaFormat::JsonSchema`** — JSON Schema (Draft 7+), validated via
//!   the `jsonschema` crate. Heavier, established, ecosystem-friendly.
//! - **`SchemaFormat::JsonLd`** — JSON-LD with optional schema.org `@type`.
//!   Phase 4 ships parse-only validation (verify the document is well-formed
//!   and has a `@context` or `@type`); full SHACL / vocab-checking lands
//!   when a payload pulls on it.
//!
//! New formats can be added without rearchitecting — the [`SchemaValidator`]
//! trait + format dispatch make it nematic-shaped extensible.
//!
//! ## Recursion and the meta-schema
//!
//! A schema engram is itself an engram, so it has a schema reference of its
//! own — the **meta-schema**, identified by [`META_SCHEMA_REF`]. The
//! meta-schema describes "this is a [`SchemaDefinition`]." Its own `schema`
//! field is `META_SCHEMA_REF` — self-referential, terminating the recursion.

use serde::{Deserialize, Serialize};

use crate::engram::{Engram, TimeBounds};
use crate::manifest::{BlobFetcher, BlobSource};
use crate::schema::{
    Hash, ManifestId, ModerationState, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, SchemaRef,
    Timestamp, TrustEnvelope, TrustLevel,
};
use crate::typed::{TypedPayload, load_typed, save_typed};
use crate::{Error, Result, Store};

/// The schema-definition format used by a schema engram. Open-ended
/// architecturally; new variants can be added without breaking existing
/// schema engrams (other consumers see an unknown variant and either skip
/// validation or surface a "validator not available" error per their own
/// policy).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchemaFormat {
    /// Eidetic's native JSON shape — primary format.
    #[serde(rename = "mere-native")]
    MereNative,
    /// JSON Schema (Draft 7+).
    #[serde(rename = "json-schema")]
    JsonSchema,
    /// JSON-LD with optional schema.org `@type`.
    #[serde(rename = "json-ld")]
    JsonLd,
}

/// Schema-definition payload for a schema engram.
///
/// `body` carries the format-specific schema document. For Mere-native, this
/// is a [`MereNativeSchemaBody`]-shaped JSON value. For JSON Schema, it is
/// the JSON Schema document itself. For JSON-LD, it is a JSON-LD context /
/// document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaDefinition {
    pub format: SchemaFormat,
    /// Human-readable identifier (e.g. `"VectorIndex/v1"`). Not load-bearing
    /// for identity — content hash of the engram is. Used for diagnostics
    /// and authoring tooling.
    pub schema_id: String,
    /// Format-specific schema body. Interpreted by the validator that
    /// matches `format`.
    pub body: serde_json::Value,
}

impl TypedPayload for SchemaDefinition {
    fn schema_ref() -> SchemaRef {
        *META_SCHEMA_REF
    }
}

// ---------------------------------------------------------------------------
// Meta-schema bootstrap
// ---------------------------------------------------------------------------

/// Canonical bytes of the meta-schema engram's payload.
///
/// These bytes describe the [`SchemaDefinition`] shape itself, in
/// Mere-native format. They are constant across all eidetic instances and
/// versions; their BLAKE3 hash is [`META_SCHEMA_REF`]'s id.
const META_SCHEMA_PAYLOAD: &[u8] = br#"{"format":"mere-native","schema_id":"eidetic.meta-schema/v1","body":{"version":1,"description":"Schema engram payload describing another schema in one of: mere-native, json-schema, json-ld.","required":["format","schema_id","body"],"fields":{"format":{"type":"enum","values":["mere-native","json-schema","json-ld"]},"schema_id":{"type":"string"},"body":{"type":"object"}}}}"#;

/// The well-known schema reference for schema engrams.
///
/// Equal to `SchemaRef::from_id(ManifestId::of_blob(META_SCHEMA_PAYLOAD))`.
/// Computed lazily because BLAKE3 hashing isn't const-fn yet.
pub fn meta_schema_ref() -> SchemaRef {
    SchemaRef::from_id(ManifestId::from_hash(Hash::of(META_SCHEMA_PAYLOAD)))
}

/// The well-known schema reference for schema engrams.
///
/// Computed lazily at first use from `META_SCHEMA_PAYLOAD`; stable for the
/// process lifetime. Callers that need a value rather than a static can use
/// [`meta_schema_ref`].
pub static META_SCHEMA_REF: std::sync::LazyLock<SchemaRef> =
    std::sync::LazyLock::new(meta_schema_ref);

/// Idempotently seed the meta-schema engram into a Store.
///
/// On first call against a fresh Store, writes the meta-schema's payload
/// bytes (BLAKE3-anchored to [`META_SCHEMA_REF`]) and its manifest. On
/// subsequent calls, returns immediately if the manifest is already
/// present.
///
/// **Call this once during init** for any consumer that will use schema
/// engrams. Without it, [`validate_against_schema`] silently tolerates
/// missing schemas (per the design pass's "tolerant on read" rule), so
/// forgetting bootstrap masks bugs rather than surfacing them.
pub async fn bootstrap_meta_schema(store: &mut dyn Store) -> Result<()> {
    let meta_id = META_SCHEMA_REF.0;
    if crate::manifest::load_manifest(store, meta_id)
        .await?
        .is_some()
    {
        return Ok(());
    }

    // Write META_SCHEMA_PAYLOAD bytes verbatim (don't re-serialize the
    // SchemaDefinition — re-serialization can reorder JSON object keys
    // and produce a different hash than META_SCHEMA_REF anchors to).
    let local_key = format!("blob:{}", Hash::of(META_SCHEMA_PAYLOAD).to_hex());
    store.save_blob(&local_key, META_SCHEMA_PAYLOAD).await?;

    let manifest = crate::manifest::BlobManifest {
        id: meta_id,
        schema: *META_SCHEMA_REF,
        content_hash: Hash::of(META_SCHEMA_PAYLOAD),
        byte_size: META_SCHEMA_PAYLOAD.len() as u64,
        created_at: Timestamp::ZERO,
        last_accessed: None,
        sources: vec![crate::manifest::BlobSource::Local { key: local_key }],
        privacy: PrivacyClass::PublicPortable,
        provenance: ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some(format!("eidetic/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp::ZERO,
        },
        trust: TrustEnvelope {
            level: TrustLevel::CheckpointAccepted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Accepted,
        },
        schema_metadata: serde_json::Value::Null,
        manifest_version: crate::manifest::BlobManifest::CURRENT_VERSION,
    };
    crate::manifest::save_manifest(store, &manifest).await?;
    Ok(())
}

/// Build the canonical meta-schema engram. Call this once at first init to
/// populate the Store with the bootstrap engram, so subsequent schema
/// engrams resolve their `schema` field correctly. Most consumers should
/// use [`bootstrap_meta_schema`] instead, which writes the engram into
/// the Store idempotently.
pub fn meta_schema_engram() -> Engram {
    Engram::new(
        *META_SCHEMA_REF,
        META_SCHEMA_PAYLOAD.to_vec(),
        PrivacyClass::PublicPortable,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some(format!("eidetic/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp::ZERO,
        },
        TrustEnvelope {
            level: TrustLevel::CheckpointAccepted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Accepted,
        },
        TimeBounds::at(Timestamp::ZERO),
    )
}

// ---------------------------------------------------------------------------
// Validator trait + dispatch
// ---------------------------------------------------------------------------

/// Validate raw payload bytes against a parsed [`SchemaDefinition`].
///
/// Implementations interpret `definition.body` according to their format and
/// check that `payload_bytes` conforms.
pub trait SchemaValidator {
    fn format(&self) -> SchemaFormat;
    fn validate(&self, definition: &SchemaDefinition, payload_bytes: &[u8]) -> Result<()>;
}

/// Validate `payload_bytes` against `definition`, dispatching to the right
/// validator by format.
pub fn validate_payload(definition: &SchemaDefinition, payload_bytes: &[u8]) -> Result<()> {
    match definition.format {
        SchemaFormat::MereNative => MereNativeValidator.validate(definition, payload_bytes),
        SchemaFormat::JsonSchema => JsonSchemaValidator.validate(definition, payload_bytes),
        SchemaFormat::JsonLd => JsonLdValidator.validate(definition, payload_bytes),
    }
}

/// Save a schema engram to the Store. Returns its manifest id.
///
/// The schema engram's content is a [`SchemaDefinition`] serialized as JSON;
/// its own `schema` field is [`META_SCHEMA_REF`].
pub async fn save_schema(
    store: &mut dyn Store,
    definition: &SchemaDefinition,
    privacy: PrivacyClass,
    provenance: ProvenanceRecord,
    trust: TrustEnvelope,
    created_at: Timestamp,
) -> Result<ManifestId> {
    save_typed(
        store,
        definition,
        Vec::<BlobSource>::new(),
        privacy,
        provenance,
        trust,
        created_at,
    )
    .await
}

/// Load a schema engram by its manifest id. Returns `Ok(None)` if no such
/// engram is stored.
pub async fn load_schema(
    store: &mut dyn Store,
    fetcher: &mut dyn BlobFetcher,
    schema_id: ManifestId,
) -> Result<Option<SchemaDefinition>> {
    load_typed::<SchemaDefinition>(store, fetcher, schema_id).await
}

/// Recursive resolution: load the schema engram referenced by `schema_ref`,
/// then validate `payload_bytes` against it.
///
/// Forward-compatibility: if the schema engram is missing, returns
/// `Ok(())` rather than erroring (per the design pass's "tolerant on read,
/// strict on required_for_application" rule). Callers that need strict
/// validation can check schema presence themselves before calling.
pub async fn validate_against_schema(
    store: &mut dyn Store,
    fetcher: &mut dyn BlobFetcher,
    schema_ref: SchemaRef,
    payload_bytes: &[u8],
) -> Result<()> {
    let Some(definition) = load_schema(store, fetcher, schema_ref.0).await? else {
        // Tolerant default: unknown schema is not a fatal load failure.
        return Ok(());
    };
    validate_payload(&definition, payload_bytes)
}

// ---------------------------------------------------------------------------
// Mere-native validator
// ---------------------------------------------------------------------------

/// Builder for ergonomic Mere-native [`SchemaDefinition`] construction.
///
/// ```ignore
/// let definition = MereNativeSchemaBuilder::new("Counter/v1")
///     .description("Tick counter")
///     .field("ticks", MereNativeFieldSpec::U64, true)
///     .field("notes", MereNativeFieldSpec::String, false)
///     .build();
/// ```
///
/// Fields added as `required: true` go into `body.required`; all fields
/// are added to `body.fields`.
pub struct MereNativeSchemaBuilder {
    schema_id: String,
    description: String,
    fields: Vec<(String, MereNativeFieldSpec, bool)>,
    version: u32,
}

impl MereNativeSchemaBuilder {
    pub fn new(schema_id: impl Into<String>) -> Self {
        Self {
            schema_id: schema_id.into(),
            description: String::new(),
            fields: Vec::new(),
            version: 1,
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    /// Add a field. `required = true` includes the field in `body.required`.
    pub fn field(
        mut self,
        name: impl Into<String>,
        spec: MereNativeFieldSpec,
        required: bool,
    ) -> Self {
        self.fields.push((name.into(), spec, required));
        self
    }

    pub fn build(self) -> SchemaDefinition {
        let mut required = Vec::new();
        let mut fields = serde_json::Map::new();
        for (name, spec, is_required) in self.fields {
            if is_required {
                required.push(name.clone());
            }
            fields.insert(
                name,
                serde_json::to_value(&spec).expect("field spec serializes"),
            );
        }
        let body = serde_json::json!({
            "version": self.version,
            "description": self.description,
            "required": required,
            "fields": fields,
        });
        SchemaDefinition {
            format: SchemaFormat::MereNative,
            schema_id: self.schema_id,
            body,
        }
    }
}

/// Find a schema engram by its human-readable `schema_id`.
///
/// Walks every schema engram in the Store (i.e. manifests with
/// `schema == META_SCHEMA_REF`), loads each as a [`SchemaDefinition`],
/// and returns the first match. Useful when consumers want to find a
/// schema by name rather than by content hash.
///
/// Returns `Ok(None)` if no matching schema is present. Returns the
/// matching schema's manifest id along with its definition so callers
/// can build a `SchemaRef` for downstream use.
pub async fn find_schema_by_id(
    store: &mut dyn Store,
    fetcher: &mut dyn BlobFetcher,
    schema_id: &str,
) -> Result<Option<(ManifestId, SchemaDefinition)>> {
    let manifests = crate::manifest::list_manifests(store, Some(*META_SCHEMA_REF)).await?;
    for manifest in manifests {
        let definition: SchemaDefinition =
            match load_typed::<SchemaDefinition>(store, fetcher, manifest.id).await {
                Ok(Some(d)) => d,
                _ => continue,
            };
        if definition.schema_id == schema_id {
            return Ok(Some((manifest.id, definition)));
        }
    }
    Ok(None)
}

/// Mere-native schema body shape. Stored under `SchemaDefinition::body`
/// when `format == MereNative`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MereNativeSchemaBody {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub fields: std::collections::BTreeMap<String, MereNativeFieldSpec>,
}

/// Per-field schema spec. Loose intentionally — Mere-native is meant to
/// stay simple. Richer constraints belong in JSON Schema bodies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MereNativeFieldSpec {
    String,
    U64,
    I64,
    F64,
    Bool,
    Object,
    Array,
    Enum { values: Vec<String> },
}

/// Validator for `SchemaFormat::MereNative` — full structural check.
pub struct MereNativeValidator;

impl SchemaValidator for MereNativeValidator {
    fn format(&self) -> SchemaFormat {
        SchemaFormat::MereNative
    }

    fn validate(&self, definition: &SchemaDefinition, payload_bytes: &[u8]) -> Result<()> {
        if definition.format != SchemaFormat::MereNative {
            return Err(Error::new(format!(
                "MereNativeValidator received non-MereNative format: {:?}",
                definition.format
            )));
        }
        let body: MereNativeSchemaBody = serde_json::from_value(definition.body.clone())
            .map_err(|e| Error::new(format!("mere-native schema body parse: {e}")))?;

        let payload_value: serde_json::Value = serde_json::from_slice(payload_bytes)
            .map_err(|e| Error::new(format!("payload parse (mere-native): {e}")))?;
        let object = payload_value
            .as_object()
            .ok_or_else(|| Error::new("mere-native payloads must be JSON objects".to_string()))?;

        for required in &body.required {
            if !object.contains_key(required) {
                return Err(Error::new(format!(
                    "mere-native schema {}: missing required field `{}`",
                    definition.schema_id, required
                )));
            }
        }

        for (name, spec) in &body.fields {
            if let Some(value) = object.get(name) {
                check_field_spec(&definition.schema_id, name, spec, value)?;
            }
        }

        Ok(())
    }
}

fn check_field_spec(
    schema_id: &str,
    name: &str,
    spec: &MereNativeFieldSpec,
    value: &serde_json::Value,
) -> Result<()> {
    let kind_ok = match spec {
        MereNativeFieldSpec::String => value.is_string(),
        MereNativeFieldSpec::U64 => value.as_u64().is_some(),
        MereNativeFieldSpec::I64 => value.as_i64().is_some(),
        MereNativeFieldSpec::F64 => value.as_f64().is_some(),
        MereNativeFieldSpec::Bool => value.is_boolean(),
        MereNativeFieldSpec::Object => value.is_object(),
        MereNativeFieldSpec::Array => value.is_array(),
        MereNativeFieldSpec::Enum { values } => match value.as_str() {
            Some(s) => values.iter().any(|v| v == s),
            None => false,
        },
    };
    if !kind_ok {
        return Err(Error::new(format!(
            "schema {}: field `{}` failed type check (expected {:?}, got {})",
            schema_id, name, spec, value
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// JSON Schema validator
// ---------------------------------------------------------------------------

/// Validator for `SchemaFormat::JsonSchema` — thin wrapper around the
/// `jsonschema` crate.
pub struct JsonSchemaValidator;

impl SchemaValidator for JsonSchemaValidator {
    fn format(&self) -> SchemaFormat {
        SchemaFormat::JsonSchema
    }

    fn validate(&self, definition: &SchemaDefinition, payload_bytes: &[u8]) -> Result<()> {
        if definition.format != SchemaFormat::JsonSchema {
            return Err(Error::new(format!(
                "JsonSchemaValidator received non-JsonSchema format: {:?}",
                definition.format
            )));
        }

        let compiled = jsonschema::JSONSchema::compile(&definition.body).map_err(|e| {
            Error::new(format!(
                "json-schema compile ({}): {e}",
                definition.schema_id
            ))
        })?;

        let payload_value: serde_json::Value = serde_json::from_slice(payload_bytes)
            .map_err(|e| Error::new(format!("payload parse (json-schema): {e}")))?;

        if let Err(errors) = compiled.validate(&payload_value) {
            let collected: Vec<String> = errors
                .map(|e| format!("{} at {}", e, e.instance_path))
                .collect();
            return Err(Error::new(format!(
                "json-schema validation failed for {}: {}",
                definition.schema_id,
                collected.join("; ")
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// JSON-LD validator (parse-only for Phase 4)
// ---------------------------------------------------------------------------

/// Validator for `SchemaFormat::JsonLd`.
///
/// Validation surface:
///
/// 1. Payload is a JSON object that carries `@context` or `@type`.
/// 2. If the schema body declares `@type` (string OR array of strings),
///    every required type must be present in the payload's `@type`. This
///    implements basic type subsumption: a payload typed as
///    `["Article", "BlogPosting"]` satisfies a schema requiring
///    `"Article"`.
/// 3. If the schema body declares `required_props: [name, ...]`, every
///    listed property must be present in the payload as a non-null value.
///
/// Full SHACL / RDF vocabulary-checking still lands when a concrete
/// schema.org-shaped payload pulls on it (e.g. clip-of-Article engrams);
/// this validator stays in the structural + nominal-type tier and pulls
/// in no RDF dependency.
pub struct JsonLdValidator;

impl SchemaValidator for JsonLdValidator {
    fn format(&self) -> SchemaFormat {
        SchemaFormat::JsonLd
    }

    fn validate(&self, definition: &SchemaDefinition, payload_bytes: &[u8]) -> Result<()> {
        if definition.format != SchemaFormat::JsonLd {
            return Err(Error::new(format!(
                "JsonLdValidator received non-JsonLd format: {:?}",
                definition.format
            )));
        }

        let payload_value: serde_json::Value = serde_json::from_slice(payload_bytes)
            .map_err(|e| Error::new(format!("payload parse (json-ld): {e}")))?;
        let object = payload_value
            .as_object()
            .ok_or_else(|| Error::new("json-ld payload must be a JSON object".to_string()))?;
        if !object.contains_key("@context") && !object.contains_key("@type") {
            return Err(Error::new(format!(
                "json-ld schema {}: payload missing both `@context` and `@type`",
                definition.schema_id
            )));
        }

        // @type subsumption: every type required by the schema must
        // appear in the payload's @type (string or array form accepted
        // on both sides).
        if let Some(expected_types) = collect_types(definition.body.get("@type")) {
            let actual_types = collect_types(object.get("@type")).unwrap_or_default();
            for expected in &expected_types {
                if !actual_types.iter().any(|t| t == expected) {
                    return Err(Error::new(format!(
                        "json-ld schema {}: payload missing required @type {:?} \
                         (payload had {:?})",
                        definition.schema_id, expected, actual_types
                    )));
                }
            }
        }

        // required_props: every listed property must be present and non-null.
        if let Some(required) = definition
            .body
            .get("required_props")
            .and_then(|v| v.as_array())
        {
            for entry in required {
                let Some(name) = entry.as_str() else {
                    continue;
                };
                match object.get(name) {
                    Some(value) if !value.is_null() => {}
                    _ => {
                        return Err(Error::new(format!(
                            "json-ld schema {}: payload missing required property `{}`",
                            definition.schema_id, name
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}

/// Collect a JSON-LD-style `@type` value into a `Vec<String>`. Accepts a
/// string (`"Article"` -> `["Article"]`) or an array of strings. Returns
/// `None` if absent or not string/array-of-strings.
fn collect_types(value: Option<&serde_json::Value>) -> Option<Vec<String>> {
    let value = value?;
    if let Some(s) = value.as_str() {
        return Some(vec![s.to_string()]);
    }
    if let Some(arr) = value.as_array() {
        let collected: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        if collected.is_empty() {
            return None;
        }
        return Some(collected);
    }
    None
}

#[cfg(test)]
mod tests {
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
}
