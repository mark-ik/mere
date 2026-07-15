//! Reticulum-backed [`Transport`] implementation.
//!
//! [`ReticulumTransport`] runs Mere's bilateral peer-to-peer lane over Reticulum
//! links, using [`retinue`] — Mark's from-scratch Rust implementation of the
//! Reticulum Network Stack, wire-compatible with RNS 1.3.x. It is feature-gated
//! behind the `reticulum` cargo feature and is default-off. Sync (gossip / RBSR /
//! LogSync) and blob transfer remain iroh-only; this is the bilateral stream lane
//! only.
//!
//! ## Identity
//!
//! Mere's master key is a single Ed25519 keypair; a retinue identity is dual-key
//! (X25519 ECDH + Ed25519 signing). [`keys::derive_identity`] stretches the
//! 32-byte master seed into both keys via HKDF-SHA256, so the same seed always
//! yields the same retinue destination. The Mere [`PeerID`] stays the master
//! Ed25519 public key, consistent with the other transports.
//!
//! ## Discovery
//!
//! A destination hash cannot be synthesized from a [`PeerID`], so peers are learned
//! from authenticated announces (see [`announce`]). Each announce carries an
//! `app_data` binding signed by the master key, tying the announcing retinue
//! identity + destination name to a `PeerID`. `connect` resolves a peer by looking
//! it up in the announce-populated address book; `accept` waits for an inbound
//! link on the ALPN's destination.
//!
//! ## ALPN mapping
//!
//! `mere/cable/v1` maps to `DestinationName::new("mere", ["cable.v1"])`: the first
//! path segment is the Reticulum app name, the rest the dotted aspect.
//!
//! [`retinue`]: https://github.com/mark-ik/retinue

mod announce;
mod keys;
mod stream;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use identity::Ed25519Keypair;
use tokio::sync::{Mutex as TokioMutex, mpsc};
use tokio::task::JoinHandle;
use tokio::time::{Instant, interval, sleep, timeout};

use retinue::destination::DestinationName;
use retinue::endpoint::{Endpoint, LinkStream};
use retinue::hash::AddressHash;
use retinue::identity::Identity;

use crate::{Alpn, PeerID, Transport, TransportError};

use self::announce::{build_app_data, recover_peer_id};
use self::keys::derive_identity;

pub use self::stream::ReticulumStream;

/// Default interval between periodic re-announces of registered destinations.
const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(5);

