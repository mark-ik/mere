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
use transport::p2panda_transport::{KnownPeer, MdnsDiscoveryMode, RelayUrl};
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
    /// Endpoint tickets recorded from previous runs, seeded at open as
    /// best-effort dial candidates.
    ///
    /// Hints, not arguments: a ticket that fails to parse or dial is logged
    /// and skipped, because a route learned last week must degrade quietly
    /// where a value the owner just typed should fail loudly. Identity stays
    /// the writer key; this only turns a paired record into a route.
    pub peer_hints: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum KnotSyncHostError {
    #[error("Knot sync transport failed: {0}")]
    Transport(String),
    #[error(transparent)]
    Join(#[from] JoinError),
}

/// Which peers' addresses are worth writing back to settings.
///
/// Only the ones this host currently holds a live path to. The distinction
/// between `reachable` and `connected` is the whole of this function, and it
/// is not pedantry: an address the endpoint holds for a peer it is *not*
/// talking to may be exactly the stale route a working hint would replace, so
/// writing it back would overwrite good information with bad. A firewall can
/// drop every packet to a device while its address stays in the book, which
/// makes `reachable` look healthy while nothing replicates at all.
fn writers_to_refresh(peers: &[KnownPeer]) -> Vec<[u8; 32]> {
    peers
        .iter()
        .filter(|peer| peer.connected)
        .map(|peer| peer.peer.to_bytes())
        .collect()
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

        // The cached-address rung, as Graphshell has it: a device that has
        // connected once can redial after both ends restart with no discovery
        // working at all.
        for hint in &config.peer_hints {
            match transport.add_peer_ticket(hint).await {
                Ok(peer) => {
                    tracing::debug!(peer = %crate::hex32(&peer.to_bytes()), "seeded a stored dial hint")
                }
                Err(error) => {
                    tracing::warn!(%error, "a stored dial hint was unusable; skipping it")
                }
            }
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

    /// Which paired devices the transport currently associates with this
    /// space, and whether each is merely known or actually talking.
    ///
    /// Pairing records identity; this reports reachability, which is the fact
    /// a writer key cannot carry on its own.
    pub async fn known_peers(&self) -> Result<Vec<KnownPeer>, KnotSyncHostError> {
        self.transport
            .peers_for_topic(sync_overlay_topic(self.space_id))
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))
    }

    /// Where the endpoint currently believes `writer` lives, as a ticket, if
    /// it holds any addresses for it. The value the cached-address rung
    /// persists back into settings.
    pub async fn peer_ticket(&self, writer: [u8; 32]) -> Result<Option<String>, KnotSyncHostError> {
        let peer = PeerID::from_bytes(&writer)
            .map_err(|error| KnotSyncHostError::Transport(format!("paired writer {error}")))?;
        self.transport
            .peer_ticket(peer)
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))
    }

    /// Write back the addresses of devices this host is actually talking to.
    ///
    /// The other half of the cached-address rung: [`open`](Self::open) seeds
    /// stored hints, and this is what puts them there in the first place.
    /// Without it a hint only ever arrives if something outside Knot records
    /// one.
    ///
    /// Three disciplines, each of which the Graphshell lane learned the hard
    /// way:
    ///
    /// - **Connected peers only**, per [`writers_to_refresh`].
    /// - **Only on change.** [`KnotSyncHost::peer_ticket`] sorts addresses
    ///   before serialising, so an unchanged address set yields an identical
    ///   string and costs no settings write.
    /// - **Reload before saving.** The settings file has a second writer: a
    ///   `--pair-writer` invocation can land between the caller's read and
    ///   this write, so the refresh loads the latest, modifies, and saves
    ///   rather than persisting a snapshot taken seconds ago.
    pub async fn refresh_dial_hints(
        &self,
        sync: &crate::KnotSyncSettings,
        settings_file: &std::path::Path,
    ) {
        let peers = match self.known_peers().await {
            Ok(peers) => peers,
            Err(error) => {
                tracing::warn!(%error, "could not read the peer directory");
                return;
            }
        };

        for writer in writers_to_refresh(&peers) {
            let ticket = match self.peer_ticket(writer).await {
                Ok(Some(ticket)) => ticket,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(%error, "could not read a peer's current address");
                    continue;
                }
            };
            if sync.endpoint_for(&writer) == Some(ticket.as_str()) {
                continue;
            }
            let mut latest = match crate::KnotSettings::load(settings_file) {
                Ok(latest) => latest,
                Err(error) => {
                    tracing::warn!(%error, "could not reload settings to refresh a hint");
                    continue;
                }
            };
            let Some(live) = latest.sync.as_mut() else {
                continue;
            };
            // `remember_endpoint` ignores a writer that is no longer paired,
            // so an unpair landing in this window cannot be undone by a route.
            if !live.remember_endpoint(writer, &ticket) {
                continue;
            }
            match latest.save(settings_file) {
                Ok(()) => tracing::info!(
                    writer = %crate::hex32(&writer),
                    "recorded a fresh dial hint for a connected device"
                ),
                Err(error) => tracing::warn!(%error, "could not persist a refreshed dial hint"),
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A real Ed25519 public key: a peer id is a curve point, so an array of
    /// repeated bytes will not parse as one.
    fn writer(seed: u8) -> [u8; 32] {
        personae::Ed25519Keypair::from_seed([seed; 32])
            .public_key()
            .to_bytes()
    }

    fn peer(seed: u8, reachable: bool, connected: bool) -> KnownPeer {
        KnownPeer {
            peer: PeerID::from_bytes(&writer(seed)).expect("a valid peer key"),
            reachable,
            bootstrap: false,
            connected,
        }
    }

    #[test]
    fn only_connected_peers_have_their_addresses_written_back() {
        let peers = [
            // Known and addressed, but nothing is flowing: its address may be
            // the stale one a good hint would replace.
            peer(1, true, false),
            // Actually talking: this address is true right now.
            peer(2, true, true),
            // Named by discovery, no address at all.
            peer(3, false, false),
        ];

        assert_eq!(
            writers_to_refresh(&peers),
            vec![writer(2)],
            "a reachable-but-silent peer must not overwrite a working hint"
        );
    }

    #[test]
    fn nothing_is_written_back_when_no_device_is_talking() {
        let peers = [peer(1, true, false), peer(2, true, false)];
        assert!(
            writers_to_refresh(&peers).is_empty(),
            "an address book full of unreachable devices records no routes"
        );
    }
}
