// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

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
    store.put(&local_key, META_SCHEMA_PAYLOAD).await?;

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

mod validators;
pub use validators::{JsonLdValidator, JsonSchemaValidator};
#[cfg(test)]
mod tests;
