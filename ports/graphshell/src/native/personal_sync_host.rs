//! Resident owner of Graphshell's durable personal-graph replica.

use std::path::PathBuf;

use graphshell_protocol::{CardValueV1, PortableCardV1};
use muniment::RedbBackend;
use personae::IdentityProvider;
use stickleback::{JoinError, JoinedSpace, SyncStatus};
use tokio::sync::Mutex;
use transport::{P2pandaTransport, sync_overlay_topic};

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
    roster: SyncRoster,
    replica: Mutex<PersonalGraphReplica<RedbBackend>>,
    joined: JoinedSpace<PersonalGraphExt>,
    transport: P2pandaTransport,
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
        let transport = P2pandaTransport::builder(&transport_key)
            .gossip()
            .bind()
            .await
            .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))?;
        let overlay = sync_overlay_topic(config.graph);
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
        let roster = config.roster.clone();
        let graph = config.graph;
        let (endpoint, gossip) = transport
            .sync_parts()
            .ok_or_else(|| PersonalSyncHostError::Transport("gossip is unavailable".into()))?;
        let joined =
            JoinedSpace::join::<_, u64, _, _>(store, endpoint, gossip, graph, move |operation| {
                let store = accepted.clone();
                let roster = roster.clone();
                async move {
                    accept_into(&store, graph, &roster, &operation)
                        .await
                        .unwrap_or(false)
                }
            })
            .await?;

        Ok(Self {
            graph,
            store_path: config.store_path,
            roster: config.roster,
            replica: Mutex::new(replica),
            joined,
            transport,
        })
    }

    pub fn graph(&self) -> [u8; 32] {
        self.graph
    }

    pub fn roster(&self) -> &SyncRoster {
        &self.roster
    }

    pub fn sync_status(&self) -> SyncStatus {
        self.joined.sync_status()
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
        host.close().await.unwrap();

        let reopened = PersonalSyncHost::open(&identity, config()).await.unwrap();
        let cards = reopened.supplemental_cards().await.unwrap();
        assert!(
            cards
                .iter()
                .any(|card| card.card.title == "Resident graph node")
        );
    }
}