/// Default time `connect` waits for peer discovery + link activation.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Learned peers: `PeerID -> (ALPN -> that peer's retinue identity)`.
type PeerMap = Arc<StdMutex<HashMap<PeerID, HashMap<Alpn, Identity>>>>;

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

    /// Network interfaces to attach (TCP server / client).
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

    /// Bind the transport: create the retinue endpoint, attach interfaces,
    /// register destinations, and start the announce + accept driver tasks.
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
    endpoint: Arc<Endpoint>,
    peers: PeerMap,
    /// Per-ALPN inbound accept queue, fed by the accept-router task. Behind an
    /// async mutex because `accept` takes `&self` yet must own the receiver while
    /// awaiting.
    inbound: HashMap<Alpn, Arc<TokioMutex<mpsc::UnboundedReceiver<LinkStream>>>>,
    connect_timeout: Duration,
    /// Background driver tasks (accept router, announce listener, announce
    /// sender), aborted on drop.
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for ReticulumTransport {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
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
        let identity = *private_identity.public();

        let endpoint = Endpoint::new(private_identity);

        // Attach interfaces.
        for iface in interfaces {
            match iface {
                ReticulumInterface::TcpServer { bind } => {
                    endpoint.listen_tcp(bind).await?;
                }
                ReticulumInterface::TcpClient { addr } => {
                    endpoint.attach_tcp_client(addr).await?;
                }
            }
        }

        // Register one destination per ALPN. Record each ALPN's destination name
        // and its precomputed signed announce payload; wire up a per-ALPN inbound
        // accept queue keyed (for routing) by the destination hash.
        let mut names: HashMap<Alpn, DestinationName> = HashMap::new();
        let mut app_data: HashMap<Alpn, Vec<u8>> = HashMap::new();
        let mut inbound = HashMap::new();
        let mut inbound_senders: HashMap<AddressHash, mpsc::UnboundedSender<LinkStream>> =
            HashMap::new();
        for alpn in &alpns {
            let name = destination_name_for_alpn(alpn);
            let data = build_app_data(&local_peer_id, &name, &identity, master);
            let dest = name.destination_hash(&identity);
            endpoint.register(name.clone(), &data);

            let (tx, rx) = mpsc::unbounded_channel();
            inbound_senders.insert(dest, tx);
            inbound.insert(alpn.clone(), Arc::new(TokioMutex::new(rx)));
            names.insert(alpn.clone(), name);
            app_data.insert(alpn.clone(), data);
        }

        let endpoint = Arc::new(endpoint);
        let names = Arc::new(names);
        let app_data = Arc::new(app_data);
        let peers: PeerMap = Arc::new(StdMutex::new(HashMap::new()));

        let tasks = vec![
            // Accept router: dispatch each inbound link to its ALPN's queue by the
            // destination it targeted.
            tokio::spawn(run_accept_router(Arc::clone(&endpoint), inbound_senders)),
            // Announce listener: learn peers from validated announce bindings.
            tokio::spawn(run_announce_listener(
                Arc::clone(&endpoint),
                Arc::clone(&names),
                Arc::clone(&peers),
            )),
            // Announce sender: periodically re-announce our destinations.
            tokio::spawn(run_announce_sender(
                Arc::clone(&endpoint),
                names,
                app_data,
                announce_interval,
            )),
        ];

        Ok(Self {
            local_peer_id,
            endpoint,
            peers,
            inbound,
            connect_timeout,
            tasks,
        })
    }

    /// Poll the announce-populated address book for a peer's retinue identity on
    /// an ALPN, up to [`connect_timeout`](Self::connect_timeout).
    async fn resolve_peer(&self, peer: &PeerID, alpn: &Alpn) -> Result<Identity, TransportError> {
        let start = Instant::now();
        loop {
            if let Some(identity) = self
                .peers
                .lock()
                .expect("peers mutex poisoned")
                .get(peer)
                .and_then(|by_alpn| by_alpn.get(alpn))
                .copied()
            {
                return Ok(identity);
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

/// Map an ALPN string to a retinue destination name: the first `/`-segment is the
/// app name, the remainder is the dotted aspect (`mere/cable/v1` ->
/// `("mere", ["cable.v1"])`).
fn destination_name_for_alpn(alpn: &Alpn) -> DestinationName {
    let text = String::from_utf8_lossy(alpn.as_bytes());
    let mut parts = text.splitn(2, '/');
    let app = parts.next().filter(|s| !s.is_empty()).unwrap_or("mere");
    let aspect = parts.next().unwrap_or("").replace('/', ".");
    DestinationName::new(app, [aspect.as_str()])
}

/// Accept-router task: forward each inbound link to the queue for the ALPN whose
/// destination it targeted. Ends when the endpoint closes.
async fn run_accept_router(
    endpoint: Arc<Endpoint>,
    senders: HashMap<AddressHash, mpsc::UnboundedSender<LinkStream>>,
) {
    while let Ok(accepted) = endpoint.accept_on_any().await {
        if let Some(tx) = senders.get(&accepted.destination) {
            let _ = tx.send(accepted.stream);
        }
    }
}

/// Announce-listener task: drain validated announces, recover the `PeerID`
/// binding, and record `peer_id -> (alpn, identity)`.
async fn run_announce_listener(
    endpoint: Arc<Endpoint>,
    names: Arc<HashMap<Alpn, DestinationName>>,
    peers: PeerMap,
) {
    loop {
        let announce = match endpoint.next_announcement().await {
            Ok(a) => a,
            Err(_) => break,
        };
        // Match the announce to one of our ALPNs by reconstructing that ALPN's
        // destination hash from the announcing identity, then verify the binding.
        for (alpn, name) in names.iter() {
            if name.destination_hash(&announce.identity) != announce.destination {
                continue;
            }
            if let Some(peer_id) =
                recover_peer_id(&announce.app_data, name, &announce.identity)
            {
                peers
                    .lock()
                    .expect("peers mutex poisoned")
                    .entry(peer_id)
                    .or_default()
                    .insert(alpn.clone(), announce.identity);
            }
            break;
        }
    }
}

/// Announce-sender task: periodically re-announce every registered destination
/// with its precomputed, signed `app_data`.
async fn run_announce_sender(
    endpoint: Arc<Endpoint>,
    names: Arc<HashMap<Alpn, DestinationName>>,
    app_data: Arc<HashMap<Alpn, Vec<u8>>>,
    period: Duration,
) {
    let mut timer = interval(period);
    loop {
        timer.tick().await;
        for (alpn, name) in names.iter() {
            if let Some(data) = app_data.get(alpn) {
                endpoint.announce(name, data);
            }
        }
    }
}

impl Transport for ReticulumTransport {
    type Stream = ReticulumStream;

    fn local_peer_id(&self) -> PeerID {
        self.local_peer_id
    }

    async fn connect(&self, peer: PeerID, alpn: Alpn) -> Result<ReticulumStream, TransportError> {
        // Resolve the peer's retinue identity for this ALPN from learned announces.
        let identity = self.resolve_peer(&peer, &alpn).await?;
        let dest = destination_name_for_alpn(&alpn).destination_hash(&identity);

        let stream = timeout(self.connect_timeout, self.endpoint.open(dest, identity))
            .await
            .map_err(|_| TransportError::Backend("link setup timed out".into()))??;

        Ok(ReticulumStream::new(stream))
    }

    async fn accept(&self, alpn: Alpn) -> Result<ReticulumStream, TransportError> {
        let queue = self
            .inbound
            .get(&alpn)
            .cloned()
            .ok_or(TransportError::AlpnNotRegistered)?;
        let mut rx = queue.lock().await;
        match rx.recv().await {
            Some(stream) => Ok(ReticulumStream::new(stream)),
            None => Err(TransportError::Closed),
        }
    }
}

#[cfg(test)]
mod tests;
