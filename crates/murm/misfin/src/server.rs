/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The misfin receive server.
//!
//! A [`MisfinServer`] listens on a TCP port (misfin's well-known port is 1958),
//! completes a TLS handshake that **requests but does not require** a client
//! certificate, reads the single-line request, and dispatches it to a
//! [`MailboxStore`](crate::MailboxStore). Per the
//! [spec](https://github.com/JCLemme/misfin/blob/master/specification.gmi) the
//! request is `misfin://<mailbox>@<host> <message>\r\n`, at most 2048 bytes, and
//! the reply is a gemini-shaped `<status> <meta>\r\n` line.
//!
//! Status codes this server returns:
//!
//! - **20** delivered — META is the recipient mailbox's certificate fingerprint
//! - **51** mailbox doesn't exist (host served, mailbox unknown)
//! - **53** domain not serviced (no served mailbox uses that host)
//! - **59** bad request (malformed request line)
//! - **60** certificate required (the client presented none)
//!
//! Senders are tracked by certificate fingerprint (the spec: a sender's identity
//! *is* its fingerprint). Resolving the *claimed* `mailbox@host` from the cert DN
//! — which enables the "you're a liar" fingerprint-changed reply (63) and a
//! human-readable from-line — needs an x509 DN parse and is the documented next
//! step; v1 stores the fingerprint and delivers.
//!
//! The server is host-neutral: it owns no scheduling. `serve` runs until the
//! supplied shutdown future resolves, so a host (or a daemon-side
//! `SessionServiceRunner` worker) just spawns it on its runtime.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::HandshakeSignatureValid;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

use super::helpers::sha256_hex;
use super::{MailboxStore, MisfinAddress, MisfinServerError};

/// Misfin's well-known port.
pub const MISFIN_PORT: u16 = 1958;

/// The maximum request size the spec permits (the whole request line).
const MAX_REQUEST_BYTES: usize = 2048;

/// How long to wait for a client to finish sending its request before giving up.
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// A gemini-shaped misfin reply: a two-digit `status` and a `meta` string,
/// encoded as `<status> <meta>\r\n`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MisfinResponse {
    pub status: u8,
    pub meta: String,
}

impl MisfinResponse {
    fn new(status: u8, meta: &str) -> Self {
        Self {
            status,
            meta: meta.to_string(),
        }
    }

    /// The wire encoding: `<status> <meta>\r\n`.
    pub fn encode(&self) -> String {
        format!("{:02} {}\r\n", self.status, self.meta)
    }
}

/// A mailbox this server accepts mail for, with the fingerprint returned to a
/// sender on successful delivery (the status-20 META — the recipient's own
/// certificate fingerprint, so the sender can pin it).
#[derive(Clone, Debug)]
pub struct ServedMailbox {
    pub address: MisfinAddress,
    pub fingerprint: String,
}

/// Everything a [`MisfinServer`] needs besides its mailbox store: the
/// certificate it presents in the TLS handshake and the mailboxes it serves.
pub struct MisfinServerConfig {
    /// The server's leaf certificate (DER) presented during the TLS handshake.
    pub tls_certificate_der: Vec<u8>,
    /// The matching private key (PKCS#8 DER).
    pub tls_private_key_pkcs8_der: Vec<u8>,
    /// The mailboxes this server delivers to. Anything else is 51/53.
    pub served: Vec<ServedMailbox>,
}

/// The TLS-free request handler. Split out from the listener so the protocol
/// decisions are unit-testable without a socket.
struct Dispatcher {
    served: Vec<ServedMailbox>,
    store: MailboxStore,
}

impl Dispatcher {
    fn dispatch(
        &self,
        request: &str,
        peer_cert: Option<&CertificateDer<'_>>,
        now: u64,
    ) -> MisfinResponse {
        let Some(cert) = peer_cert else {
            return MisfinResponse::new(60, "Certificate required.");
        };
        let Some((target, message)) = split_request(request) else {
            return MisfinResponse::new(59, "Malformed request.");
        };
        let Some(recipient) = parse_recipient(target) else {
            return MisfinResponse::new(59, "Malformed recipient address.");
        };
        if !self.serves_host(&recipient.host) {
            return MisfinResponse::new(53, "Domain not serviced.");
        }
        let Some(served) = self.find_mailbox(&recipient) else {
            return MisfinResponse::new(51, "Mailbox doesn't exist.");
        };

        let fingerprint = sha256_hex(cert.as_ref());
        if let Err(error) = self.store.record_sender(&fingerprint, now) {
            log::warn!("misfin: recording sender failed: {error}");
            return MisfinResponse::new(40, "Temporary server error.");
        }
        match self.store.store(&recipient, &fingerprint, None, message, now) {
            Ok(_) => MisfinResponse::new(20, &served.fingerprint),
            Err(error) => {
                log::warn!("misfin: storing message failed: {error}");
                MisfinResponse::new(40, "Temporary server error.")
            }
        }
    }

