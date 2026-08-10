// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The typed failures every grant, pairing, and enrollment path returns.

use std::fmt;

/// Wire/validation error for a signed device grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceGrantError {
    Encode,
    Decode,
    DelegatorMismatch,
    InvalidDelegatorPublicKey,
    InvalidSignatureLength,
}

impl fmt::Display for DeviceGrantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode => write!(f, "device grant CBOR encoding failed"),
            Self::Decode => write!(f, "device grant CBOR decoding failed"),
            Self::DelegatorMismatch => write!(f, "delegator keypair does not match payload"),
            Self::InvalidDelegatorPublicKey => {
                write!(f, "device grant carries invalid delegator public key bytes")
            }
            Self::InvalidSignatureLength => {
                write!(f, "device grant signature is not 64 bytes")
            }
        }
    }
}

impl std::error::Error for DeviceGrantError {}

/// Crypto/format error for wrapped private-epoch material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WrappedEpochError {
    UnsupportedWrapFormat(String),
    InvalidWrappedKeyLength,
    Encrypt,
    Decrypt,
}

impl fmt::Display for WrappedEpochError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedWrapFormat(format) => {
                write!(f, "unsupported wrapped private epoch format: {format}")
            }
            Self::InvalidWrappedKeyLength => {
                write!(
                    f,
                    "wrapped private epoch bytes are shorter than an XChaCha20 nonce"
                )
            }
            Self::Encrypt => write!(f, "private epoch wrap encryption failed"),
            Self::Decrypt => write!(f, "private epoch wrap decryption failed"),
        }
    }
}

impl std::error::Error for WrappedEpochError {}

/// Pairing-transcript derivation error for remote-auth grants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairingMaterialError {
    EmptyPairingSecret,
}

impl fmt::Display for PairingMaterialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPairingSecret => write!(f, "pairing secret must not be empty"),
        }
    }
}

impl std::error::Error for PairingMaterialError {}

/// Encode/decode failure for remote-auth pairing tickets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairingTicketError {
    Encode,
    Decode,
}

impl fmt::Display for PairingTicketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode => write!(f, "remote-auth pairing ticket encoding failed"),
            Self::Decode => write!(f, "remote-auth pairing ticket decoding failed"),
        }
    }
}

impl std::error::Error for PairingTicketError {}

/// Human entry failure for a formatted pairing code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairingCodeError {
    InvalidLength,
    InvalidHex,
}

impl fmt::Display for PairingCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength => write!(f, "pairing code does not carry 16 secret bytes"),
            Self::InvalidHex => write!(f, "pairing code contains non-hex digits"),
        }
    }
}

impl std::error::Error for PairingCodeError {}

/// Encode/decode failure for a remote-auth enrollment bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnrollmentBundleError {
    Encode,
    Decode,
}

impl fmt::Display for EnrollmentBundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode => write!(f, "remote-auth enrollment bundle encoding failed"),
            Self::Decode => write!(f, "remote-auth enrollment bundle decoding failed"),
        }
    }
}

impl std::error::Error for EnrollmentBundleError {}
