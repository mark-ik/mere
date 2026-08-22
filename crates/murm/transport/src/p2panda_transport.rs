//! p2panda-net-backed [`Transport`] implementation — the production transport.
//!
//! [`P2pandaTransport`] makes `p2panda-net`'s `Endpoint` the endpoint authority:
//! it runs a real iroh QUIC endpoint (via p2panda-net), demultiplexes incoming
//! connections by ALPN through p2panda-net's `Endpoint::accept` (iroh
//! `ProtocolHandler`) into per-ALPN queues that [`Transport::accept`] drains, and
//! opens a fresh bidirectional stream per [`Transport::connect`]. It optionally
//! serves the `iroh-blobs` protocol off the same endpoint.
//!
//! Compared to the retired hand-rolled `IrohTransport` Router, this gains
//! p2panda-net's discovery (`Discovery` / `MdnsDiscovery`), relay/hole-punching,
//! NAT port-mapping, and actor supervision for free, and keeps the same
//! one-endpoint / many-protocols / raw-ALPN-stream behavior the `Transport`
//! trait promises.
//!
//! ## Identity
//!
//! Constructed from an [`identity::Ed25519Keypair`] or a provider-neutral raw
//! 32-byte Ed25519 signing seed. The latter lets sibling applications use an
//! external identity provider such as Personae without coupling this crate to
//! it. In either case the seed bridges the ed25519-dalek major-version boundary
//! without forcing either side to upgrade.
//!
//! ## Discovery
//!
//! Three mechanisms share the one endpoint, each opt-in via the builder:
//!
//! - **Explicit peer** (tests, LAN): a peer's [`endpoint_addr`] (its real
//!   interface addresses, with loopback covering wildcard binds) is shared
//!   out-of-band and inserted via [`add_peer`].
//! - **mDNS** (`builder().mdns(..)`): same-network peers auto-populate the
//!   address book.
//! - **Random-walk** (`builder().discovery()`): walkers explore outward from
//!   bootstrap nodes already in the address book to reach internet peers; the
//!   handle is held for the transport's life.
//!
//! [`Ed25519Keypair::to_seed`]: identity::Ed25519Keypair::to_seed
//! [`endpoint_addr`]: P2pandaTransport::endpoint_addr
//! [`add_peer`]: P2pandaTransport::add_peer

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::task::{Context, Poll};
use std::time::Duration;

use identity::Ed25519Keypair;
use iroh::EndpointAddr;
use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh_tickets::endpoint::EndpointTicket;
use p2panda_core::{SigningKey, Topic, VerifyingKey};
use p2panda_net::addrs::NodeInfo;
use p2panda_net::discovery::DiscoveryConfig;
pub use p2panda_net::Endpoint;
pub use p2panda_net::gossip::Gossip;

use p2panda_net::gossip::GossipHandle;
// Re-exported: [`P2pandaTransportBuilder::mdns`] is public but its argument
// type was not, so no consumer outside this crate could call it.
pub use p2panda_net::iroh_mdns::MdnsDiscoveryMode;
// Re-exported so a consumer can configure a relay without taking a direct iroh
// dependency of its own.
pub use iroh::RelayUrl;
use muniment::{JsonCodec, MemoryBackend};
use p2panda_net::address_book::AddressBookStoreHandle;
use p2panda_net::{AddressBook, Discovery, MdnsDiscovery};
use stickleback::MunimentAddressBook;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{Mutex as TokioMutex, mpsc};

use crate::blobs::{BlobHash, BlobPeerAuthorizer, BlobReadAuthorizer, BlobScope, BlobStore};
use crate::{AcceptedSession, Alpn, IngressContext, PeerID, Transport, TransportError};

/// A bidirectional p2panda-net QUIC stream presented as `AsyncRead + AsyncWrite`.
///
/// `_connection` is held so the QUIC connection stays open for the stream's life.
pub struct P2pandaStream {
    send: SendStream,
    recv: RecvStream,
    _connection: Connection,
    shutdown_ack: Option<Pin<Box<dyn Future<Output = std::io::Result<()>> + Send>>>,
    shutdown_finished: bool,
    shutdown_complete: bool,
}

impl std::fmt::Debug for P2pandaStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("P2pandaStream").finish_non_exhaustive()
    }
}

impl AsyncRead for P2pandaStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // Disambiguate to the tokio AsyncRead impl (RecvStream also has an
        // inherent read with a different error type).
        <RecvStream as AsyncRead>::poll_read(Pin::new(&mut self.recv), cx, buf)
    }
}

impl AsyncWrite for P2pandaStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        <SendStream as AsyncWrite>::poll_write(Pin::new(&mut self.send), cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        <SendStream as AsyncWrite>::poll_flush(Pin::new(&mut self.send), cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        if self.shutdown_complete {
            return Poll::Ready(Ok(()));
        }
        if self.shutdown_ack.is_none() {
            // Create this before finishing so `stopped` cannot miss the
            // acknowledgement that all stream bytes reached the peer. The
            // connection handle is the stream's last handle in the usual
            // one-stream case; dropping it immediately after `finish` can
            // otherwise race the peer reading a small final frame.
            let stopped = self.send.stopped();
            self.shutdown_ack = Some(Box::pin(async move {
                match stopped.await {
                    Ok(None) => Ok(()),
                    Ok(Some(code)) => Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionReset,
                        format!("peer stopped stream before acknowledging final bytes: {code}"),
                    )),
                    Err(error) => Err(std::io::Error::from(error)),
                }
            }));
        }
        if !self.shutdown_finished {
            match <SendStream as AsyncWrite>::poll_shutdown(Pin::new(&mut self.send), cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => self.shutdown_finished = true,
            }
        }
        let acknowledgement = self
            .shutdown_ack
            .as_mut()
            .expect("shutdown acknowledgement initialized");
        match acknowledgement.as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => {
                self.shutdown_ack = None;
                self.shutdown_complete = result.is_ok();
                Poll::Ready(result)
            }
        }
    }
}

