/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use super::*;
use super::transport::{send_message_with_paths, ensure_identity_with_root,
    forget_known_host_with_path, trust_status_with_path};
use super::helpers::persist_known_hosts_to_path;

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
