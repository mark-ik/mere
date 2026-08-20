//! **Titulus** — the inscription that says what a thing is.
//!
//! A titulus was the placard naming what hung beneath it, and the label on a
//! scroll or amphora identifying its contents. This crate is that label for a
//! projected resource: a small, portable, semantic description an endpoint
//! hands a host, plus the bounded contract for what may be done about it.
//!
//! Deliberately not a widget tree. A [`PortableCardV1`] carries a title,
//! labeled values, badges, and content addresses — enough for a host to render
//! something honest about a resource it does not own, and not enough to make
//! the host a rendering engine for somebody else's application truth.
//!
//! [`ActionFormV1`] is the other half: an endpoint-authored input contract
//! bounded to exact string choices, so a host can compose an action payload
//! without interpreting application meaning.
//!
//! Extracted from the projection protocol (now [`chirograph`]) 2026-08-14: the
//! card vocabulary is neutral, and the identity port wanted it without the
//! wire. See the mere design docs for the keeper founding that surfaced it.
//!
//! [`chirograph`]: https://crates.io/crates/chirograph

#![warn(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

/// A content address for a separately transferred resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    /// The address of these bytes (BLAKE3).
    pub fn of(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// A bounded, endpoint-authored input form for one advertised action.
///
/// Version one deliberately composes only a JSON object with a mandatory
/// `schema` string and named endpoint-supplied choices. It is enough for exact
/// selections such as saved-record IDs or digests, without turning Graphshell
/// into a generic JSON editor or asking a host to interpret application truth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionFormV1 {
    /// Must exactly equal the advertised action's `payload_schema`.
    pub schema: String,
    /// The named inputs, in the order a host should present them.
    pub fields: Vec<ActionFormFieldV1>,
}

impl ActionFormV1 {
    /// An empty form for `schema`, which must match the action it belongs to.
    pub fn new(schema: impl Into<String>) -> Self {
        Self {
            schema: schema.into(),
            fields: Vec::new(),
        }
    }

    /// Append one input.
    pub fn with_field(mut self, field: ActionFormFieldV1) -> Self {
        self.fields.push(field);
        self
    }

    /// Check static form facts before a host exposes the action.
    pub fn validate(&self) -> Result<(), ActionFormError> {
        if self.schema.trim().is_empty() {
            return Err(ActionFormError::EmptyFormSchema);
        }
        let mut names = BTreeSet::new();
        for field in &self.fields {
            if field.name.trim().is_empty() {
                return Err(ActionFormError::EmptyFieldName);
            }
            if field.name == "schema" {
                return Err(ActionFormError::ReservedFieldName);
            }
            if !names.insert(field.name.clone()) {
                return Err(ActionFormError::DuplicateField(field.name.clone()));
            }
            if field.label.trim().is_empty() {
                return Err(ActionFormError::EmptyFieldLabel(field.name.clone()));
            }
            if field.choices.is_empty() {
                return Err(ActionFormError::EmptyChoices(field.name.clone()));
            }
            let mut values = BTreeSet::new();
            for choice in &field.choices {
                if choice.value.trim().is_empty() {
                    return Err(ActionFormError::EmptyChoiceValue(field.name.clone()));
                }
                if choice.label.trim().is_empty() {
                    return Err(ActionFormError::EmptyChoiceLabel(field.name.clone()));
                }
                if !values.insert(choice.value.clone()) {
                    return Err(ActionFormError::DuplicateChoiceValue {
                        field: field.name.clone(),
                        value: choice.value.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Build the version-one JSON payload after checking the endpoint's form.
    pub fn compose_payload(
        &self,
        expected_schema: &str,
        values: &BTreeMap<String, String>,
    ) -> Result<Vec<u8>, ActionFormError> {
        self.validate()?;
        if self.schema != expected_schema {
            return Err(ActionFormError::SchemaMismatch {
                expected: expected_schema.to_string(),
                form: self.schema.clone(),
            });
        }

        for name in values.keys() {
            if !self.fields.iter().any(|field| field.name == *name) {
                return Err(ActionFormError::UnknownField(name.clone()));
            }
        }
        for field in &self.fields {
            let value = values.get(&field.name);
            if field.required && value.is_none_or(|value| value.is_empty()) {
                return Err(ActionFormError::MissingField(field.name.clone()));
            }
            let Some(value) = value else {
                continue;
            };
            if !field.choices.iter().any(|choice| choice.value == *value) {
                return Err(ActionFormError::InvalidChoice {
                    field: field.name.clone(),
                    value: value.clone(),
                });
            }
        }

        #[derive(Serialize)]
        struct Payload<'a> {
            schema: &'a str,
            #[serde(flatten)]
            values: &'a BTreeMap<String, String>,
        }

        serde_json::to_vec(&Payload {
            schema: &self.schema,
            values,
        })
        .map_err(|error| ActionFormError::Encode(error.to_string()))
    }
}

/// One named input in an [`ActionFormV1`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionFormFieldV1 {
    /// JSON property name. `schema` is reserved for the form schema marker.
    pub name: String,
    /// What a host shows beside the input.
    pub label: String,
    /// Longer guidance, or empty.
    pub description: String,
    /// Whether composing without this field is refused. Defaults to true, so
    /// an older endpoint's form does not silently become optional.
    #[serde(default = "required_action_form_field")]
    pub required: bool,
    /// The exact values the endpoint will accept. A host offers these and
    /// nothing else.
    pub choices: Vec<ActionFormChoiceV1>,
}

const fn required_action_form_field() -> bool {
    true
}

impl ActionFormFieldV1 {
    /// A required field offering exactly these choices.
    pub fn choice(
        name: impl Into<String>,
        label: impl Into<String>,
        choices: impl IntoIterator<Item = ActionFormChoiceV1>,
    ) -> Self {
        Self {
            name: name.into(),
            label: label.into(),
            description: String::new(),
            required: true,
            choices: choices.into_iter().collect(),
        }
    }

    /// Add the longer guidance.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Let this field be omitted when the payload is composed.
    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }
}

/// One exact selectable value supplied by the endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionFormChoiceV1 {
    /// Opaque payload value. Hosts display the label and return this exact value.
    pub value: String,
    /// What the host shows in place of the value.
    pub label: String,
    /// Longer guidance for this one choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ActionFormChoiceV1 {
    /// A choice carrying `value` behind `label`.
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
        }
    }

