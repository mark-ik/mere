// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Content refs for carry records: [`CarryRef`] and its display/serde forms.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The hash function behind a [`CarryRef`]. BLAKE3-256 today.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CarryHashFn {
    /// BLAKE3-256, 32-byte digest.
    Blake3,
}

impl CarryHashFn {
    fn label(self) -> &'static str {
        match self {
            CarryHashFn::Blake3 => "blake3",
        }
    }
}

/// A content ref in a carry record: a digest tagged with the function that
/// produced it, serialized as the display string `<fn>:<hex>`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CarryRef {
    func: CarryHashFn,
    digest: [u8; 32],
}

impl CarryRef {
    /// Compute the BLAKE3-256 ref of a byte slice.
    pub fn of(bytes: &[u8]) -> Self {
        Self {
            func: CarryHashFn::Blake3,
            digest: *blake3::hash(bytes).as_bytes(),
        }
    }

    /// The raw digest bytes.
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    /// The hash function tag.
    pub fn func(&self) -> CarryHashFn {
        self.func
    }
}

impl fmt::Display for CarryRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.func.label())?;
        for byte in self.digest {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for CarryRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CarryRef({self})")
    }
}

/// Error parsing a [`CarryRef`] display string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarryRefParseError(String);

impl fmt::Display for CarryRefParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid carry ref: {}", self.0)
    }
}

impl std::error::Error for CarryRefParseError {}

impl FromStr for CarryRef {
    type Err = CarryRefParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (func, hex) = s
            .split_once(':')
            .ok_or_else(|| CarryRefParseError(format!("missing ':' in {s:?}")))?;
        let func = match func {
            "blake3" => CarryHashFn::Blake3,
            other => return Err(CarryRefParseError(format!("unknown hash fn {other:?}"))),
        };
        if hex.len() != 64 {
            return Err(CarryRefParseError(format!(
                "expected 64 hex chars, got {}",
                hex.len()
            )));
        }
        let mut digest = [0u8; 32];
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            let pair = std::str::from_utf8(chunk)
                .map_err(|_| CarryRefParseError(format!("non-utf8 hex in {s:?}")))?;
            digest[i] = u8::from_str_radix(pair, 16)
                .map_err(|_| CarryRefParseError(format!("non-hex byte {pair:?}")))?;
        }
        Ok(Self { func, digest })
    }
}

impl Serialize for CarryRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for CarryRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}
