/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Misfin client for Mere.
//!
//! [Misfin](https://misfin.org) is a gemini-style mail protocol: a message
//! is delivered over TLS to a recipient mailbox addressed as
//! `misfin://user@host`, authenticated by self-signed client certificates
//! (the sender's identity *is* its certificate fingerprint). This crate
//! owns the client side — certificate generation and on-disk storage, the
//! TLS handshake with trust-on-first-use peer verification, and message
//! send/receive — with no dependency on the rest of the comms stack.

use std::fs;
use std::path::PathBuf;

use rustls::pki_types::CertificateDer;
use serde::{Deserialize, Serialize};

const MISFIN_USER_ID_OID: [u64; 7] = [0, 9, 2342, 19200300, 100, 1, 1];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MisfinAddress {
    pub mailbox: String,
    pub host: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MisfinSender {
    pub address: MisfinAddress,
    pub blurb: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MisfinIdentitySpec {
    pub address: MisfinAddress,
    pub blurb: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MisfinGemmail {
    pub sender: Option<MisfinSender>,
    pub recipients: Vec<MisfinAddress>,
    pub timestamp: Option<String>,
    pub subject: Option<String>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MisfinIdentityStatus {
    pub address: String,
    pub path: Option<PathBuf>,
    pub exists: bool,
    pub blurb: Option<String>,
    pub certificate_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PersistedMisfinIdentity {
    address: String,
    blurb: Option<String>,
    certificate_der_hex: String,
    private_key_der_hex: String,
}

#[derive(Debug, Clone)]
struct MisfinClientIdentity {
    certificate_chain: Vec<CertificateDer<'static>>,
    private_key_der: Vec<u8>,
}

impl MisfinAddress {
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        let (mailbox, host) = trimmed
            .split_once('@')
            .ok_or_else(|| format!("Invalid Misfin address '{trimmed}'."))?;
        if mailbox.is_empty() || host.is_empty() {
            return Err(format!("Invalid Misfin address '{trimmed}'."));
        }
        Ok(Self {
            mailbox: mailbox.to_string(),
            host: host.to_ascii_lowercase(),
        })
    }

    pub fn from_url(url: &url::Url) -> Result<Self, String> {
        let mailbox = url.username().trim();
        if mailbox.is_empty() {
            return Err(
                "Misfin URL is missing the recipient mailbox in the username position.".to_string(),
            );
        }
        let host = url
            .host_str()
            .ok_or_else(|| "Misfin URL is missing a host.".to_string())?;
        Self::parse(&format!("{mailbox}@{host}"))
    }

    pub fn as_addr_spec(&self) -> String {
        format!("{}@{}", self.mailbox, self.host)
    }
}

pub fn identity_status(spec: &MisfinIdentitySpec) -> Result<MisfinIdentityStatus, String> {
    identity_status_with_root(spec, misfin_identity_root().as_deref())
}

pub fn ensure_identity(spec: &MisfinIdentitySpec) -> Result<MisfinIdentityStatus, String> {
    ensure_identity_with_root(spec, misfin_identity_root().as_deref())
}

pub fn rotate_identity(spec: &MisfinIdentitySpec) -> Result<MisfinIdentityStatus, String> {
    rotate_identity_with_root(spec, misfin_identity_root().as_deref())
}

pub fn forget_identity(spec: &MisfinIdentitySpec) -> Result<bool, String> {
    forget_identity_with_root(spec, misfin_identity_root().as_deref())
}

pub fn url_string_for_address(address: &MisfinAddress, explicit_port: Option<u16>) -> String {
    if let Some(port) = explicit_port {
        format!("misfin://{}@{}:{port}", address.mailbox, address.host)
    } else {
        format!("misfin://{}@{}", address.mailbox, address.host)
    }
}

pub fn parse_gemmail(text: &str) -> MisfinGemmail {
    let mut sender = None;
    let mut recipients = None;
    let mut timestamp = None;
    let mut body_lines = Vec::new();

    for line in text.lines() {
        let line = line.trim_end_matches('\r');

        if sender.is_none() {
            if let Some(parsed_sender) = parse_sender_line(line) {
                sender = Some(parsed_sender);
                continue;
            }
        }
        if recipients.is_none() {
            if let Some(parsed_recipients) = parse_recipients_line(line) {
                recipients = Some(parsed_recipients);
                continue;
            }
        }
        if timestamp.is_none() {
            if let Some(parsed_timestamp) = parse_timestamp_line(line) {
                timestamp = Some(parsed_timestamp);
                continue;
            }
        }

        body_lines.push(line.to_string());
    }

    let subject = body_lines.iter().find_map(|line| {
        line.strip_prefix("### ")
            .or_else(|| line.strip_prefix("## "))
            .or_else(|| line.strip_prefix("# "))
            .map(|heading| heading.trim().to_string())
    });

    MisfinGemmail {
        sender,
        recipients: recipients.unwrap_or_default(),
        timestamp,
        subject,
        body: body_lines.join("\n"),
    }
}

mod helpers;
#[cfg(feature = "server")]
mod mailbox;
#[cfg(feature = "server")]
mod server;
#[cfg(test)]
mod tests;
mod transport;

use helpers::{
    misfin_identity_root, parse_recipients_line, parse_sender_line, parse_timestamp_line,
};
use transport::{
    ensure_identity_with_root, forget_identity_with_root, identity_status_with_root,
    rotate_identity_with_root,
};

#[cfg(feature = "server")]
pub use mailbox::{MailboxStore, ReceivedMessage, SenderSeen};
#[cfg(feature = "server")]
pub use server::{
    BoundMisfinServer, MISFIN_PORT, MisfinResponse, MisfinServer, MisfinServerConfig, ServedMailbox,
};

/// An error from the misfin receive server (the `server` feature): a mailbox
/// store failure, a TLS / server-config failure, or a socket failure.
#[cfg(feature = "server")]
#[derive(Debug)]
pub enum MisfinServerError {
    /// A mailbox-store failure (redb or message (de)serialization).
    Storage(String),
    /// A TLS or server-configuration failure (bad cert/key, provider).
    Config(String),
    /// A socket / IO failure (bind, accept).
    Io(String),
}

#[cfg(feature = "server")]
impl std::fmt::Display for MisfinServerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(message) => write!(formatter, "misfin mailbox storage error: {message}"),
            Self::Config(message) => write!(formatter, "misfin server config error: {message}"),
            Self::Io(message) => write!(formatter, "misfin server IO error: {message}"),
        }
    }
}

#[cfg(feature = "server")]
impl std::error::Error for MisfinServerError {}

/// DER material for a Misfin client identity: the certificate (leaf, DER) and the
/// private key (PKCS#8 DER). This is the shape a client-cert TLS sender consumes:
/// e.g. for `errand::misfin_send`, wrap `certificate_der` in a `CertificateDer`
/// and `private_key_pkcs8_der` in `PrivateKeyDer::Pkcs8`.
pub struct MisfinIdentityMaterial {
    /// The leaf client certificate, DER-encoded.
    pub certificate_der: Vec<u8>,
    /// The private key, PKCS#8 DER-encoded.
    pub private_key_pkcs8_der: Vec<u8>,
}

/// The SHA-256 hex fingerprint of a certificate's DER bytes — a misfin identity
/// (the value the server returns as the status-20 META, and a `ServedMailbox`
/// fingerprint).
pub fn certificate_fingerprint(certificate_der: &[u8]) -> String {
    helpers::sha256_hex(certificate_der)
}

/// Deterministically derive a Misfin client identity from a 32-byte **Ed25519**
/// seed.
///
/// The reproducible counterpart of [`ensure_identity`]: rather than generate a
/// random ECDSA certificate and persist it, this derives the entire identity from
/// `seed` + `spec`. Ed25519 signs deterministically, and the cert uses a fixed
/// serial + validity, so the same seed + address always reproduce the same
/// certificate and SHA-256 fingerprint. The seed is the caller's — typically an
/// identity vault's `derive_keypair(identity_salt(&address))?.to_seed()` — so the
/// Misfin identity is reproducible from the vault, with no certificate to back up.
///
/// The Misfin spec mandates no key algorithm (any valid self-signed x509), and
/// real servers accept Ed25519 client certs, so this stays interoperable.
pub fn deterministic_identity(
    seed: &[u8; 32],
    spec: &MisfinIdentitySpec,
) -> Result<MisfinIdentityMaterial, String> {
    let identity = transport::deterministic_identity(seed, spec)?;
    let certificate_der = identity
        .certificate_chain
        .first()
        .map(|cert| cert.as_ref().to_vec())
        .ok_or_else(|| "Misfin identity produced no certificate".to_string())?;
    Ok(MisfinIdentityMaterial {
        certificate_der,
        private_key_pkcs8_der: identity.private_key_der,
    })
}

/// The recommended identity-vault derivation salt for a Misfin `address`:
/// domain-separated and **per-address**, so `derive_keypair(identity_salt(addr))`
/// yields a stable key for that address and **distinct (unlinkable)** keys across
/// addresses. Feed the result to `derive_keypair`, then `to_seed()` into
/// [`deterministic_identity`].
pub fn identity_salt(address: &MisfinAddress) -> Vec<u8> {
    [
        b"mere/misfin/identity/v1/".as_slice(),
        address.as_addr_spec().as_bytes(),
    ]
    .concat()
}

#[cfg(test)]
mod deterministic_identity_tests {
    use super::*;

    fn spec(addr: &str) -> MisfinIdentitySpec {
        MisfinIdentitySpec {
            address: MisfinAddress::parse(addr).unwrap(),
            blurb: Some("Test".to_string()),
        }
    }

    #[test]
    fn same_seed_reproduces_a_byte_identical_identity() {
        let seed = [7u8; 32];
        let a = deterministic_identity(&seed, &spec("alice@example.test")).unwrap();
        let b = deterministic_identity(&seed, &spec("alice@example.test")).unwrap();
        // Ed25519 deterministic signatures + fixed serial/validity => a byte-stable
        // cert, hence a stable SHA-256 fingerprint (the Misfin identity).
        assert_eq!(a.certificate_der, b.certificate_der, "cert is reproducible");
        assert_eq!(a.private_key_pkcs8_der, b.private_key_pkcs8_der);
        assert!(!a.certificate_der.is_empty());
    }

    #[test]
    fn a_different_seed_yields_a_different_identity() {
        let s = spec("alice@example.test");
        let a = deterministic_identity(&[7u8; 32], &s).unwrap();
        let c = deterministic_identity(&[8u8; 32], &s).unwrap();
        assert_ne!(a.certificate_der, c.certificate_der);
    }

    #[test]
    fn the_identity_salt_is_per_address() {
        let alice = MisfinAddress::parse("alice@example.test").unwrap();
        let bob = MisfinAddress::parse("bob@example.test").unwrap();
        assert_ne!(
            identity_salt(&alice),
            identity_salt(&bob),
            "addresses derive distinct keys"
        );
        let alice_again = MisfinAddress::parse("alice@example.test").unwrap();
        assert_eq!(
            identity_salt(&alice),
            identity_salt(&alice_again),
            "stable per address"
        );
    }
}
