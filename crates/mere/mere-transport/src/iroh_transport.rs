//! iroh-backed [`Transport`] implementation.
//!
//! [`IrohTransport`] runs a real iroh `Endpoint` (QUIC + Noise + iroh-base
//! identity), demultiplexes incoming connections through iroh's
//! [`Router`](iroh::protocol::Router) pattern, and routes each accepted
//! bidirectional stream to a per-ALPN queue that [`Transport::accept`]
//! drains. [`Transport::connect`] opens a fresh QUIC connection per call
//! and returns a single bidirectional stream.
//!
//! Optionally serves the **iroh-blobs** and **iroh-gossip** protocols off
//! the same endpoint via the builder. Configure with
//! [`IrohTransport::builder`].
//!
//! ## Status
//!
//! Phase 2C v2. Conscious limitations:
//!
//! - **One bi-stream per connection** for our custom Cable/co-op ALPNs.
//!   `connect()` opens a fresh connection each call; the connection drops
//!   with the returned stream. (iroh-blobs / iroh-gossip / future
//!   Router-registered protocols can use the connection however they
//!   like — multi-stream, etc.)
//! - **Discovery: explicit only.** Peers must be registered via
//!   [`IrohTransport::add_peer`] before they can be reached. n0-DNS
//!   discovery is intentionally not wired up so tests don't accidentally
//!   publish to the public relay network. The discovery toggle is a
//!   follow-up.
//!
//! ## Identity
//!
//! Constructed from an [`mere_identity::Ed25519Keypair`]; the iroh
//! `SecretKey` is built from the raw 32-byte seed via [`Ed25519Keypair::to_seed`].
//! This crosses the ed25519-dalek major-version boundary (mere-identity uses
//! `2.x`; iroh 0.98 uses `3.0-pre`) without forcing either side to upgrade.
//!
//! [`Ed25519Keypair::to_seed`]: mere_identity::Ed25519Keypair::to_seed

use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::task::{Context, Poll};

use std::collections::HashMap;

use iroh::address_lookup::memory::MemoryLookup;
use iroh::endpoint::{Builder, Connection};
use iroh::protocol::{AcceptError, ProtocolHandler, Router, RouterBuilder};
use iroh::{Endpoint, EndpointAddr, SecretKey};
use mere_identity::Ed25519Keypair;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{Mutex as TokioMutex, mpsc};

use crate::blobs::BlobStore;
use crate::{Alpn, NodeId, Transport, TransportError};

/// A bidirectional iroh QUIC stream presented as a single
/// `AsyncRead + AsyncWrite` value.
pub struct IrohStream {
    send: iroh::endpoint::SendStream,
    recv: iroh::endpoint::RecvStream,
    /// Keeps the QUIC connection open for as long as the stream is alive.
    /// Field is unread but load-bearing — its drop closes the connection.
    _connection: Connection,
}

impl std::fmt::Debug for IrohStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IrohStream").finish_non_exhaustive()
    }
}

impl AsyncRead for IrohStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        <iroh::endpoint::RecvStream as AsyncRead>::poll_read(Pin::new(&mut self.recv), cx, buf)
    }
}

impl AsyncWrite for IrohStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        <iroh::endpoint::SendStream as AsyncWrite>::poll_write(Pin::new(&mut self.send), cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        <iroh::endpoint::SendStream as AsyncWrite>::poll_flush(Pin::new(&mut self.send), cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        <iroh::endpoint::SendStream as AsyncWrite>::poll_shutdown(Pin::new(&mut self.send), cx)
    }
}

/// Per-ALPN queue: a sender end held by the `QueueProtocolHandler`, a
/// receiver end drained by [`Transport::accept`].
struct AlpnQueue {
    tx: mpsc::UnboundedSender<IrohStream>,
    rx: Arc<TokioMutex<mpsc::UnboundedReceiver<IrohStream>>>,
}

impl Clone for AlpnQueue {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            rx: Arc::clone(&self.rx),
        }
    }
}