    fn serves_host(&self, host: &str) -> bool {
        self.served.iter().any(|mailbox| mailbox.address.host == host)
    }

    fn find_mailbox(&self, recipient: &MisfinAddress) -> Option<&ServedMailbox> {
        self.served
            .iter()
            .find(|mailbox| &mailbox.address == recipient)
    }
}

/// Split a request line into `(target, message)` on the first space, validating
/// the misfin scheme. The message is the remainder verbatim (it may be empty or
/// contain further spaces).
fn split_request(request: &str) -> Option<(&str, &str)> {
    let (target, message) = request.trim_start().split_once(' ')?;
    if !target.starts_with("misfin://") {
        return None;
    }
    Some((target, message))
}

fn parse_recipient(target: &str) -> Option<MisfinAddress> {
    let url = url::Url::parse(target).ok()?;
    MisfinAddress::from_url(&url).ok()
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// A misfin server ready to bind. Build with [`MisfinServer::new`], then
/// [`bind`](MisfinServer::bind) to start listening.
pub struct MisfinServer {
    acceptor: TlsAcceptor,
    dispatcher: Arc<Dispatcher>,
}

impl MisfinServer {
    /// Build a server from its `config` and a mailbox `store`. Constructs the
    /// TLS acceptor (a client-cert-requesting, TOFU-permissive handshake); fails
    /// only if the supplied certificate / key are unusable.
    pub fn new(config: MisfinServerConfig, store: MailboxStore) -> Result<Self, MisfinServerError> {
        let cert = CertificateDer::from(config.tls_certificate_der);
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(config.tls_private_key_pkcs8_der));
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let tls_config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|error| MisfinServerError::Config(error.to_string()))?
            .with_client_cert_verifier(Arc::new(AcceptAnyClient))
            .with_single_cert(vec![cert], key)
            .map_err(|error| MisfinServerError::Config(error.to_string()))?;
        Ok(Self {
            acceptor: TlsAcceptor::from(Arc::new(tls_config)),
            dispatcher: Arc::new(Dispatcher {
                served: config.served,
                store,
            }),
        })
    }

    /// Bind the listener on `addr`. Use a `:0` port and
    /// [`local_addr`](BoundMisfinServer::local_addr) to discover the chosen port.
    pub async fn bind(self, addr: SocketAddr) -> Result<BoundMisfinServer, MisfinServerError> {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|error| MisfinServerError::Io(error.to_string()))?;
        Ok(BoundMisfinServer {
            acceptor: self.acceptor,
            dispatcher: self.dispatcher,
            listener,
        })
    }
}

/// A bound misfin server, listening but not yet accepting. Call
/// [`serve`](BoundMisfinServer::serve) to run the accept loop.
pub struct BoundMisfinServer {
    acceptor: TlsAcceptor,
    dispatcher: Arc<Dispatcher>,
    listener: TcpListener,
}

impl BoundMisfinServer {
    /// The address the listener bound to (resolves a `:0` request).
    pub fn local_addr(&self) -> Result<SocketAddr, MisfinServerError> {
        self.listener
            .local_addr()
            .map_err(|error| MisfinServerError::Io(error.to_string()))
    }

    /// Accept connections until `shutdown` resolves. Each connection is handled
    /// on its own task; a handshake or read failure drops that connection only.
    pub async fn serve(self, shutdown: impl Future<Output = ()>) -> Result<(), MisfinServerError> {
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                accepted = self.listener.accept() => match accepted {
                    Ok((tcp, _peer)) => {
                        let acceptor = self.acceptor.clone();
                        let dispatcher = self.dispatcher.clone();
                        tokio::spawn(handle_connection(acceptor, dispatcher, tcp));
                    }
                    Err(error) => log::warn!("misfin: accept failed: {error}"),
                },
            }
        }
        Ok(())
    }
}

async fn handle_connection(acceptor: TlsAcceptor, dispatcher: Arc<Dispatcher>, tcp: TcpStream) {
    let mut tls = match acceptor.accept(tcp).await {
        Ok(tls) => tls,
        Err(error) => {
            log::debug!("misfin: TLS handshake failed: {error}");
            return;
        }
    };
    let request = match read_request(&mut tls).await {
        Ok(request) => request,
        Err(error) => {
            log::debug!("misfin: reading request failed: {error}");
            return;
        }
    };
    let peer_cert = tls
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|certs| certs.first())
        .cloned();
    let response = dispatcher.dispatch(&request, peer_cert.as_ref(), unix_now());
    if let Err(error) = tls.write_all(response.encode().as_bytes()).await {
        log::debug!("misfin: writing response failed: {error}");
    }
    let _ = tls.shutdown().await;
}

