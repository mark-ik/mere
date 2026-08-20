//! The resident host for the epoch carriage lane.
//!
//! Joins one graph's carriage topic, admits slots through
//! [`CarriageAdmissionPolicy`], and answers recovery: a device that lost its
//! wrapped-epoch record asks its slot back from a peer replica while the
//! lease is live, without re-pairing. The lane grammar (design_docs,
//! 2026-08-18) is what this hosts; the replica-set ruling (wallet roster
//! grants carriage, pairing list routes) is enforced by who gets a lease
//! issued and who gets paired, not here.
//!
//! Two ways onto the wire. [`CarriageHost::open`] binds its own endpoint,
//! mirroring `PersonalSyncHost`'s wiring; [`CarriageHost::attach`] joins the
//! carriage topic on the sync host's already-bound endpoint, which is the
//! grammar's sibling-topic ruling made physical. The fold was unblocked by
//! `add_topics`, the append form of `set_topics`, so the second lane's
//! overlay tag no longer clobbers the first's.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use muniment::RedbBackend;
use p2panda_core::{Body, Hash, Header, Operation, SigningKey, Topic, VerifyingKey};
use pandect::BlindedSlotId;
use personae::{Ed25519Keypair, IdentityProvider};
use stickleback::{JoinedSpace, MunimentStore, OperationProcessor, lane_id};
use tokio::sync::RwLock;
use transport::p2panda_transport::MdnsDiscoveryMode;
use transport::{P2pandaTransport, PeerID, Transport, sync_overlay_topic};

use crate::carriage::{
    CarriageAdmissionPolicy, CarriageCeilings, CarriageExt, CarriagePurgeProposal, HeldLease,
    carriage_log, carriage_topic, propose_carriage_purge, sign_lease,
};

use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;

/// Everything opening a carriage host needs to know.
pub struct CarriageHostConfig {
    /// The personal graph whose carriage topic this host joins.
    pub graph: [u8; 32],
    /// Where the carriage store lives, beside the graph's own store.
    pub store_path: PathBuf,
    /// One trusted issuer root per persona of the owner (the M5 shape).
    pub trusted_roots: Vec<[u8; 32]>,
    /// The ceilings this host can assert beyond the absolute backstop.
    pub ceilings: CarriageCeilings,
    /// Peer tickets to dial and tag onto the carriage overlay.
    pub peer_tickets: Vec<String>,
    /// Paired node ids to tag onto the overlay, dialable via discovery.
    pub paired_nodes: Vec<[u8; 32]>,
}

