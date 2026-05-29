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

fn send_message_with_paths(
    url: &url::Url,
    sender: &MisfinIdentitySpec,
    message: &str,
    known_hosts: &MisfinKnownHostsStore,
    identity_root: Option<&Path>,
    redirect_depth: usize,
) -> Result<MisfinSendOutcome, String> {
    if redirect_depth >= MISFIN_MAX_REDIRECTS {
        return Err("Misfin redirect limit exceeded.".to_string());
    }

    let recipient = MisfinAddress::from_url(url)?;
    let port = url.port().unwrap_or(MISFIN_DEFAULT_PORT);
    let authority = format!("{}:{port}", recipient.host);
    let request = MisfinRequest {
        recipient: recipient.clone(),
        message: message.to_string(),
    }
    .encode()?;
    let identity = load_or_create_identity(sender, identity_root)?;

    let stream = connect(&recipient.host, port)?;
    let verifier = Arc::new(MisfinTofuVerifier::new(authority, known_hosts.clone()));
    let client_config =
        ClientConfig::builder_with_provider(rustls::crypto::aws_lc_rs::default_provider().into())
            .with_protocol_versions(rustls::DEFAULT_VERSIONS)
            .expect("rustls default protocol versions should be valid for Misfin client")
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_client_auth_cert(
                identity.certificate_chain.clone(),
                PrivateKeyDer::try_from(identity.private_key_der.clone())
                    .map_err(|error| format!("Misfin private key decode failed: {error}"))?,
            )
            .map_err(|error| format!("Misfin client certificate setup failed: {error}"))?;
    let server_name = server_name_for_host(&recipient.host)?;
    let connection = ClientConnection::new(Arc::new(client_config), server_name)
        .map_err(|error| format!("Misfin TLS client setup failed: {error}"))?;
    let mut tls_stream = StreamOwned::new(connection, stream);

    tls_stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("Misfin request write failed: {error}"))?;
    tls_stream
        .flush()
        .map_err(|error| format!("Misfin request flush failed: {error}"))?;

    if tls_stream.conn.peer_certificates().is_none() {
        return Err("Misfin TLS handshake completed without a peer certificate.".to_string());
    }

    let mut reader = BufReader::new(tls_stream);
    let response = read_misfin_response(&mut reader)?;

    if matches!(response.status, 30 | 31) {
        let redirected_address = MisfinAddress::parse(&response.meta)?;
        let redirected_url = redirected_url(url, &redirected_address)?;
        let mut outcome = send_message_with_paths(
            &redirected_url,
            sender,
            message,
            known_hosts,
            identity_root,
            redirect_depth + 1,
        )?;
        if response.status == 31 {
            outcome.permanent_redirect = Some(redirected_address);
        }
        return Ok(outcome);
    }

    Ok(MisfinSendOutcome {
        final_recipient: recipient,
        status: response.status,
        recipient_fingerprint: (response.status == 20)
            .then(|| normalize_fingerprint(&response.meta)),
        meta: response.meta,
        permanent_redirect: None,
    })
}

fn load_or_create_identity(
    spec: &MisfinIdentitySpec,
    identity_root: Option<&Path>,
) -> Result<MisfinClientIdentity, String> {
    let Some(identity_root) = identity_root else {
        return generate_identity(spec);
    };

    fs::create_dir_all(identity_root)
        .map_err(|error| format!("Failed to create Misfin identity directory: {error}"))?;
    let path = identity_root.join(format!(
        "{}.json",
        sanitize_filename(&spec.address.as_addr_spec())
    ));

    if path.exists() {
        let content = fs::read_to_string(&path).map_err(|error| {
            format!(
                "Failed to read Misfin identity '{}': {error}",
                path.display()
            )
        })?;
        let persisted: PersistedMisfinIdentity =
            serde_json::from_str(&content).map_err(|error| {
                format!(
                    "Failed to parse Misfin identity '{}': {error}",
                    path.display()
                )
            })?;
        return Ok(MisfinClientIdentity {
            certificate_chain: vec![CertificateDer::from(decode_hex(
                &persisted.certificate_der_hex,
            )?)],
            private_key_der: decode_hex(&persisted.private_key_der_hex)?,
        });
    }

    let identity = generate_identity(spec)?;
    let persisted = PersistedMisfinIdentity {
        address: spec.address.as_addr_spec(),
        blurb: spec.blurb.clone(),
        certificate_der_hex: encode_hex(identity.certificate_chain[0].as_ref()),
        private_key_der_hex: encode_hex(&identity.private_key_der),
    };
    let content = serde_json::to_string_pretty(&persisted).map_err(|error| {
        format!(
            "Failed to serialize Misfin identity '{}': {error}",
            path.display()
        )
    })?;
    fs::write(&path, content).map_err(|error| {
        format!(
            "Failed to persist Misfin identity '{}': {error}",
            path.display()
        )
    })?;
    Ok(identity)
}