    /// Add the longer guidance.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Reasons a host must not compose an action payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionFormError {
    /// Values were supplied for an action that advertised no form. Refused
    /// rather than guessing an encoding the endpoint never described.
    NoInputForm,
    /// The form's own schema string is blank.
    EmptyFormSchema,
    /// The form belongs to a different action than the one being composed.
    SchemaMismatch {
        /// The schema the action advertised.
        expected: String,
        /// The schema the form carries.
        form: String,
    },
    /// A field has no JSON property name.
    EmptyFieldName,
    /// A field claimed the name `schema`, which the form writes itself.
    ReservedFieldName,
    /// Two fields share one name.
    DuplicateField(String),
    /// The named field has nothing to show beside its input.
    EmptyFieldLabel(String),
    /// The named field offers nothing to choose, so it can never be satisfied.
    EmptyChoices(String),
    /// A choice in the named field carries no value.
    EmptyChoiceValue(String),
    /// A choice in the named field has nothing to show.
    EmptyChoiceLabel(String),
    /// Two choices in one field carry the same value.
    DuplicateChoiceValue {
        /// The field holding the repeat.
        field: String,
        /// The value offered twice.
        value: String,
    },
    /// A required field had no value.
    MissingField(String),
    /// A value was supplied under a name the form never advertised.
    UnknownField(String),
    /// A value was supplied that the endpoint did not offer. This is the
    /// bound that keeps a host from inventing payloads.
    InvalidChoice {
        /// The field the value was offered for.
        field: String,
        /// The value that was not on offer.
        value: String,
    },
    /// The composed object would not serialize.
    Encode(String),
}

impl fmt::Display for ActionFormError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoInputForm => write!(formatter, "action has no advertised input form"),
            Self::EmptyFormSchema => write!(formatter, "action form schema is empty"),
            Self::SchemaMismatch { expected, form } => {
                write!(
                    formatter,
                    "action schema {expected} does not match form schema {form}"
                )
            }
            Self::EmptyFieldName => write!(formatter, "action form field name is empty"),
            Self::ReservedFieldName => {
                write!(formatter, "action form field name schema is reserved")
            }
            Self::DuplicateField(field) => write!(formatter, "action form repeats field {field}"),
            Self::EmptyFieldLabel(field) => {
                write!(formatter, "action form field {field} has no label")
            }
            Self::EmptyChoices(field) => {
                write!(formatter, "action form field {field} has no choices")
            }
            Self::EmptyChoiceValue(field) => {
                write!(
                    formatter,
                    "action form field {field} has an empty choice value"
                )
            }
            Self::EmptyChoiceLabel(field) => {
                write!(
                    formatter,
                    "action form field {field} has an empty choice label"
                )
            }
            Self::DuplicateChoiceValue { field, value } => {
                write!(
                    formatter,
                    "action form field {field} repeats choice value {value}"
                )
            }
            Self::MissingField(field) => write!(formatter, "action form requires field {field}"),
            Self::UnknownField(field) => {
                write!(formatter, "action form does not define field {field}")
            }
            Self::InvalidChoice { field, value } => {
                write!(
                    formatter,
                    "action form field {field} does not offer choice {value}"
                )
            }
            Self::Encode(error) => {
                write!(formatter, "could not encode action form payload: {error}")
            }
        }
    }
}