/// One device's seat on one graph's carriage topic.
pub struct CarriageHost {
    graph: [u8; 32],
    store: MunimentStore<RedbBackend, CarriageExt>,
    joined: JoinedSpace<CarriageExt>,
    /// Owned in standalone mode; `None` when attached to the personal sync
    /// host's endpoint, which then outlives and closes the transport.
    transport: Option<P2pandaTransport>,
    node_id: [u8; 32],
    writer: SigningKey,
    trusted_roots: Vec<[u8; 32]>,
    ceilings: CarriageCeilings,
    held: Arc<RwLock<HashMap<BlindedSlotId, HeldLease>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum CarriageHostError {
    #[error("carriage transport failed: {0}")]
    Transport(String),
    #[error("carriage store failed: {0}")]
    Store(#[from] muniment::StoreError),
    #[error("carriage identity failed: {0}")]
    Identity(#[from] personae::IdentityError),
    #[error("carriage lane refused the slot: {0}")]
    Refused(String),
    #[error("carriage lane failed to process: {0}")]
    Process(String),
    #[error("carriage lane join failed: {0}")]
    Join(#[from] stickleback::JoinError),
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// This device's carriage writer identity, derived per graph like the graph
/// lane's own, under a distinct salt so the two writers cannot be confused.
fn carriage_identity_salt(graph: [u8; 32]) -> Vec<u8> {
    let mut salt = Vec::with_capacity(64);
    salt.extend_from_slice(b"graphshell.carriage.writer.v1/");
    salt.extend_from_slice(&graph);
    salt
}

/// Rebuild the held-slot view from the store, newest issue per slot.
///
/// This is what makes admission's ordering check honest across a restart: the
/// view is the store's, not a cache that could survive the store changing
/// under it. A prune leaves at most the head per (author, slot) log, but two
/// authors may have written one slot, so the fold still takes the max.
async fn scan_held(
    store: &MunimentStore<RedbBackend, CarriageExt>,
    graph: [u8; 32],
) -> Result<HashMap<BlindedSlotId, HeldLease>, CarriageHostError> {
    let topic = Topic::from(carriage_topic(graph));
    let by_author: std::collections::BTreeMap<VerifyingKey, Vec<[u8; 32]>> =
        TopicStore::<Topic, VerifyingKey, [u8; 32]>::resolve(store, &topic).await?;
    let mut held: HashMap<BlindedSlotId, HeldLease> = HashMap::new();
    for (author, mut logs) in by_author {
        logs.sort_unstable();
        logs.dedup();
        for log_id in logs {
            let entries = LogStore::<Operation<CarriageExt>, VerifyingKey, [u8; 32], u32, Hash>::get_log_entries(
                store, &author, &log_id, None, None,
            )
            .await?
            .unwrap_or_default();
            for (operation, _) in entries {
                let ext = &operation.header.extensions;
                let Some(payload) = operation.header.payload_hash else {
                    continue;
                };
                let lease = HeldLease {
                    issue: ext.issue,
                    payload,
                    expires_at_ms: ext.expires_at_ms,
                };
                held.entry(ext.slot)
                    .and_modify(|current| {
                        if lease.issue > current.issue {
                            *current = lease;
                        }
                    })
                    .or_insert(lease);
            }
        }
    }
    Ok(held)
}

impl CarriageHost {
    pub async fn open<P: IdentityProvider + ?Sized>(
        identity: &P,
        config: CarriageHostConfig,
    ) -> Result<Self, CarriageHostError> {
        if let Some(parent) = config.store_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| CarriageHostError::Transport(error.to_string()))?;
        }
        let backend = RedbBackend::open(&config.store_path)?;
        let store: MunimentStore<RedbBackend, CarriageExt> = MunimentStore::new(backend);
        let held = Arc::new(RwLock::new(scan_held(&store, config.graph).await?));

        let transport_key = identity.derive_keypair(&carriage_identity_salt(config.graph))?;
        let writer = SigningKey::from_bytes(&transport_key.to_seed());
        let transport = P2pandaTransport::builder(&transport_key)
            .gossip()
            .mdns(MdnsDiscoveryMode::Active)
            .bind()
            .await
            .map_err(|error| CarriageHostError::Transport(error.to_string()))?;

        let overlay = sync_overlay_topic(carriage_topic(config.graph));
        for node in &config.paired_nodes {
            let peer = PeerID::from_bytes(node)
                .map_err(|error| CarriageHostError::Transport(format!("paired node: {error}")))?;
            transport
                .set_topics(peer, &[overlay])
                .await
                .map_err(|error| CarriageHostError::Transport(error.to_string()))?;
        }
        for ticket in &config.peer_tickets {
            let peer = transport
                .add_peer_ticket(ticket)
                .await
                .map_err(|error| CarriageHostError::Transport(error.to_string()))?;
            transport
                .set_topics(peer, &[overlay])
                .await
                .map_err(|error| CarriageHostError::Transport(error.to_string()))?;
        }

        let (endpoint, gossip) = transport
            .sync_parts()
            .ok_or_else(|| CarriageHostError::Transport("gossip is unavailable".into()))?;
        let graph = config.graph;
        let joined = Self::join_lane(
            graph,
            store.clone(),
            endpoint,
            gossip,
            config.trusted_roots.clone(),
            config.ceilings,
            Arc::clone(&held),
        )
        .await?;

        Ok(Self {
            graph,
            store,
            joined,
            node_id: transport.local_peer_id().to_bytes(),
            transport: Some(transport),
            writer,
            trusted_roots: config.trusted_roots,
            ceilings: config.ceilings,
            held,
        })
    }

    /// Join the carriage topic on an already-bound personal sync endpoint,
    /// instead of opening a second endpoint per device.
    ///
    /// This is the fold the module header used to rule out: it became
    /// possible when `add_topics` landed as the append form of `set_topics`,
    /// so the carriage overlay tag no longer clobbers the graph lane's. The
    /// writer identity stays carriage's own derivation; only the wire is
    /// shared. Peers reach an attached host through the sync host's ticket
    /// and pairing, which is the layering ruling made physical: the pairing
    /// list routes, and it routes both lanes at once.
    pub async fn attach<P: IdentityProvider + ?Sized>(
        identity: &P,
        sync: &super::personal_sync_host::PersonalSyncHost,
        store_path: PathBuf,
        trusted_roots: Vec<[u8; 32]>,
        ceilings: CarriageCeilings,
        peers: &[[u8; 32]],
    ) -> Result<Self, CarriageHostError> {
        let graph = sync.graph();
        if let Some(parent) = store_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| CarriageHostError::Transport(error.to_string()))?;
        }
        let backend = RedbBackend::open(&store_path)?;
        let store: MunimentStore<RedbBackend, CarriageExt> = MunimentStore::new(backend);
        let held = Arc::new(RwLock::new(scan_held(&store, graph).await?));
        let transport_key = identity.derive_keypair(&carriage_identity_salt(graph))?;
        let writer = SigningKey::from_bytes(&transport_key.to_seed());

        let overlay = sync_overlay_topic(carriage_topic(graph));
        for node in peers {
            sync.add_topics(*node, &[overlay])
                .await
                .map_err(|error| CarriageHostError::Transport(error.to_string()))?;
        }
        let (endpoint, gossip) = sync
            .sync_parts()
            .ok_or_else(|| CarriageHostError::Transport("gossip is unavailable".into()))?;
        let joined = Self::join_lane(
            graph,
            store.clone(),
            endpoint,
            gossip,
            trusted_roots.clone(),
            ceilings,
            Arc::clone(&held),
        )
        .await?;
        Ok(Self {
            graph,
            store,
            joined,
            node_id: sync.node_id(),
            transport: None,
            writer,
            trusted_roots,
            ceilings,
            held,
        })
    }

    pub fn node_id(&self) -> [u8; 32] {
        self.node_id
    }

    /// A dial ticket, in standalone mode only. An attached host shares the
    /// sync host's endpoint, so peers dial that host's ticket.
    pub async fn ticket(&self) -> Result<String, CarriageHostError> {
        let Some(transport) = &self.transport else {
            return Err(CarriageHostError::Transport(
                "attached carriage shares the sync host's endpoint; use its ticket".into(),
            ));
        };
        transport
            .ticket()
            .await
            .map_err(|error| CarriageHostError::Transport(error.to_string()))
    }

    /// Join the carriage lane over any endpoint, own or shared: the one
    /// intake both constructors speak, so an attached host admits exactly
    /// what a standalone one would.
    async fn join_lane(
        graph: [u8; 32],
        store: MunimentStore<RedbBackend, CarriageExt>,
        endpoint: transport::p2panda_transport::Endpoint,
        gossip: transport::p2panda_transport::Gossip,
        trusted_roots: Vec<[u8; 32]>,
        ceilings: CarriageCeilings,
        held: Arc<RwLock<HashMap<BlindedSlotId, HeldLease>>>,
    ) -> Result<JoinedSpace<CarriageExt>, CarriageHostError> {
        let intake_store = store.clone();
        Ok(JoinedSpace::join::<_, [u8; 32], _, _>(
            lane_id("graphshell/carriage/v1", graph),
            store,
            endpoint,
            gossip,
            carriage_topic(graph),
            move |operation| {
                let store = intake_store.clone();
                let held = Arc::clone(&held);
                let trusted_roots = trusted_roots.clone();
                async move {
                    let view = held.read().await.clone();
                    let policy = CarriageAdmissionPolicy::new(
                        graph,
                        now_ms(),
                        trusted_roots,
                        ceilings,
                        Some(view),
                    );
                    let processor = OperationProcessor::new(store, policy);
                    match processor.process(&operation).await {
                        Ok(outcome) if outcome.inserted() => {
                            let ext = &operation.header.extensions;
                            if let Some(payload) = operation.header.payload_hash {
                                held.write().await.insert(
                                    ext.slot,
                                    HeldLease {
                                        issue: ext.issue,
                                        payload,
                                        expires_at_ms: ext.expires_at_ms,
                                    },
                                );
                            }
                            true
                        }
                        Ok(_) => true,
                        Err(error) => {
                            // Same discipline as the graph lane: a refused
                            // slot and a broken intake both answer false, so
                            // the cause survives only if logged here.
                            tracing::warn!(%error, "carriage lane refused or failed an incoming slot");
                            false
                        }
                    }
                }
            },
        )
        .await?)
    }

    /// Publish one slot version: sign the lease, chain it onto this writer's
    /// per-slot log, admit it through the same policy peers apply, and only
    /// then push it onto the live lane.
    ///
    /// Going through admission locally is what makes issue-side mistakes
    /// loud: a lease that violates a ceiling is refused here, at the issuer,
    /// rather than silently dropped by every peer.
    pub async fn publish_slot(
        &self,
        issuer: &Ed25519Keypair,
        slot: BlindedSlotId,
        issue: u64,
        expires_at_ms: u64,
        record: Vec<u8>,
        ceilings: CarriageCeilings,
    ) -> Result<(), CarriageHostError> {
        let body = Body::new(&record);
        let ext = CarriageExt {
            graph: self.graph,
            slot,
            issue,
            expires_at_ms,
            prune_flag: p2panda_core::prune::PruneFlag::new(true),
            issuer_signature: sign_lease(
                issuer,
                self.graph,
                slot,
                issue,
                expires_at_ms,
                body.hash(),
            ),
        };
        let verifying_key = self.writer.verifying_key();
        let log_id = carriage_log(slot);
        let entries =
            LogStore::<Operation<CarriageExt>, VerifyingKey, [u8; 32], u32, Hash>::get_log_entries(
                &self.store,
                &verifying_key,
                &log_id,
                None,
                None,
            )
            .await?
            .unwrap_or_default();
        let latest = entries
            .into_iter()
            .map(|(operation, _)| operation)
            .max_by_key(|operation| operation.header.seq_num);
        let (seq_num, backlink) = match latest {
            Some(operation) => (operation.header.seq_num + 1, Some(operation.hash)),
            None => (0, None),
        };
        let mut header = Header {
            version: 1,
            verifying_key,
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            seq_num,
            backlink,
            extensions: ext,
        };
        header.sign(&self.writer);
        let operation = Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        };

        let view = self.held.read().await.clone();
        let policy = CarriageAdmissionPolicy::new(
            self.graph,
            now_ms(),
            self.trusted_roots.clone(),
            ceilings,
            Some(view),
        );
        let processor = OperationProcessor::new(self.store.clone(), policy);
        let outcome = processor
            .process(&operation)
            .await
            .map_err(|error| CarriageHostError::Refused(error.to_string()))?;
        if outcome.inserted() {
            self.held.write().await.insert(
                slot,
                HeldLease {
                    issue,
                    payload: operation.header.payload_hash.expect("body was attached"),
                    expires_at_ms,
                },
            );
        }
        self.joined
            .publish(operation)
            .map_err(CarriageHostError::Join)?;
        Ok(())
    }

