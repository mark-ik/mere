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
use p2panda_net::gossip::{Gossip, GossipHandle};
use p2panda_net::iroh_mdns::MdnsDiscoveryMode;
use p2panda_net::{AddressBook, Discovery, Endpoint, MdnsDiscovery};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{Mutex as TokioMutex, mpsc};

use crate::blobs::BlobStore;
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

type AlpnQueues =
    Arc<StdMutex<HashMap<Alpn, Arc<TokioMutex<mpsc::UnboundedReceiver<QueuedStream>>>>>>;

/// Builder for [`P2pandaTransport`]. Use [`P2pandaTransport::builder`].
pub struct P2pandaTransportBuilder<'a> {
    signing_seed: [u8; 32],
    alpns: Vec<Alpn>,
    blobs: Option<&'a BlobStore>,
    mdns: Option<MdnsDiscoveryMode>,
    discovery: Option<DiscoveryConfig>,
    gossip: bool,
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
            mdns: self.mdns,
            discovery: self.discovery,
            gossip: self.gossip,
        }
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
            self.mdns,
            self.discovery,
            self.gossip,
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
            mdns: None,
            discovery: None,
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
            mdns: None,
            discovery: None,
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
        Self::bind_inner(signing_seed, alpns, None, None, None, false).await
    }

    /// Bind with the given ALPNs and serve iroh-blobs against the provided store.
    pub async fn bind_with_blobs(
        master: &Ed25519Keypair,
        alpns: Vec<Alpn>,
        blobs: Option<&BlobStore>,
    ) -> Result<Self, TransportError> {
        Self::bind_inner(master.to_seed(), alpns, blobs, None, None, false).await
    }

    async fn bind_inner(
        signing_seed: [u8; 32],
        alpns: Vec<Alpn>,
        blobs: Option<&BlobStore>,
        mdns: Option<MdnsDiscoveryMode>,
        discovery: Option<DiscoveryConfig>,
        gossip: bool,
    ) -> Result<Self, TransportError> {
        let signing_key = SigningKey::from_bytes(&signing_seed);
        let peer_id = PeerID::from_bytes(signing_key.verifying_key().as_bytes())
            .map_err(|error| TransportError::Backend(format!("transport key: {error}")))?;
        let address_book = AddressBook::builder()
            .spawn()
            .await
            .map_err(|e| TransportError::Backend(format!("address book: {e}")))?;
        let endpoint = Endpoint::builder(address_book.clone())
            .signing_key(signing_key)
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
            endpoint
                .accept(iroh_blobs::ALPN, blobs_protocol)
                .await
                .map_err(|e| TransportError::Backend(format!("blobs register: {e}")))?;
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