fn identity_status_with_root(
    spec: &MisfinIdentitySpec,
    identity_root: Option<&Path>,
) -> Result<MisfinIdentityStatus, String> {
    let path = identity_root.map(|root| identity_path_for_spec(spec, root));
    let Some(path) = path else {
        return Ok(MisfinIdentityStatus {
            address: spec.address.as_addr_spec(),
            path: None,
            exists: false,
            blurb: spec.blurb.clone(),
            certificate_fingerprint: None,
        });
    };

    if !path.exists() {
        return Ok(MisfinIdentityStatus {
            address: spec.address.as_addr_spec(),
            path: Some(path),
            exists: false,
            blurb: spec.blurb.clone(),
            certificate_fingerprint: None,
        });
    }

    let content = fs::read_to_string(&path).map_err(|error| {
        format!(
            "Failed to read Misfin identity '{}': {error}",
            path.display()
        )
    })?;
    let persisted: PersistedMisfinIdentity = serde_json::from_str(&content).map_err(|error| {
        format!(
            "Failed to parse Misfin identity '{}': {error}",
            path.display()
        )
    })?;
    let certificate_der = decode_hex(&persisted.certificate_der_hex)?;

    Ok(MisfinIdentityStatus {
        address: persisted.address,
        path: Some(path),
        exists: true,
        blurb: persisted.blurb,
        certificate_fingerprint: Some(sha256_hex(&certificate_der)),
    })
}

fn ensure_identity_with_root(
    spec: &MisfinIdentitySpec,
    identity_root: Option<&Path>,
) -> Result<MisfinIdentityStatus, String> {
    let _ = load_or_create_identity(spec, identity_root)?;
    identity_status_with_root(spec, identity_root)
}

fn rotate_identity_with_root(
    spec: &MisfinIdentitySpec,
    identity_root: Option<&Path>,
) -> Result<MisfinIdentityStatus, String> {
    let _ = forget_identity_with_root(spec, identity_root)?;
    ensure_identity_with_root(spec, identity_root)
}

fn forget_identity_with_root(
    spec: &MisfinIdentitySpec,
    identity_root: Option<&Path>,
) -> Result<bool, String> {
    let Some(identity_root) = identity_root else {
        return Ok(false);
    };
    let path = identity_path_for_spec(spec, identity_root);
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_file(&path).map_err(|error| {
        format!(
            "Failed to remove Misfin identity '{}': {error}",
            path.display()
        )
    })?;
    Ok(true)
}

fn trust_status_with_path(
    url: &url::Url,
    known_hosts_path: Option<&Path>,
) -> Result<MisfinTrustStatus, String> {
    let authority = authority_for_url(url)?;
    let path = known_hosts_path.map(Path::to_path_buf);
    let fingerprint_sha256 = if let Some(path) = known_hosts_path {
        load_known_hosts_from_path(path)
            .map_err(|error| {
                format!(
                    "Failed to read Misfin known hosts '{}': {error}",
                    path.display()
                )
            })?
            .get(&authority)
            .map(|record| record.fingerprint_sha256.clone())
    } else {
        None
    };

    Ok(MisfinTrustStatus {
        authority,
        path,
        fingerprint_sha256,
    })
}

fn forget_known_host_with_path(
    url: &url::Url,
    known_hosts_path: Option<&Path>,
) -> Result<bool, String> {
    let Some(path) = known_hosts_path else {
        return Ok(false);
    };
    let authority = authority_for_url(url)?;
    let mut records = load_known_hosts_from_path(path).map_err(|error| {
        format!(
            "Failed to read Misfin known hosts '{}': {error}",
            path.display()
        )
    })?;
    let removed = records.remove(&authority).is_some();
    if removed {
        persist_known_hosts_to_path(path, records.values().cloned().collect())?;
    }
    Ok(removed)
}

