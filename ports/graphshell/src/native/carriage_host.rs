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
//! Its own endpoint, for now. The grammar puts carriage on a sibling topic of
//! the same transport stack, and this host mirrors `PersonalSyncHost`'s
//! wiring exactly; folding both lanes onto one bound endpoint is a later
//! step, because `set_topics` replaces a peer's topic set and the two hosts
//! would clobber each other's overlay tags today.

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

/// What one commissioning pass published, and what it honestly could not.
#[derive(Debug, Default)]
pub struct CarriagePublishReport {
    /// Each (device, persona) whose slot went onto the lane.
    pub published: Vec<(pandect::DeviceId, pandect::PersonaId)>,
    /// Leased devices whose wrapping key the wallet never retained, so their
    /// slots cannot be addressed. Pairing retains one; direct issue does not.
    pub skipped_no_wrapping_key: Vec<pandect::DeviceId>,
    /// Certificates obliged to carry epoch material that have no record yet.
    pub skipped_no_record: usize,
    /// Certificates whose grant expiry has already passed.
    pub skipped_grant_expired: usize,
}

/// One device's seat on one graph's carriage topic.
pub struct CarriageHost {
    graph: [u8; 32],
    store: MunimentStore<RedbBackend, CarriageExt>,
    joined: JoinedSpace<CarriageExt>,
    transport: P2pandaTransport,
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
        let intake_store = store.clone();
        let intake_held = Arc::clone(&held);
        let graph = config.graph;
        let trusted_roots = config.trusted_roots.clone();
        let ceilings = config.ceilings;
        let joined = JoinedSpace::join::<_, [u8; 32], _, _>(
            lane_id("graphshell/carriage/v1", graph),
            store.clone(),
            endpoint,
            gossip,
            carriage_topic(graph),
            move |operation| {
                let store = intake_store.clone();
                let held = Arc::clone(&intake_held);
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
        .await?;

        Ok(Self {
            graph,
            store,
            joined,
            transport,
            writer,
            trusted_roots: config.trusted_roots,
            ceilings: config.ceilings,
            held,
        })
    }

    pub fn node_id(&self) -> [u8; 32] {
        self.transport.local_peer_id().to_bytes()
    }

    pub async fn ticket(&self) -> Result<String, CarriageHostError> {
        self.transport
            .ticket()
            .await
            .map_err(|error| CarriageHostError::Transport(error.to_string()))
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

    /// Publish every slot this wallet's roster says should ride this lane.
    ///
    /// The issue-path integration: after grants are issued or refreshed
    /// through `pandect`, the wallet host calls this to put each leased
    /// device's wrapped-epoch records on the carriage topic. The roster is
    /// the authority for *whether* (the ruled layering: wallet roster grants
    /// carriage, pairing list routes it); this reads `CarriagePolicy` off
    /// each `DeviceRecord` and publishes only for devices leased onto this
    /// host's graph.
    ///
    /// Skips are reported rather than erred, because each is a normal state:
    /// a device with no retained wrapping key cannot have its slot addressed
    /// (the direct issue path retains none; pairing does), a certificate with
    /// no epoch record has nothing to carry, and an already-expired grant has
    /// nothing left to lease.
    pub async fn publish_grant_carriage(
        &self,
        data_root: &std::path::Path,
    ) -> Result<CarriagePublishReport, CarriageHostError> {
        let seed = pandect::load_identity_seed(data_root)
            .map_err(|error| CarriageHostError::Transport(error.to_string()))?
            .ok_or_else(|| {
                CarriageHostError::Refused("wallet root missing identity seed".into())
            })?;
        let provider = personae::InMemoryProvider::from_seed(seed);
        let roster = pandect::load_device_roster(data_root)
            .map_err(|error| CarriageHostError::Transport(error.to_string()))?
            .unwrap_or_else(pandect::DeviceRoster::new);
        let bridge = pandect::load_remote_auth_wrapping_key_bridge(data_root)
            .map_err(|error| CarriageHostError::Transport(error.to_string()))?
            .unwrap_or_default();

        let mut report = CarriagePublishReport::default();
        for device in &roster.devices {
            if roster.revoked.contains(&device.device_id) {
                continue;
            }
            let pandect::CarriagePolicy::Leased { max_ttl_ms, graph } = device.carriage else {
                continue;
            };
            if graph != self.graph {
                continue;
            }
            let Some(wrapping_key) = bridge
                .keys
                .iter()
                .find(|key| key.device_id == device.device_id)
                .map(|key| key.wrapping_key)
            else {
                report.skipped_no_wrapping_key.push(device.device_id);
                continue;
            };
            let set = pandect::load_device_grant_set(data_root, device.device_id)
                .map_err(|error| CarriageHostError::Transport(error.to_string()))?;
            for (persona, certificate) in &set.personas {
                if !pandect::requires_epoch_material(certificate) {
                    continue;
                }
                let certificate_id = certificate.certificate.id();
                let Some(record) = pandect::load_wrapped_epoch_record(data_root, certificate_id)
                    .map_err(|error| CarriageHostError::Transport(error.to_string()))?
                else {
                    report.skipped_no_record += 1;
                    continue;
                };
                let bytes = pandect::encode_epoch_record(&record)
                    .map_err(|error| CarriageHostError::Refused(error.to_string()))?;

                let now = now_ms();
                let mut expires_at_ms = now.saturating_add(max_ttl_ms);
                if let Some(grant_expiry) = certificate.certificate.expires_at_ms {
                    expires_at_ms = expires_at_ms.min(grant_expiry);
                }
                if expires_at_ms <= now {
                    report.skipped_grant_expired += 1;
                    continue;
                }

                let slot = pandect::blinded_slot_id(certificate_id, wrapping_key);
                let issue = self
                    .held
                    .read()
                    .await
                    .get(&slot)
                    .map(|lease| lease.issue + 1)
                    .unwrap_or(1);
                // The persona's chain-root keypair: the same authority whose
                // public key verifiers hold as this persona's TrustedRoot.
                let issuer = provider
                    .derive_keypair(&personae::carry::persona_wallet_salt(*persona))
                    .map_err(CarriageHostError::Identity)?;
                self.publish_slot(
                    &issuer,
                    slot,
                    issue,
                    expires_at_ms,
                    bytes,
                    CarriageCeilings {
                        device_max_ttl_ms: Some(max_ttl_ms),
                        grant_expires_at_ms: certificate.certificate.expires_at_ms,
                    },
                )
                .await?;
                report.published.push((device.device_id, *persona));
            }
        }
        Ok(report)
    }

    pub async fn close(self) -> Result<(), CarriageHostError> {
        self.joined.leave();
        self.transport
            .close()
            .await
            .map_err(|error| CarriageHostError::Transport(error.to_string()))
    }
}

#[cfg(test)]
#[path = "carriage_host_tests.rs"]
mod tests;
