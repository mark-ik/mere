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
//! Constructed from an [`identity::Ed25519Keypair`]; the p2panda `SigningKey` is
//! built from the raw 32-byte seed via [`Ed25519Keypair::to_seed`], bridging the
//! ed25519-dalek major-version boundary without forcing either side to upgrade.
//!
//! ## Discovery
//!
//! Three mechanisms share the one endpoint, each opt-in via the builder:
//!
//! - **Explicit peer** (tests, LAN): a peer's loopback [`endpoint_addr`] is
//!   shared out-of-band and inserted via [`add_peer`].
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
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::task::{Context, Poll};

use identity::Ed25519Keypair;
use iroh::EndpointAddr;
use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::protocol::{AcceptError, ProtocolHandler};
use p2panda_core::{SigningKey, Topic, VerifyingKey};
use p2panda_net::addrs::NodeInfo;
use p2panda_net::discovery::DiscoveryConfig;
use p2panda_net::gossip::{Gossip, GossipHandle};
use p2panda_net::iroh_mdns::MdnsDiscoveryMode;
use p2panda_net::{AddressBook, Discovery, Endpoint, MdnsDiscovery};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{Mutex as TokioMutex, mpsc};

use crate::blobs::BlobStore;
use crate::{Alpn, PeerID, Transport, TransportError};

/// A bidirectional p2panda-net QUIC stream presented as `AsyncRead + AsyncWrite`.
///
/// `_connection` is held so the QUIC connection stays open for the stream's life.
pub struct P2pandaStream {
    send: SendStream,
    recv: RecvStream,
    _connection: Connection,
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
        <SendStream as AsyncWrite>::poll_shutdown(Pin::new(&mut self.send), cx)
    }
}

/// Registered per ALPN on the p2panda-net endpoint; pushes each accepted
/// bi-stream to a queue that [`Transport::accept`] drains.
#[derive(Debug, Clone)]
struct StreamQueueHandler {
    tx: mpsc::UnboundedSender<P2pandaStream>,
}

impl ProtocolHandler for StreamQueueHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let (send, recv) = connection.accept_bi().await.map_err(AcceptError::from_err)?;
        let _ = self.tx.send(P2pandaStream {
            send,
            recv,
            _connection: connection,
        });
        Ok(())
    }
}

type AlpnQueues =
    Arc<StdMutex<HashMap<Alpn, Arc<TokioMutex<mpsc::UnboundedReceiver<P2pandaStream>>>>>>;