/// A queued inbound p2panda stream and the peer the transport authenticated.
///
/// The peer is captured in the handler, where the [`Connection`] is still in
/// hand: `accept` drains a queue and no longer has it. `None` only if the
/// remote key fails to decode, which would mean the connection is not usable
/// as an authenticated peer anyway.
struct QueuedStream {
    stream: P2pandaStream,
    peer: Option<PeerID>,
}

/// Registered per ALPN on the p2panda-net endpoint; pushes each accepted
/// bi-stream to a queue that [`Transport::accept`] drains.
#[derive(Debug, Clone)]
struct StreamQueueHandler {
    tx: mpsc::UnboundedSender<QueuedStream>,
}

impl ProtocolHandler for StreamQueueHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        // p2panda authenticates its connections, so the remote id is a
        // transport fact (plan D4) and may be reported as the peer.
        let peer = PeerID::from_bytes(connection.remote_id().as_bytes()).ok();
        let (send, recv) = connection
            .accept_bi()
            .await
            .map_err(AcceptError::from_err)?;
        let _ = self.tx.send(QueuedStream {
            stream: P2pandaStream {
                send,
                recv,
                _connection: connection,
                shutdown_ack: None,
                shutdown_finished: false,
                shutdown_complete: false,
            },
            peer,
        });
        Ok(())
    }
}

/// Domain-authorized wrapper around the ordinary iroh-blobs protocol.
///
/// Authorization is evaluated from the transport-authenticated remote key
/// before the peer can name or read any hash in the store.
struct AuthorizedBlobsProtocol {
    inner: iroh_blobs::BlobsProtocol,
    authorizer: BlobPeerAuthorizer,
}

/// Hash-scoped wrapper around the ordinary iroh-blobs request handlers.
///
/// iroh authenticates the remote endpoint and parses each request. Murm asks
/// the domain authorizer about every named hash before delegating the allowed
/// request to iroh-blobs' public provider implementation.
struct ScopedBlobsProtocol {
    store: iroh_blobs::api::Store,
    scope: BlobScope,
    authorizer: BlobReadAuthorizer,
}

impl std::fmt::Debug for ScopedBlobsProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopedBlobsProtocol")
            .field("scope", &self.scope)
            .field("readers", &self.authorizer.readers(self.scope).len())
            .finish_non_exhaustive()
    }
}

impl ProtocolHandler for ScopedBlobsProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let peer = *connection.remote_id().as_bytes();
        while let Ok(mut pair) = iroh_blobs::provider::StreamPair::accept(
            &connection,
            iroh_blobs::provider::events::EventSender::DEFAULT,
        )
        .await
        {
            let request = match pair.read_request().await {
                Ok(request) => request,
                Err(_) => continue,
            };
            if !scoped_request_allowed(&self.store, &self.authorizer, self.scope, &peer, &request)
                .await
            {
                connection.close(
                    iroh_blobs::protocol::ERR_PERMISSION,
                    b"hash is not authorized in this blob scope",
                );
                break;
            }
            let store = self.store.clone();
            tokio::spawn(async move {
                match request {
                    iroh_blobs::protocol::Request::Get(request) => {
                        let _ = iroh_blobs::provider::handle_get(pair, store, request).await;
                    }
                    iroh_blobs::protocol::Request::GetMany(request) => {
                        let _ = iroh_blobs::provider::handle_get_many(pair, store, request).await;
                    }
                    iroh_blobs::protocol::Request::Observe(request) => {
                        let _ = iroh_blobs::provider::handle_observe(pair, store, request).await;
                    }
                    _ => {}
                }
            });
        }
        Ok(())
    }
}

async fn scoped_request_allowed(
    store: &iroh_blobs::api::Store,
    authorizer: &BlobReadAuthorizer,
    scope: BlobScope,
    peer: &[u8; 32],
    request: &iroh_blobs::protocol::Request,
) -> bool {
    use iroh_blobs::protocol::Request;

    match request {
        Request::Get(request) => {
            if !authorizer.allows(scope, peer, BlobHash::from(request.hash)) {
                return false;
            }
            if request.ranges.is_blob() {
                return true;
            }
            let Ok(bytes) = store.get_bytes(request.hash).await else {
                return false;
            };
            let Ok(children) = iroh_blobs::hashseq::HashSeq::try_from(bytes) else {
                return false;
            };
            request
                .ranges
                .iter_infinite()
                .take(children.len() + 1)
                .enumerate()
                .skip(1)
                .filter(|(_, ranges)| !ranges.is_empty())
                .all(|(offset, _)| {
                    children
                        .get(offset - 1)
                        .map(|hash| authorizer.allows(scope, peer, BlobHash::from(hash)))
                        .unwrap_or(false)
                })
        }
        Request::GetMany(request) => request
            .hashes
            .iter()
            .zip(request.ranges.iter_infinite())
            .filter(|(_, ranges)| !ranges.is_empty())
            .all(|(hash, _)| authorizer.allows(scope, peer, BlobHash::from(*hash))),
        Request::Observe(request) => authorizer.allows(scope, peer, BlobHash::from(request.hash)),
        // Scoped serving is read-only. Unknown slots and pushes are refused.
        _ => false,
    }
}

