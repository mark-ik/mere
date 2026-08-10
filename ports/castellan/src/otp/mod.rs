// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! One-time passwords: HOTP ([RFC 4226]) and TOTP ([RFC 6238]).
//!
//! This is the castellan *exercising* a chatelaine item. The secret is
//! material that is damaged by disclosure, so nothing here returns it; the
//! only outputs are codes, which are meant to be shown once and then are
//! worthless.
//!
//! Scope is deliberately the algorithm and its import format. Storage,
//! presentation, and release-through-the-gate are later slices; see the
//! castellan OTP plan.
//!
//! ```
//! use castellan::otp::{Otp, OtpAlgorithm};
//!
//! // RFC 6238's first published vector, on the SHA-1 seed.
//! let otp = Otp::totp(b"12345678901234567890".to_vec())
//!     .with_digits(8)
//!     .unwrap();
//! assert_eq!(otp.code_at_unix_time(59).unwrap(), "94287082");
//! assert_eq!(otp.algorithm(), OtpAlgorithm::Sha1);
//! ```
//!
//! [RFC 4226]: https://www.rfc-editor.org/rfc/rfc4226
//! [RFC 6238]: https://www.rfc-editor.org/rfc/rfc6238

pub mod base32;
mod uri;

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Sha256, Sha512};

pub use base32::Base32Error;
pub use uri::{OtpUriError, parse_otpauth_uri};

/// The default time step, in seconds. RFC 6238 §5.2 recommends 30.
pub const DEFAULT_PERIOD_SECS: u64 = 30;
/// The default code length. Six digits is what authenticators show.
pub const DEFAULT_DIGITS: u32 = 6;

/// The HMAC hash behind a code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OtpAlgorithm {
    /// SHA-1. The RFC 4226 original and what essentially every issuer uses.
    #[default]
    Sha1,
    /// SHA-256.
    Sha256,
    /// SHA-512.
    Sha512,
}

impl OtpAlgorithm {
    /// The spelling used in an `otpauth://` URI.
    pub fn as_uri_str(self) -> &'static str {
        match self {
            OtpAlgorithm::Sha1 => "SHA1",
            OtpAlgorithm::Sha256 => "SHA256",
            OtpAlgorithm::Sha512 => "SHA512",
        }
    }
}

impl fmt::Display for OtpAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_uri_str())
    }
}

/// Whether codes advance on a clock or on a stored counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OtpKind {
    /// Time-based (RFC 6238), stepping every `period` seconds from `t0`.
    Totp {
        /// Seconds per step.
        period: u64,
        /// The epoch the step count is measured from. Zero in practice.
        t0: u64,
    },
    /// Counter-based (RFC 4226), advancing only when used.
    Hotp {
        /// The next counter value to use.
        counter: u64,
    },
}

/// Why a generator could not be built or a code could not be produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OtpError {
    /// Digit counts outside 6..=10 are not representable: dynamic truncation
    /// yields 31 bits, so 10 digits is the ceiling, and RFC 4226 sets 6 as the
    /// floor for the code to carry enough entropy.
    UnsupportedDigits(u32),
    /// A zero time step would divide by zero.
    ZeroPeriod,
    /// The requested time is before the generator's `t0`.
    TimeBeforeEpoch,
    /// The host clock is before the Unix epoch.
    ClockBeforeUnixEpoch,
    /// An empty secret. RFC 4226 §4 R6 requires at least 128 bits, but any
    /// secret at all is the check worth enforcing here; issuers vary.
    EmptySecret,
}

impl fmt::Display for OtpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OtpError::UnsupportedDigits(n) => {
                write!(f, "{n} digits is outside the supported range of 6 to 10")
            }
            OtpError::ZeroPeriod => f.write_str("the time step must be at least one second"),
            OtpError::TimeBeforeEpoch => {
                f.write_str("the requested time is before this generator's epoch")
            }
            OtpError::ClockBeforeUnixEpoch => f.write_str("the host clock is before 1970"),
            OtpError::EmptySecret => f.write_str("the shared secret is empty"),
        }
    }
}

impl std::error::Error for OtpError {}

/// A configured one-time-password generator.
///
/// The secret is held but never handed back: `Debug` redacts it, and there is
/// no accessor. A caller that needs the secret bytes is doing storage, which
/// is the chatelaine's job, not this type's.
#[derive(Clone)]
pub struct Otp {
    secret: Vec<u8>,
    algorithm: OtpAlgorithm,
    digits: u32,
    kind: OtpKind,
}

impl fmt::Debug for Otp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Otp")
            .field("secret", &format_args!("<{} bytes redacted>", self.secret.len()))
            .field("algorithm", &self.algorithm)
            .field("digits", &self.digits)
            .field("kind", &self.kind)
            .finish()
    }
}

impl Otp {
    /// A time-based generator with the default 30-second step and 6 digits.
    pub fn totp(secret: Vec<u8>) -> Self {
        Self {
            secret,
            algorithm: OtpAlgorithm::default(),
            digits: DEFAULT_DIGITS,
            kind: OtpKind::Totp {
                period: DEFAULT_PERIOD_SECS,
                t0: 0,
            },
        }
    }

    /// A counter-based generator starting at `counter`.
    pub fn hotp(secret: Vec<u8>, counter: u64) -> Self {
        Self {
            secret,
            algorithm: OtpAlgorithm::default(),
            digits: DEFAULT_DIGITS,
            kind: OtpKind::Hotp { counter },
        }
    }