/// Builder for [`P2pandaTransport`]. Use [`P2pandaTransport::builder`].
pub struct P2pandaTransportBuilder<'a> {
    master: &'a Ed25519Keypair,
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
    pub fn blobs(mut self, store: &'a BlobStore) -> Self {
        self.blobs = Some(store);
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
            self.master,
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
            master,
            alpns: Vec::new(),
            blobs: None,
            mdns: None,
            discovery: None,
            gossip: false,
        }
    }

    /// Bind with just the given Mere ALPNs (no discovery; explicit `add_peer`).
    pub async fn bind(master: &Ed25519Keypair, alpns: Vec<Alpn>) -> Result<Self, TransportError> {
        Self::bind_inner(master, alpns, None, None, None, false).await
    }

    /// Bind with the given ALPNs and serve iroh-blobs against the provided store.
    pub async fn bind_with_blobs(
        master: &Ed25519Keypair,
        alpns: Vec<Alpn>,
        blobs: Option<&BlobStore>,
    ) -> Result<Self, TransportError> {
        Self::bind_inner(master, alpns, blobs, None, None, false).await
    }

    async fn bind_inner(
        master: &Ed25519Keypair,
        alpns: Vec<Alpn>,
        blobs: Option<&BlobStore>,
        mdns: Option<MdnsDiscoveryMode>,
        discovery: Option<DiscoveryConfig>,
        gossip: bool,
    ) -> Result<Self, TransportError> {
        let signing_key = SigningKey::from_bytes(&master.to_seed());
        let address_book = AddressBook::builder()
            .spawn()
            .await
            .map_err(|e| TransportError::Backend(format!("address book: {e}")))?;
        let endpoint = Endpoint::builder(address_book.clone())
            .signing_key(signing_key)
            .spawn()
            .await
            .map_err(|e| TransportError::Backend(format!("endpoint: {e}")))?;
        let peer_id = PeerID::from_public_key(master.public_key());

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

    /// This node's dialable loopback [`EndpointAddr`]. Pass to a peer's
    /// [`add_peer`](Self::add_peer) so they can connect without discovery — the
    /// test pattern. Wildcard bind addresses are rewritten to loopback.
    pub async fn endpoint_addr(&self) -> Result<EndpointAddr, TransportError> {
        let iroh_ep = self
            .endpoint
            .endpoint()
            .await
            .map_err(|e| TransportError::Backend(format!("endpoint(): {e}")))?;
        let mut addr = EndpointAddr::new(iroh_ep.id());
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
        })
    }

    async fn accept(&self, alpn: Alpn) -> Result<P2pandaStream, TransportError> {
        let rx = self
            .queues
            .lock()
            .unwrap()
            .get(&alpn)
            .cloned()
            .ok_or(TransportError::AlpnNotRegistered)?;
        let mut rx = rx.lock().await;
        rx.recv().await.ok_or(TransportError::Closed)
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
mod tests {
    use super::*;
    use identity::{IdentityProvider, InMemoryProvider};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn make_inputs(seed: u8) -> (Ed25519Keypair, PeerID) {
        let provider = InMemoryProvider::from_seed([seed; 32]);
        let kp = provider.master_keypair().clone();
        let peer_id = PeerID::from_public_key(provider.master_public_key());
        (kp, peer_id)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn paired_p2panda_transports_round_trip_bytes() {
        let alpn = Alpn::new("mere/test/v1");
        let (alice_kp, alice_id) = make_inputs(1);
        let (bob_kp, bob_id) = make_inputs(2);

        let alice = P2pandaTransport::bind(&alice_kp, vec![alpn.clone()])
            .await
            .expect("bind alice");
        let bob = P2pandaTransport::bind(&bob_kp, vec![alpn.clone()])
            .await
            .expect("bind bob");

        alice
            .add_peer(bob.endpoint_addr().await.unwrap())
            .await
            .expect("alice.add_peer");
        bob.add_peer(alice.endpoint_addr().await.unwrap())
            .await
            .expect("bob.add_peer");

        assert_eq!(alice.local_peer_id(), alice_id);
        assert_eq!(bob.local_peer_id(), bob_id);

        let payload = b"hello over p2panda-net".to_vec();
        let reply = b"hi back".to_vec();

        let alice_task = {
            let alpn = alpn.clone();
            let payload = payload.clone();
            let reply_len = reply.len();
            tokio::spawn(async move {
                let mut s = alice.connect(bob_id, alpn).await.expect("connect");
                s.write_all(&payload).await.expect("alice write");
                s.flush().await.expect("alice flush");
                let mut buf = vec![0u8; reply_len];
                s.read_exact(&mut buf).await.expect("alice read");
                buf
            })
        };

        let mut bob_stream = bob.accept(alpn).await.expect("bob accept");
        let mut buf = vec![0u8; payload.len()];
        bob_stream.read_exact(&mut buf).await.expect("bob read");
        assert_eq!(buf, payload);
        bob_stream.write_all(&reply).await.expect("bob write");
        bob_stream.flush().await.expect("bob flush");

        let alice_recv = alice_task.await.expect("alice task");
        assert_eq!(alice_recv, reply);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unregistered_alpn_accept_returns_error() {
        let (kp, _) = make_inputs(7);
        let t = P2pandaTransport::bind(&kp, vec![Alpn::new("mere/test/v1")])
            .await
            .expect("bind");
        let err = t
            .accept(Alpn::new("mere/other/v1"))
            .await
            .expect_err("must error");
        assert!(matches!(err, TransportError::AlpnNotRegistered));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn binds_with_mdns_discovery() {
        // The discovery capability wires up: the mDNS service spawns off the
        // same endpoint without error. (End-to-end LAN discovery is multi-host
        // and not deterministically testable on a single machine.)
        let (kp, _) = make_inputs(9);
        let t = P2pandaTransport::builder(&kp)
            .alpns(vec![Alpn::new("mere/cable/v1")])
            .mdns(MdnsDiscoveryMode::Active)
            .bind()
            .await
            .expect("bind with mdns");
        assert!(t.endpoint_addr().await.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn binds_with_random_walk_discovery() {
        // The random-walk discovery service spawns off the same endpoint and
        // its walker actors stay alive on the held handle. (End-to-end
        // discovery needs a populated bootstrap set + reachable peers, which is
        // a multi-host deployment concern, not single-machine-deterministic.)
        let (kp, _) = make_inputs(11);
        let t = P2pandaTransport::builder(&kp)
            .alpns(vec![Alpn::new("mere/cable/v1")])
            .discovery()
            .bind()
            .await
            .expect("bind with random-walk discovery");
        assert!(t.endpoint_addr().await.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn binds_with_mdns_and_random_walk_together() {
        // LAN (mDNS) and internet (random-walk) discovery coexist on one
        // endpoint; both handles are held.
        let (kp, _) = make_inputs(12);
        let cfg = DiscoveryConfig {
            random_walkers_count: 4,
            ..DiscoveryConfig::default()
        };
        let t = P2pandaTransport::builder(&kp)
            .alpns(vec![Alpn::new("mere/cable/v1")])
            .mdns(MdnsDiscoveryMode::Active)
            .discovery_config(cfg)
            .bind()
            .await
            .expect("bind with mdns + random-walk");
        assert!(t.endpoint_addr().await.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn gossip_propagates_ops_between_subscribed_peers() {
        use tokio_stream::StreamExt;

        let topic = [0x5e; 32];
        let (alice_kp, alice_id) = make_inputs(20);
        let (bob_kp, bob_id) = make_inputs(21);

        let alice = P2pandaTransport::builder(&alice_kp)
            .gossip()
            .bind()
            .await
            .expect("bind alice");
        let bob = P2pandaTransport::builder(&bob_kp)
            .gossip()
            .bind()
            .await
            .expect("bind bob");

        // Explicit bootstrap: cross-register transport addresses and tag the
        // topic so gossip can form the overlay. (Discovery does this in
        // production; here we seed it deterministically.)
        alice
            .add_peer(bob.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        alice.set_topics(bob_id, &[topic]).await.unwrap();
        bob.add_peer(alice.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        bob.set_topics(alice_id, &[topic]).await.unwrap();

        // Both subscribe to the space topic.
        let alice_handle = alice.subscribe(topic).await.expect("alice subscribe");
        let bob_handle = bob.subscribe(topic).await.expect("bob subscribe");
        let mut bob_rx = bob_handle.subscribe();

        // Publish until bob receives (the gossip overlay needs a moment to form
        // the neighbour link), bounded by a timeout. This is a real convergence
        // proof: bytes Alice broadcasts reach Bob over the gossip overlay.
        let payload = b"a synced cabal operation".to_vec();
        let received = tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                alice_handle.publish(payload.clone()).await.expect("publish");
                tokio::select! {
                    msg = bob_rx.next() => {
                        if let Some(Ok(bytes)) = msg
                            && bytes == payload
                        {
                            break bytes;
                        }
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {}
                }
            }
        })
        .await
        .expect("bob received the gossip-broadcast operation within the timeout");

        assert_eq!(received, payload, "bob received exactly what alice published");
    }
}
