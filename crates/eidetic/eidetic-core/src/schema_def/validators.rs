/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use super::{SchemaDefinition, SchemaFormat, SchemaValidator};
use crate::{Error, Result};
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

