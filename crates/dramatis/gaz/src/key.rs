//! The stable middle a contact is rooted on.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A contact's root public key.
///
/// Thirty-two bytes, the width of an Ed25519 public key, which is what
/// `personae` mints and what murm, Nostr, and the DID methods all carry. A
/// contact is rooted here rather than on a handle, so a peer who moves hosts
/// stays the same contact.
///
/// gaz stores keys and compares them. It never verifies a signature with one:
/// crypto belongs to the trust plane, which is why this crate depends on no
/// cryptography at all.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContactKey([u8; 32]);

/// Hex in text formats, raw bytes in binary ones.
///
/// Not decoration. A key is a map key in [`ContactBook`], and JSON map keys
/// must be strings, so a byte array cannot round-trip through the JSON codec
/// at all. Hex also makes a stored book readable by a person, which is worth
/// something for a file that holds who you know. Binary formats keep the raw
/// bytes, so postcard pays nothing for the courtesy.
///
/// [`ContactBook`]: crate::ContactBook
impl Serialize for ContactKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_str(&self.to_hex())
        } else {
            self.0.serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for ContactKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            let text = String::deserialize(deserializer)?;
            Self::from_hex(&text).map_err(serde::de::Error::custom)
        } else {
            <[u8; 32]>::deserialize(deserializer).map(Self)
        }
    }
}

impl ContactKey {
    /// The width of a key in bytes.
    pub const LEN: usize = 32;

    /// Wrap raw key bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the raw key bytes, for handing to a verifier that does own crypto.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hex, 64 characters.
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(64);
        for byte in self.0 {
            out.push(hex_digit(byte >> 4));
            out.push(hex_digit(byte & 0x0f));
        }
        out
    }

    /// Parse lowercase or uppercase hex, 64 characters.
    pub fn from_hex(text: &str) -> Result<Self, KeyParseError> {
        let bytes = text.as_bytes();
        if bytes.len() != 64 {
            return Err(KeyParseError::Length { found: bytes.len() });
        }
        let mut out = [0u8; 32];
        for (index, pair) in bytes.chunks_exact(2).enumerate() {
            let high = hex_value(pair[0])?;
            let low = hex_value(pair[1])?;
            out[index] = (high << 4) | low;
        }
        Ok(Self(out))
    }

    /// The first eight hex characters, for logs and compact UI.
    ///
    /// A prefix is never an identity. Compare whole keys.
    pub fn short(&self) -> String {
        self.to_hex()[..8].to_string()
    }
}

const fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'a' + nibble - 10) as char,
    }
}

const fn hex_value(byte: u8) -> Result<u8, KeyParseError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(KeyParseError::Digit),
    }
}

/// Full hex, so a key round-trips through `to_string` and `parse`.
impl fmt::Display for ContactKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Abbreviated, so a log line stays readable.
impl fmt::Debug for ContactKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContactKey({})", self.short())
    }
}

impl FromStr for ContactKey {
    type Err = KeyParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::from_hex(text)
    }
}

impl From<[u8; 32]> for ContactKey {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// Why a hex string was not a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyParseError {
    /// Not 64 hex characters.
    Length {
        /// How many characters were supplied.
        found: usize,
    },
    /// A character outside `[0-9a-fA-F]`.
    Digit,
}

impl fmt::Display for KeyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Length { found } => {
                write!(f, "a contact key is 64 hex characters, found {found}")
            }
            Self::Digit => f.write_str("a contact key holds hex digits only"),
        }
    }
}

impl core::error::Error for KeyParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ContactKey {
        let mut bytes = [0u8; 32];
        for (index, slot) in bytes.iter_mut().enumerate() {
            *slot = index as u8;
        }
        ContactKey::from_bytes(bytes)
    }

    #[test]
    fn hex_round_trips() {
        let key = sample();
        assert_eq!(ContactKey::from_hex(&key.to_hex()), Ok(key));
    }

    #[test]
    fn parses_uppercase() {
        let key = sample();
        let upper = key.to_hex().to_uppercase();
        assert_eq!(ContactKey::from_hex(&upper), Ok(key));
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            ContactKey::from_hex("abcd"),
            Err(KeyParseError::Length { found: 4 })
        );
    }

    #[test]
    fn rejects_non_hex() {
        let bad = "z".repeat(64);
        assert_eq!(ContactKey::from_hex(&bad), Err(KeyParseError::Digit));
    }

    #[test]
    fn display_is_full_and_debug_is_short() {
        let key = sample();
        assert_eq!(key.to_string().len(), 64);
        assert_eq!(key.short().len(), 8);
        assert!(format!("{key:?}").contains(&key.short()));
    }

    #[test]
    fn serde_round_trips() {
        let key = sample();
        let json = serde_json::to_string(&key).unwrap();
        assert_eq!(serde_json::from_str::<ContactKey>(&json).unwrap(), key);
    }

    #[test]
    fn json_carries_a_hex_string_not_a_byte_array() {
        let json = serde_json::to_string(&sample()).unwrap();
        assert!(
            json.starts_with('"'),
            "a key must be a JSON string so it can serve as a map key, got {json}"
        );
        assert_eq!(json, format!("\"{}\"", sample().to_hex()));
    }

    #[test]
    fn bad_hex_fails_to_deserialize() {
        assert!(serde_json::from_str::<ContactKey>("\"nope\"").is_err());
    }
}