/// Read the request line: bytes up to the first CRLF, capped at 2048, with a
/// read timeout. Trailing CR/LF are trimmed from the returned string.
async fn read_request<S>(stream: &mut S) -> io::Result<String>
where
    S: AsyncRead + Unpin,
{
    let mut buf = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    loop {
        let read = tokio::time::timeout(REQUEST_READ_TIMEOUT, stream.read(&mut byte)).await;
        let count = match read {
            Ok(Ok(count)) => count,
            Ok(Err(error)) => return Err(error),
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "misfin request read timed out",
                ));
            }
        };
        if count == 0 {
            break;
        }
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n") || buf.len() >= MAX_REQUEST_BYTES {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&buf)
        .trim_end_matches(['\r', '\n'])
        .to_string())
}

/// A client-certificate verifier that requests a certificate but accepts any the
/// client presents (TOFU-permissive) and does not *require* one — a client with
/// no certificate still completes the handshake, so the server can reply 60 at
/// the application layer rather than failing the handshake. Never use for
/// CA-anchored TLS.
#[derive(Debug)]
struct AcceptAnyClient;

impl ClientCertVerifier for AcceptAnyClient {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        false
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::ED25519,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{deterministic_identity, MisfinIdentitySpec};

    fn spec(addr: &str) -> MisfinIdentitySpec {
        MisfinIdentitySpec {
            address: MisfinAddress::parse(addr).unwrap(),
            blurb: Some("Test".to_string()),
        }
    }

    /// Mint a certificate DER for use as a peer/client certificate in a test.
    fn cert_der(seed: u8, addr: &str) -> Vec<u8> {
        deterministic_identity(&[seed; 32], &spec(addr))
            .unwrap()
            .certificate_der
    }

    fn dispatcher_for(mailbox: &str) -> (Dispatcher, String) {
        let identity = deterministic_identity(&[9u8; 32], &spec(mailbox)).unwrap();
        let fingerprint = sha256_hex(&identity.certificate_der);
        let dispatcher = Dispatcher {
            served: vec![ServedMailbox {
                address: MisfinAddress::parse(mailbox).unwrap(),
                fingerprint: fingerprint.clone(),
            }],
            store: MailboxStore::in_memory().unwrap(),
        };
        (dispatcher, fingerprint)
    }

    #[test]
    fn response_encodes_status_and_meta() {
        assert_eq!(MisfinResponse::new(20, "abc").encode(), "20 abc\r\n");
        assert_eq!(
            MisfinResponse::new(51, "Mailbox doesn't exist.").encode(),
            "51 Mailbox doesn't exist.\r\n"
        );
    }

    #[test]
    fn no_certificate_is_rejected_with_60() {
        let (dispatcher, _) = dispatcher_for("mark@example.test");
        let response = dispatcher.dispatch("misfin://mark@example.test hi", None, 1);
        assert_eq!(response.status, 60);
    }

    #[test]
    fn malformed_request_is_rejected_with_59() {
        let (dispatcher, _) = dispatcher_for("mark@example.test");
        let cert = CertificateDer::from(cert_der(3, "ana@other.test"));
        // No space → no message.
        assert_eq!(
            dispatcher.dispatch("misfin://mark@example.test", Some(&cert), 1).status,
            59
        );
        // Wrong scheme.
        assert_eq!(
            dispatcher.dispatch("gemini://mark@example.test hi", Some(&cert), 1).status,
            59
        );
    }

    #[test]
    fn unserved_host_is_rejected_with_53() {
        let (dispatcher, _) = dispatcher_for("mark@example.test");
        let cert = CertificateDer::from(cert_der(3, "ana@other.test"));
        let response = dispatcher.dispatch("misfin://mark@elsewhere.test hi", Some(&cert), 1);
        assert_eq!(response.status, 53);
    }

    #[test]
    fn unknown_mailbox_on_served_host_is_rejected_with_51() {
        let (dispatcher, _) = dispatcher_for("mark@example.test");
        let cert = CertificateDer::from(cert_der(3, "ana@other.test"));
        let response = dispatcher.dispatch("misfin://ghost@example.test hi", Some(&cert), 1);
        assert_eq!(response.status, 51);
    }