    /// Choose the HMAC hash.
    pub fn with_algorithm(mut self, algorithm: OtpAlgorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// Set the code length. Supported range is 6 to 10 inclusive.
    pub fn with_digits(mut self, digits: u32) -> Result<Self, OtpError> {
        if !(6..=10).contains(&digits) {
            return Err(OtpError::UnsupportedDigits(digits));
        }
        self.digits = digits;
        Ok(self)
    }

    /// Set the time step. Time-based generators only; ignored for HOTP.
    pub fn with_period(mut self, period: u64) -> Result<Self, OtpError> {
        if period == 0 {
            return Err(OtpError::ZeroPeriod);
        }
        if let OtpKind::Totp { t0, .. } = self.kind {
            self.kind = OtpKind::Totp { period, t0 };
        }
        Ok(self)
    }

    /// The HMAC hash in use.
    pub fn algorithm(&self) -> OtpAlgorithm {
        self.algorithm
    }

    /// The code length.
    pub fn digits(&self) -> u32 {
        self.digits
    }

    /// Whether this generator steps on a clock or a counter.
    pub fn kind(&self) -> OtpKind {
        self.kind
    }

    /// The code for an explicit Unix time, in seconds.
    ///
    /// Time-based generators derive the counter from the clock; counter-based
    /// ones ignore the argument and use their stored counter.
    pub fn code_at_unix_time(&self, unix_secs: u64) -> Result<String, OtpError> {
        self.code_for_counter(self.counter_at_unix_time(unix_secs)?)
    }

    /// The code for the host clock's current time.
    pub fn code_now(&self) -> Result<String, OtpError> {
        self.code_at_unix_time(now_unix_secs()?)
    }

    /// The counter this generator would use at `unix_secs`.
    pub fn counter_at_unix_time(&self, unix_secs: u64) -> Result<u64, OtpError> {
        match self.kind {
            OtpKind::Totp { period, t0 } => {
                if period == 0 {
                    return Err(OtpError::ZeroPeriod);
                }
                let elapsed = unix_secs.checked_sub(t0).ok_or(OtpError::TimeBeforeEpoch)?;
                Ok(elapsed / period)
            }
            OtpKind::Hotp { counter } => Ok(counter),
        }
    }

    /// Seconds until the current time-based code is replaced.
    ///
    /// Returns `None` for counter-based generators, which do not expire.
    pub fn seconds_remaining_at(&self, unix_secs: u64) -> Option<u64> {
        match self.kind {
            OtpKind::Totp { period, t0 } if period > 0 => {
                let elapsed = unix_secs.checked_sub(t0)?;
                Some(period - (elapsed % period))
            }
            _ => None,
        }
    }

    /// The code for an explicit counter, which is the RFC 4226 primitive.
    pub fn code_for_counter(&self, counter: u64) -> Result<String, OtpError> {
        if self.secret.is_empty() {
            return Err(OtpError::EmptySecret);
        }
        let digest = self.hmac(&counter.to_be_bytes());
        Ok(format_code(truncate(&digest), self.digits))
    }

    /// Whether `candidate` matches the code at `unix_secs`, allowing the
    /// adjacent `skew_steps` in each direction.
    ///
    /// A skew of 1 is the usual choice: it forgives a slow typist and a clock
    /// a few seconds out, at the cost of widening the window a guess can land
    /// in. Comparison is constant-time so a rejected code leaks nothing about
    /// how much of it was right.
    pub fn verify_at_unix_time(
        &self,
        candidate: &str,
        unix_secs: u64,
        skew_steps: u64,
    ) -> Result<bool, OtpError> {
        let centre = self.counter_at_unix_time(unix_secs)?;
        let mut matched = false;
        for counter in centre.saturating_sub(skew_steps)..=centre.saturating_add(skew_steps) {
            let expected = self.code_for_counter(counter)?;
            // No early exit: every candidate window is compared so the time
            // taken does not reveal which step matched.
            matched |= constant_time_eq(expected.as_bytes(), candidate.as_bytes());
        }
        Ok(matched)
    }

    fn hmac(&self, message: &[u8]) -> Vec<u8> {
        // Concrete per hash rather than generic: the RustCrypto bounds that
        // make one generic function work are considerably harder to read than
        // three lines of expansion.
        macro_rules! mac_with {
            ($hash:ty) => {{
                let mut mac = <Hmac<$hash> as Mac>::new_from_slice(&self.secret)
                    .expect("HMAC accepts a key of any length");
                mac.update(message);
                mac.finalize().into_bytes().to_vec()
            }};
        }
        match self.algorithm {
            OtpAlgorithm::Sha1 => mac_with!(Sha1),
            OtpAlgorithm::Sha256 => mac_with!(Sha256),
            OtpAlgorithm::Sha512 => mac_with!(Sha512),
        }
    }
}

/// RFC 4226 §5.3 dynamic truncation: the low nibble of the last byte picks the
/// offset, and the four bytes there become a 31-bit integer.
fn truncate(digest: &[u8]) -> u32 {
    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let slice = &digest[offset..offset + 4];
    u32::from_be_bytes([slice[0] & 0x7f, slice[1], slice[2], slice[3]])
}

fn format_code(value: u32, digits: u32) -> String {
    let modulus = 10u32.pow(digits);
    format!("{:0width$}", value % modulus, width = digits as usize)
}

fn now_unix_secs() -> Result<u64, OtpError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| OtpError::ClockBeforeUnixEpoch)
}

/// Compare without an early exit, so timing does not reveal a prefix match.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests;
