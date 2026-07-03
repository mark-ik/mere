//! Reticulum-backed [`Transport`] implementation.
//!
//! [`ReticulumTransport`] runs Mere's bilateral peer-to-peer lane over Reticulum
//! packet links (the Beechat Rust port, crate [`reticulum`]). It is feature-gated
//! behind the `reticulum` cargo feature and is default-off. Sync (gossip / RBSR /
//! LogSync) and blob transfer remain iroh-only; this is the bilateral stream lane
//! only.
//!
//! ## Identity
//!
//! Mere's master key is a single Ed25519 keypair; Reticulum needs a dual-key
//! identity (X25519 ECDH + Ed25519 signing). [`keys::derive_identity`] stretches
//! the 32-byte master seed into both keys via HKDF-SHA256, so the same seed always
//! yields the same Reticulum destination. The Mere [`PeerID`] stays the master
//! Ed25519 public key, consistent with the other transports.
//!
//! ## Discovery
//!
//! A destination hash cannot be synthesized from a [`PeerID`], so peers are learned
//! from authenticated announces (see [`announce`]). `connect` resolves a peer by
//! looking it up in the announce-populated address book; `accept` waits for an
//! inbound link on the ALPN's destination.
//!
//! ## ALPN mapping
//!
//! `mere/cable/v1` maps to `DestinationName::new("mere", "cable.v1")`: the first
//! path segment is the Reticulum app name, the rest the dotted aspect.
//!
//! [`reticulum`]: https://crates.io/crates/reticulum

mod announce;
mod keys;
mod stream;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use identity::Ed25519Keypair;
use tokio::sync::{Mutex as TokioMutex, broadcast};
use tokio::time::{Instant, sleep, timeout};

use reticulum::destination::link::{LinkEvent, LinkEventData, LinkId, LinkStatus};
use reticulum::destination::{DestinationDesc, DestinationName, SingleInputDestination};
use reticulum::hash::AddressHash;
use reticulum::transport::{Transport as ReticulumStack, TransportConfig};

use crate::{Alpn, PeerID, Transport, TransportError};

use self::announce::{
    NameHashKey, announce_listener, announce_sender, build_app_data, name_hash_key,
};
use self::keys::derive_identity;
use self::stream::{LinkSide, bridge_link};

pub use self::stream::ReticulumStream;

/// Default interval between periodic re-announces of registered destinations.
const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(5);

/// Default time `connect` waits for peer discovery + link activation.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// A network interface to attach to a [`ReticulumTransport`].
#[derive(Clone, Debug)]
pub enum ReticulumInterface {
    /// TCP server accepting incoming connections.
    TcpServer {
        /// Address to bind, e.g. `127.0.0.1:4242`.
        bind: SocketAddr,
    },
    /// TCP client dialing a peer server.
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

/// Builder for [`ReticulumTransport`].
pub struct ReticulumTransportBuilder<'a> {
    master: &'a Ed25519Keypair,
    alpns: Vec<Alpn>,
    interfaces: Vec<ReticulumInterface>,
    announce_interval: Duration,
    connect_timeout: Duration,
}

