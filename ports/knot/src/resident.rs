//! Always-on Knot sync.
//!
//! [`KnotSyncStore::join`](crate::KnotSyncStore::join) has been production code
//! for a while, but nothing shipped ever called it: `transport` was a
//! dev-dependency, so Knot's p2panda convergence was real and exercised only by
//! tests. The shipped endpoint bound no transport at all and therefore never
//! synchronised anything. This is the missing half.
//!
//! Identity here is deliberately split, because carrying a persona's private
//! epoch between devices makes the two halves pull in opposite directions:
//!
//! - the **vault key** and **space id** must be identical across a persona's
//!   devices, or they cannot decrypt or address the same space;
//! - the **writer key** must not be, because its public half is also the
//!   transport node id. Two devices deriving one writer would be a single node
//!   on the network and a single author in a per-author log.
//!
//! [`StartupUnlockedPersonalVault`](crate::StartupUnlockedPersonalVault)
//! resolves that by mixing the device's own Personae root into the writer
//! derivation only.

use stickleback::{JoinError, JoinedSpace, SyncStatus};
use transport::p2panda_transport::{MdnsDiscoveryMode, RelayUrl};
use transport::{P2pandaTransport, PeerID, Transport, sync_overlay_topic};

use crate::sync::{KnotSyncExt, KnotSyncFileStore};

/// How this device reaches the persona's other devices.
#[derive(Clone, Debug, Default)]
pub struct KnotSyncHostConfig {
    /// Writer keys of this persona's other devices. Each doubles as that
    /// device's transport node id, so one value serves both reachability and
    /// admission; unlike the personal graph, Knot does not need them recorded
    /// separately.
    pub paired_writers: Vec<[u8; 32]>,
    /// iroh relays. Empty leaves this device LAN-only: p2panda registers no
    /// relay by default.
    pub relay_urls: Vec<RelayUrl>,
}

#[derive(Debug, thiserror::Error)]
pub enum KnotSyncHostError {
    #[error("Knot sync transport failed: {0}")]
    Transport(String),
    #[error(transparent)]
    Join(#[from] JoinError),
}

/// A bound transport and live LogSync session over one Knot space.
pub struct KnotSyncHost {
    joined: JoinedSpace<KnotSyncExt>,
    transport: P2pandaTransport,
    space_id: [u8; 32],
}

impl KnotSyncHost {
    /// Bind a transport for `signing_seed` and join `store`'s space.
    ///
    /// The transport key is the writer seed, so a device's node id and its
    /// author identity are the same value. That is what lets a paired writer
    /// serve as both the thing admitted and the thing dialled.
    pub async fn open(
        store: &KnotSyncFileStore,
        signing_seed: [u8; 32],
        config: KnotSyncHostConfig,
    ) -> Result<Self, KnotSyncHostError> {
        let mut builder = P2pandaTransport::builder_from_seed(signing_seed)
            .gossip()
            .mdns(MdnsDiscoveryMode::Active);
        for url in config.relay_urls {
            builder = builder.relay_url(url);
        }
        let transport = builder
            .bind()
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))?;

        let overlay = sync_overlay_topic(store.space_id());
        for writer in &config.paired_writers {
            let peer = PeerID::from_bytes(writer)
                .map_err(|error| KnotSyncHostError::Transport(format!("paired writer {error}")))?;
            transport
                .set_topics(peer, &[overlay])
                .await
                .map_err(|error| KnotSyncHostError::Transport(error.to_string()))?;
        }

        let (endpoint, gossip) = transport
            .sync_parts()
            .ok_or_else(|| KnotSyncHostError::Transport("gossip is unavailable".into()))?;
        let joined = store.join(endpoint, gossip).await?;
        Ok(Self {
            joined,
            transport,
            space_id: store.space_id(),
        })
    }

    /// This device's node id, which is also its writer key: what the other
    /// devices must admit.
    pub fn node_id(&self) -> [u8; 32] {
        self.transport.local_peer_id().to_bytes()
    }

    pub fn sync_status(&self) -> SyncStatus {
        self.joined.sync_status()
    }

    /// A ticket for the across-network case a relay cannot serve. Rebuilt on
    /// every bind, so it is a bootstrap value and never a stored one.
    pub async fn ticket(&self) -> Result<String, KnotSyncHostError> {
        self.transport
            .ticket()
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))
    }

    /// Admit and reach another device without a restart.
    pub async fn pair_writer(&self, writer: [u8; 32]) -> Result<(), KnotSyncHostError> {
        let peer = PeerID::from_bytes(&writer)
            .map_err(|error| KnotSyncHostError::Transport(format!("paired writer {error}")))?;
        self.transport
            .set_topics(peer, &[sync_overlay_topic(self.space_id)])
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))
    }
}
