// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Explicit compatibility with Valve's Steam Guard mobile codes.
//!
//! Valve does not publish this as an OTP standard. The compatibility shape is
//! therefore kept separate from [`super::Otp`] and `otpauth://`: a 20-byte
//! base64 `shared_secret`, HMAC-SHA1 over a 30-second Unix counter, and Valve's
//! five-character alphabet. The checked [compatibility corpus] is maintained
//! by the Steam-focused `steamguard` library rather than presented as a Valve
//! specification.
//!
//! [compatibility corpus]: https://github.com/dyc3/steamguard-cli/blob/87005d262695bea1b2bee682dabc03559856f00c/steamguard/src/token.rs#L244-L249

use std::fmt;

use base64::Engine as _;
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use zeroize::Zeroizing;

const SHARED_SECRET_BYTES: usize = 20;
const CODE_PERIOD_SECS: u64 = 30;
const CODE_CHARACTERS: &[u8; 26] = b"23456789BCDFGHJKMNPQRTVWXY";

/// Failure while importing Steam Guard compatibility material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SteamGuardError {
    /// The `shared_secret` was not standard base64.
    InvalidBase64,
    /// Steam Guard shared secrets are exactly 20 bytes after decoding.
    InvalidSecretLength {
        /// Number of decoded bytes supplied.
        bytes: usize,
    },
    /// The account label was empty, padded, too long, or contained controls.
    InvalidAccount,
}

impl fmt::Display for SteamGuardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBase64 => formatter.write_str("Steam shared_secret is not valid base64"),
            Self::InvalidSecretLength { bytes } => write!(
                formatter,
                "Steam shared_secret decoded to {bytes} bytes; expected {SHARED_SECRET_BYTES}"
            ),
            Self::InvalidAccount => formatter
                .write_str("Steam account must be trimmed printable text of at most 256 bytes"),
        }
    }
}

impl std::error::Error for SteamGuardError {}

/// A Steam Guard mobile-code compatibility generator.
///
/// The shared secret is redacted from `Debug`, zeroized on drop, and has no
/// accessor. Stored use should go through the resident release gate.
#[derive(Clone)]
pub struct SteamGuard {
    secret: Zeroizing<[u8; SHARED_SECRET_BYTES]>,
}

impl fmt::Debug for SteamGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SteamGuard")
            .field("secret", &"<20 bytes redacted>")
            .finish()
    }
}

impl SteamGuard {
    /// Decode the base64 `shared_secret` carried by Steam authenticator files.
    pub fn from_base64_shared_secret(shared_secret: &str) -> Result<Self, SteamGuardError> {
        Ok(Self {
            secret: decode_shared_secret(shared_secret)?,
        })
    }

    pub(super) fn from_secret_bytes(secret: &[u8]) -> Result<Self, SteamGuardError> {
        let bytes = secret.len();
        let secret = secret
            .try_into()
            .map_err(|_| SteamGuardError::InvalidSecretLength { bytes })?;
        Ok(Self {
            secret: Zeroizing::new(secret),
        })
    }

    /// Generate Valve's five-character code for an explicit Unix second.
    pub fn code_at_unix_time(&self, unix_secs: u64) -> String {
        let counter = (unix_secs / CODE_PERIOD_SECS).to_be_bytes();
        let mut mac = <Hmac<Sha1> as KeyInit>::new_from_slice(self.secret.as_slice())
            .expect("HMAC accepts a 20-byte key");
        mac.update(&counter);
        let digest = mac.finalize().into_bytes();
        let offset = usize::from(digest[19] & 0x0f);
        let mut value = u32::from_be_bytes([
            digest[offset] & 0x7f,
            digest[offset + 1],
            digest[offset + 2],
            digest[offset + 3],
        ]);
        let mut code = [0; 5];
        for character in &mut code {
            *character = CODE_CHARACTERS[value as usize % CODE_CHARACTERS.len()];
            value /= CODE_CHARACTERS.len() as u32;
        }
        String::from_utf8(code.to_vec()).expect("the Steam alphabet is ASCII")
    }

    /// Whole seconds before the current Steam code changes.
    pub fn seconds_remaining_at(&self, unix_secs: u64) -> u64 {
        CODE_PERIOD_SECS - (unix_secs % CODE_PERIOD_SECS)
    }
}

pub(super) fn decode_shared_secret(
    shared_secret: &str,
) -> Result<Zeroizing<[u8; SHARED_SECRET_BYTES]>, SteamGuardError> {
    let decoded = Zeroizing::new(
        base64::engine::general_purpose::STANDARD
            .decode(shared_secret)
            .map_err(|_| SteamGuardError::InvalidBase64)?,
    );
    let bytes = decoded.len();
    let secret = decoded
        .as_slice()
        .try_into()
        .map_err(|_| SteamGuardError::InvalidSecretLength { bytes })?;
    Ok(Zeroizing::new(secret))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compatibility corpus from steamguard 0.18.4's independently maintained
    // token tests. Valve has not published a normative algorithm or corpus.
    const SHARED_SECRET: &str = "zvIayp3JPvtvX/QGHqsqKBk/44s=";

    #[test]
    fn matches_the_steamguard_cli_compatibility_corpus() {
        let generator = SteamGuard::from_base64_shared_secret(SHARED_SECRET).unwrap();

        assert_eq!(generator.code_at_unix_time(1_616_374_841), "2F9J5");
        assert_eq!(generator.seconds_remaining_at(1_616_374_841), 19);
        assert!(!format!("{generator:?}").contains(SHARED_SECRET));
    }

    #[test]
    fn refuses_non_steam_secret_shapes() {
        assert_eq!(
            SteamGuard::from_base64_shared_secret("not base64").unwrap_err(),
            SteamGuardError::InvalidBase64
        );
        assert!(matches!(
            SteamGuard::from_base64_shared_secret("AQID").unwrap_err(),
            SteamGuardError::InvalidSecretLength { bytes: 3 }
        ));
    }
}