impl std::fmt::Debug for AuthorizedBlobsProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizedBlobsProtocol")
            .field("admitted_peers", &self.authorizer.peers().len())
            .finish_non_exhaustive()
    }
}

impl ProtocolHandler for AuthorizedBlobsProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        if !self.authorizer.allows(connection.remote_id().as_bytes()) {
            return Err(AcceptError::from_err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "peer is not authorized for this blob store",
            )));
        }
        self.inner.accept(connection).await
    }

    async fn shutdown(&self) {
        self.inner.shutdown().await;
    }
}

type AlpnQueues =
    Arc<StdMutex<HashMap<Alpn, Arc<TokioMutex<mpsc::UnboundedReceiver<QueuedStream>>>>>>;

/// A peer the address book associates with a topic.
///
/// Identity and reachability are separate facts and this type keeps them so. A
/// paired device is named by its peer id forever; whether it can be reached
/// right now is a property of what discovery has currently resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KnownPeer {
    /// The peer's durable identity. Never changes, unlike its address.
    pub peer: PeerID,
    /// False when the address book holds the identity but no transport
    /// information yet: discovery has named this peer without yet saying how
    /// to reach it.
    ///
    /// This is knowledge of an address, NOT a live link. A peer can be
    /// `reachable` for hours while every packet to it is dropped; read
    /// [`connected`](Self::connected) for the question most callers mean.
    pub reachable: bool,
    /// Whether this node is configured as a discovery bootstrap locally.
    pub bootstrap: bool,
    /// Whether the endpoint currently holds an ACTIVE path to this peer.
    ///
    /// The honest answer to "are we talking to it", as opposed to "do we know
    /// where it lives". Distinguishing these is not pedantry: a firewall rule
    /// silently dropped every inbound packet to one of these devices for
    /// hours, and because the address book still held an address, the host
    /// reported the peer as reachable throughout and looked healthy while
    /// nothing whatsoever replicated (2026-08-03).
    pub connected: bool,
}

/// Builder for [`P2pandaTransport`]. Use [`P2pandaTransport::builder`].
pub struct P2pandaTransportBuilder<'a> {
    signing_seed: [u8; 32],
    alpns: Vec<Alpn>,
    blobs: Option<&'a BlobStore>,
    blob_authorizer: Option<BlobPeerAuthorizer>,
    scoped_blob_authorizer: Option<(BlobScope, BlobReadAuthorizer)>,
    mdns: Option<MdnsDiscoveryMode>,
    discovery: Option<DiscoveryConfig>,
    gossip: bool,
    relay_urls: Vec<iroh::RelayUrl>,
}

impl<'a> P2pandaTransportBuilder<'a> {
    /// Mere-defined ALPNs to register; each gets its own accept queue.
    pub fn alpns(mut self, alpns: Vec<Alpn>) -> Self {
        self.alpns = alpns;
        self
    }

