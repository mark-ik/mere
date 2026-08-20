//! Knot's content-class pack.

use chartulary::{CLASS_FACET, ClassRegistry, ContentClass, FacetId};
use eidetic::{MereNativeFieldSpec, MereNativeSchemaBuilder, SchemaDefinition, SchemaFormat};
use pandect::SchemaFacetValidator;
use serde_json::json;

/// The general files-in-place class.
pub const FILE_CLASS: &str = "knot.file";
/// Authored text. Its required facets include the general file profile.
pub const NOTE_CLASS: &str = "knot.note";
/// Disk-observation metadata shared by every file.
pub const FILE_DOCUMENT_FACET: &str = "file.document";
/// Format metadata for authored text.
pub const NOTE_DOCUMENT_FACET: &str = "note.document";

/// The classes and schemas Knot ships through the same data seams a pack uses.
pub struct KnotContentClasses {
    /// Known class definitions.
    pub registry: ClassRegistry,
    /// Preloaded facet schemas.
    pub validator: SchemaFacetValidator,
}

impl KnotContentClasses {
    /// Build Knot's built-in pack.
    pub fn new() -> Self {
        let mut validator = SchemaFacetValidator::new();
        validator.register(
            FacetId::new(FILE_DOCUMENT_FACET),
            MereNativeSchemaBuilder::new("knot.file/v1")
                .description("A file observed in place by Knot")
                .field("version", MereNativeFieldSpec::U64, true)
                .field("address", MereNativeFieldSpec::String, true)
                .field("byte_size", MereNativeFieldSpec::U64, true)
                .field("extension", MereNativeFieldSpec::String, false)
                .build(),
        );
        validator.register(
            FacetId::new(NOTE_DOCUMENT_FACET),
            MereNativeSchemaBuilder::new("knot.note/v1")
                .description("An authored text document observed in place by Knot")
                .field("version", MereNativeFieldSpec::U64, true)
                .field("format", MereNativeFieldSpec::String, true)
                .build(),
        );
        validator.register(
            FacetId::new(CLASS_FACET),
            SchemaDefinition {
                format: SchemaFormat::JsonSchema,
                schema_id: "chartulary.class/v1".to_string(),
                body: json!({"type": "string", "minLength": 1}),
            },
        );

        let mut registry = ClassRegistry::new();
        registry.register(
            ContentClass::new(
                FILE_CLASS,
                [(
                    FacetId::new(FILE_DOCUMENT_FACET),
                    "knot.file/v1".to_string(),
                )],
            )
            .with_label("File"),
        );
        registry.register(
            ContentClass::new(
                NOTE_CLASS,
                [
                    (
                        FacetId::new(FILE_DOCUMENT_FACET),
                        "knot.file/v1".to_string(),
                    ),
                    (
                        FacetId::new(NOTE_DOCUMENT_FACET),
                        "knot.note/v1".to_string(),
                    ),
                ],
            )
            .with_label("Note"),
        );

        Self {
            registry,
            validator,
        }
    }
}

impl Default for KnotContentClasses {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use chartulary::{AcceptAll, ClassMembership, FacetId, FacetStore, FacetValidator, NodeFacets};
    use serde_json::json;

    use super::*;

    #[test]
    fn note_class_admits_a_valid_file_and_note_profile() {
        let classes = KnotContentClasses::new();
        let mut facets = FacetStore::<String>::new();
        let id = "note".to_string();
        facets
            .set(
                id.clone(),
                FacetId::new(CLASS_FACET),
                json!(NOTE_CLASS),
                &classes.validator,
            )
            .unwrap();
        facets
            .set(
                id.clone(),
                FacetId::new(FILE_DOCUMENT_FACET),
                json!({
                    "version": 1,
                    "address": "file:///notes/field.knot",
                    "byte_size": 42,
                    "extension": "knot",
                }),
                &classes.validator,
            )
            .unwrap();
        facets
            .set(
                id.clone(),
                FacetId::new(NOTE_DOCUMENT_FACET),
                json!({"version": 1, "format": "knot"}),
                &classes.validator,
            )
            .unwrap();

        let node = facets.facets_of(&id).unwrap();
        let ClassMembership::Known(class) = classes.registry.membership(node) else {
            panic!("registered note class should be known");
        };
        class.admits(node, &classes.validator).unwrap();
    }

    #[test]
    fn unknown_class_stays_inert_and_discoverable() {
        let classes = KnotContentClasses::new();
        let mut facets = FacetStore::<String>::new();
        let id = "foreign".to_string();
        facets
            .set(
                id.clone(),
                FacetId::new(CLASS_FACET),
                json!("pack.foreign"),
                &AcceptAll,
            )
            .unwrap();

        match classes.registry.membership(facets.facets_of(&id).unwrap()) {
            ClassMembership::Unknown(class) => assert_eq!(class.as_str(), "pack.foreign"),
            other => panic!("expected an inert unknown class, got {other:?}"),
        }
    }

    #[test]
    fn schemas_reject_malformed_known_profiles() {
        let classes = KnotContentClasses::new();
        assert!(
            classes
                .validator
                .validate(
                    &FacetId::new(NOTE_DOCUMENT_FACET),
                    &json!({"version": "one", "format": "knot"}),
                )
                .is_err()
        );
        let empty = NodeFacets::new();
        let note = classes
            .registry
            .get(&chartulary::ClassId::new(NOTE_CLASS))
            .unwrap();
        assert!(note.admits(&empty, &classes.validator).is_err());
    }
}
