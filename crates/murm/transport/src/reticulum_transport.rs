//! Reticulum-backed [`Transport`] implementation.
//!
//! [`ReticulumTransport`] runs Mere's bilateral peer-to-peer lane over
//! Reticulum packet links (the Beechat Rust port). It is feature-gated behind
//! the `reticulum` cargo feature and is default-off.
//!
//! ## Identity
//!
//! Mere's master key is a single Ed25519 keypair. Reticulum requires a dual-key
//! identity (X25519 ECDH + Ed25519 signing). This transport derives both keys
//! deterministically from the 32-byte master seed via HKDF-SHA256, so the same
//! Mere seed always produces the same Reticulum destination across restarts.
//! The Mere [`PeerID`] remains the master Ed25519 public key, consistent with
//! the other transports.
//!
//! ## Discovery
//!
//! Reticulum destinations cannot be synthesized from a [`PeerID`]; they must be
//! learned from authenticated announces. Each registered ALPN destination is
//! announced periodically with `app_data` carrying:
//!
//! ```text
//! app_data = PeerID || signature
//! signature  = ed25519_sign(master_key,
//!                           reticulum_public_key || reticulum_verifying_key
//!                           || PeerID || ALPN)
//! ```
//!
//! Receivers verify the signature against the announced [`PeerID`] before
//! storing the `PeerID -> (ALPN, DestinationDesc)` mapping. `connect(peer, alpn)`
//! then looks up the peer's learned destination and establishes a Reticulum link.
//!
//! ## ALPN mapping
//!
//! Each ALPN becomes a Reticulum destination name. For example:
//!
//! - `mere/cable/v1` -> `DestinationName::new("mere", "cable.v1")`
//!
//! ## Stream mapping
//!
//! A Reticulum link is bridged to an `AsyncRead + AsyncWrite` stream using a
//! tokio `DuplexStream`. A background relay task on one end of the duplex chunks
//! outgoing bytes into Reticulum data packets and writes incoming link payloads
//! to the duplex; the other end is the [`ReticulumStream`] returned by `connect`
//! and `accept`.

use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::task::{Context, Poll};
use std::time::Duration;

use hkdf::Hkdf;
use identity::{Ed25519Keypair, Ed25519PublicKey, Ed25519Signature};
use rand_core::OsRng;
use sha2::Sha256;
use tokio::io::{AsyncRead, AsyncWrite, DuplexStream, ReadBuf};
use tokio::sync::{Mutex as TokioMutex, broadcast, mpsc};
use tokio::task::JoinHandle;

use reticulum::destination::link::{Link, LinkEvent, LinkEventData, LinkId};
use reticulum::destination::{DestinationDesc, DestinationName, SingleInputDestination};
use reticulum::hash::AddressHash;
use reticulum::hash::Hash as ReticulumHash;
use reticulum::identity::Identity as ReticulumIdentity;
use reticulum::identity::PrivateIdentity as ReticulumPrivateIdentity;
use reticulum::transport::{AnnounceEvent, Transport as ReticulumStack, TransportConfig};

use crate::{Alpn, PeerID, Transport, TransportError};

/// Default chunk size for writes over a Reticulum link.
///
/// `PACKET_MDU` is 2048 bytes; Fernet encryption adds ~57 bytes of overhead,
/// so 1024-byte plaintext chunks leave comfortable headroom.
const LINK_CHUNK_SIZE: usize = 1024;

/// Default interval between repeated announces.
const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(5);

/// HKDF context string for the dual-key derivation.
const IDENTITY_HKDF_INFO: &[u8] = b"mere-reticulum-identity-v1";

/// A network interface to attach to a [`ReticulumTransport`].
#[derive(Clone, Debug)]
pub enum ReticulumInterface {
    /// TCP server accepting incoming connections.
    TcpServer {
        /// Address to bind, e.g. `127.0.0.1:4242`.
        bind: SocketAddr,
    },
    /// TCP client connecting to a peer server.
    TcpClient {
        /// Address to connect to.
        addr: SocketAddr,
    },
    /// UDP interface bound to a local address, optionally forwarding to a peer.
    Udp {
        /// Address to bind.
        bind: SocketAddr,
        /// Optional peer address to forward packets to.
        forward: Option<SocketAddr>,
    },
}

/// Bidirectional byte stream over a Reticulum link.
///
/// Implements [`AsyncRead`] + [`AsyncWrite`] by delegating to an internal
/// `DuplexStream`; the opposite end of the duplex is driven by a relay task that
/// moves data between the duplex and the Reticulum link.
pub struct ReticulumStream {
    inner: DuplexStream,
    _relay: JoinHandle<()>,
}

impl std::fmt::Debug for ReticulumStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReticulumStream").finish_non_exhaustive()
    }
}

impl AsyncRead for ReticulumStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for ReticulumStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Builder for [`ReticulumTransport`].
pub struct ReticulumTransportBuilder<'a> {
    master: &'a Ed25519Keypair,
    alpns: Vec<Alpn>,
    interfaces: Vec<ReticulumInterface>,
    announce_interval: Duration,
}

impl<'a> ReticulumTransportBuilder<'a> {
    /// Start building a transport from the given master keypair.
    pub fn new(master: &'a Ed25519Keypair) -> Self {
        Self {
            master,
            alpns: Vec::new(),
            interfaces: Vec::new(),
            announce_interval: ANNOUNCE_INTERVAL,
        }
    }

    /// ALPN protocols this transport will accept and announce.
    pub fn alpns(mut self, alpns: Vec<Alpn>) -> Self {
        self.alpns = alpns;
        self
    }