fn generate_identity(spec: &MisfinIdentitySpec) -> Result<MisfinClientIdentity, String> {
    let key_pair =
        KeyPair::generate().map_err(|error| format!("Misfin key generation failed: {error}"))?;
    let mut params = CertificateParams::new(vec![spec.address.host.clone()])
        .map_err(|error| format!("Misfin certificate params failed: {error}"))?;
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(
        DnType::CustomDnType(MISFIN_USER_ID_OID.to_vec()),
        spec.address.mailbox.clone(),
    );
    distinguished_name.push(
        DnType::CommonName,
        spec.blurb
            .clone()
            .unwrap_or_else(|| spec.address.as_addr_spec()),
    );
    params.distinguished_name = distinguished_name;
    params.not_before = rcgen::date_time_ymd(2024, 1, 1);
    params.not_after = rcgen::date_time_ymd(2099, 12, 31);

    let cert = params
        .self_signed(&key_pair)
        .map_err(|error| format!("Misfin identity certificate generation failed: {error}"))?;

    Ok(MisfinClientIdentity {
        certificate_chain: vec![CertificateDer::from(cert.der().to_vec())],
        private_key_der: key_pair.serialize_der(),
    })
}

fn connect(host: &str, port: u16) -> Result<TcpStream, String> {
    let mut last_error = None;

    for address in resolve_socket_addrs(host, port)? {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(stream) => {
                stream
                    .set_read_timeout(Some(IO_TIMEOUT))
                    .map_err(|error| format!("Failed to configure Misfin read timeout: {error}"))?;
                stream
                    .set_write_timeout(Some(IO_TIMEOUT))
                    .map_err(|error| {
                        format!("Failed to configure Misfin write timeout: {error}")
                    })?;
                return Ok(stream);
            }
            Err(error) => last_error = Some(error),
        }
    }

    let error = last_error
        .map(|error| error.to_string())
        .unwrap_or_else(|| "no socket addresses resolved".to_string());
    Err(format!("Connection to {host}:{port} failed: {error}"))
}

fn resolve_socket_addrs(host: &str, port: u16) -> Result<Vec<std::net::SocketAddr>, String> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|error| format!("Failed to resolve {host}:{port}: {error}"))?
        .collect::<Vec<_>>();

    if addresses.is_empty() {
        return Err(format!("No socket address resolved for {host}:{port}."));
    }

    Ok(addresses)
}

fn server_name_for_host(host: &str) -> Result<ServerName<'static>, String> {
    if let Ok(address) = host.parse::<IpAddr>() {
        return Ok(ServerName::IpAddress(address.into()));
    }

    ServerName::try_from(host.to_string())
        .map_err(|error| format!("Invalid Misfin host '{host}': {error}"))
}

fn read_misfin_response<R: std::io::Read>(
    reader: &mut BufReader<R>,
) -> Result<MisfinResponse, String> {
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof && !line.is_empty() => {}
        Err(error) => return Err(format!("Misfin response read failed: {error}")),
    }

    if line.is_empty() {
        return Err("Misfin response was empty.".to_string());
    }

    parse_misfin_response(&line)
}

fn redirected_url(current_url: &url::Url, address: &MisfinAddress) -> Result<url::Url, String> {
    let mut redirected =
        url::Url::parse(&url_string_for_address(address, None)).map_err(|error| {
            format!(
                "Invalid redirected Misfin address '{}': {error}",
                address.as_addr_spec()
            )
        })?;
    if let Some(port) = current_url.port() {
        redirected
            .set_port(Some(port))
            .map_err(|_| format!("Failed to preserve explicit Misfin port {port} on redirect."))?;
    }
    Ok(redirected)
}

fn authority_for_url(url: &url::Url) -> Result<String, String> {
    let recipient = MisfinAddress::from_url(url)?;
    Ok(format!(
        "{}:{}",
        recipient.host,
        url.port().unwrap_or(MISFIN_DEFAULT_PORT)
    ))
}

fn parse_sender_line(line: &str) -> Option<MisfinSender> {
    let remainder = line.strip_prefix('<')?.trim();
    if remainder.is_empty() {
        return None;
    }
    let (address, blurb) = split_once_whitespace(remainder);
    let address = MisfinAddress::parse(address).ok()?;
    Some(MisfinSender {
        address,
        blurb: blurb.map(|value| value.to_string()),
    })
}