    /// Serve the iroh-blobs protocol against the given store. Peers can then
    /// `fetch_from` this transport's PeerID.
    pub fn blobs<'b>(self, store: &'b BlobStore) -> P2pandaTransportBuilder<'b> {
        P2pandaTransportBuilder {
            signing_seed: self.signing_seed,
            alpns: self.alpns,
            blobs: Some(store),
            blob_authorizer: None,
            scoped_blob_authorizer: None,
            mdns: self.mdns,
            discovery: self.discovery,
            gossip: self.gossip,
            relay_urls: self.relay_urls,
        }
    }

    /// Serve blobs only to peers admitted by the domain-owned authorizer.
    ///
    /// The peer identity comes from the authenticated QUIC connection. The
    /// transport does not infer pairing, membership, or capability authority.
    pub fn authorized_blobs<'b>(
        self,
        store: &'b BlobStore,
        authorizer: BlobPeerAuthorizer,
    ) -> P2pandaTransportBuilder<'b> {
        P2pandaTransportBuilder {
            signing_seed: self.signing_seed,
            alpns: self.alpns,
            blobs: Some(store),
            blob_authorizer: Some(authorizer),
            scoped_blob_authorizer: None,
            mdns: self.mdns,
            discovery: self.discovery,
            gossip: self.gossip,
            relay_urls: self.relay_urls,
        }
    }

    /// Serve only hashes retained by `scope` to that scope's current readers.
    pub fn scoped_blobs<'b>(
        self,
        store: &'b BlobStore,
        scope: BlobScope,
        authorizer: BlobReadAuthorizer,
    ) -> P2pandaTransportBuilder<'b> {
        P2pandaTransportBuilder {
            signing_seed: self.signing_seed,
            alpns: self.alpns,
            blobs: Some(store),
            blob_authorizer: None,
            scoped_blob_authorizer: Some((scope, authorizer)),
            mdns: self.mdns,
            discovery: self.discovery,
            gossip: self.gossip,
            relay_urls: self.relay_urls,
        }
    }

    /// Register an iroh relay, so peers that cannot reach each other directly
    /// still connect.
    ///
    /// p2panda registers no relay by default: the relay map is built purely
    /// from what is added here, and without one a peer is reachable only at a
    /// directly routable address that the other side already knows. mDNS
    /// supplies that on a shared link and nothing supplies it off one, which
    /// is why an unrelayed transport is a LAN-only transport.
    ///
    /// A relay carries connection metadata for whoever operates it. Which
    /// relay to trust is therefore an owner's decision, not a default worth
    /// baking in.
    pub fn relay_url(mut self, url: iroh::RelayUrl) -> Self {
        self.relay_urls.push(url);
        self
    }

    /// Enable mDNS discovery so peers on the same local network auto-populate
    /// the address book (no explicit `add_peer` needed on a LAN).
    /// `MdnsDiscoveryMode::Active` actively queries; `Passive` only responds.
    pub fn mdns(mut self, mode: MdnsDiscoveryMode) -> Self {
        self.mdns = Some(mode);
        self
    }

    /// Enable random-walk discovery for internet peers: walkers explore the
    /// network outward from the bootstrap nodes already in the address book
    /// (added via [`add_peer`](P2pandaTransport::add_peer) or surfaced by
    /// mDNS), resolving more peers' transport info over time. Uses the default
    /// [`DiscoveryConfig`] (2 walkers); see
    /// [`discovery_config`](Self::discovery_config) to tune it.
    pub fn discovery(mut self) -> Self {
        self.discovery = Some(DiscoveryConfig::default());
        self
    }

    /// Enable random-walk discovery with an explicit [`DiscoveryConfig`]
    /// (e.g. more walkers, a different reset-walk probability).
    pub fn discovery_config(mut self, config: DiscoveryConfig) -> Self {
        self.discovery = Some(config);
        self
    }

    /// Enable the gossip overlay, so [`subscribe`](P2pandaTransport::subscribe)
    /// can join space topics and broadcast/receive ephemeral messages (the
    /// live-sync path: peers subscribed to a space converge by publishing their
    /// operations to each other).
    pub fn gossip(mut self) -> Self {
        self.gossip = true;
        self
    }

    /// Bind the p2panda-net endpoint. Consumes the builder.
    pub async fn bind(self) -> Result<P2pandaTransport, TransportError> {
        P2pandaTransport::bind_inner(
            self.signing_seed,
            self.alpns,
            self.blobs,
            self.blob_authorizer,
            self.scoped_blob_authorizer,
            self.mdns,
            self.discovery,
            self.gossip,
            self.relay_urls,
        )
        .await
    }
}

/// p2panda-net-backed [`Transport`].
pub struct P2pandaTransport {
    endpoint: Endpoint,
    address_book: AddressBook,
    peer_id: PeerID,
    queues: AlpnQueues,
    /// mDNS discovery handle, held so the service keeps running while the
    /// transport is alive.
    _mdns: Option<MdnsDiscovery>,
    /// Random-walk discovery handle, held so the walkers keep running while the
    /// transport is alive (dropping it stops the discovery actor).
    _discovery: Option<Discovery>,
    /// Gossip overlay handle (the endpoint authority for space topics). `None`
    /// unless built with [`builder().gossip()`](P2pandaTransportBuilder::gossip);
    /// `subscribe`/`set_topics` use it.
    gossip: Option<Gossip>,
}