    /// Recover one slot's record bytes, refusing an expired lease on read.
    ///
    /// This is the whole point of the lane: the caller lost its wallet file
    /// and gets it back from what peers replicated, then unwraps it with the
    /// pairing key it still holds. `None` is honest for both "never held" and
    /// "held but expired", because a replica must not serve stale material.
    pub async fn recover(&self, slot: BlindedSlotId) -> Result<Option<Vec<u8>>, CarriageHostError> {
        let lease = { self.held.read().await.get(&slot).copied() };
        let Some(lease) = lease else {
            return Ok(None);
        };
        if lease.expires_at_ms <= now_ms() {
            return Ok(None);
        }
        let held = scan_held(&self.store, self.graph).await?;
        let Some(current) = held.get(&slot) else {
            return Ok(None);
        };
        let topic = Topic::from(carriage_topic(self.graph));
        let by_author: std::collections::BTreeMap<VerifyingKey, Vec<[u8; 32]>> =
            TopicStore::<Topic, VerifyingKey, [u8; 32]>::resolve(&self.store, &topic).await?;
        for (author, logs) in by_author {
            if !logs.contains(&carriage_log(slot)) {
                continue;
            }
            let entries = LogStore::<Operation<CarriageExt>, VerifyingKey, [u8; 32], u32, Hash>::get_log_entries(
                &self.store,
                &author,
                &carriage_log(slot),
                None,
                None,
            )
            .await?
            .unwrap_or_default();
            for (operation, _) in entries {
                if operation.header.payload_hash == Some(current.payload) {
                    if let Some(body) = operation.body {
                        return Ok(Some(body.to_bytes()));
                    }
                }
            }
        }
        Ok(None)
    }