impl<'a> ReticulumTransportBuilder<'a> {
    /// Start building a transport from the given master keypair.
    pub fn new(master: &'a Ed25519Keypair) -> Self {
        Self {
            master,
            alpns: Vec::new(),
            interfaces: Vec::new(),
            announce_interval: ANNOUNCE_INTERVAL,
            connect_timeout: CONNECT_TIMEOUT,
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

    /// How often to re-announce registered destinations.
    pub fn announce_interval(mut self, interval: Duration) -> Self {
        self.announce_interval = interval;
        self
    }

    /// How long `connect` waits for peer discovery + link activation.
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Bind the transport: create the Reticulum stack, attach interfaces,
    /// register destinations, and start the announce listener/sender.
    pub async fn bind(self) -> Result<ReticulumTransport, TransportError> {
        ReticulumTransport::bind_inner(
            self.master,
            self.alpns,
            self.interfaces,
            self.announce_interval,
            self.connect_timeout,
        )
        .await
    }
}

/// Reticulum-backed implementation of the [`Transport`] trait.
pub struct ReticulumTransport {
    local_peer_id: PeerID,
    inner: Arc<ReticulumStack>,
    destinations: Arc<StdMutex<HashMap<Alpn, Arc<TokioMutex<SingleInputDestination>>>>>,
    app_data_by_alpn: Arc<HashMap<Alpn, Vec<u8>>>,
    peers: Arc<StdMutex<HashMap<PeerID, HashMap<Alpn, DestinationDesc>>>>,
    connect_timeout: Duration,
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
        connect_timeout: Duration,
    ) -> Result<Self, TransportError> {
        let local_peer_id = PeerID::from_public_key(master.public_key());
        let private_identity = derive_identity(master);

        let config = TransportConfig::new("mere", &private_identity, true);
        // Owned mutably until every destination is registered: `add_destination`
        // takes `&mut self`, which is unreachable once the stack is behind `Arc`.
        let mut inner = ReticulumStack::new(config);

        // Attach interfaces.
        {
            let iface_manager = inner.iface_manager();
            let mut manager = iface_manager.lock().await;
            for iface in interfaces {
                match iface {
                    ReticulumInterface::TcpServer { bind } => {
                        use reticulum::iface::tcp_server::TcpServer;
                        manager.spawn(
                            TcpServer::new(bind.to_string(), inner.iface_manager()),
                            TcpServer::spawn,
                        );
                    }
                    ReticulumInterface::TcpClient { addr } => {
                        use reticulum::iface::tcp_client::TcpClient;
                        manager.spawn(TcpClient::new(addr.to_string()), TcpClient::spawn);
                    }
                    ReticulumInterface::Udp { bind, forward } => {
                        use reticulum::iface::udp::UdpInterface;
                        let forward = forward.map(|a| a.to_string());
                        manager.spawn(
                            UdpInterface::new(bind.to_string(), forward),
                            UdpInterface::spawn,
                        );
                    }
                }
            }
        }

        // Register one incoming destination per ALPN, and precompute each
        // destination's signed announce payload.
        let mut destinations = HashMap::new();
        let mut name_to_alpn: HashMap<NameHashKey, Alpn> = HashMap::new();
        let mut app_data_by_alpn: HashMap<Alpn, Vec<u8>> = HashMap::new();
        for alpn in &alpns {
            let name = destination_name_for_alpn(alpn);
            let dest = inner.add_destination(private_identity.clone(), name).await;
            let ret_identity = { dest.lock().await.desc.identity };
            let app_data = build_app_data(&local_peer_id, &name, &ret_identity, master);
            name_to_alpn.insert(name_hash_key(&name), alpn.clone());
            app_data_by_alpn.insert(alpn.clone(), app_data);
            destinations.insert(alpn.clone(), dest);
        }

        let inner = Arc::new(inner);
        let destinations = Arc::new(StdMutex::new(destinations));
        let app_data_by_alpn = Arc::new(app_data_by_alpn);
        let peers = Arc::new(StdMutex::new(HashMap::new()));

        // Announce listener: learn peers from validated announces.
        let announce_rx = inner.recv_announces().await;
        tokio::spawn(announce_listener(
            announce_rx,
            Arc::new(name_to_alpn),
            Arc::clone(&peers),
        ));

        // Announce sender: periodically re-announce our destinations.
        tokio::spawn(announce_sender(
            Arc::clone(&inner),
            Arc::clone(&destinations),
            Arc::clone(&app_data_by_alpn),
            announce_interval,
        ));

        Ok(Self {
            local_peer_id,
            inner,
            destinations,
            app_data_by_alpn,
            peers,
            connect_timeout,
        })
    }

    /// Immediately (re)announce the destination for one ALPN.
    ///
    /// Useful in tests to avoid waiting for the periodic announce timer. Uses the
    /// precomputed signed payload, so it needs no access to the master secret.
    pub async fn send_announce_now(&self, alpn: &Alpn) -> Result<(), TransportError> {
        let dest = self
            .destinations
            .lock()
            .expect("destinations mutex poisoned")
            .get(alpn)
            .cloned()
            .ok_or(TransportError::AlpnNotRegistered)?;
        let app_data = self
            .app_data_by_alpn
            .get(alpn)
            .cloned()
            .ok_or(TransportError::AlpnNotRegistered)?;
        self.inner
            .send_announce(&dest, Some(app_data.as_slice()))
            .await;
        Ok(())
    }

    /// Poll the announce-populated address book for a peer's destination on an
    /// ALPN, up to [`connect_timeout`](Self::connect_timeout).
    async fn resolve_peer(
        &self,
        peer: &PeerID,
        alpn: &Alpn,
    ) -> Result<DestinationDesc, TransportError> {
        let start = Instant::now();
        loop {
            if let Some(desc) = self
                .peers
                .lock()
                .expect("peers mutex poisoned")
                .get(peer)
                .and_then(|by_alpn| by_alpn.get(alpn))
                .copied()
            {
                return Ok(desc);
            }
            if start.elapsed() >= self.connect_timeout {
                return Err(TransportError::Backend(
                    "peer not discovered before timeout".into(),
                ));
            }
            sleep(Duration::from_millis(100)).await;
        }
    }
}

/// Map an ALPN string to a Reticulum destination name: the first `/`-segment is
/// the app name, the remainder is the dotted aspect (`mere/cable/v1` ->
/// `("mere", "cable.v1")`).
fn destination_name_for_alpn(alpn: &Alpn) -> DestinationName {
    let text = String::from_utf8_lossy(alpn.as_bytes());
    let mut parts = text.splitn(2, '/');
    let app = parts.next().filter(|s| !s.is_empty()).unwrap_or("mere");
    let aspect = parts.next().unwrap_or("").replace('/', ".");
    DestinationName::new(app, &aspect)
}

/// Wait for an outbound link to activate, matching by link id.
async fn wait_for_out_activation(
    events: &mut broadcast::Receiver<LinkEventData>,
    link_id: LinkId,
    dur: Duration,
) -> Result<(), TransportError> {
    let wait = async {
        loop {
            match events.recv().await {
                Ok(ev) if ev.id == link_id => match ev.event {
                    LinkEvent::Activated | LinkEvent::Data(_) => return Ok(()),
                    LinkEvent::Closed => return Err(TransportError::ConnectionRefused),
                },
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return Err(TransportError::Closed),
            }
        }
    };
    timeout(dur, wait)
        .await
        .map_err(|_| TransportError::Backend("link activation timed out".into()))?
}

/// Wait for an inbound link to activate on one of our destinations, returning its
/// link id. Blocks until a peer connects (per the [`Transport::accept`] contract).
async fn wait_for_in_activation(
    events: &mut broadcast::Receiver<LinkEventData>,
    my_addr: AddressHash,
) -> Result<LinkId, TransportError> {
    loop {
        match events.recv().await {
            Ok(ev) if ev.address_hash == my_addr => match ev.event {
                LinkEvent::Activated | LinkEvent::Data(_) => return Ok(ev.id),
                LinkEvent::Closed => continue,
            },
            Ok(_) => continue,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => return Err(TransportError::Closed),
        }
    }
}

impl Transport for ReticulumTransport {
    type Stream = ReticulumStream;

