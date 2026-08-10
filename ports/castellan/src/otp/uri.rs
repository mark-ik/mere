// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `otpauth://` Key Uri Format, which is how a QR code hands over a
//! secret.
//!
//! The format has no RFC. It is a de-facto standard set by Google
//! Authenticator's wiki page and followed by every issuer since, so this
//! parser is written to what is actually emitted rather than to a spec:
//! padding is often missing from the secret, the issuer is often stated twice
//! (once as a label prefix and once as a parameter), and unknown parameters
//! turn up and must be ignored rather than rejected.

use std::fmt;

use super::{DEFAULT_DIGITS, DEFAULT_PERIOD_SECS, Otp, OtpAlgorithm, OtpError, base32};

/// What a parsed `otpauth://` URI carried, alongside the generator itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtpUri {
    /// The account this credential is for, with any issuer prefix stripped.
    pub account: String,
    /// The issuing service, if the URI stated one.
    pub issuer: Option<String>,
}

/// Why an `otpauth://` URI could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OtpUriError {
    /// Not an `otpauth://` URI at all.
    NotOtpauth,
    /// The type segment was neither `totp` nor `hotp`.
    UnknownType(String),
    /// No `secret` parameter, which is the one thing a URI must carry.
    MissingSecret,
    /// The secret was not valid base32.
    Secret(base32::Base32Error),
    /// A `hotp://` URI without the `counter` it requires.
    MissingCounter,
    /// A numeric parameter that was not a number.
    InvalidNumber {
        /// The parameter name.
        parameter: &'static str,
        /// What was found instead.
        value: String,
    },
    /// An `algorithm` value outside SHA1 / SHA256 / SHA512.
    UnknownAlgorithm(String),
    /// The generator rejected the configuration.
    Configuration(OtpError),
}

impl fmt::Display for OtpUriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OtpUriError::NotOtpauth => f.write_str("not an otpauth:// uri"),
            OtpUriError::UnknownType(t) => write!(f, "unknown otp type {t:?}"),
            OtpUriError::MissingSecret => f.write_str("the uri carries no secret parameter"),
            OtpUriError::Secret(e) => write!(f, "the secret is not valid base32: {e}"),
            OtpUriError::MissingCounter => {
                f.write_str("an hotp uri must carry a counter parameter")
            }
            OtpUriError::InvalidNumber { parameter, value } => {
                write!(f, "{parameter} is not a number: {value:?}")
            }
            OtpUriError::UnknownAlgorithm(a) => write!(f, "unknown algorithm {a:?}"),
            OtpUriError::Configuration(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OtpUriError {}

/// Parse an `otpauth://totp/...` or `otpauth://hotp/...` URI.
///
/// Unknown query parameters are ignored rather than rejected, because issuers
/// add their own and a credential that is otherwise valid should still import.
pub fn parse_otpauth_uri(uri: &str) -> Result<(Otp, OtpUri), OtpUriError> {
    let rest = strip_scheme(uri).ok_or(OtpUriError::NotOtpauth)?;

    let (kind_and_label, query) = match rest.split_once('?') {
        Some((head, query)) => (head, query),
        None => (rest, ""),
    };
    let (kind, label) = match kind_and_label.split_once('/') {
        Some((kind, label)) => (kind, label),
        None => (kind_and_label, ""),
    };

    let is_totp = match kind.to_ascii_lowercase().as_str() {
        "totp" => true,
        "hotp" => false,
        other => return Err(OtpUriError::UnknownType(other.to_string())),
    };

    let params = parse_query(query);
    let get = |name: &str| params.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());

    let secret = base32::decode(get("secret").ok_or(OtpUriError::MissingSecret)?)
        .map_err(OtpUriError::Secret)?;

    // The label is `issuer:account` or just `account`. When both the prefix
    // and the `issuer=` parameter are present they normally agree; the
    // parameter is the authority when they do not, because the prefix is the
    // older convention and more often stale.
    let label = percent_decode(label);
    let (label_issuer, account) = match label.split_once(':') {
        Some((issuer, account)) => (Some(issuer.trim().to_string()), account.trim().to_string()),
        None => (None, label.trim().to_string()),
    };
    let issuer = get("issuer").map(percent_decode).or(label_issuer);

    let algorithm = match get("algorithm") {
        None => OtpAlgorithm::default(),
        Some(a) => match a.to_ascii_uppercase().as_str() {
            "SHA1" => OtpAlgorithm::Sha1,
            "SHA256" => OtpAlgorithm::Sha256,
            "SHA512" => OtpAlgorithm::Sha512,
            other => return Err(OtpUriError::UnknownAlgorithm(other.to_string())),
        },
    };

    let digits = match get("digits") {
        None => DEFAULT_DIGITS,
        Some(d) => d.parse().map_err(|_| OtpUriError::InvalidNumber {
            parameter: "digits",
            value: d.to_string(),
        })?,
    };

    let otp = if is_totp {
        let period = match get("period") {
            None => DEFAULT_PERIOD_SECS,
            Some(p) => p.parse().map_err(|_| OtpUriError::InvalidNumber {
                parameter: "period",
                value: p.to_string(),
            })?,
        };
        Otp::totp(secret)
            .with_algorithm(algorithm)
            .with_digits(digits)
            .map_err(OtpUriError::Configuration)?
            .with_period(period)
            .map_err(OtpUriError::Configuration)?
    } else {
        let counter_text = get("counter").ok_or(OtpUriError::MissingCounter)?;
        let counter = counter_text
            .parse()
            .map_err(|_| OtpUriError::InvalidNumber {
                parameter: "counter",
                value: counter_text.to_string(),
            })?;
        Otp::hotp(secret, counter)
            .with_algorithm(algorithm)
            .with_digits(digits)
            .map_err(OtpUriError::Configuration)?
    };

    Ok((otp, OtpUri { account, issuer }))
}

fn strip_scheme(uri: &str) -> Option<&str> {
    let lower = uri.to_ascii_lowercase();
    lower
        .starts_with("otpauth://")
        .then(|| &uri["otpauth://".len()..])
}

fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) => (k.to_ascii_lowercase(), v.to_string()),
            None => (pair.to_ascii_lowercase(), String::new()),
        })
        .collect()
}

/// Decode `%XX` escapes and `+` as space. Invalid escapes are left as written
/// rather than treated as failure: a mangled label should not stop a valid
/// secret from importing.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    None => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