    #[test]
    fn delivered_message_is_stored_and_acknowledged_with_20() {
        let (dispatcher, recipient_fingerprint) = dispatcher_for("mark@example.test");
        let sender_der = cert_der(3, "ana@other.test");
        let cert = CertificateDer::from(sender_der.clone());

        let response =
            dispatcher.dispatch("misfin://mark@example.test Hello Mark", Some(&cert), 1234);
        assert_eq!(response.status, 20);
        assert_eq!(response.meta, recipient_fingerprint, "META is the recipient's fingerprint");

        let inbox = dispatcher.store.list("mark@example.test").unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].body, "Hello Mark");
        assert_eq!(inbox[0].sender_fingerprint, sha256_hex(&sender_der));
        assert_eq!(inbox[0].received_at, 1234);
    }

    #[tokio::test]
    async fn round_trip_delivers_over_tls() {
        let server_identity = deterministic_identity(&[1u8; 32], &spec("mark@example.test")).unwrap();
        let server_fingerprint = sha256_hex(&server_identity.certificate_der);
        let store = MailboxStore::in_memory().unwrap();
        let config = MisfinServerConfig {
            tls_certificate_der: server_identity.certificate_der.clone(),
            tls_private_key_pkcs8_der: server_identity.private_key_pkcs8_der.clone(),
            served: vec![ServedMailbox {
                address: MisfinAddress::parse("mark@example.test").unwrap(),
                fingerprint: server_fingerprint.clone(),
            }],
        };
        let server = MisfinServer::new(config, store.clone()).unwrap();
        let bound = server
            .bind("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let addr = bound.local_addr().unwrap();

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let serve = tokio::spawn(async move {
            let _ = bound
                .serve(async {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        let sender = deterministic_identity(&[2u8; 32], &spec("ana@other.test")).unwrap();
        let reply = client_send(
            addr,
            "misfin://mark@example.test",
            "Hi from Ana",
            sender.certificate_der,
            sender.private_key_pkcs8_der,
        )
        .await;

        assert!(
            reply.starts_with(&format!("20 {server_fingerprint}")),
            "expected 20 + server fingerprint, got: {reply:?}"
        );
        let inbox = store.list("mark@example.test").unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].body, "Hi from Ana");

        let _ = shutdown_tx.send(());
        let _ = serve.await;
    }

    /// A minimal misfin client for the round-trip test: presents a client cert,
    /// accepts any server cert, sends one request, returns the raw reply.
    async fn client_send(
        addr: SocketAddr,
        recipient: &str,
        message: &str,
        cert_der: Vec<u8>,
        key_pkcs8_der: Vec<u8>,
    ) -> String {
        use rustls::client::danger::ServerCertVerified;
        use rustls::pki_types::ServerName;

        #[derive(Debug)]
        struct AcceptAnyServer;
        impl rustls::client::danger::ServerCertVerifier for AcceptAnyServer {
            fn verify_server_cert(
                &self,
                _end_entity: &CertificateDer<'_>,
                _intermediates: &[CertificateDer<'_>],
                _server_name: &ServerName<'_>,
                _ocsp: &[u8],
                _now: UnixTime,
            ) -> Result<ServerCertVerified, rustls::Error> {
                Ok(ServerCertVerified::assertion())
            }
            fn verify_tls12_signature(
                &self,
                _message: &[u8],
                _cert: &CertificateDer<'_>,
                _dss: &DigitallySignedStruct,
            ) -> Result<HandshakeSignatureValid, rustls::Error> {
                Ok(HandshakeSignatureValid::assertion())
            }
            fn verify_tls13_signature(
                &self,
                _message: &[u8],
                _cert: &CertificateDer<'_>,
                _dss: &DigitallySignedStruct,
            ) -> Result<HandshakeSignatureValid, rustls::Error> {
                Ok(HandshakeSignatureValid::assertion())
            }
            fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
                vec![SignatureScheme::ED25519, SignatureScheme::ECDSA_NISTP256_SHA256]
            }
        }

        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAnyServer))
            .with_client_auth_cert(
                vec![CertificateDer::from(cert_der)],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pkcs8_der)),
            )
            .unwrap();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        let tcp = TcpStream::connect(addr).await.unwrap();
        let server_name = ServerName::try_from("example.test").unwrap();
        let mut tls = connector.connect(server_name, tcp).await.unwrap();
        tls.write_all(format!("{recipient} {message}\r\n").as_bytes())
            .await
            .unwrap();
        let mut raw = Vec::new();
        tls.read_to_end(&mut raw).await.unwrap();
        String::from_utf8_lossy(&raw).to_string()
    }
}
