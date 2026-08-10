// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Validated resource and implementation identifiers.
//!
//! A [`ResourceId`] such as `esp.embed.lexical/v1` names *what* a job asks for;
//! an [`ImplementationId`] such as `mesh.lexical.fnv1a/v1` names *which build*
//! answered it. Both are extensible strings, not closed enums: registering a
//! resource must not require a wire or board edit.
//!
//! The grammar is `path '/' 'v' digits`, where `path` is dot-separated segments
//! of lowercase alphanumerics, `-`, and `_`. Validation happens before any
//! store mutation, so an unparseable id never reaches the board.

use serde::{Deserialize, Serialize};

/// Longest accepted identifier, in bytes. Generous for real names, small
/// enough that a hostile post cannot inflate the signed body.
pub const MAX_IDENT_LEN: usize = 128;

/// What a job asks for: an extensible, validated resource name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceId(String);

/// Which build of a resource produced an output.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ImplementationId(String);

/// Why an identifier was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IdentError {
    #[error("identifier is empty")]
    Empty,
    #[error("identifier is longer than {MAX_IDENT_LEN} bytes")]
    TooLong,
    #[error("identifier must be `path/vN`, found {0:?}")]
    Shape(String),
    #[error("identifier path segment {0:?} is not lowercase alphanumeric")]
    Segment(String),
    #[error("identifier version {0:?} is not `vN`")]
    Version(String),
}

macro_rules! validated_ident {
    ($name:ident) => {
        impl $name {
            /// Parse and validate.
            pub fn parse(raw: &str) -> Result<Self, IdentError> {
                validate(raw)?;
                Ok(Self(raw.to_string()))
            }

            /// The identifier text.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Re-check a decoded identifier. Wire bytes are attacker-supplied,
            /// so the constructor's guarantee is re-established on decode.
            pub fn validate(&self) -> Result<(), IdentError> {
                validate(&self.0)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl std::str::FromStr for $name {
            type Err = IdentError;

            fn from_str(raw: &str) -> Result<Self, IdentError> {
                Self::parse(raw)
            }
        }
    };
}

validated_ident!(ResourceId);
validated_ident!(ImplementationId);

fn validate(raw: &str) -> Result<(), IdentError> {
    if raw.is_empty() {
        return Err(IdentError::Empty);
    }
    if raw.len() > MAX_IDENT_LEN {
        return Err(IdentError::TooLong);
    }
    let Some((path, version)) = raw.rsplit_once('/') else {
        return Err(IdentError::Shape(raw.to_string()));
    };
    if path.is_empty() {
        return Err(IdentError::Shape(raw.to_string()));
    }
    for segment in path.split('.') {
        if !valid_segment(segment) {
            return Err(IdentError::Segment(segment.to_string()));
        }
    }
    let Some(digits) = version.strip_prefix('v') else {
        return Err(IdentError::Version(version.to_string()));
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(IdentError::Version(version.to_string()));
    }
    Ok(())
}

/// A segment starts and ends alphanumeric; `-` and `_` may appear inside.
fn valid_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    let (Some(first), Some(last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    let edge = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    edge(first) && edge(last) && bytes.iter().all(|b| edge(b) || matches!(b, b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_ids_parse_and_round_trip() {
        for raw in [
            "esp.embed.lexical/v1",
            "mesh.echo/v1",
            "mesh.blake3/v1",
            "a/v0",
            "burn.matmul_f32/v12",
            "some-vendor.thing/v3",
        ] {
            let id = ResourceId::parse(raw).unwrap_or_else(|e| panic!("{raw}: {e}"));
            assert_eq!(id.as_str(), raw);
            assert_eq!(id.to_string(), raw);
            assert!(ImplementationId::parse(raw).is_ok());
        }
    }

    #[test]
    fn malformed_ids_are_refused() {
        for raw in [
            "",
            "no-version",
            "esp.embed.lexical/",
            "esp.embed.lexical/1",
            "esp.embed.lexical/vx",
            "/v1",
            "esp..embed/v1",
            "ESP.embed/v1",
            "esp.embed lexical/v1",
        ] {
            assert!(
                ResourceId::parse(raw).is_err(),
                "{raw:?} should not parse as a resource id"
            );
        }
        let long = format!("{}/v1", "a".repeat(MAX_IDENT_LEN));
        assert_eq!(ResourceId::parse(&long), Err(IdentError::TooLong));
    }

    #[test]
    fn a_decoded_id_is_revalidated() {
        // Wire bytes bypass `parse`, so `validate` is the decode-side guard.
        let forged: ResourceId = p2panda_core::cbor::decode_cbor(
            p2panda_core::cbor::encode_cbor(&"NOT AN ID")
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        assert!(forged.validate().is_err());
    }

    #[test]
    fn ids_order_and_hash_as_their_text() {
        let a = ResourceId::parse("esp.embed.lexical/v1").unwrap();
        let b = ResourceId::parse("mesh.echo/v1").unwrap();
        assert!(a < b);
        assert_eq!(a, ResourceId::parse("esp.embed.lexical/v1").unwrap());
    }
}