impl std::error::Error for ActionFormError {}

/// One labeled value in a portable card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardValueV1 {
    /// What the value is called.
    pub label: String,
    /// The value itself, already rendered to text by the endpoint.
    pub value: String,
}

/// A deliberately small semantic card, not a serialized widget tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortableCardV1 {
    /// What the card is about.
    pub title: String,
    /// Labeled facts, in the order the endpoint wants them read.
    pub values: Vec<CardValueV1>,
    /// Short status words a host may render as chips.
    pub badges: Vec<String>,
    /// Addresses of separately transferred resources. The bytes are fetched
    /// through the carrier; the card carries only where to ask.
    pub media: Vec<ContentHash>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> ActionFormV1 {
        ActionFormV1::new("fixture.save/v1").with_field(ActionFormFieldV1::choice(
            "digest",
            "Saved facts",
            [
                ActionFormChoiceV1::new("facts-a", "Morning facts"),
                ActionFormChoiceV1::new("facts-b", "Evening facts"),
            ],
        ))
    }

    #[test]
    fn a_form_composes_only_the_exact_values_the_endpoint_offered() {
        // The bound that makes this safe to hand a host: it returns the
        // endpoint's own opaque value, never something the host composed.
        let values = BTreeMap::from([("digest".to_string(), "facts-b".to_string())]);
        let payload: serde_json::Value =
            serde_json::from_slice(&form().compose_payload("fixture.save/v1", &values).unwrap())
                .unwrap();
        assert_eq!(
            payload,
            serde_json::json!({"schema": "fixture.save/v1", "digest": "facts-b"})
        );
    }

    #[test]
    fn a_value_outside_the_offered_choices_is_refused() {
        // A host that invents a value is the failure this form exists to make
        // impossible; it must not reach the endpoint as a plausible payload.
        let values = BTreeMap::from([("digest".to_string(), "facts-invented".to_string())]);
        assert!(matches!(
            form().compose_payload("fixture.save/v1", &values),
            Err(ActionFormError::InvalidChoice { .. })
        ));
    }

    #[test]
    fn the_form_must_match_the_schema_it_is_composed_against() {
        let values = BTreeMap::from([("digest".to_string(), "facts-a".to_string())]);
        assert!(matches!(
            form().compose_payload("fixture.other/v1", &values),
            Err(ActionFormError::SchemaMismatch { .. })
        ));
    }

    #[test]
    fn a_missing_required_value_and_an_unknown_name_are_both_refused() {
        assert!(matches!(
            form().compose_payload("fixture.save/v1", &BTreeMap::new()),
            Err(ActionFormError::MissingField(_))
        ));
        let stray = BTreeMap::from([("nowhere".to_string(), "x".to_string())]);
        assert!(matches!(
            form().compose_payload("fixture.save/v1", &stray),
            Err(ActionFormError::UnknownField(_))
        ));
    }

    #[test]
    fn an_optional_field_may_be_omitted() {
        let optional = ActionFormV1::new("fixture.save/v1").with_field(
            ActionFormFieldV1::choice("digest", "Saved facts", [ActionFormChoiceV1::new("a", "A")])
                .optional(),
        );
        let payload: serde_json::Value = serde_json::from_slice(
            &optional
                .compose_payload("fixture.save/v1", &BTreeMap::new())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(payload, serde_json::json!({"schema": "fixture.save/v1"}));
    }

    #[test]
    fn schema_is_a_reserved_field_name_because_the_form_writes_it_itself() {
        let clashing = ActionFormV1::new("fixture.save/v1").with_field(ActionFormFieldV1::choice(
            "schema",
            "Schema",
            [ActionFormChoiceV1::new("x", "X")],
        ));
        assert_eq!(clashing.validate(), Err(ActionFormError::ReservedFieldName));
    }

    #[test]
    fn a_content_hash_addresses_bytes_and_prints_as_hex() {
        let hash = ContentHash::of(b"a projected resource");
        assert_eq!(hash, ContentHash::of(b"a projected resource"));
        assert_ne!(hash, ContentHash::of(b"a different one"));
        let rendered = hash.to_string();
        assert_eq!(rendered.len(), 64);
        assert!(rendered.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn a_card_round_trips_and_carries_only_what_a_host_may_render() {
        let card = PortableCardV1 {
            title: "Identity vault".to_string(),
            values: vec![CardValueV1 {
                label: "Protection".to_string(),
                value: "OS-protected".to_string(),
            }],
            badges: vec!["Personae".to_string()],
            media: vec![ContentHash::of(b"thumb")],
        };
        let json = serde_json::to_string(&card).unwrap();
        assert_eq!(serde_json::from_str::<PortableCardV1>(&json).unwrap(), card);
    }
}