type AlpnMap = Arc<StdMutex<HashMap<Alpn, AlpnQueue>>>;

#[derive(Clone)]
struct QueueProtocolHandler {
    tx: mpsc::UnboundedSender<IrohStream>,
}

impl std::fmt::Debug for QueueProtocolHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueueProtocolHandler")
            .finish_non_exhaustive()
    }
}

impl ProtocolHandler for QueueProtocolHandler {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let (send, recv) = conn.accept_bi().await.map_err(AcceptError::from_err)?;
        let _ = self.tx.send(IrohStream {
            send,
            recv,
            _connection: conn,
        });
        Ok(())
    }
}

/// Builder for [`IrohTransport`].
///
/// Use [`IrohTransport::builder`] to construct.
pub struct IrohTransportBuilder<'a> {
    master: &'a Ed25519Keypair,
    alpns: Vec<Alpn>,
    blobs: Option<&'a BlobStore>,
    enable_gossip: bool,
}

impl<'a> IrohTransportBuilder<'a> {
    /// Add Mere-defined ALPNs to register. Each gets its own per-ALPN
    /// accept queue.
    pub fn alpns(mut self, alpns: Vec<Alpn>) -> Self {
        self.alpns = alpns;
        self
    }

    /// Serve the iroh-blobs protocol against the given store. Peers can
    /// then `fetch_from` this transport's NodeId.
    pub fn blobs(mut self, store: &'a BlobStore) -> Self {
        self.blobs = Some(store);
        self
    }

    /// Serve the iroh-gossip protocol off the same endpoint.
    pub fn gossip(mut self) -> Self {
        self.enable_gossip = true;
        self
    }

    /// Bind the endpoint and spawn the router. Consumes the builder.
    pub async fn bind(self) -> Result<IrohTransport, TransportError> {
        IrohTransport::bind_inner(self.master, self.alpns, self.blobs, self.enable_gossip).await
    }
}

/// iroh-backed [`Transport`].
///
/// Internally runs an iroh [`Router`] that demultiplexes incoming
/// connections by ALPN. Mere-defined ALPNs each get a queue; optional
/// `iroh-blobs` and `iroh-gossip` handlers can be wired in via
/// [`IrohTransport::builder`].
pub struct IrohTransport {
    endpoint: Endpoint,
    node_id: NodeId,
    alpn_queues: AlpnMap,
    address_book: MemoryLookup,
    /// The Router owns the accept loop and the registered protocol
    /// handlers. Held in an `Option` so `Drop` can take it.
    router: Option<Router>,
    /// Gossip handle, when `gossip()` was set on the builder.
    gossip: Option<iroh_gossip::Gossip>,
}