impl P2pandaTransport {
    /// Start a builder for a new p2panda-net transport.
    pub fn builder(master: &Ed25519Keypair) -> P2pandaTransportBuilder<'_> {
        P2pandaTransportBuilder {
            signing_seed: master.to_seed(),
            alpns: Vec::new(),
            blobs: None,
            blob_authorizer: None,
            scoped_blob_authorizer: None,
            mdns: None,
            discovery: None,
            relay_urls: Vec::new(),
            gossip: false,
        }
    }

    /// Start a builder from raw Ed25519 signing-key seed material.
    ///
    /// This is the identity-provider-neutral boundary for sibling apps. A
    /// Personae vault or another provider derives a protocol-scoped key and
    /// passes its 32-byte seed; transport never needs that provider's type.
    pub fn builder_from_seed(signing_seed: [u8; 32]) -> P2pandaTransportBuilder<'static> {
        P2pandaTransportBuilder {
            signing_seed,
            alpns: Vec::new(),
            blobs: None,
            blob_authorizer: None,
            scoped_blob_authorizer: None,
            mdns: None,
            discovery: None,
            relay_urls: Vec::new(),
            gossip: false,
        }
    }

    /// Bind with just the given Mere ALPNs (no discovery; explicit `add_peer`).
    pub async fn bind(master: &Ed25519Keypair, alpns: Vec<Alpn>) -> Result<Self, TransportError> {
        Self::bind_seed(master.to_seed(), alpns).await
    }

    /// Bind from raw protocol-scoped Ed25519 seed material.
    pub async fn bind_seed(
        signing_seed: [u8; 32],
        alpns: Vec<Alpn>,
    ) -> Result<Self, TransportError> {
        Self::bind_inner(
            signing_seed,
            alpns,
            None,
            None,
            None,
            None,
            None,
            false,
            Vec::new(),
        )
        .await
    }

    /// Bind with the given ALPNs and serve iroh-blobs against the provided store.
    pub async fn bind_with_blobs(
        master: &Ed25519Keypair,
        alpns: Vec<Alpn>,
        blobs: Option<&BlobStore>,
    ) -> Result<Self, TransportError> {
        Self::bind_inner(
            master.to_seed(),
            alpns,
            blobs,
            None,
            None,
            None,
            None,
            false,
            Vec::new(),
        )
        .await
    }

    /// Bind and serve blobs only to transport-authenticated admitted peers.
    pub async fn bind_with_authorized_blobs(
        master: &Ed25519Keypair,
        alpns: Vec<Alpn>,
        blobs: &BlobStore,
        authorizer: BlobPeerAuthorizer,
    ) -> Result<Self, TransportError> {
        Self::bind_inner(
            master.to_seed(),
            alpns,
            Some(blobs),
            Some(authorizer),
            None,
            None,
            None,
            false,
            Vec::new(),
        )
        .await
    }

    async fn bind_inner(
        signing_seed: [u8; 32],
        alpns: Vec<Alpn>,
        blobs: Option<&BlobStore>,
        blob_authorizer: Option<BlobPeerAuthorizer>,
        scoped_blob_authorizer: Option<(BlobScope, BlobReadAuthorizer)>,
        mdns: Option<MdnsDiscoveryMode>,
        discovery: Option<DiscoveryConfig>,
        gossip: bool,
        relay_urls: Vec<iroh::RelayUrl>,
    ) -> Result<Self, TransportError> {
        let signing_key = SigningKey::from_bytes(&signing_seed);
        let peer_id = PeerID::from_bytes(signing_key.verifying_key().as_bytes())
            .map_err(|error| TransportError::Backend(format!("transport key: {error}")))?;
        // p2panda's own default here was an in-memory SQLite store; this is the
        // same lifetime over muniment instead, which keeps sqlx out of the graph.
        // A caller wanting the address book to survive a restart hands a durable
        // muniment backend in place of `MemoryBackend`.
        let store = AddressBookStoreHandle::new(MunimentAddressBook::<_, JsonCodec>::new(
            MemoryBackend::new(),
        ));
        let address_book = AddressBook::builder()
            .store(store)
            .spawn()
            .await
            .map_err(|e| TransportError::Backend(format!("address book: {e}")))?;
        let mut endpoint_builder = Endpoint::builder(address_book.clone()).signing_key(signing_key);
        for url in relay_urls {
            endpoint_builder = endpoint_builder.relay_url(url);
        }
        let endpoint = endpoint_builder
            .spawn()
            .await
            .map_err(|e| TransportError::Backend(format!("endpoint: {e}")))?;
        let queues: AlpnQueues = Arc::new(StdMutex::new(HashMap::new()));
        for alpn in &alpns {
            let (tx, rx) = mpsc::unbounded_channel();
            endpoint
                .accept(alpn.as_bytes(), StreamQueueHandler { tx })
                .await
                .map_err(|e| TransportError::Backend(format!("accept register: {e}")))?;
            queues
                .lock()
                .unwrap()
                .insert(alpn.clone(), Arc::new(TokioMutex::new(rx)));
        }

        if let Some(store) = blobs {
            let blobs_protocol = iroh_blobs::BlobsProtocol::new(store.store(), None);
            if let Some((scope, authorizer)) = scoped_blob_authorizer {
                endpoint
                    .accept(
                        iroh_blobs::ALPN,
                        ScopedBlobsProtocol {
                            store: store.store().clone(),
                            scope,
                            authorizer,
                        },
                    )
                    .await
                    .map_err(|e| TransportError::Backend(format!("scoped blobs register: {e}")))?;
            } else if let Some(authorizer) = blob_authorizer {
                endpoint
                    .accept(
                        iroh_blobs::ALPN,
                        AuthorizedBlobsProtocol {
                            inner: blobs_protocol,
                            authorizer,
                        },
                    )
                    .await
                    .map_err(|e| {
                        TransportError::Backend(format!("authorized blobs register: {e}"))
                    })?;
            } else {
                endpoint
                    .accept(iroh_blobs::ALPN, blobs_protocol)
                    .await
                    .map_err(|e| TransportError::Backend(format!("blobs register: {e}")))?;
            }
        }

        // Optional LAN discovery: mDNS populates the address book so peers on
        // the same network are reachable without an explicit `add_peer`.
        let mdns_handle = match mdns {
            Some(mode) => Some(
                MdnsDiscovery::builder(address_book.clone(), endpoint.clone())
                    .mode(mode)
                    .spawn()
                    .await
                    .map_err(|e| TransportError::Backend(format!("mdns: {e}")))?,
            ),
            None => None,
        };

        // Optional internet discovery: random walkers explore outward from the
        // bootstrap nodes already known to the address book, resolving more
        // peers' transport info over time. The handle is held so the walkers
        // keep running for the transport's life.
        let discovery_handle = match discovery {
            Some(config) => Some(
                Discovery::builder(address_book.clone(), endpoint.clone())
                    .config(config)
                    .spawn()
                    .await
                    .map_err(|e| TransportError::Backend(format!("discovery: {e}")))?,
            ),
            None => None,
        };

        // Optional gossip overlay: ephemeral broadcast among peers subscribed to
        // the same space topic. Held so the overlay actor lives for the
        // transport; `subscribe`/`set_topics` drive it.
        let gossip_handle = if gossip {
            Some(
                Gossip::builder(address_book.clone(), endpoint.clone())
                    .spawn()
                    .await
                    .map_err(|e| TransportError::Backend(format!("gossip: {e}")))?,
            )
        } else {
            None
        };

        Ok(Self {
            endpoint,
            address_book,
            peer_id,
            queues,
            _mdns: mdns_handle,
            _discovery: discovery_handle,
            gossip: gossip_handle,
        })
    }

    /// This node's dialable [`EndpointAddr`]: iroh's current candidates (the
    /// machine's real interface addresses, plus a relay when one is reached),
    /// with wildcard binds also rewritten to loopback so in-process peers —
    /// the test pattern — stay dialable even with no network up. Pass to a
    /// peer's [`add_peer`](Self::add_peer), or share as a
    /// [`ticket`](Self::ticket), so they can connect without discovery.
    pub async fn endpoint_addr(&self) -> Result<EndpointAddr, TransportError> {
        let iroh_ep = self
            .endpoint
            .endpoint()
            .await
            .map_err(|e| TransportError::Backend(format!("endpoint(): {e}")))?;
        // iroh discovers its direct (interface) addresses asynchronously just
        // after bind; give it a beat so the address carries the LAN
        // candidates a remote machine actually needs, not only the loopback
        // fallback below.
        let mut addr = iroh_ep.addr();
        for _ in 0..40 {
            if addr.ip_addrs().any(|a| !a.ip().is_loopback()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
            addr = iroh_ep.addr();
        }
        // Wildcard binds are not dialable as-is; add their loopback rewrite
        // so same-machine pairs connect even on an interface-less host.
        for sock in iroh_ep.bound_sockets() {
            let dial = if sock.ip().is_unspecified() {
                let ip = if sock.is_ipv4() {
                    IpAddr::V4(Ipv4Addr::LOCALHOST)
                } else {
                    IpAddr::V6(Ipv6Addr::LOCALHOST)
                };
                SocketAddr::new(ip, sock.port())
            } else {
                sock
            };
            addr = addr.with_ip_addr(dial);
        }
        Ok(addr)
    }

    /// Gracefully stop the underlying iroh endpoint.
    ///
    /// Call this when a bounded transport owner is finished. Dropping an open
    /// iroh endpoint aborts its remaining connections and makes the remote end
    /// report a lost connection even after all application data was delivered.
    pub async fn close(&self) -> Result<(), TransportError> {
        let endpoint = self
            .endpoint
            .endpoint()
            .await
            .map_err(|e| TransportError::Backend(format!("endpoint(): {e}")))?;
        endpoint.close().await;
        Ok(())
    }

    /// Register a peer's [`EndpointAddr`] so connect-by-PeerID resolves without
    /// discovery (DNS / mDNS / random-walk).
    pub async fn add_peer(&self, addr: EndpointAddr) -> Result<(), TransportError> {
        self.address_book
            .insert_node_info(NodeInfo::from(addr))
            .await
            .map_err(|e| TransportError::Backend(format!("insert_node_info: {e}")))?;
        Ok(())
    }

    /// This node's dialable address as a shareable **ticket** string. A peer
    /// pastes it into [`add_peer_ticket`](Self::add_peer_ticket) to dial this node
    /// without discovery — the string-friendly form of [`endpoint_addr`] for an
    /// out-of-band exchange (e.g. a host's "connect to peer" verb).
    ///
    /// [`endpoint_addr`]: Self::endpoint_addr
    pub async fn ticket(&self) -> Result<String, TransportError> {
        let addr = self.endpoint_addr().await?;
        Ok(EndpointTicket::from(addr).to_string())
    }

    /// Parse a peer's [`ticket`](Self::ticket), register its transport info
    /// ([`add_peer`](Self::add_peer)), and return its [`PeerID`] — so the caller
    /// can tag it ([`set_topics`](Self::set_topics)) to bootstrap an overlay.
    pub async fn add_peer_ticket(&self, ticket: &str) -> Result<PeerID, TransportError> {
        let ticket: EndpointTicket = ticket
            .trim()
            .parse()
            .map_err(|e| TransportError::Backend(format!("parse ticket: {e}")))?;
        let addr = EndpointAddr::from(ticket);
        let peer = PeerID::from_bytes(addr.id.as_bytes())
            .map_err(|e| TransportError::Backend(format!("ticket peer id: {e}")))?;
        self.add_peer(addr).await?;
        Ok(peer)
    }

    /// Join a space's gossip overlay (topic = e.g. a cabal / moot id) and return
    /// a [`GossipHandle`] to broadcast bytes to, and receive bytes from, peers
    /// subscribed to the same space. This is the **live-sync** path: peers
    /// converge by publishing their encoded operations here, and ingesting what
    /// they receive. Offline catch-up (RBSR) is `LogSync`, not yet wired.
    ///
    /// Requires [`builder().gossip()`](P2pandaTransportBuilder::gossip). Gossip
    /// bootstraps from address-book peers tagged with this topic (by discovery,
    /// or [`set_topics`](Self::set_topics) for explicit bootstrap), so
    /// tag/announce peers before subscribing.
    pub async fn subscribe(&self, topic: [u8; 32]) -> Result<GossipHandle, TransportError> {
        let gossip = self
            .gossip
            .as_ref()
            .ok_or_else(|| TransportError::Backend("gossip not enabled".to_string()))?;
        gossip
            .stream(Topic::from(topic))
            .await
            .map_err(|e| TransportError::Backend(format!("gossip subscribe: {e}")))
    }

    /// Peers the address book associates with `topic`, however they were
    /// learned: mDNS, a ticket, or an explicit [`add_peer`](Self::add_peer).
    ///
    /// The reporting half of [`set_topics`](Self::set_topics). A host that
    /// pairs on peer id needs to answer "which of my devices can I actually
    /// reach right now", and a peer id alone cannot answer that: the address
    /// book may hold the identity with no transport information yet.
    ///
    /// This node is excluded. The address book registers the local node
    /// against its own subscribed topics, so the raw query returns self as a
    /// reachable peer, which every caller would have to filter out.
    pub async fn peers_for_topic(&self, topic: [u8; 32]) -> Result<Vec<KnownPeer>, TransportError> {
        let infos = self
            .address_book
            .node_infos_by_topics([Topic::from(topic)])
            .await
            .map_err(|e| TransportError::Backend(format!("node_infos_by_topics: {e}")))?;
        let local = self.peer_id.to_bytes();
        // One handle for the whole sweep. A failure to obtain it is reported
        // as "nothing is connected" rather than as an error: the address-book
        // half of this answer is still worth returning, and a caller that
        // cannot tell "not connected" from "could not ask" is exactly the
        // problem this field exists to end.
        let endpoint = self.endpoint.endpoint().await.ok();
        let mut peers = Vec::new();
        for info in infos {
            if info.node_id.as_bytes() == &local {
                continue;
            }
            let peer = PeerID::from_bytes(info.node_id.as_bytes())
                .map_err(|e| TransportError::Backend(format!("peer id: {e}")))?;
            let connected = match &endpoint {
                Some(endpoint) => endpoint
                    .remote_info(
                        iroh::PublicKey::from_bytes(info.node_id.as_bytes()).map_err(|e| {
                            TransportError::Backend(format!("peer key for remote_info: {e}"))
                        })?,
                    )
                    .await
                    .map(|remote| {
                        remote.addrs().any(|addr| {
                            matches!(addr.usage(), iroh::endpoint::TransportAddrUsage::Active)
                        })
                    })
                    .unwrap_or(false),
                None => false,
            };
            peers.push(KnownPeer {
                peer,
                reachable: info.transports.is_some(),
                bootstrap: info.bootstrap,
                connected,
            });
        }
        Ok(peers)
    }

    /// Stop treating `peer` as a member of `topic`'s overlay: the inverse of
    /// [`set_topics`](Self::set_topics) for a single topic.
    ///
    /// The peer stays in the address book, so re-pairing later does not have
    /// to rediscover it. Only the overlay association goes, which is what
    /// unpairing means: the device is still a device, it is no longer on this
    /// graph.
    pub async fn remove_topic(&self, peer: PeerID, topic: [u8; 32]) -> Result<(), TransportError> {
        let node_id = VerifyingKey::from_bytes(&peer.to_bytes())
            .map_err(|e| TransportError::Backend(format!("peer key: {e}")))?;
        self.address_book
            .remove_topic(node_id, Topic::from(topic))
            .await
            .map_err(|e| TransportError::Backend(format!("remove_topic: {e}")))
    }

    /// Tag a known peer as interested in the given topics, so gossip can
    /// bootstrap its overlay to them. In production, discovery does this
    /// confidentially; this is the explicit-bootstrap path (pair with
    /// [`add_peer`](Self::add_peer), which supplies the peer's transport info).
    pub async fn set_topics(
        &self,
        peer: PeerID,
        topics: &[[u8; 32]],
    ) -> Result<(), TransportError> {
        let node_id = VerifyingKey::from_bytes(&peer.to_bytes())
            .map_err(|e| TransportError::Backend(format!("peer key: {e}")))?;
        self.address_book
            .set_topics(node_id, topics.iter().map(|t| Topic::from(*t)))
            .await
            .map_err(|e| TransportError::Backend(format!("set_topics: {e}")))
    }

    /// Tag `peer` with additional overlay topics, keeping its existing set.
    ///
    /// The append form of [`set_topics`](Self::set_topics), which replaces.
    /// Two lanes sharing one endpoint (e.g. a personal graph and its carriage
    /// sibling) must use this for the second lane's tag, or each host clobbers
    /// the other's overlay and neither converges.
    pub async fn add_topics(
        &self,
        peer: PeerID,
        topics: &[[u8; 32]],
    ) -> Result<(), TransportError> {
        let node_id = VerifyingKey::from_bytes(&peer.to_bytes())
            .map_err(|e| TransportError::Backend(format!("peer key: {e}")))?;
        for topic in topics {
            self.address_book
                .add_topic(node_id, Topic::from(*topic))
                .await
                .map_err(|e| TransportError::Backend(format!("add_topic: {e}")))?;
        }
        Ok(())
    }

    /// The endpoint + gossip handles a `LogSync` session needs, as owned clones.
    ///
    /// `None` if the transport was built without
    /// [`gossip`](P2pandaTransportBuilder::gossip) (LogSync rides the gossip
    /// overlay for peer-sampling). `murm` uses these to assemble a LogSync over a
    /// cabal's store for offline catch-up; pair with [`sync_overlay_topic`] +
    /// [`set_topics`](Self::set_topics) to bootstrap the overlay explicitly
    /// (discovery does this in production).
    pub fn sync_parts(&self) -> Option<(Endpoint, Gossip)> {
        self.gossip
            .as_ref()
            .map(|g| (self.endpoint.clone(), g.clone()))
    }

    /// The peer's current dialable address set as a ticket, if the endpoint
    /// holds any addresses for it. `None` when the peer is only a name.
    ///
    /// This is what makes an address survivable: the ticket a peer HANDS OUT
    /// describes itself as of its last bind, but the endpoint's own view of a
    /// peer it has talked to is fresher than any handed-out ticket, because
    /// every path change updates it. Persisting this after real contact is the
    /// cached-address rung of the resolver ladder: a device that has connected
    /// once can redial through the relay after both ends restart, with no
    /// discovery working at all.
    ///
    /// Addresses are sorted before serializing so the same address set always
    /// yields the same ticket string, letting a caller compare tickets to
    /// decide whether anything actually changed.
    pub async fn peer_ticket(&self, peer: PeerID) -> Result<Option<String>, TransportError> {
        let endpoint = self
            .endpoint
            .endpoint()
            .await
            .map_err(|e| TransportError::Backend(format!("endpoint: {e}")))?;
        let id = iroh::PublicKey::from_bytes(&peer.to_bytes())
            .map_err(|e| TransportError::Backend(format!("peer key: {e}")))?;
        let Some(info) = endpoint.remote_info(id).await else {
            return Ok(None);
        };
        let mut addrs: Vec<_> = info.into_addrs().map(|addr| addr.into_addr()).collect();
        if addrs.is_empty() {
            return Ok(None);
        }
        addrs.sort_by_key(|addr| format!("{addr:?}"));
        let addr = EndpointAddr::from_parts(id, addrs);
        Ok(Some(EndpointTicket::from(addr).to_string()))
    }

    /// Open a raw iroh `Connection` to a peer for an arbitrary ALPN.
    ///
    /// Lower-level than [`Transport::connect`]; used by
    /// [`BlobStore::fetch_from`](crate::BlobStore::fetch_from) to drive the
    /// iroh-blobs fetch dance.
    pub async fn connect_raw(
        &self,
        peer: PeerID,
        alpn: &[u8],
    ) -> Result<Connection, TransportError> {
        let node_id = VerifyingKey::from_bytes(&peer.to_bytes())
            .map_err(|e| TransportError::Backend(format!("peer key: {e}")))?;
        self.endpoint
            .connect(node_id, alpn)
            .await
            .map_err(|e| TransportError::Backend(format!("connect: {e}")))
    }
}