fn parse_recipients_line(line: &str) -> Option<Vec<MisfinAddress>> {
    let remainder = line.strip_prefix(':')?.trim();
    let mut recipients = Vec::new();
    for part in remainder.split_whitespace() {
        recipients.push(MisfinAddress::parse(part).ok()?);
    }
    Some(recipients)
}

fn parse_timestamp_line(line: &str) -> Option<String> {
    let remainder = line.strip_prefix('@')?.trim();
    if remainder.is_empty() {
        None
    } else {
        Some(remainder.to_string())
    }
}

fn split_once_whitespace(input: &str) -> (&str, Option<&str>) {
    let Some(index) = input.find(char::is_whitespace) else {
        return (input, None);
    };
    let head = &input[..index];
    let tail = input[index..].trim();
    if tail.is_empty() {
        (head, None)
    } else {
        (head, Some(tail))
    }
}

fn identity_path_for_spec(spec: &MisfinIdentitySpec, identity_root: &Path) -> PathBuf {
    identity_root.join(format!(
        "{}.json",
        sanitize_filename(&spec.address.as_addr_spec())
    ))
}

#[cfg(not(any(test, feature = "test-support")))]
fn misfin_known_hosts_path() -> Option<PathBuf> {
    let mut path = dirs::config_dir()?;
    path.push("graphshell");
    path.push("misfin_known_hosts.json");
    Some(path)
}

#[cfg(any(test, feature = "test-support"))]
fn misfin_known_hosts_path() -> Option<PathBuf> {
    None
}

#[cfg(not(any(test, feature = "test-support")))]
fn misfin_identity_root() -> Option<PathBuf> {
    let mut path = dirs::config_dir()?;
    path.push("graphshell");
    path.push("misfin_identities");
    Some(path)
}

#[cfg(any(test, feature = "test-support"))]
fn misfin_identity_root() -> Option<PathBuf> {
    None
}

fn load_known_hosts_from_path(
    path: &Path,
) -> Result<HashMap<String, MisfinKnownHostRecord>, std::io::Error> {
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let content = fs::read_to_string(path)?;
    match serde_json::from_str::<Vec<MisfinKnownHostRecord>>(&content) {
        Ok(records) => Ok(records
            .into_iter()
            .map(|record| (record.authority.clone(), record))
            .collect()),
        Err(error) => {
            log::warn!("misfin known-hosts load failed: {error}; resetting known-hosts store");
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, "[]")?;
            Ok(HashMap::new())
        }
    }
}

fn persist_known_hosts_to_path(
    path: &Path,
    mut records: Vec<MisfinKnownHostRecord>,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create Misfin known-hosts parent '{}': {error}",
                parent.display()
            )
        })?;
    }
    records.sort_by(|left, right| left.authority.cmp(&right.authority));
    let content = serde_json::to_string_pretty(&records).map_err(|error| {
        format!(
            "Failed to serialize Misfin known hosts '{}': {error}",
            path.display()
        )
    })?;
    fs::write(path, content).map_err(|error| {
        format!(
            "Failed to persist Misfin known hosts '{}': {error}",
            path.display()
        )
    })
}

fn normalize_fingerprint(input: &str) -> String {
    input
        .chars()
        .filter(|character| character.is_ascii_hexdigit())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn sanitize_filename(input: &str) -> String {
    input
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '@') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    encode_hex(&digest)
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(nibble_to_hex(byte >> 4));
        output.push(nibble_to_hex(byte & 0x0f));
    }
    output
}

fn decode_hex(input: &str) -> Result<Vec<u8>, String> {
    if input.len() % 2 != 0 {
        return Err("Hex payload length must be even.".to_string());
    }

    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len() / 2);
    let mut index = 0;
    while index < bytes.len() {
        let high = from_hex_digit(bytes[index])?;
        let low = from_hex_digit(bytes[index + 1])?;
        output.push((high << 4) | low);
        index += 2;
    }
    Ok(output)
}

fn nibble_to_hex(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => unreachable!("nibble values must be in 0..=15"),
    }
}