impl IrohTransport {
    /// Start a builder for a new iroh transport.
    pub fn builder(master: &Ed25519Keypair) -> IrohTransportBuilder<'_> {
        IrohTransportBuilder {
            master,
            alpns: Vec::new(),
            blobs: None,
            enable_gossip: false,
        }
    }

    /// Bind a new iroh transport with just the given Mere ALPNs.
    ///
    /// Convenience over [`builder`](Self::builder) for callers that
    /// don't need iroh-blobs / iroh-gossip.
    #[tracing::instrument(
        level = "info",
        skip(master, alpns),
        fields(alpn_count = alpns.len()),
    )]
    pub async fn bind(master: &Ed25519Keypair, alpns: Vec<Alpn>) -> Result<Self, TransportError> {
        Self::builder(master).alpns(alpns).bind().await
    }

    /// Bind a new iroh transport with the given Mere ALPNs and serve
    /// iroh-blobs against the provided store.
    #[tracing::instrument(
        level = "info",
        skip(master, alpns, blobs),
        fields(alpn_count = alpns.len(), with_blobs = blobs.is_some()),
    )]
    pub async fn bind_with_blobs(
        master: &Ed25519Keypair,
        alpns: Vec<Alpn>,
        blobs: Option<&BlobStore>,
    ) -> Result<Self, TransportError> {
        let mut b = Self::builder(master).alpns(alpns);
        if let Some(s) = blobs {
            b = b.blobs(s);
        }
        b.bind().await
    }

    async fn bind_inner(
        master: &Ed25519Keypair,
        alpns: Vec<Alpn>,
        blobs: Option<&BlobStore>,
        enable_gossip: bool,
    ) -> Result<Self, TransportError> {
        let secret_key = SecretKey::from_bytes(&master.to_seed());

        // Tell the endpoint *all* the ALPNs it will serve — including
        // iroh-blobs and iroh-gossip if requested.
        let mut alpn_bytes: Vec<Vec<u8>> = alpns.iter().map(|a| a.as_bytes().to_vec()).collect();
        if blobs.is_some() {
            alpn_bytes.push(iroh_blobs::ALPN.to_vec());
        }
        if enable_gossip {
            alpn_bytes.push(iroh_gossip::ALPN.to_vec());
        }

        // Use iroh's MemoryLookup as the address book — peers added via
        // `add_peer` are stored here; iroh's connect flow consults it
        // automatically. iroh-gossip / iroh-blobs / our own connect_raw
        // all benefit without us re-implementing peer-addr lookup
        // separately for each.
        let address_book = MemoryLookup::new();

        let endpoint = Builder::empty()
            .secret_key(secret_key)
            .alpns(alpn_bytes)
            .crypto_provider(iroh::tls::default_provider())
            .address_lookup(address_book.clone())
            .bind()
            .await
            .map_err(|e| TransportError::Backend(format!("endpoint bind: {e}")))?;

        let endpoint_id = endpoint.id();
        let node_id_bytes: [u8; 32] = *endpoint_id.as_bytes();
        let node_id = NodeId::from_bytes(&node_id_bytes)
            .map_err(|e| TransportError::Backend(format!("derive NodeId: {e:?}")))?;

        // Build the per-ALPN queue map and register a handler per ALPN.
        let alpn_queues: AlpnMap = Arc::new(StdMutex::new(HashMap::new()));
        let mut router_builder: RouterBuilder = Router::builder(endpoint.clone());
        {
            let mut guard = alpn_queues.lock().unwrap();
            for alpn in &alpns {
                let (tx, rx) = mpsc::unbounded_channel();
                guard.insert(
                    alpn.clone(),
                    AlpnQueue {
                        tx: tx.clone(),
                        rx: Arc::new(TokioMutex::new(rx)),
                    },
                );
                router_builder =
                    router_builder.accept(alpn.as_bytes().to_vec(), QueueProtocolHandler { tx });
            }
        }

        if let Some(store) = blobs {
            let blobs_protocol = iroh_blobs::BlobsProtocol::new(store.store(), None);
            router_builder = router_builder.accept(iroh_blobs::ALPN, blobs_protocol);
        }

        let gossip = if enable_gossip {
            let g = iroh_gossip::Gossip::builder().spawn(endpoint.clone());
            router_builder = router_builder.accept(iroh_gossip::ALPN, g.clone());
            Some(g)
        } else {
            None
        };

        let router = router_builder.spawn();

        Ok(Self {
            endpoint,
            node_id,
            alpn_queues,
            address_book,
            router: Some(router),
            gossip,
        })
    }

    /// Local iroh `EndpointAddr` populated from bound UDP sockets.
    ///
    /// Builds the EndpointAddr from `Endpoint::bound_sockets()` directly
    /// rather than `Endpoint::addr()`, so the addresses are usable
    /// immediately after `bind()` without waiting for relay/DNS
    /// initialization. Pass to a peer's
    /// [`add_peer`](IrohTransport::add_peer) so they can connect without
    /// DNS discovery — the test pattern.
    pub fn endpoint_addr(&self) -> EndpointAddr {
        let id = self.endpoint.id();
        let mut addr = EndpointAddr::new(id);
        for sock in self.endpoint.bound_sockets() {
            // Rewrite the wildcard `0.0.0.0` / `[::]` to loopback so the
            // peer can actually dial the address (the wildcard is a bind
            // hint, not a routable destination).
            let dial = if sock.ip().is_unspecified() {
                use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
                let loop_ip: IpAddr = if sock.is_ipv4() {
                    IpAddr::V4(Ipv4Addr::LOCALHOST)
                } else {
                    IpAddr::V6(Ipv6Addr::LOCALHOST)
                };
                SocketAddr::new(loop_ip, sock.port())
            } else {
                sock
            };
            addr = addr.with_ip_addr(dial);
        }
        addr
    }

    /// Register a peer's iroh `EndpointAddr` with the local transport.
    ///
    /// Adds the addr to iroh's `MemoryLookup` so subsequent connect calls
    /// (ours, plus iroh-blobs / iroh-gossip / any other protocol that
    /// uses the endpoint's address book) can resolve the peer by NodeId
    /// alone.
    #[tracing::instrument(level = "debug", skip(self, addr))]
    pub fn add_peer(&self, addr: EndpointAddr) -> Result<(), TransportError> {
        self.address_book.add_endpoint_info(addr);
        Ok(())
    }

    /// Open a raw iroh `Connection` to a peer for an arbitrary ALPN.
    ///
    /// Lower-level than [`Transport::connect`] — returns the
    /// `iroh::Connection` directly so callers can drive it however the
    /// target protocol requires. Used by [`BlobStore::fetch_from`] to
    /// run iroh-blobs' multi-stream fetch dance.
    #[tracing::instrument(
        level = "debug",
        skip(self),
        fields(?peer, alpn_len = alpn.len()),
    )]
    pub async fn connect_raw(
        &self,
        peer: NodeId,
        alpn: &[u8],
    ) -> Result<Connection, TransportError> {
        let endpoint_id = iroh::EndpointId::from_bytes(&peer.to_bytes())
            .map_err(|e| TransportError::Backend(format!("peer EndpointId: {e:?}")))?;
        // Pass just the EndpointId — iroh's MemoryLookup (registered at
        // build time and populated by `add_peer`) fills in the addresses.
        self.endpoint
            .connect(endpoint_id, alpn)
            .await
            .map_err(|e| TransportError::Backend(format!("connect: {e}")))
    }

    /// The `iroh-gossip` handle, present iff the transport was built
    /// with [`IrohTransportBuilder::gossip`]. Use to subscribe to topics
    /// and broadcast messages.
    pub fn gossip(&self) -> Option<&iroh_gossip::Gossip> {
        self.gossip.as_ref()
    }
}

