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
use std::str::FromStr;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;

/// A content address for a separately transferred resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    /// The address of these bytes (BLAKE3).
    pub fn of(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    /// Parse the lowercase hexadecimal digest used by iroh's BLAKE3 address.
    pub fn from_hex(value: &str) -> Result<Self, ContentReferenceError> {
        parse_lower_hex_32(value)
            .map(Self)
            .ok_or(ContentReferenceError::InvalidBlake3)
    }

    /// The raw BLAKE3 digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
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

/// RFC 6920 Named Information URI carrying one full SHA-256 digest.
///
/// The canonical form is `ni:///sha-256;<base64url>` with no authority,
/// query, or padding. Restricting emission to that mandatory algorithm and
/// spelling gives Graphshell and Knot one interoperable portable identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sha256NamedInformation([u8; 32]);

impl Sha256NamedInformation {
    /// Canonical prefix for the supported RFC 6920 form.
    pub const PREFIX: &'static str = "ni:///sha-256;";

    /// Name these bytes portably.
    pub fn of(bytes: &[u8]) -> Self {
        Self(sha2::Sha256::digest(bytes).into())
    }

    /// Construct from the lowercase hexadecimal SHA-256 representation used
    /// by existing browser-file metadata.
    pub fn from_hex(value: &str) -> Result<Self, ContentReferenceError> {
        parse_lower_hex_32(value)
            .map(Self)
            .ok_or(ContentReferenceError::InvalidSha256Hex)
    }

    /// The named digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Verify bytes against the portable identity.
    pub fn verify(&self, bytes: &[u8]) -> Result<(), ContentReferenceError> {
        (Self::of(bytes) == *self)
            .then_some(())
            .ok_or(ContentReferenceError::Sha256Mismatch)
    }
}

impl fmt::Display for Sha256NamedInformation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(Self::PREFIX)?;
        formatter.write_str(&base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(self.0))
    }
}

impl FromStr for Sha256NamedInformation {
    type Err = ContentReferenceError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let encoded = value
            .strip_prefix(Self::PREFIX)
            .ok_or(ContentReferenceError::InvalidNamedInformation)?;
        if encoded.len() != 43 || encoded.contains('=') {
            return Err(ContentReferenceError::InvalidNamedInformation);
        }
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| ContentReferenceError::InvalidNamedInformation)?;
        let digest: [u8; 32] = decoded
            .try_into()
            .map_err(|_| ContentReferenceError::InvalidNamedInformation)?;
        Ok(Self(digest))
    }
}

impl Serialize for Sha256NamedInformation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Sha256NamedInformation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// One portable byte identity beside the BLAKE3 address used for transfer.
///
/// Media type, source URI, and artifact role deliberately live outside this
/// type: they describe provenance and use, while these three fields identify
/// and bound the bytes themselves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortableContentRefV1 {
    /// RFC 6920 portable identity.
    pub portable_id: Sha256NamedInformation,
    /// BLAKE3 address used by the local and iroh content stores.
    #[serde(with = "blake3_transport_serde")]
    pub transport: ContentHash,
    /// Exact byte length, checked before either digest is trusted.
    pub byte_size: u64,
}

impl PortableContentRefV1 {
    /// Name bytes in both portable and transport domains.
    pub fn of(bytes: &[u8]) -> Self {
        Self {
            portable_id: Sha256NamedInformation::of(bytes),
            transport: ContentHash::of(bytes),
            byte_size: u64::try_from(bytes.len()).expect("slice length fits u64"),
        }
    }

    /// Verify length, iroh/BLAKE3 address, then portable SHA-256 identity.
    pub fn verify_bytes(&self, bytes: &[u8]) -> Result<(), ContentReferenceError> {
        if u64::try_from(bytes.len()).ok() != Some(self.byte_size) {
            return Err(ContentReferenceError::ByteSizeMismatch);
        }
        if ContentHash::of(bytes) != self.transport {
            return Err(ContentReferenceError::Blake3Mismatch);
        }
        self.portable_id.verify(bytes)
    }
}

