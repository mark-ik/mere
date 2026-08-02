//! Resident owner of Graphshell's durable personal-graph replica.

use std::path::PathBuf;

use graphshell_protocol::{CardValueV1, PortableCardV1};
use muniment::RedbBackend;
use personae::IdentityProvider;
use std::sync::Arc;
use stickleback::{JoinError, JoinedSpace, SyncStatus};

use tokio::sync::{Mutex, RwLock};
use transport::p2panda_transport::MdnsDiscoveryMode;
use transport::p2panda_transport::{KnownPeer, RelayUrl};
use transport::{BlobHash, BlobStore, P2pandaTransport, PeerID, Transport, sync_overlay_topic};

use crate::identity_endpoint::SupplementalCard;
use crate::personal_sync::{
    PersonalGraphError, PersonalGraphEvent, PersonalGraphExt, PersonalGraphIdentityError,
    PersonalGraphReplica, SyncRoster, SyncSelection, accept_into, personal_graph_identity_salt,
};

const MAX_NODE_CARDS: usize = 128;

#[derive(Clone, Debug)]
pub struct PersonalSyncHostConfig {
    pub graph: [u8; 32],
    pub store_path: PathBuf,
    pub roster: SyncRoster,
    pub selection: SyncSelection,
    pub peer_tickets: Vec<String>,
    /// Per-graph transport node ids of paired devices.
    ///
    /// This is what pairing persists. A ticket carries the peer's current
    /// address and is rebuilt on every bind, so a stored ticket goes stale the
    /// next time that device restarts. A node id is derived from the peer's
    /// master seed and this graph's salt, so it is stable; mDNS supplies the
    /// address that the ticket used to carry.
    pub paired_nodes: Vec<[u8; 32]>,
    /// iroh relays to register. Empty leaves this transport LAN-only.
    pub relay_urls: Vec<RelayUrl>,
}