fn from_hex_digit(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("Invalid hex digit '{}'.", byte as char)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::{ServerConfig, ServerConnection};
    use tempfile::TempDir;

    #[test]
    fn misfin_request_encodes_single_line_message() {
        let request = MisfinRequest {
            recipient: MisfinAddress::parse("queen@hive.com").expect("address should parse"),
            message: "Where's the flowers at".to_string(),
        };

        assert_eq!(
            request.encode().expect("request should encode"),
            "misfin://queen@hive.com Where's the flowers at\r\n"
        );
    }

    #[test]
    fn misfin_request_rejects_multiline_wire_message() {
        let request = MisfinRequest {
            recipient: MisfinAddress::parse("queen@hive.com").expect("address should parse"),
            message: "# Subject\nBody".to_string(),
        };

        assert!(request.encode().is_err());
    }

    #[test]
    fn misfin_response_parses_status_and_meta() {
        let response = parse_misfin_response("20 abcd1234\r\n").expect("response should parse");

        assert_eq!(response.status, 20);
        assert_eq!(response.meta, "abcd1234");
    }

    #[test]
    fn gemmail_extracts_metadata_and_subject() {
        let gemmail = parse_gemmail(
            "< friend@example.com Friendly Person\n: one@example.com two@example.com\n@ 2023-05-09T19:39:15Z\n# A note on flowers\n\nThe green ones bite.\n",
        );

        assert_eq!(
            gemmail
                .sender
                .as_ref()
                .map(|sender| sender.address.as_addr_spec()),
            Some("friend@example.com".to_string())
        );
        assert_eq!(gemmail.recipients.len(), 2);
        assert_eq!(gemmail.timestamp.as_deref(), Some("2023-05-09T19:39:15Z"));
        assert_eq!(gemmail.subject.as_deref(), Some("A note on flowers"));
        assert_eq!(gemmail.body, "# A note on flowers\n\nThe green ones bite.");
    }

    #[test]
    fn misfin_send_message_writes_request_and_reads_success() {
        let tempdir = TempDir::new().expect("temp dir should be created");
        let known_hosts =
            MisfinKnownHostsStore::new_for_tests(tempdir.path().join("misfin_known_hosts.json"));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let port = listener.local_addr().expect("address").port();

        let server = std::thread::spawn(move || {
            let config = build_test_tls_config("localhost");
            let (stream, _) = listener.accept().expect("accept");
            let mut tls = StreamOwned::new(
                ServerConnection::new(Arc::new(config)).expect("server connection"),
                stream,
            );
            let mut reader = BufReader::new(tls);
            let mut request = String::new();
            std::io::BufRead::read_line(&mut reader, &mut request).expect("request line");
            assert_eq!(request, "misfin://queen@localhost Hello bees\r\n");

            tls = reader.into_inner();
            tls.write_all(b"20 abcdef1234\r\n").expect("response");
            tls.flush().expect("flush");
        });

        let url =
            url::Url::parse(&format!("misfin://queen@localhost:{port}")).expect("url should parse");
        let sender = MisfinIdentitySpec {
            address: MisfinAddress::parse("worker@hive.local").expect("sender should parse"),
            blurb: Some("Worker Bee".to_string()),
        };
        let outcome = send_message_with_paths(
            &url,
            &sender,
            "Hello bees",
            &known_hosts,
            Some(tempdir.path()),
            0,
        )
        .expect("Misfin send should succeed");

        assert_eq!(outcome.final_recipient.as_addr_spec(), "queen@localhost");
        assert_eq!(outcome.status, 20);
        assert_eq!(outcome.recipient_fingerprint.as_deref(), Some("abcdef1234"));
        assert!(tempdir.path().join("worker@hive.local.json").exists());
        server.join().expect("server joins cleanly");
    }

    #[test]
    fn misfin_send_message_follows_redirects_on_explicit_port() {
        let tempdir = TempDir::new().expect("temp dir should be created");
        let known_hosts =
            MisfinKnownHostsStore::new_for_tests(tempdir.path().join("misfin_known_hosts.json"));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let port = listener.local_addr().expect("address").port();

        let server = std::thread::spawn(move || {
            let config = build_test_tls_config("localhost");

            let (first_stream, _) = listener.accept().expect("first accept");
            let mut first_tls = StreamOwned::new(
                ServerConnection::new(Arc::new(config.clone())).expect("first connection"),
                first_stream,
            );
            let mut first_reader = BufReader::new(first_tls);
            let mut first_request = String::new();
            std::io::BufRead::read_line(&mut first_reader, &mut first_request)
                .expect("first request line");
            assert_eq!(first_request, "misfin://queen@localhost Hello bees\r\n");
            first_tls = first_reader.into_inner();
            first_tls
                .write_all(b"31 queen2@localhost\r\n")
                .expect("redirect response");
            first_tls.flush().expect("flush");

            let (second_stream, _) = listener.accept().expect("second accept");
            let mut second_tls = StreamOwned::new(
                ServerConnection::new(Arc::new(config)).expect("second connection"),
                second_stream,
            );
            let mut second_reader = BufReader::new(second_tls);
            let mut second_request = String::new();
            std::io::BufRead::read_line(&mut second_reader, &mut second_request)
                .expect("second request line");
            assert_eq!(second_request, "misfin://queen2@localhost Hello bees\r\n");
            second_tls = second_reader.into_inner();
            second_tls
                .write_all(b"20 fedcba\r\n")
                .expect("success response");
            second_tls.flush().expect("flush");
        });

        let url =
            url::Url::parse(&format!("misfin://queen@localhost:{port}")).expect("url should parse");
        let sender = MisfinIdentitySpec {
            address: MisfinAddress::parse("worker@hive.local").expect("sender should parse"),
            blurb: Some("Worker Bee".to_string()),
        };
        let outcome = send_message_with_paths(
            &url,
            &sender,
            "Hello bees",
            &known_hosts,
            Some(tempdir.path()),
            0,
        )
        .expect("Misfin redirect should succeed");

        assert_eq!(outcome.final_recipient.as_addr_spec(), "queen2@localhost");
        assert_eq!(
            outcome
                .permanent_redirect
                .map(|address| address.as_addr_spec()),
            Some("queen2@localhost".to_string())
        );
        assert_eq!(outcome.recipient_fingerprint.as_deref(), Some("fedcba"));
        server.join().expect("server joins cleanly");
    }

    #[test]
    fn identity_status_reports_persisted_identity() {
        let tempdir = TempDir::new().expect("temp dir should be created");
        let spec = MisfinIdentitySpec {
            address: MisfinAddress::parse("worker@hive.local").expect("sender should parse"),
            blurb: Some("Worker Bee".to_string()),
        };

        let status = ensure_identity_with_root(&spec, Some(tempdir.path()))
            .expect("identity should be created");

        assert!(status.exists);
        assert_eq!(status.address, "worker@hive.local");
        assert!(status.path.expect("identity path should exist").exists());
        assert!(status.certificate_fingerprint.is_some());
    }

    #[test]
    fn forget_known_host_removes_persisted_record() {
        let tempdir = TempDir::new().expect("temp dir should be created");
        let path = tempdir.path().join("misfin_known_hosts.json");
        persist_known_hosts_to_path(
            &path,
            vec![MisfinKnownHostRecord {
                authority: "localhost:1958".to_string(),
                fingerprint_sha256: "abc123".to_string(),
            }],
        )
        .expect("known hosts should persist");

        let url = url::Url::parse("misfin://queen@localhost").expect("url should parse");
        let removed = forget_known_host_with_path(&url, Some(&path))
            .expect("known host removal should succeed");
        let status = trust_status_with_path(&url, Some(&path)).expect("status should load");

        assert!(removed);
        assert!(status.fingerprint_sha256.is_none());
    }

    fn build_test_tls_config(hostname: &str) -> ServerConfig {
        let key_pair = KeyPair::generate().expect("keypair should generate");
        let mut params = CertificateParams::new(vec![hostname.to_string()])
            .expect("certificate params should build");
        params.not_before = rcgen::date_time_ymd(2024, 1, 1);
        params.not_after = rcgen::date_time_ymd(2099, 12, 31);

        let cert = params
            .self_signed(&key_pair)
            .expect("self-signed cert should build");
        let cert_der = CertificateDer::from(cert.der().to_vec());
        let key_der = rustls::pki_types::PrivateKeyDer::try_from(key_pair.serialize_der())
            .expect("key der should convert");

        ServerConfig::builder_with_provider(rustls::crypto::aws_lc_rs::default_provider().into())
            .with_protocol_versions(rustls::DEFAULT_VERSIONS)
            .expect("rustls default protocol versions should be valid for Misfin test server")
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .expect("server config should build")
    }
}