/// A malformed or conflicting portable content reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentReferenceError {
    /// The URI was not the canonical mandatory SHA-256 NI form.
    InvalidNamedInformation,
    /// An existing browser SHA-256 digest was not lowercase 32-byte hex.
    InvalidSha256Hex,
    /// A BLAKE3 transport address was not lowercase 32-byte hex.
    InvalidBlake3,
    /// The declared byte length differed.
    ByteSizeMismatch,
    /// The BLAKE3 transport address differed.
    Blake3Mismatch,
    /// The RFC 6920 SHA-256 identity differed.
    Sha256Mismatch,
}

impl fmt::Display for ContentReferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidNamedInformation => {
                "content name is not canonical ni:///sha-256;<base64url>"
            }
            Self::InvalidSha256Hex => "SHA-256 digest is not 64 lowercase hexadecimal characters",
            Self::InvalidBlake3 => {
                "BLAKE3 transport address is not blake3:<64 lowercase hexadecimal characters>"
            }
            Self::ByteSizeMismatch => "content byte length does not match its reference",
            Self::Blake3Mismatch => "content bytes do not match their BLAKE3 transport address",
            Self::Sha256Mismatch => "content bytes do not match their RFC 6920 SHA-256 identity",
        })
    }
}

impl std::error::Error for ContentReferenceError {}

fn parse_lower_hex_32(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let mut digest = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let hex = std::str::from_utf8(pair).ok()?;
        digest[index] = u8::from_str_radix(hex, 16).ok()?;
    }
    Some(digest)
}

mod blake3_transport_serde {
    use super::{ContentHash, ContentReferenceError};
    use serde::{Deserialize, Serializer};

    pub fn serialize<S>(hash: &ContentHash, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("blake3:{hash}"))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ContentHash, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let hex = value
            .strip_prefix("blake3:")
            .ok_or_else(|| serde::de::Error::custom(ContentReferenceError::InvalidBlake3))?;
        ContentHash::from_hex(hex).map_err(serde::de::Error::custom)
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
    fn portable_content_reference_uses_canonical_ni_and_blake3_spellings() {
        let reference = PortableContentRefV1::of(b"portable bytes");
        reference.verify_bytes(b"portable bytes").unwrap();
        let json = serde_json::to_value(&reference).unwrap();
        let portable = json["portable_id"].as_str().unwrap();
        assert!(portable.starts_with(Sha256NamedInformation::PREFIX));
        assert!(!portable.contains('='));
        assert!(json["transport"].as_str().unwrap().starts_with("blake3:"));
        assert_eq!(
            serde_json::from_value::<PortableContentRefV1>(json).unwrap(),
            reference,
        );
    }

    #[test]
    fn portable_and_transport_conflicts_fail_closed() {
        let bytes = b"portable bytes";
        let mut wrong_portable = PortableContentRefV1::of(bytes);
        wrong_portable.portable_id = Sha256NamedInformation::of(b"different");
        assert_eq!(
            wrong_portable.verify_bytes(bytes),
            Err(ContentReferenceError::Sha256Mismatch),
        );

        let mut wrong_transport = PortableContentRefV1::of(bytes);
        wrong_transport.transport = ContentHash::of(b"different");
        assert_eq!(
            wrong_transport.verify_bytes(bytes),
            Err(ContentReferenceError::Blake3Mismatch),
        );
    }

    #[test]
    fn named_information_parser_rejects_private_or_noncanonical_spellings() {
        let named = Sha256NamedInformation::of(b"named bytes");
        assert_eq!(
            named.to_string().parse::<Sha256NamedInformation>(),
            Ok(named)
        );
        assert!(
            "urn:sha256:abababababababababababababababababababababababababababababababab"
                .parse::<Sha256NamedInformation>()
                .is_err()
        );
        assert!(
            format!("{}{}=", Sha256NamedInformation::PREFIX, "a".repeat(43))
                .parse::<Sha256NamedInformation>()
                .is_err()
        );
    }

    #[test]
    fn named_information_matches_the_rfc_6920_hello_world_vector() {
        const RFC_URI: &str = "ni:///sha-256;f4OxZX_x_FO5LcGBSKHWXfwtSx-j1ncoSt3SABJtkGk";
        let named = Sha256NamedInformation::of(b"Hello World!");
        assert_eq!(named.to_string(), RFC_URI);
        assert_eq!(RFC_URI.parse::<Sha256NamedInformation>(), Ok(named));
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
