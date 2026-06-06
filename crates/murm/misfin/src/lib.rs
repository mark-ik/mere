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

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, Error, SignatureScheme, StreamOwned,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};


const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const IO_TIMEOUT: Duration = Duration::from_secs(10);
const MISFIN_DEFAULT_PORT: u16 = 1958;
const MISFIN_MAX_REDIRECTS: usize = 5;
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
pub struct MisfinRequest {
    pub recipient: MisfinAddress,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MisfinResponse {
    pub status: u16,
    pub meta: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MisfinSendOutcome {
    pub final_recipient: MisfinAddress,
    pub status: u16,
    pub meta: String,
    pub recipient_fingerprint: Option<String>,
    pub permanent_redirect: Option<MisfinAddress>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MisfinIdentityStatus {
    pub address: String,
    pub path: Option<PathBuf>,
    pub exists: bool,
    pub blurb: Option<String>,
    pub certificate_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MisfinTrustStatus {
    pub authority: String,
    pub path: Option<PathBuf>,
    pub fingerprint_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct MisfinKnownHostRecord {
    authority: String,
    fingerprint_sha256: String,
}

#[derive(Debug, Clone)]
struct MisfinKnownHostsStore {
    path: Option<PathBuf>,
    records: Arc<RwLock<HashMap<String, MisfinKnownHostRecord>>>,
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

#[derive(Debug)]
struct MisfinTofuVerifier {
    authority: String,
    known_hosts: MisfinKnownHostsStore,
    supported_algs: WebPkiSupportedAlgorithms,
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

impl MisfinRequest {
    pub fn encode(&self) -> Result<String, String> {
        if self.message.contains(['\r', '\n']) {
            return Err(
                "Misfin request messages must fit on a single wire line; multiline gemmail belongs in stored/forwarded message bodies, not the transaction request."
                    .to_string(),
            );
        }

        let request = format!(
            "misfin://{} {}\r\n",
            self.recipient.as_addr_spec(),
            self.message
        );
        if request.len() > 2048 {
            return Err("Misfin request exceeded the 2048-byte wire limit.".to_string());
        }
        Ok(request)
    }
}

impl MisfinKnownHostsStore {
    fn load_default() -> Self {
        let path = misfin_known_hosts_path();
        let records = path
            .as_ref()
            .and_then(|path| load_known_hosts_from_path(path).ok())
            .unwrap_or_default();
        Self {
            path,
            records: Arc::new(RwLock::new(records)),
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    fn new_for_tests(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            records: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn remember_or_verify(
        &self,
        authority: &str,
        certificate: &CertificateDer<'_>,
    ) -> Result<(), String> {
        let fingerprint = sha256_hex(certificate.as_ref());
        let mut records = self
            .records
            .write()
            .expect("misfin known-hosts lock poisoned");

        match records.get(authority) {
            Some(existing) if existing.fingerprint_sha256 == fingerprint => Ok(()),
            Some(existing) => Err(format!(
                "Misfin certificate changed for {authority}. Stored fingerprint {stored}, received {received}.",
                stored = existing.fingerprint_sha256,
                received = fingerprint,
            )),
            None => {
                records.insert(
                    authority.to_string(),
                    MisfinKnownHostRecord {
                        authority: authority.to_string(),
                        fingerprint_sha256: fingerprint,
                    },
                );
                drop(records);
                self.persist();
                Ok(())
            }
        }
    }

    fn persist(&self) {
        let Some(path) = self.path.as_ref() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut records = self
            .records
            .read()
            .expect("misfin known-hosts lock poisoned")
            .values()
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| left.authority.cmp(&right.authority));
        let Ok(content) = serde_json::to_string_pretty(&records) else {
            return;
        };
        let _ = fs::write(path, content);
    }
}

impl MisfinTofuVerifier {
    fn new(authority: String, known_hosts: MisfinKnownHostsStore) -> Self {
        let supported_algs =
            rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms;
        Self {
            authority,
            known_hosts,
            supported_algs,
        }
    }
}

impl ServerCertVerifier for MisfinTofuVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        self.known_hosts
            .remember_or_verify(&self.authority, end_entity)
            .map_err(Error::General)?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(message, cert, dss, &self.supported_algs)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(message, cert, dss, &self.supported_algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.supported_algs.supported_schemes()
    }
}

pub fn send_message(
    url: &url::Url,
    sender: &MisfinIdentitySpec,
    message: &str,
) -> Result<MisfinSendOutcome, String> {
    let known_hosts = MisfinKnownHostsStore::load_default();
    let identity_root = misfin_identity_root();
    send_message_with_paths(
        url,
        sender,
        message,
        &known_hosts,
        identity_root.as_deref(),
        0,
    )
}

#[cfg(any(test, feature = "test-support"))]
pub fn send_message_for_tests(
    url: &url::Url,
    sender: &MisfinIdentitySpec,
    message: &str,
    known_hosts_path: &Path,
    identity_root: &Path,
) -> Result<MisfinSendOutcome, String> {
    let known_hosts = MisfinKnownHostsStore::new_for_tests(known_hosts_path.to_path_buf());
    send_message_with_paths(url, sender, message, &known_hosts, Some(identity_root), 0)
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

pub fn trust_status(url: &url::Url) -> Result<MisfinTrustStatus, String> {
    trust_status_with_path(url, misfin_known_hosts_path().as_deref())
}

pub fn forget_known_host(url: &url::Url) -> Result<bool, String> {
    forget_known_host_with_path(url, misfin_known_hosts_path().as_deref())
}

pub fn url_string_for_address(address: &MisfinAddress, explicit_port: Option<u16>) -> String {
    if let Some(port) = explicit_port {
        format!("misfin://{}@{}:{port}", address.mailbox, address.host)
    } else {
        format!("misfin://{}@{}", address.mailbox, address.host)
    }
}

pub fn parse_misfin_response(line: &str) -> Result<MisfinResponse, String> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    if trimmed.len() < 2 {
        return Err(
            "Misfin response was shorter than the required two-digit status code.".to_string(),
        );
    }
    let status = trimmed[..2]
        .parse::<u16>()
        .map_err(|error| format!("Invalid Misfin status code '{}': {error}", &trimmed[..2]))?;
    let meta = trimmed[2..].trim_start().to_string();
    Ok(MisfinResponse { status, meta })
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

mod transport;
mod helpers;
#[cfg(test)]
mod tests;

use helpers::{sha256_hex, load_known_hosts_from_path, misfin_identity_root,
    misfin_known_hosts_path, parse_sender_line, parse_recipients_line,
    parse_timestamp_line, split_once_whitespace};
use transport::{send_message_with_paths, identity_status_with_root, ensure_identity_with_root,
    rotate_identity_with_root, forget_identity_with_root, trust_status_with_path,
    forget_known_host_with_path};