#[derive(Debug, thiserror::Error)]
pub enum PersonalSyncHostError {
    #[error(transparent)]
    Store(#[from] muniment::StoreError),
    #[error(transparent)]
    Identity(#[from] PersonalGraphIdentityError),
    #[error(transparent)]
    IdentityProvider(#[from] personae::IdentityError),
    #[error(transparent)]
    Graph(#[from] PersonalGraphError),
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error("personal-sync transport failed: {0}")]
    Transport(String),
    #[error("personal-sync store path has no parent")]
    MissingStoreParent,
}

/// Durable replica, live LogSync session, and transport kept by the device host.
pub struct PersonalSyncHost {
    graph: [u8; 32],
    store_path: PathBuf,
    roster: Arc<RwLock<SyncRoster>>,
    replica: Mutex<PersonalGraphReplica<RedbBackend>>,
    joined: JoinedSpace<PersonalGraphExt>,
    transport: P2pandaTransport,
    /// Bytes this device serves to its paired siblings, and the bytes it has
    /// fetched from them.
    ///
    /// Disk-backed rather than in memory: this store IS the durable copy for
    /// a transfer in flight, so a host restart between "fetched" and "applied"
    /// must not send the bytes back over the wire. It is also why a device can
    /// still answer for a blob after the session that received it ended.
    blobs: BlobStore,
}

impl PersonalSyncHost {
    pub async fn open<P: IdentityProvider + ?Sized>(
        identity: &P,
        config: PersonalSyncHostConfig,
    ) -> Result<Self, PersonalSyncHostError> {
        let parent = config
            .store_path
            .parent()
            .ok_or(PersonalSyncHostError::MissingStoreParent)?;
        std::fs::create_dir_all(parent).map_err(|error| {
            PersonalSyncHostError::Store(muniment::StoreError::Backend(error.to_string()))
        })?;
        let backend = RedbBackend::open(&config.store_path)?;
        let replica = PersonalGraphReplica::for_identity(
            backend,
            config.graph,
            identity,
            config.roster.clone(),
            config.selection,
        )?;
        let transport_key = identity.derive_keypair(&personal_graph_identity_salt(config.graph))?;
        // Beside the graph store and named for the same graph, so the two
        // halves of one device's state cannot be moved apart by accident.
        let blob_root = config.store_path.with_extension("blobs");
        let blobs = BlobStore::open(&blob_root)
            .await
            .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))?;
        // Active mDNS is what makes a paired node id dialable without a stored
        // ticket: it populates the address book, so tagging a known peer with
        // the overlay topic is enough to bootstrap gossip. g5_peer proved this
        // path Fedora-to-Windows and Windows-to-Fedora under H10.
        //
        // `.blobs` serves the iroh-blobs ALPN off this same endpoint, so a
        // sibling reaches bytes through the pairing it already has. Without it
        // the lane replicated `ObserveBlobAvailability` records saying which
        // device held which blob, and offered no way to ask for one.
        let mut builder = P2pandaTransport::builder(&transport_key)
            .gossip()
            .mdns(MdnsDiscoveryMode::Active)
            .blobs(&blobs);
        for url in config.relay_urls.clone() {
            builder = builder.relay_url(url);
        }
        let transport = builder
            .bind()
            .await
            .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))?;
        let overlay = sync_overlay_topic(config.graph);
        for node in &config.paired_nodes {
            let peer = PeerID::from_bytes(node).map_err(|error| {
                PersonalSyncHostError::Transport(format!("paired node id: {error}"))
            })?;
            transport
                .set_topics(peer, &[overlay])
                .await
                .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))?;
        }
        for ticket in &config.peer_tickets {
            let peer = transport
                .add_peer_ticket(ticket)
                .await
                .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))?;
            transport
                .set_topics(peer, &[overlay])
                .await
                .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))?;
        }
        let store = replica.sync_store();
        let accepted = store.clone();
        // Shared rather than captured by value: pairing a device while the
        // host runs has to change who is admitted, not only who is reachable.
        // A roster baked in at open time meant a device could become reachable
        // and still have every write refused until the next restart.
        let roster = Arc::new(RwLock::new(config.roster.clone()));
        let admitting = Arc::clone(&roster);
        let graph = config.graph;
        let (endpoint, gossip) = transport
            .sync_parts()
            .ok_or_else(|| PersonalSyncHostError::Transport("gossip is unavailable".into()))?;
        let joined = JoinedSpace::join::<_, u64, _, _>(
            // Scoped to kind AND graph: two personal graphs on one endpoint
            // would otherwise share a protocol id and stop converging.
            stickleback::lane_id("graphshell/personal-graph/v1", graph),
            store,
            endpoint,
            gossip,
            graph,
            move |operation| {
                let store = accepted.clone();
                let admitting = Arc::clone(&admitting);
                async move {
                    let roster = admitting.read().await.clone();
                    match accept_into(&store, graph, &roster, &operation).await {
                        Ok(inserted) => inserted,
                        Err(error) => {
                            // A refused operation and a failed one both have to
                            // answer `false` to LogSync, so the cause survives
                            // only if it is logged here. Without this, a peer
                            // whose intake is broken looks exactly like a peer
                            // correctly refusing an off-roster writer.
                            tracing::warn!(
                                graph = %short_hex(&graph),
                                operation = %short_hex(operation.hash.as_bytes()),
                                writer = %short_hex(operation.header.verifying_key.as_bytes()),
                                %error,
                                "personal sync could not process an incoming operation"
                            );
                            false
                        }
                    }
                }
            },
        )
        .await?;

        Ok(Self {
            graph,
            store_path: config.store_path,
            roster,
            replica: Mutex::new(replica),
            joined,
            transport,
        })
    }

    pub fn graph(&self) -> [u8; 32] {
        self.graph
    }

    pub async fn roster(&self) -> SyncRoster {
        self.roster.read().await.clone()
    }

    /// Replace the set of roots admitted to write this graph.
    ///
    /// The authority half of [`pair_node`](Self::pair_node). Reachability
    /// without admission is a device that connects and has everything it sends
    /// refused, so the two must move together.
    /// Both copies move together. The replica re-checks stored operations
    /// against its own roster when projecting, so admitting a writer on intake
    /// without admitting it here stores the operation and then fails the whole
    /// projection on the way out.
    pub async fn set_roster(&self, roster: SyncRoster) {
        *self.roster.write().await = roster.clone();
        self.replica.lock().await.set_roster(roster);
    }

    pub fn sync_status(&self) -> SyncStatus {
        self.joined.sync_status()
    }

    /// This device's stable per-graph node id: what a peer stores to pair with
    /// it. Survives restarts, unlike [`ticket`](Self::ticket).
    pub fn node_id(&self) -> [u8; 32] {
        self.transport.local_peer_id().to_bytes()
    }

    /// Tag another device onto this graph's overlay on the live transport, so
    /// pairing takes effect without restarting the resident host.
    pub async fn pair_node(&self, node_id: [u8; 32]) -> Result<(), PersonalSyncHostError> {
        let peer = PeerID::from_bytes(&node_id).map_err(|error| {
            PersonalSyncHostError::Transport(format!("paired node id: {error}"))
        })?;
        self.transport
            .set_topics(peer, &[sync_overlay_topic(self.graph)])
            .await
            .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))
    }

    /// Drop a device from this graph's overlay on the live transport.
    ///
    /// The peer stays in the address book, so re-pairing later does not have
    /// to rediscover it. Only its membership of this graph goes.
    pub async fn unpair_node(&self, node_id: [u8; 32]) -> Result<(), PersonalSyncHostError> {
        let peer = PeerID::from_bytes(&node_id).map_err(|error| {
            PersonalSyncHostError::Transport(format!("paired node id: {error}"))
        })?;
        self.transport
            .remove_topic(peer, sync_overlay_topic(self.graph))
            .await
            .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))
    }

    /// Which devices the transport currently associates with this graph.
    ///
    /// Pairing records identity; this reports reachability, which is the fact
    /// a peer id cannot carry on its own.
    pub async fn known_peers(&self) -> Result<Vec<KnownPeer>, PersonalSyncHostError> {
        self.transport
            .peers_for_topic(sync_overlay_topic(self.graph))
            .await
            .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))
    }

    pub async fn ticket(&self) -> Result<String, PersonalSyncHostError> {
        self.transport
            .ticket()
            .await
            .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))
    }

    pub async fn author(
        &self,
        events: Vec<PersonalGraphEvent>,
    ) -> Result<(), PersonalSyncHostError> {
        let operation = self.replica.lock().await.author(events).await?;
        self.joined.publish(operation)?;
        Ok(())
    }

    /// Leave live sync and wait until the durable store can be reopened.
    pub async fn close(self) -> Result<(), PersonalSyncHostError> {
        let store_path = self.store_path.clone();
        drop(self.joined);
        drop(self.transport);
        drop(self.replica);

        let mut last_error = None;
        for _ in 0..50 {
            match RedbBackend::open(&store_path) {
                Ok(probe) => {
                    drop(probe);
                    return Ok(());
                }
                Err(error) => last_error = Some(error),
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        Err(PersonalSyncHostError::Store(
            last_error.expect("the close probe ran at least once"),
        ))
    }

    /// Public, read-only cards added to the already-admitted device surface.
    pub async fn supplemental_cards(&self) -> Result<Vec<SupplementalCard>, PersonalSyncHostError> {
        let projection = self.replica.lock().await.projection().await?;
        let mut cards = vec![SupplementalCard {
            adapter: "graphshell.personal-sync".into(),
            source_id: hex(&self.graph),
            card: PortableCardV1 {
                title: "Personal graph sync".into(),
                values: vec![
                    value("Graph", hex(&self.graph)),
                    value("Nodes", projection.graph.node_count().to_string()),
                    value("Relations", projection.graph.edge_count().to_string()),
                    value(
                        "Access records",
                        projection.access_records.len().to_string(),
                    ),
                    value(
                        "Blob observations",
                        projection.blob_availability.len().to_string(),
                    ),
                    value("Conflicts", projection.conflicts.len().to_string()),
                    value("Pending history", projection.pending.len().to_string()),
                ],
                badges: vec!["Durable".into(), "Personae admitted".into()],
                media: Vec::new(),
            },
        }];

        let mut nodes = projection
            .graph
            .nodes()
            .map(|(_, node)| node)
            .collect::<Vec<_>>();
        nodes.sort_by_key(|node| node.id);
        for node in nodes.into_iter().take(MAX_NODE_CARDS) {
            let mut tags = node.tags.iter().cloned().collect::<Vec<_>>();
            tags.sort();
            cards.push(SupplementalCard {
                adapter: "mere.graph".into(),
                source_id: node.id.to_string(),
                card: PortableCardV1 {
                    title: if node.title.is_empty() {
                        node.url().to_string()
                    } else {
                        node.title.clone()
                    },
                    values: vec![
                        value("Address", node.url()),
                        value(
                            "Tags",
                            if tags.is_empty() {
                                "none".into()
                            } else {
                                tags.join(", ")
                            },
                        ),
                    ],
                    badges: vec!["Synced graph node".into()],
                    media: Vec::new(),
                },
            });
        }

        for (blob, devices) in &projection.available_blobs {
            cards.push(SupplementalCard {
                adapter: "graphshell.blob-availability".into(),
                source_id: hex(blob),
                card: PortableCardV1 {
                    title: format!("Blob {}", short_hex(blob)),
                    values: vec![
                        value(
                            "Available on",
                            devices.iter().cloned().collect::<Vec<_>>().join(", "),
                        ),
                        value(
                            "Observations",
                            projection
                                .blob_availability
                                .iter()
                                .filter(|observation| observation.blob == *blob)
                                .count()
                                .to_string(),
                        ),
                    ],
                    badges: vec![
                        "Availability only".into(),
                        "Bytes stay out of graph sync".into(),
                    ],
                    media: Vec::new(),
                },
            });
        }

        Ok(cards)
    }
}

fn value(label: impl Into<String>, value: impl Into<String>) -> CardValueV1 {
    CardValueV1 {
        label: label.into(),
        value: value.into(),
    }
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn short_hex(bytes: &[u8; 32]) -> String {
    hex(bytes)[..12].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use personae::{IdentityProvider, InMemoryProvider};
    use uuid::Uuid;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn resident_host_reopens_and_projects_public_cards() {
        let directory = tempfile::tempdir().unwrap();
        let identity = InMemoryProvider::from_seed([0x61; 32]);
        let graph = [0x62; 32];
        let config = || PersonalSyncHostConfig {
            graph,
            store_path: directory.path().join("resident.redb"),
            roster: SyncRoster::new([identity.master_public_key().to_bytes()]),
            selection: SyncSelection::default().with_blob_availability(true),
            peer_tickets: Vec::new(),
            paired_nodes: Vec::new(),
            relay_urls: Vec::new(),
        };
        let node = Uuid::from_u128(0x63);

        let host = PersonalSyncHost::open(&identity, config()).await.unwrap();
        host.author(vec![
            PersonalGraphEvent::AddNode {
                id: node,
                address: "https://resident.test/".into(),
                title: "Resident graph node".into(),
            },
            PersonalGraphEvent::ObserveBlobAvailability {
                observation: crate::personal_sync::BlobAvailabilityObservation {
                    record_id: Uuid::from_u128(0x64),
                    container_id: node,
                    blob: [0x65; 32],
                    device: "resident-device".into(),
                    available: true,
                    at_ms: 1,
                },
            },
        ])
        .await
        .unwrap();
        let cards = host.supplemental_cards().await.unwrap();
        assert!(
            cards
                .iter()
                .any(|card| card.card.title == "Resident graph node")
        );
        assert!(
            cards
                .iter()
                .any(|card| card.card.title.starts_with("Blob "))
        );
        let node_id = host.node_id();
        host.close().await.unwrap();

        let reopened = PersonalSyncHost::open(&identity, config()).await.unwrap();
        let cards = reopened.supplemental_cards().await.unwrap();
        assert!(
            cards
                .iter()
                .any(|card| card.card.title == "Resident graph node")
        );
        assert_eq!(
            reopened.node_id(),
            node_id,
            "the node id must survive a restart: pairing persists it, and a \
             peer that stored it has no other way back to this device"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_paired_node_id_is_tagged_onto_the_overlay_without_an_address() {
        let directory = tempfile::tempdir().unwrap();
        let identity = InMemoryProvider::from_seed([0x71; 32]);
        let graph = [0x72; 32];
        // The peer's node id alone, with no ticket and so no address. This is
        // the whole point of pairing on node id: mDNS supplies the address
        // later, so opening must not require one now.
        let peer_node = InMemoryProvider::from_seed([0x73; 32])
            .master_public_key()
            .to_bytes();

        let host = PersonalSyncHost::open(
            &identity,
            PersonalSyncHostConfig {
                graph,
                store_path: directory.path().join("paired.redb"),
                roster: SyncRoster::new([identity.master_public_key().to_bytes(), peer_node]),
                selection: SyncSelection::default(),
                peer_tickets: Vec::new(),
                paired_nodes: vec![peer_node],
                relay_urls: Vec::new(),
            },
        )
        .await
        .unwrap();

        assert_ne!(
            host.node_id(),
            peer_node,
            "the host's own node id is derived per graph, so it must not \
             collide with a roster root"
        );
        host.close().await.unwrap();
    }
}
