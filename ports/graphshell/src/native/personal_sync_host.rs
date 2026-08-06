//! Resident owner of Graphshell's durable personal-graph replica.

use std::path::PathBuf;

use graphshell_protocol::{
    ActionFormChoiceV1, ActionFormFieldV1, ActionFormV1, CardValueV1, PortableCardV1,
};
use muniment::RedbBackend;
use personae::IdentityProvider;
use std::sync::Arc;
use stickleback::{JoinError, JoinedSpace, SyncStatus};

use tokio::sync::{Mutex, RwLock};
use transport::p2panda_transport::MdnsDiscoveryMode;
use transport::p2panda_transport::{KnownPeer, RelayUrl};
use transport::{BlobHash, BlobStore, P2pandaTransport, PeerID, Transport, sync_overlay_topic};
use uuid::Uuid;

use crate::native::browser_host::now_ms;
use crate::native::owner_settings::parse_hex32;
use crate::transfer_offer::{TransferOfferV1, offers_in, transfer_offer_rule};

use crate::identity_endpoint::{SupplementalCard, TRANSFER_ACCEPT_INTENT, TRANSFER_ACCEPT_SCHEMA};
use crate::identity_projection::IdentityProjectionAction;
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
    /// Stored dial hints: each paired device's last known endpoint ticket, as
    /// persisted by the cached-address rung. Unlike `peer_tickets`, applied
    /// best-effort: an argument the owner just typed should fail loudly, but a
    /// hint recorded last week must degrade to "discovery will have to find
    /// them" rather than stop the host from opening.
    pub peer_hints: Vec<String>,
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
        let transport_key = identity.derive_keypair(&personal_graph_identity_salt(config.graph))?;
        // The host knows which device it is; callers should not have to say so.
        // Leaving this to the caller is what makes an addressed-object filter
        // quietly inert, which is worse than absent because the lane still
        // looks configured. The transfer rule registers unconditionally: it
        // governs how a carrier node behaves when its facet is *not* selected,
        // so gating the rule on the lane would disable exactly the case it is
        // for. One carrier class exists today; a second composes here.
        let device = hex(&transport_key.public_key().to_bytes());
        let selection = config
            .selection
            .with_local_device(&device)
            .with_synthetic_addresses([transfer_offer_rule()]);
        let replica = PersonalGraphReplica::for_identity(
            backend,
            config.graph,
            identity,
            config.roster.clone(),
            selection,
        )?;
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
        // The selection above was stamped with a device key derived before the
        // bind. If the transport ever named itself differently, offers would be
        // filtered against an identity no peer addresses, and the symptom would
        // be an empty inbox rather than an error.
        if transport.local_peer_id().to_bytes() != transport_key.public_key().to_bytes() {
            return Err(PersonalSyncHostError::Transport(format!(
                "transport bound as {} but this device addresses itself as {device}",
                hex(&transport.local_peer_id().to_bytes())
            )));
        }
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
        // Best-effort, unlike the loop above, and the difference is the
        // provenance of the string: an argument the owner just typed should
        // fail loudly, a hint recorded weeks ago must degrade. A device whose
        // stored hint has rotted still opens, still serves, and still reaches
        // anything discovery can find; it has only lost the shortcut.
        for hint in &config.peer_hints {
            match transport.add_peer_ticket(hint).await {
                Ok(peer) => {
                    if let Err(error) = transport.set_topics(peer, &[overlay]).await {
                        tracing::warn!(%error, "could not tag a stored dial hint onto the overlay");
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "a stored dial hint did not parse; skipping it");
                }
            }
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
            blobs,
        })
    }

    pub fn graph(&self) -> [u8; 32] {
        self.graph
    }

    /// This device's blob store: what it serves to siblings, and where a
    /// fetched blob lands.
    pub fn blobs(&self) -> &BlobStore {
        &self.blobs
    }

    /// Fetch a blob from a paired device that has advertised holding it.
    ///
    /// `node` is the peer's per-graph transport node id, the same identifier
    /// pairing persists and the overlay dials. On success the bytes are in the
    /// local store and flushed, so a restart before the caller applies them
    /// does not cost a second transfer.
    ///
    /// Refuses a peer this host has not paired with. Reachability is not
    /// authority, and a blob hash names bytes rather than a right to them; the
    /// paired set is what says whose advertisement this device will act on.
    pub async fn fetch_blob(
        &self,
        node: [u8; 32],
        blob: [u8; 32],
    ) -> Result<(), PersonalSyncHostError> {
        let hash = BlobHash::from_bytes(blob);
        if self
            .blobs
            .has(hash)
            .await
            .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))?
        {
            return Ok(());
        }
        let peer = PeerID::from_bytes(&node)
            .map_err(|error| PersonalSyncHostError::Transport(format!("peer node id: {error}")))?;
        if !self.is_paired(&peer).await {
            return Err(PersonalSyncHostError::Transport(format!(
                "{} is not a paired device of this graph",
                short_hex(&node)
            )));
        }
        self.blobs
            .fetch_from(&self.transport, peer, hash)
            .await
            .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))?;
        self.blobs
            .flush()
            .await
            .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))?;
        tracing::info!(
            peer = %short_hex(&node),
            blob = %short_hex(&blob),
            "fetched a blob from a paired device"
        );
        Ok(())
    }

    /// Whether this peer is one this host has been told to reach for this
    /// graph. `known_peers` is the live view, so a device paired since open
    /// counts without a restart.
    ///
    /// A transport that cannot answer is treated as "not paired": refusing a
    /// fetch is recoverable, fetching from an unverified peer is not.
    async fn is_paired(&self, peer: &PeerID) -> bool {
        match self.known_peers().await {
            Ok(peers) => peers.iter().any(|known| &known.peer == peer),
            Err(error) => {
                tracing::warn!(%error, "could not list paired devices; refusing the fetch");
                false
            }
        }
    }

    /// Hold bytes for `container`, and tell the graph this device has them.
    ///
    /// The observation names this device by its per-graph node id rather than
    /// a human label, because the only consumer that matters is a sibling
    /// deciding whom to dial, and a label is not dialable. The label a person
    /// reads belongs to the paired-device record, which already carries one.
    ///
    /// Flushes before authoring, so the claim is never published ahead of the
    /// bytes it promises.
    pub async fn stage_blob(
        &self,
        container: Uuid,
        bytes: Vec<u8>,
    ) -> Result<[u8; 32], PersonalSyncHostError> {
        let hash = self
            .blobs
            .put_bytes(bytes)
            .await
            .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))?;
        self.blobs
            .flush()
            .await
            .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))?;
        let blob = hash.to_bytes();
        self.author(vec![PersonalGraphEvent::ObserveBlobAvailability {
            observation: crate::personal_sync::BlobAvailabilityObservation {
                record_id: Uuid::new_v4(),
                container_id: container,
                blob,
                device: hex(&self.node_id()),
                available: true,
                at_ms: now_ms(),
            },
        }])
        .await?;
        tracing::info!(
            blob = %short_hex(&blob),
            container = %container,
            "staged a blob and advertised it to paired devices"
        );
        Ok(blob)
    }

    /// Devices the graph says currently hold `blob`, newest observation first.
    ///
    /// Only entries naming a dialable node id are returned. An observation
    /// written before devices identified themselves this way carries a human
    /// label, which cannot be dialed and is dropped here rather than surfacing
    /// as a peer that never answers.
    pub async fn blob_holders(
        &self,
        blob: [u8; 32],
    ) -> Result<Vec<[u8; 32]>, PersonalSyncHostError> {
        let projection = self.replica.lock().await.projection().await?;
        let mut observations = projection
            .blob_availability
            .iter()
            .filter(|observation| observation.blob == blob && observation.available)
            .collect::<Vec<_>>();
        observations.sort_by_key(|observation| std::cmp::Reverse(observation.at_ms));
        let mut holders = Vec::new();
        for observation in observations {
            let Ok(node) = parse_hex32(&observation.device) else {
                continue;
            };
            if node != self.node_id() && !holders.contains(&node) {
                holders.push(node);
            }
        }
        Ok(holders)
    }

    /// Fetch `blob` from whichever paired device advertised holding it.
    ///
    /// Tries holders newest-observation first and stops at the first success,
    /// so a device that has since gone offline costs a dial rather than the
    /// transfer. Returns the node that supplied the bytes.
    pub async fn fetch_blob_by_availability(
        &self,
        blob: [u8; 32],
    ) -> Result<[u8; 32], PersonalSyncHostError> {
        if self
            .blobs
            .has(BlobHash::from_bytes(blob))
            .await
            .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))?
        {
            return Ok(self.node_id());
        }
        let holders = self.blob_holders(blob).await?;
        if holders.is_empty() {
            return Err(PersonalSyncHostError::Transport(format!(
                "no paired device has advertised blob {}",
                short_hex(&blob)
            )));
        }
        let mut last = None;
        for node in &holders {
            match self.fetch_blob(*node, blob).await {
                Ok(()) => return Ok(*node),
                Err(error) => {
                    tracing::warn!(
                        peer = %short_hex(node),
                        blob = %short_hex(&blob),
                        %error,
                        "a device that advertised this blob did not supply it"
                    );
                    last = Some(error);
                }
            }
        }
        Err(last.unwrap_or_else(|| {
            PersonalSyncHostError::Transport(format!("blob {} is unreachable", short_hex(&blob)))
        }))
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

    /// Where the endpoint currently believes `node` lives, as a ticket, if it
    /// holds any addresses for it. The value the cached-address rung persists.
    pub async fn peer_ticket(
        &self,
        node: [u8; 32],
    ) -> Result<Option<String>, PersonalSyncHostError> {
        let peer = PeerID::from_bytes(&node)
            .map_err(|error| PersonalSyncHostError::Transport(format!("peer node id: {error}")))?;
        self.transport
            .peer_ticket(peer)
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

    /// Whether this host can advertise bytes to its paired devices.
    ///
    /// Staging without it holds bytes no sibling can learn to ask for, so
    /// callers that are about to move bytes check this first.
    pub async fn serves_blobs(&self) -> bool {
        self.replica.lock().await.serves_blob_availability()
    }

    /// Transfers waiting for this device, oldest first, plus the ones it sent.
    ///
    /// Reads the projection, so an offer addressed elsewhere is absent here
    /// even though this device holds the operation that carries it. That is a
    /// display boundary, not a confidentiality one: the personal lane is
    /// plaintext to every device the roster admits.
    pub async fn offers(&self) -> Result<Vec<TransferOfferV1>, PersonalSyncHostError> {
        let projection = self.replica.lock().await.projection().await?;
        Ok(offers_in(&projection))
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
            actions: Vec::new(),
        }];

        let offers = offers_in(&projection);
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
                // A waiting transfer is the one graph node a person can act
                // on: everything else here is a view of state that already
                // happened. The transfer id is bound into the payload, so the
                // decision names one transfer rather than "the" transfer.
                actions: offers
                    .iter()
                    .find(|offer| offer.transfer_id == node.id)
                    .map(|offer| {
                        vec![IdentityProjectionAction {
                            intent: TRANSFER_ACCEPT_INTENT,
                            schema: TRANSFER_ACCEPT_SCHEMA,
                            label: "Accept transfer",
                            // `payload` pre-binds fields for the *native*
                            // identity UI. The browser composes this one from
                            // the form below, so pre-binding here would be a
                            // value nothing reads.
                            payload: None,
                            native_only: true,
                            // One field, one advertised choice: this card's
                            // transfer. The id has to travel as an advertised
                            // value because that is the only thing the bridge
                            // will put in a payload, and a decision that does
                            // not name its transfer is ambiguous the moment
                            // two are waiting.
                            input_form: Some(
                                ActionFormV1::new(TRANSFER_ACCEPT_SCHEMA).with_field(
                                    ActionFormFieldV1::choice(
                                        "transfer_id",
                                        "Transfer",
                                        [ActionFormChoiceV1::new(
                                            offer.transfer_id.to_string(),
                                            format!(
                                                "{} object(s), {} blob(s)",
                                                offer.nodes, offer.blobs
                                            ),
                                        )],
                                    )
                                    .with_description(
                                        "Confirm which waiting transfer to bring onto this device.",
                                    ),
                                ),
                            ),
                        }]
                    })
                    .unwrap_or_default(),
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
                actions: Vec::new(),
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
    use crate::transfer_offer::TRANSFER_OFFER_FACET;
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
            peer_hints: Vec::new(),
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
                peer_hints: Vec::new(),
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

    /// S1: bytes staged on one device are fetched, byte-identical, by a paired
    /// sibling that learned of them only through the graph.
    ///
    /// Two hosts on one graph, paired by node id. The source stages bytes; the
    /// destination is told nothing but the hash, resolves the holder from the
    /// replicated availability record, and fetches over the same endpoint the
    /// graph syncs on.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_staged_blob_is_fetched_by_a_paired_device_from_the_graph_alone() {
        let directory = tempfile::tempdir().unwrap();
        let graph = [0x81; 32];
        let source_identity = InMemoryProvider::from_seed([0x82; 32]);
        let destination_identity = InMemoryProvider::from_seed([0x83; 32]);
        let roster = SyncRoster::new([
            source_identity.master_public_key().to_bytes(),
            destination_identity.master_public_key().to_bytes(),
        ]);
        let container = Uuid::from_u128(0x84);
        let payload = b"the bytes that have to cross a real endpoint".to_vec();

        let source = PersonalSyncHost::open(
            &source_identity,
            PersonalSyncHostConfig {
                graph,
                store_path: directory.path().join("source.redb"),
                roster: roster.clone(),
                selection: SyncSelection::default().with_blob_availability(true),
                peer_tickets: Vec::new(),
                peer_hints: Vec::new(),
                paired_nodes: Vec::new(),
                relay_urls: Vec::new(),
            },
        )
        .await
        .unwrap();
        let destination = PersonalSyncHost::open(
            &destination_identity,
            PersonalSyncHostConfig {
                graph,
                store_path: directory.path().join("destination.redb"),
                roster,
                selection: SyncSelection::default().with_blob_availability(true),
                peer_tickets: vec![source.ticket().await.unwrap()],
                peer_hints: Vec::new(),
                paired_nodes: Vec::new(),
                relay_urls: Vec::new(),
            },
        )
        .await
        .unwrap();
        source.pair_node(destination.node_id()).await.unwrap();

        let blob = source.stage_blob(container, payload.clone()).await.unwrap();

        // The destination learns the holder from the replicated observation,
        // never from the test. Give sync a moment to deliver it.
        let mut holders = Vec::new();
        for _ in 0..50 {
            holders = destination.blob_holders(blob).await.unwrap();
            if !holders.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(
            holders,
            vec![source.node_id()],
            "the availability record must name the source by a dialable node id"
        );

        let supplier = destination.fetch_blob_by_availability(blob).await.unwrap();
        assert_eq!(supplier, source.node_id());
        assert_eq!(
            destination
                .blobs()
                .get_bytes(BlobHash::from_bytes(blob))
                .await
                .unwrap(),
            payload,
            "the fetched bytes must be the staged bytes"
        );

        source.close().await.unwrap();
        destination.close().await.unwrap();
    }

    /// The filter has to hold on the product path, not only in the module that
    /// defines it. Three resident hosts on one roster, one offer: the addressee
    /// and the sender project it, the third device does not, and none of them
    /// were configured by hand to make that happen.
    #[tokio::test]
    async fn an_offer_over_live_sync_reaches_its_addressee_and_not_a_third_device() {
        let directory = tempfile::tempdir().unwrap();
        let graph = [0x91; 32];
        let identities = [
            InMemoryProvider::from_seed([0x92; 32]),
            InMemoryProvider::from_seed([0x93; 32]),
            InMemoryProvider::from_seed([0x94; 32]),
        ];
        let roster = SyncRoster::new(
            identities
                .iter()
                .map(|identity| identity.master_public_key().to_bytes()),
        );
        // Every device selects the offer facet. What differs is who they are.
        let selection = || SyncSelection::default().with_facets([TRANSFER_OFFER_FACET]);

        let mut hosts = Vec::new();
        for (index, identity) in identities.iter().enumerate() {
            let peer_tickets = match hosts.first() {
                Some(first) => vec![PersonalSyncHost::ticket(first).await.unwrap()],
                None => Vec::new(),
            };
            hosts.push(
                PersonalSyncHost::open(
                    identity,
                    PersonalSyncHostConfig {
                        graph,
                        store_path: directory.path().join(format!("device-{index}.redb")),
                        roster: roster.clone(),
                        selection: selection(),
                        peer_tickets,
                        peer_hints: Vec::new(),
                        paired_nodes: Vec::new(),
                        relay_urls: Vec::new(),
                    },
                )
                .await
                .unwrap(),
            );
        }
        for node in [hosts[1].node_id(), hosts[2].node_id()] {
            hosts[0].pair_node(node).await.unwrap();
        }

        let offer = TransferOfferV1 {
            schema: TRANSFER_OFFER_FACET.to_string(),
            transfer_id: Uuid::from_u128(0x95),
            operation: crate::transfer::TransferOperation::Copy,
            source: endpoint_for(&hosts[0]),
            destination: endpoint_for(&hosts[1]),
            pairing_id: "pairing-live".to_string(),
            manifest_blob: eidetic::Hash::of(b"manifest"),
            manifest_byte_len: 2048,
            nodes: 2,
            relations: 1,
            blobs: 1,
            blob_bytes: 44,
            offered_at_ms: now_ms(),
        };
        hosts[0]
            .author(crate::transfer_offer::offer_events(&offer).unwrap())
            .await
            .unwrap();

        let mut addressed = Vec::new();
        for _ in 0..50 {
            addressed = hosts[1].offers().await.unwrap();
            if !addressed.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(addressed, vec![offer.clone()]);
        assert_eq!(
            hosts[0].offers().await.unwrap(),
            vec![offer],
            "a source sees what it sent"
        );
        assert!(
            hosts[2].offers().await.unwrap().is_empty(),
            "a third device on the same roster is not the addressee"
        );

        for host in hosts {
            host.close().await.unwrap();
        }
    }

    fn endpoint_for(host: &PersonalSyncHost) -> crate::transfer::TransferEndpointV1 {
        crate::transfer::TransferEndpointV1 {
            graph: format!("graphshell://graph/{}", hex(&host.graph())),
            persona: "personae://persona/owner".to_string(),
            device: format!("personae://device/{}", hex(&host.node_id())),
        }
    }

    /// A rotted dial hint must cost the shortcut, never the host: the hint is
    /// what a previous run believed, and this run has no way to check it other
    /// than by trying.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_garbled_stored_dial_hint_does_not_stop_the_host_opening() {
        let directory = tempfile::tempdir().unwrap();
        let identity = InMemoryProvider::from_seed([0xa1; 32]);
        let host = PersonalSyncHost::open(
            &identity,
            PersonalSyncHostConfig {
                graph: [0xa2; 32],
                store_path: directory.path().join("hinted.redb"),
                roster: SyncRoster::new([identity.master_public_key().to_bytes()]),
                selection: SyncSelection::default(),
                peer_tickets: Vec::new(),
                peer_hints: vec!["not a ticket at all".into(), String::new()],
                paired_nodes: Vec::new(),
                relay_urls: Vec::new(),
            },
        )
        .await
        .expect("a stored hint is best-effort; garbage must not stop the host");
        host.close().await.unwrap();
    }

    /// Reachability is not authority. A blob hash names bytes, not a right to
    /// them, so an unpaired peer is refused before any dial.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fetching_from_an_unpaired_device_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let identity = InMemoryProvider::from_seed([0x91; 32]);
        let stranger = InMemoryProvider::from_seed([0x92; 32])
            .master_public_key()
            .to_bytes();
        let host = PersonalSyncHost::open(
            &identity,
            PersonalSyncHostConfig {
                graph: [0x93; 32],
                store_path: directory.path().join("lonely.redb"),
                roster: SyncRoster::new([identity.master_public_key().to_bytes()]),
                selection: SyncSelection::default().with_blob_availability(true),
                peer_tickets: Vec::new(),
                peer_hints: Vec::new(),
                paired_nodes: Vec::new(),
                relay_urls: Vec::new(),
            },
        )
        .await
        .unwrap();

        let error = host
            .fetch_blob(stranger, [0x94; 32])
            .await
            .expect_err("an unpaired device must not be dialed");
        assert!(
            error.to_string().contains("not a paired device"),
            "the refusal must name the reason, got: {error}"
        );
        host.close().await.unwrap();
    }

    /// The counterpart to `smoke-transfer-accept.mjs`. That checks the bridge
    /// turns this form into the payload the host expects back; this checks the
    /// host emits the form at all.
    ///
    /// Both halves are needed because the failure is silent from either side:
    /// a card whose action carries no form renders no control, and an action
    /// whose form advertises no transfer submits a decision naming nothing.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_waiting_transfer_advertises_an_accept_form_naming_that_transfer() {
        let directory = tempfile::tempdir().unwrap();
        let identity = InMemoryProvider::from_seed([0xb1; 32]);
        let host = PersonalSyncHost::open(
            &identity,
            PersonalSyncHostConfig {
                graph: [0xb2; 32],
                store_path: directory.path().join("accept-form.redb"),
                roster: SyncRoster::new([identity.master_public_key().to_bytes()]),
                selection: SyncSelection::default().with_facets([TRANSFER_OFFER_FACET]),
                peer_tickets: Vec::new(),
                peer_hints: Vec::new(),
                paired_nodes: Vec::new(),
                relay_urls: Vec::new(),
            },
        )
        .await
        .unwrap();

        let device = format!("personae://device/{}", hex(&host.node_id()));
        let offer = TransferOfferV1 {
            schema: TRANSFER_OFFER_FACET.to_string(),
            transfer_id: Uuid::from_u128(0xb3),
            operation: crate::transfer::TransferOperation::Copy,
            source: crate::transfer::TransferEndpointV1 {
                graph: "graphshell://graph/accept".to_string(),
                persona: "personae://persona/owner".to_string(),
                device: device.clone(),
            },
            destination: crate::transfer::TransferEndpointV1 {
                graph: "graphshell://graph/accept".to_string(),
                persona: "personae://persona/owner".to_string(),
                device,
            },
            pairing_id: "pairing-accept".to_string(),
            manifest_blob: eidetic::Hash::of(b"manifest"),
            manifest_byte_len: 512,
            nodes: 2,
            relations: 1,
            blobs: 1,
            blob_bytes: 44,
            offered_at_ms: 1_700_000_000_000,
        };
        host.author(crate::transfer_offer::offer_events(&offer).unwrap())
            .await
            .unwrap();

        let cards = host.supplemental_cards().await.unwrap();
        let accept = cards
            .iter()
            .flat_map(|card| card.actions.iter())
            .find(|action| action.intent == TRANSFER_ACCEPT_INTENT)
            .expect("a waiting transfer advertises accept");

        let form = accept
            .input_form
            .as_ref()
            .expect("the browser can only compose a payload it was given a form for");
        form.validate().expect("the advertised form is well formed");
        assert_eq!(form.schema, accept.schema);
        assert_eq!(form.fields.len(), 1);
        assert_eq!(form.fields[0].name, "transfer_id");
        assert_eq!(
            form.fields[0]
                .choices
                .iter()
                .map(|choice| choice.value.as_str())
                .collect::<Vec<_>>(),
            [offer.transfer_id.to_string().as_str()],
            "the only accept-able transfer is the one this card is about"
        );

        // Every other card stays read-only. Opening the action door for one
        // card must not open it for the projection at large.
        assert!(
            cards
                .iter()
                .filter(|card| card.source_id != offer.transfer_id.to_string())
                .all(|card| card.actions.is_empty())
        );

        host.close().await.unwrap();
    }
}
