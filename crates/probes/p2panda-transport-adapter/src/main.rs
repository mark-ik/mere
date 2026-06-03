//! First build slice: a `P2pandaTransport: transport::Transport` adapter over
//! `p2panda-net::Endpoint`, validated by running the REAL `murm` Cable snapshot
//! sync over it between two nodes on loopback.
//!
//! This is `IrohTransport`'s structure (an AsyncRead+AsyncWrite stream wrapper
//! plus a per-ALPN accept queue fed by an iroh `ProtocolHandler`), but the
//! endpoint authority is `p2panda-net::Endpoint` instead of our hand-rolled iroh
//! `Router`. The unchanged murm + Cable code runs over it via the `Transport`
//! trait, which proves the "replace the router, keep the behavior" path.
//!
//! No live-crate edits: this is a throwaway probe path-depending on the real
//! crates. The live dependency swap (adding p2panda-net to the `transport`
//! crate, retiring the iroh Router) is gated on Mark's go-ahead.
//!
//! See design_docs/mere_docs/implementation_strategy/2026-06-01_p2panda_substrate_spike_plan.md.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::task::{Context, Poll};

use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};
use iroh::EndpointAddr;
use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::protocol::{AcceptError, ProtocolHandler};
use murm::{CabalKey, Murm, verify_post};
use p2panda_core::{SigningKey, VerifyingKey};
use p2panda_net::addrs::NodeInfo;
use p2panda_net::{AddressBook, Endpoint};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{Mutex as TokioMutex, mpsc};
use transport::{Alpn, PeerID, Transport, TransportError};

/// A bidirectional p2panda-net stream presented as `AsyncRead + AsyncWrite`.
/// `_connection` is held so the QUIC connection stays open for the stream's life.
struct P2pandaStream {
    send: SendStream,
    recv: RecvStream,
    _connection: Connection,
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
/// bi-stream to a queue that `Transport::accept` drains.
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

/// `transport::Transport` over `p2panda-net::Endpoint`.
struct P2pandaTransport {
    endpoint: Endpoint,
    address_book: AddressBook,
    peer_id: PeerID,
    queues: AlpnQueues,
}

impl P2pandaTransport {
    /// Build over a p2panda-net endpoint whose identity is the Mere master key
    /// (bridged via the raw 32-byte ed25519 seed, the same trick `IrohTransport`
    /// uses), registering an accept queue per ALPN.
    async fn bind(master: &Ed25519Keypair, alpns: Vec<Alpn>) -> Result<Self, TransportError> {
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

        Ok(Self {
            endpoint,
            address_book,
            peer_id,
            queues,
        })
    }

    /// This node's `NodeInfo` (loopback address), to hand to a peer's address book.
    async fn node_info(&self) -> Result<NodeInfo, TransportError> {
        let iroh_ep = self
            .endpoint
            .endpoint()
            .await
            .map_err(|e| TransportError::Backend(format!("endpoint(): {e}")))?;
        Ok(NodeInfo::from(loopback_addr(&iroh_ep)))
    }

    /// Seed the address book so connect-by-PeerID resolves without discovery.
    async fn add_peer(&self, info: NodeInfo) -> Result<(), TransportError> {
        self.address_book
            .insert_node_info(info)
            .await
            .map_err(|e| TransportError::Backend(format!("insert_node_info: {e}")))?;
        Ok(())
    }
}

impl Transport for P2pandaTransport {
    type Stream = P2pandaStream;

    fn local_peer_id(&self) -> PeerID {
        self.peer_id
    }

    async fn connect(&self, peer: PeerID, alpn: Alpn) -> Result<P2pandaStream, TransportError> {
        let node_id = VerifyingKey::from_bytes(&peer.to_bytes())
            .map_err(|e| TransportError::Backend(format!("peer key: {e}")))?;
        let conn = self
            .endpoint
            .connect(node_id, alpn.as_bytes())
            .await
            .map_err(|e| TransportError::Backend(format!("connect: {e}")))?;
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

/// Wildcard-to-loopback rewrite, same as `IrohTransport::endpoint_addr`.
fn loopback_addr(ep: &iroh::Endpoint) -> EndpointAddr {
    let mut addr = EndpointAddr::new(ep.id());
    for sock in ep.bound_sockets() {
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
    addr
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cable_alpn = Alpn::new("mere/cable/v1");

    let alice_provider = InMemoryProvider::from_seed([101; 32]);
    let bob_provider = InMemoryProvider::from_seed([202; 32]);
    let alice_kp = alice_provider.master_keypair().clone();
    let bob_kp = bob_provider.master_keypair().clone();

    let alice_t = P2pandaTransport::bind(&alice_kp, vec![cable_alpn.clone()]).await?;
    let bob_t = P2pandaTransport::bind(&bob_kp, vec![cable_alpn.clone()]).await?;

    // Cross-register loopback node info so connect-by-PeerID works (no discovery).
    alice_t.add_peer(bob_t.node_info().await?).await?;
    bob_t.add_peer(alice_t.node_info().await?).await?;

    let bob_node = bob_t.local_peer_id();

    let alice_provider_arc: Arc<dyn IdentityProvider> = Arc::new(alice_provider);
    let bob_provider_arc: Arc<dyn IdentityProvider> = Arc::new(bob_provider);

    // Real Murm over the adapter — unchanged murm/Cable code.
    let alice = Murm::new(alice_provider_arc, alice_t);
    let bob = Arc::new(Murm::new(bob_provider_arc, bob_t));

    let cabal_key = CabalKey::new([0xcd; 32]);
    let alice_cabal = alice.open_cabal(&cabal_key)?;
    let bob_cabal = bob.open_cabal(&cabal_key)?;
    assert_eq!(alice_cabal.id(), bob_cabal.id());

    alice_cabal.send_text_at("session", "uno", 1)?;
    alice_cabal.send_text_at("session", "dos", 2)?;
    alice_cabal.send_text_at("session", "tres", 3)?;

    let bob_for_accept = Arc::clone(&bob);
    let bob_task = tokio::spawn(async move { bob_for_accept.accept_cable_connection().await });

    let cabal_id = *alice_cabal.id();
    let pushed = alice.push_cabal_to_peer(bob_node, &cabal_id).await?;
    let ingested = bob_task.await??;

    println!("alice pushed {pushed} posts; bob ingested {ingested}");
    assert_eq!(pushed, 3, "alice pushed 3 posts");
    assert_eq!(ingested, 3, "bob ingested 3 posts");

    let bob_history = bob_cabal.history("session");
    assert_eq!(bob_history.len(), 3, "bob's history mirrors alice's");
    for post in bob_history {
        assert!(verify_post(&post), "ingested post signature verifies");
    }

    println!("\n--- P2pandaTransport adapter VERDICT ---");
    println!("Real Murm<P2pandaTransport> ran a Cable cabal snapshot sync between two nodes:");
    println!("alice authored 3 posts, pushed over p2panda-net::Endpoint on mere/cable/v1, bob");
    println!("accepted + ingested all 3, signatures verify. Unchanged murm + Cable code works");
    println!("over p2panda-net via the Transport trait. Endpoint authority is now");
    println!("p2panda-net::Endpoint (gains discovery + sync); the hand-rolled iroh Router retires.");
    Ok(())
}