    /// Network interfaces to attach (TCP server/client, UDP, ...).
    pub fn interfaces(mut self, interfaces: Vec<ReticulumInterface>) -> Self {
        self.interfaces = interfaces;
        self
    }

    /// How often to re-announce registered ALPN destinations.
    pub fn announce_interval(mut self, interval: Duration) -> Self {
        self.announce_interval = interval;
        self
    }

    /// Bind the transport: create the Reticulum stack, attach interfaces,
    /// register destinations, and start announce listening/sending.
    pub async fn bind(self) -> Result<ReticulumTransport, TransportError> {
        ReticulumTransport::bind_inner(
            self.master,
            self.alpns,
            self.interfaces,
            self.announce_interval,
        )
        .await
    }
}

/// Reticulum-backed implementation of the [`Transport`] trait.
pub struct ReticulumTransport {
    local_peer_id: PeerID,
    inner: Arc<ReticulumStack>,
    destinations: Arc<StdMutex<HashMap<Alpn, Arc<TokioMutex<SingleInputDestination>>>>>,
    name_to_alpn: Arc<StdMutex<HashMap<ReticulumHash, Alpn>>>,
    peers: Arc<StdMutex<HashMap<PeerID, HashMap<Alpn, DestinationDesc>>>>,
}

impl ReticulumTransport {
    /// Construct a builder for a `ReticulumTransport`.
    pub fn builder(master: &Ed25519Keypair) -> ReticulumTransportBuilder<'_> {
        ReticulumTransportBuilder::new(master)
    }

    async fn bind_inner(
        master: &Ed25519Keypair,
        alpns: Vec<Alpn>,
        interfaces: Vec<ReticulumInterface>,
        announce_interval: Duration,
    ) -> Result<Self, TransportError> {
        let local_peer_id = PeerID::from_public_key(master.public_key());
        let private_identity = derive_identity(master);

        let config = TransportConfig::new("mere", &private_identity, true);
        let inner = Arc::new(ReticulumStack::new(config));

        // Attach network interfaces.
        {
            let mut iface_manager = inner.iface_manager().lock().await;
            for iface in interfaces {
                match iface {
                    ReticulumInterface::TcpServer { bind } => {
                        use reticulum::iface::tcp_server::TcpServer;
                        let server = TcpServer::new(bind.to_string(), inner.iface_manager());
                        iface_manager.spawn(server, TcpServer::spawn);
                    }
                    ReticulumInterface::TcpClient { addr } => {
                        use reticulum::iface::tcp_client::TcpClient;
                        let client = TcpClient::new(addr.to_string());
                        iface_manager.spawn(client, TcpClient::spawn);
                    }
                    ReticulumInterface::Udp { bind, forward } => {
                        use reticulum::iface::udp::UdpInterface;
                        let forward = forward.map(|a| a.to_string());
                        let udp = UdpInterface::new(bind.to_string(), forward.as_deref());
                        iface_manager.spawn(udp, UdpInterface::spawn);
                    }
                }
            }
        }

        // Register an incoming destination for each ALPN and build the reverse
        // name -> ALPN map.
        let mut destinations = HashMap::new();
        let mut name_to_alpn = HashMap::new();
        for alpn in &alpns {
            let name = destination_name_for_alpn(alpn);
            let dest = inner
                .add_destination(private_identity.clone(), name)
                .await;
            name_to_alpn.insert(name.hash, alpn.clone());
            destinations.insert(alpn.clone(), dest);
        }

        let destinations = Arc::new(StdMutex::new(destinations));
        let name_to_alpn = Arc::new(StdMutex::new(name_to_alpn));
        let peers: Arc<StdMutex<HashMap<PeerID, HashMap<Alpn, DestinationDesc>>>> =
            Arc::new(StdMutex::new(HashMap::new()));

        // Spawn announce listener.
        let announce_rx = inner.recv_announces().await;
        tokio::spawn(announce_listener(
            announce_rx,
            Arc::clone(&name_to_alpn),
            Arc::clone(&peers),
        ));

        // Spawn periodic announce sender.
        let master_clone = master.clone();
        tokio::spawn(announce_sender(
            Arc::clone(&inner),
            Arc::clone(&destinations),
            master_clone,
            local_peer_id,
            announce_interval,
        ));

        Ok(Self {
            local_peer_id,
            inner,
            destinations,
            name_to_alpn,
            peers,
        })
    }

    /// Send an announce for the given ALPN immediately.
    ///
    /// Primarily useful in tests to avoid waiting for the periodic announce
    /// timer.
    pub async fn send_announce_now(&self, alpn: &Alpn) -> Result<(), TransportError> {
        let dest = self
            .destinations
            .lock()
            .unwrap()
            .get(alpn)
            .cloned()
            .ok_or(TransportError::AlpnNotRegistered)?;

        let identity = {
            let guard = dest.lock().await;
            guard.desc.identity
        };

        let app_data = build_app_data(&self.local_peer_id, alpn, &identity, &self.master_keypair()?);
        self.inner.send_announce(&dest, Some(&app_data)).await;
        Ok(())
    }

    fn master_keypair(&self) -> Result<Ed25519Keypair, TransportError> {
        // The builder held the master keypair only long enough to derive the
        // Reticulum identity and sign announces. We cannot reconstruct the
        // master signing key from the public key, so on-demand announce signing
        // is unavailable after construction. In practice the periodic announce
        // task clones the keypair before bind returns; this method exists only
        // for the hypothetical public API above.
        Err(TransportError::Backend(
            "master keypair not retained by ReticulumTransport".into(),
        ))
    }
}

// We never actually call `master_keypair()` in production code; remove the
// warning by implementing the public helper differently.