    /// How many live slots peers could recover from this host right now.
    pub async fn held_count(&self) -> usize {
        self.held.read().await.len()
    }

    /// The purge dry run over this host's held slots.
    pub async fn propose_purge(&self) -> CarriagePurgeProposal {
        let held = self.held.read().await.clone();
        propose_carriage_purge(now_ms(), Some(&held))
    }

    /// Execute a reviewed purge: delete each expired slot's operations and
    /// drop them from the held view. Refuses a blocked proposal outright.
    pub async fn execute_purge(
        &self,
        proposal: &CarriagePurgeProposal,
    ) -> Result<usize, CarriageHostError> {
        if !proposal.is_executable() {
            return Err(CarriageHostError::Refused(
                "a blocked purge proposal is not executable".into(),
            ));
        }
        let topic = Topic::from(carriage_topic(self.graph));
        let by_author: std::collections::BTreeMap<VerifyingKey, Vec<[u8; 32]>> =
            TopicStore::<Topic, VerifyingKey, [u8; 32]>::resolve(&self.store, &topic).await?;
        let mut purged = 0usize;
        for slot in &proposal.expired {
            for (author, logs) in &by_author {
                if !logs.contains(&carriage_log(*slot)) {
                    continue;
                }
                let entries = LogStore::<Operation<CarriageExt>, VerifyingKey, [u8; 32], u32, Hash>::get_log_entries(
                    &self.store,
                    author,
                    &carriage_log(*slot),
                    None,
                    None,
                )
                .await?
                .unwrap_or_default();
                for (operation, _) in entries {
                    if self.store.delete_operation(&operation.hash).await? {
                        purged += 1;
                    }
                }
            }
            self.held.write().await.remove(slot);
        }
        Ok(purged)
    }

    pub async fn close(self) -> Result<(), CarriageHostError> {
        self.joined.leave();
        if let Some(transport) = self.transport {
            transport
                .close()
                .await
                .map_err(|error| CarriageHostError::Transport(error.to_string()))?;
        }
        Ok(())
    }
}

#[path = "carriage_host_commissioning.rs"]
mod commissioning;
pub use commissioning::{
    CarriagePublishReport, CarriageRetractReport, RETRACTION_TTL_MS, RetractionTarget,
};

#[cfg(test)]
#[path = "carriage_host_tests.rs"]
mod tests;