    fn local_peer_id(&self) -> PeerID {
        self.local_peer_id
    }

    async fn connect(&self, peer: PeerID, alpn: Alpn) -> Result<ReticulumStream, TransportError> {
        // Resolve the peer's destination for this ALPN from learned announces.
        let desc = self.resolve_peer(&peer, &alpn).await?;
        let peer_addr = desc.address_hash;

        // Subscribe before linking so the activation event is not missed.
        let mut events = self.inner.out_link_events();

        // Create (or reuse) the outbound link.
        let link = self.inner.link(desc).await;
        let (link_id, already_active) = {
            let guard = link.lock().await;
            (*guard.id(), guard.status() == LinkStatus::Active)
        };

        if !already_active {
            wait_for_out_activation(&mut events, link_id, self.connect_timeout).await?;
        }

        Ok(bridge_link(
            Arc::clone(&self.inner),
            events,
            LinkSide::Out(peer_addr),
            link_id,
        ))
    }

    async fn accept(&self, alpn: Alpn) -> Result<ReticulumStream, TransportError> {
        let dest = self
            .destinations
            .lock()
            .expect("destinations mutex poisoned")
            .get(&alpn)
            .cloned()
            .ok_or(TransportError::AlpnNotRegistered)?;
        let my_addr = { dest.lock().await.desc.address_hash };

        let mut events = self.inner.in_link_events();
        let link_id = wait_for_in_activation(&mut events, my_addr).await?;

        Ok(bridge_link(
            Arc::clone(&self.inner),
            events,
            LinkSide::In(my_addr),
            link_id,
        ))
    }
}

#[cfg(test)]
mod tests;
