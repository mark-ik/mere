// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The `otpauth://` Key Uri Format, which is how a QR code hands over a
//! secret.
//!
//! The format has no RFC. It is a de-facto standard set by Google
//! Authenticator's wiki page and followed by every issuer since, so this
//! parser is written to what is actually emitted while keeping the format's
//! security-relevant invariants: padding is often missing from the secret,
//! the issuer is often stated twice, and unknown parameters turn up and must
//! be ignored. Duplicate known parameters and conflicting issuer identities
//! are rejected rather than interpreted differently by different consumers.

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
    /// The path carried no account label.
    MissingLabel,
    /// An issuer position was present but empty.
    EmptyIssuer,
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
    /// The Key URI format permits six- or eight-digit codes.
    UnsupportedDigits(u32),
    /// A security-relevant query parameter appeared more than once.
    DuplicateParameter(&'static str),
    /// The issuer label prefix and `issuer=` parameter disagreed.
    IssuerMismatch {
        /// Issuer parsed from the label prefix.
        label: String,
        /// Issuer parsed from the query parameter.
        parameter: String,
    },
    /// The generator rejected the configuration.
    Configuration(OtpError),
}

impl fmt::Display for OtpUriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OtpUriError::NotOtpauth => f.write_str("not an otpauth:// uri"),
            OtpUriError::UnknownType(t) => write!(f, "unknown otp type {t:?}"),
            OtpUriError::MissingSecret => f.write_str("the uri carries no secret parameter"),
            OtpUriError::MissingLabel => f.write_str("the uri carries no account label"),
            OtpUriError::EmptyIssuer => f.write_str("the uri carries an empty issuer"),
            OtpUriError::Secret(e) => write!(f, "the secret is not valid base32: {e}"),
            OtpUriError::MissingCounter => f.write_str("a hotp uri must carry a counter parameter"),
            OtpUriError::InvalidNumber { parameter, value } => {
                write!(f, "{parameter} is not a number: {value:?}")
            }
            OtpUriError::UnknownAlgorithm(a) => write!(f, "unknown algorithm {a:?}"),
            OtpUriError::UnsupportedDigits(digits) => {
                write!(f, "the key uri format supports 6 or 8 digits, not {digits}")
            }
            OtpUriError::DuplicateParameter(parameter) => {
                write!(f, "the uri repeats the {parameter} parameter")
            }
            OtpUriError::IssuerMismatch { label, parameter } => write!(
                f,
                "issuer label {label:?} does not match issuer parameter {parameter:?}"
            ),
            OtpUriError::Configuration(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OtpUriError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OtpUriError::Secret(error) => Some(error),
            OtpUriError::Configuration(error) => Some(error),
            _ => None,
        }
    }
}

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

    let get = |name| query_value(query, name);

    let secret = base32::decode(get("secret")?.ok_or(OtpUriError::MissingSecret)?)
        .map_err(OtpUriError::Secret)?;

    // The label is `issuer:account` or just `account`. When both the prefix
    // and the `issuer=` parameter are present the format requires agreement.
    // Rejecting disagreement prevents different consumers from presenting a
    // credential under different issuer identities.
    let label = percent_decode(label);
    let (label_issuer, account) = match label.split_once(':') {
        Some((issuer, account)) => (Some(issuer.trim().to_string()), account.trim().to_string()),
        None => (None, label.trim().to_string()),
    };
    if account.is_empty() {
        return Err(OtpUriError::MissingLabel);
    }
    if label_issuer.as_deref() == Some("") {
        return Err(OtpUriError::EmptyIssuer);
    }
    let parameter_issuer = get("issuer")?.map(percent_decode);
    if parameter_issuer.as_deref() == Some("") {
        return Err(OtpUriError::EmptyIssuer);
    }
    if let (Some(label), Some(parameter)) = (&label_issuer, &parameter_issuer)
        && label != parameter
    {
        return Err(OtpUriError::IssuerMismatch {
            label: label.clone(),
            parameter: parameter.clone(),
        });
    }
    let issuer = parameter_issuer.or(label_issuer);

    let algorithm = match get("algorithm")? {
        None => OtpAlgorithm::default(),
        Some(a) => match a.to_ascii_uppercase().as_str() {
            "SHA1" => OtpAlgorithm::Sha1,
            "SHA256" => OtpAlgorithm::Sha256,
            "SHA512" => OtpAlgorithm::Sha512,
            other => return Err(OtpUriError::UnknownAlgorithm(other.to_string())),
        },
    };

    let digits = match get("digits")? {
        None => DEFAULT_DIGITS,
        Some(d) => d.parse().map_err(|_| OtpUriError::InvalidNumber {
            parameter: "digits",
            value: d.to_string(),
        })?,
    };
    if !matches!(digits, 6 | 8) {
        return Err(OtpUriError::UnsupportedDigits(digits));
    }

    let otp = if is_totp {
        let period = match get("period")? {
            None => DEFAULT_PERIOD_SECS,
            Some(p) => p.parse().map_err(|_| OtpUriError::InvalidNumber {
                parameter: "period",
                value: p.to_string(),
            })?,
        };
        Otp::totp(secret)
            .map_err(OtpUriError::Configuration)?
            .with_algorithm(algorithm)
            .with_digits(digits)
            .map_err(OtpUriError::Configuration)?
            .with_period(period)
            .map_err(OtpUriError::Configuration)?
    } else {
        let counter_text = get("counter")?.ok_or(OtpUriError::MissingCounter)?;
        let counter = counter_text
            .parse()
            .map_err(|_| OtpUriError::InvalidNumber {
                parameter: "counter",
                value: counter_text.to_string(),
            })?;
        Otp::hotp(secret, counter)
            .map_err(OtpUriError::Configuration)?
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

fn query_value<'a>(query: &'a str, name: &'static str) -> Result<Option<&'a str>, OtpUriError> {
    let mut found = None;
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key.eq_ignore_ascii_case(name) && found.replace(value).is_some() {
            return Err(OtpUriError::DuplicateParameter(name));
        }
    }
    Ok(found)
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