impl Drop for IrohTransport {
    fn drop(&mut self) {
        // The Router aborts its accept loop when dropped (per iroh docs).
        // Graceful shutdown would be `router.shutdown().await`, but Drop
        // is sync — for v0 the abort is acceptable.
        drop(self.router.take());
    }
}

impl Transport for IrohTransport {
    type Stream = IrohStream;

    fn local_node_id(&self) -> NodeId {
        self.node_id
    }

    async fn connect(&self, peer: NodeId, alpn: Alpn) -> Result<Self::Stream, TransportError> {
        let connection = self.connect_raw(peer, alpn.as_bytes()).await?;

        let (send, recv) = connection
            .open_bi()
            .await
            .map_err(|e| TransportError::Backend(format!("open_bi: {e}")))?;

        Ok(IrohStream {
            send,
            recv,
            _connection: connection,
        })
    }

    async fn accept(&self, alpn: Alpn) -> Result<Self::Stream, TransportError> {
        let queue = self
            .alpn_queues
            .lock()
            .unwrap()
            .get(&alpn)
            .cloned()
            .ok_or(TransportError::AlpnNotRegistered)?;

        let mut rx = queue.rx.lock().await;
        rx.recv().await.ok_or(TransportError::Closed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mere_identity::{IdentityProvider, InMemoryProvider};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn make_transport_inputs(seed: u8) -> (Ed25519Keypair, NodeId) {
        let provider = InMemoryProvider::from_seed([seed; 32]);
        let kp = provider.master_keypair().clone();
        let node_id = NodeId::from_public_key(provider.master_public_key());
        (kp, node_id)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn paired_iroh_transports_round_trip_bytes() {
        let alpn = Alpn::new("mere/test/v1");

        let (alice_kp, alice_id) = make_transport_inputs(1);
        let (bob_kp, bob_id) = make_transport_inputs(2);

        let alice = IrohTransport::bind(&alice_kp, vec![alpn.clone()])
            .await
            .expect("bind alice");
        let bob = IrohTransport::bind(&bob_kp, vec![alpn.clone()])
            .await
            .expect("bind bob");

        alice.add_peer(bob.endpoint_addr()).expect("alice.add_peer");
        bob.add_peer(alice.endpoint_addr()).expect("bob.add_peer");

        assert_eq!(alice.local_node_id(), alice_id);
        assert_eq!(bob.local_node_id(), bob_id);

        let payload = b"hello over iroh".to_vec();
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
        let alpn_registered = Alpn::new("mere/test/v1");
        let alpn_other = Alpn::new("mere/other/v1");

        let (kp, _) = make_transport_inputs(7);
        let t = IrohTransport::bind(&kp, vec![alpn_registered])
            .await
            .expect("bind");

        let err = t.accept(alpn_other).await.expect_err("must error");
        assert!(matches!(err, TransportError::AlpnNotRegistered));
    }

    /// End-to-end iroh-gossip: alice and bob join the same topic; alice
    /// broadcasts a message; bob receives it.
    ///
    /// Validates: gossip's ProtocolHandler integration with our Router,
    /// MemoryLookup's role in connect-by-NodeId from inside iroh-gossip's
    /// internal connect path, and the gossip API surface exposed via
    /// [`IrohTransport::gossip`].
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn paired_iroh_transports_exchange_gossip_message() {
        use bytes::Bytes;
        use iroh_gossip::proto::TopicId;

        let (alice_kp, _) = make_transport_inputs(33);
        let (bob_kp, _) = make_transport_inputs(44);

        let alice = IrohTransport::builder(&alice_kp)
            .gossip()
            .bind()
            .await
            .expect("alice bind");
        let bob = IrohTransport::builder(&bob_kp)
            .gossip()
            .bind()
            .await
            .expect("bob bind");

        // Cross-register addresses (no DNS).
        alice.add_peer(bob.endpoint_addr()).unwrap();
        bob.add_peer(alice.endpoint_addr()).unwrap();

        let topic = TopicId::from_bytes([0xaa; 32]);

        // Bob's EndpointId for alice's bootstrap list, and vice versa.
        let alice_eid = iroh::EndpointId::from_bytes(&alice.local_node_id().to_bytes()).unwrap();
        let bob_eid = iroh::EndpointId::from_bytes(&bob.local_node_id().to_bytes()).unwrap();

        // Bob subscribes (with alice as bootstrap) in a task and waits
        // for the first message.
        let bob_recv = {
            let bob_gossip = bob.gossip().unwrap().clone();
            tokio::spawn(async move {
                use tokio_stream::StreamExt;
                let mut topic_handle = bob_gossip
                    .subscribe(topic, vec![alice_eid])
                    .await
                    .expect("bob subscribe");
                topic_handle.joined().await.expect("bob joined");
                while let Some(event) = topic_handle.next().await {
                    let event = event.expect("event");
                    if let iroh_gossip::api::Event::Received(msg) = event {
                        return msg.content.to_vec();
                    }
                }
                panic!("bob never received a message");
            })
        };

        // Alice subscribes (with bob as bootstrap), waits to be joined,
        // then broadcasts.
        let mut alice_topic = alice
            .gossip()
            .unwrap()
            .subscribe(topic, vec![bob_eid])
            .await
            .expect("alice subscribe");
        alice_topic.joined().await.expect("alice joined");
        alice_topic
            .broadcast(Bytes::from_static(b"gossip from alice"))
            .await
            .expect("broadcast");

        let bytes = bob_recv.await.expect("bob task");
        assert_eq!(bytes, b"gossip from alice");
    }
}