impl Transport for P2pandaTransport {
    type Stream = P2pandaStream;

    fn local_peer_id(&self) -> PeerID {
        self.peer_id
    }

    async fn connect(&self, peer: PeerID, alpn: Alpn) -> Result<P2pandaStream, TransportError> {
        let conn = self.connect_raw(peer, alpn.as_bytes()).await?;
        let (send, recv) = conn
            .open_bi()
            .await
            .map_err(|e| TransportError::Backend(format!("open_bi: {e}")))?;
        Ok(P2pandaStream {
            send,
            recv,
            _connection: conn,
            shutdown_ack: None,
            shutdown_finished: false,
            shutdown_complete: false,
        })
    }

    async fn accept(&self, alpn: Alpn) -> Result<AcceptedSession<P2pandaStream>, TransportError> {
        let rx = self
            .queues
            .lock()
            .unwrap()
            .get(&alpn)
            .cloned()
            .ok_or(TransportError::AlpnNotRegistered)?;
        let mut rx = rx.lock().await;
        let queued = rx.recv().await.ok_or(TransportError::Closed)?;
        Ok(AcceptedSession::new(
            queued.stream,
            alpn,
            queued.peer,
            IngressContext::p2panda(),
        ))
    }
}

/// Mix value p2panda-net's sync manager uses to derive the gossip *overlay*
/// topic from a sync topic (replicated from its private `GOSSIP_TOPIC_MIX_VALUE`,
/// generated randomly upstream to avoid collisions between the membership overlay
/// and application gossip on the same topic).
const GOSSIP_TOPIC_MIX_VALUE: [u8; 32] = [
    253, 6, 251, 217, 173, 228, 215, 244, 130, 181, 150, 142, 220, 244, 49, 219, 35, 94, 163, 197,
    229, 93, 143, 227, 97, 61, 38, 202, 63, 250, 26, 233,
];

/// The gossip overlay topic a `LogSync` session actually joins for a given sync
/// topic: `BLAKE3(topic ++ mix)`.
///
/// p2panda-net's sync manager joins gossip on this *derived* topic, not the raw
/// sync topic, so explicit peer-tagging ([`P2pandaTransport::set_topics`]) must
/// use this value for the LogSync overlay to form (tagging the raw topic forms
/// no neighbour link). In production, discovery announces it; this is for
/// explicit / test bootstrap. The upstream constant is private, so it is
/// replicated here; see the redb-logstore probe.
pub fn sync_overlay_topic(sync_topic: [u8; 32]) -> [u8; 32] {
    let hash = p2panda_core::Hash::digest(
        [sync_topic.as_slice(), GOSSIP_TOPIC_MIX_VALUE.as_slice()].concat(),
    );
    *hash.as_bytes()
}

#[cfg(test)]
mod tests;
