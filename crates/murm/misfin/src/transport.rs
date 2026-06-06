/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![allow(unused_imports)]
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, StreamOwned};

use super::*;
use super::helpers::*;

pub(super) fn send_message_with_paths(
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

pub(super) fn load_or_create_identity(
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

pub(super) fn identity_status_with_root(
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

pub(super) fn ensure_identity_with_root(
    spec: &MisfinIdentitySpec,
    identity_root: Option<&Path>,
) -> Result<MisfinIdentityStatus, String> {
    let _ = load_or_create_identity(spec, identity_root)?;
    identity_status_with_root(spec, identity_root)
}

pub(super) fn rotate_identity_with_root(
    spec: &MisfinIdentitySpec,
    identity_root: Option<&Path>,
) -> Result<MisfinIdentityStatus, String> {
    let _ = forget_identity_with_root(spec, identity_root)?;
    ensure_identity_with_root(spec, identity_root)
}

pub(super) fn forget_identity_with_root(
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

pub(super) fn trust_status_with_path(
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

pub(super) fn forget_known_host_with_path(
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

pub(super) fn generate_identity(spec: &MisfinIdentitySpec) -> Result<MisfinClientIdentity, String> {
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

