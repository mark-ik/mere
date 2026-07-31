//! Bring-up and pairing for the resident host's personal-graph sync.
//!
//! Split out of the device-host binary so the settings resolution, the data
//! root migration and the pairing write are ordinary library code with tests,
//! rather than logic reachable only by running the resident host.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use personae::{IdentityProvider, ProfileId};

use crate::native::device_broker::DeviceSupplementalCards;
use crate::native::owner_settings::{
    self, DataRootMigration, OwnerSettings, OwnerSettingsError, SyncOverrides,
};
use crate::native::personal_sync_host::{
    PersonalSyncHost, PersonalSyncHostConfig, PersonalSyncHostError,
};
use crate::personal_sync::{PersonalGraphEvent, SyncRoster, SyncSelection};

const PERSONAL_GRAPH_DOMAIN: &[u8] = b"mere.graphshell/personal-graph/v1";

/// How often the resident host re-reads the settings file to pick up a device
/// paired while it was running.
const PAIRING_POLL: std::time::Duration = std::time::Duration::from_secs(5);

/// The graph a name refers to. Both devices must use the same name.
pub fn personal_graph_id(name: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(PERSONAL_GRAPH_DOMAIN);
    hasher.update(name.as_bytes());
    *hasher.finalize().as_bytes()
}

#[derive(Debug, thiserror::Error)]
pub enum DeviceSyncError {
    #[error(transparent)]
    Settings(#[from] OwnerSettingsError),
    #[error(transparent)]
    Host(#[from] PersonalSyncHostError),
    #[error(
        "no personal graph is configured for profile {profile:?}; set sync.graph in {path} first"
    )]
    NoGraphConfigured { profile: String, path: String },
}

/// A node to author into the graph as the host starts.
///
/// A deliberate stopgap. The product path for editing the graph is a typed
/// intent over the admitted session, which is the H9 lane; until that exists
/// the resident host is a sync engine with no input, and nothing can be proven
/// to converge because nothing can be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeedNote {
    pub address: String,
    pub title: String,
}

/// Resolve the data root, moving it out of the Personae vault once if it is
/// still there. An explicit override is the owner naming a location, so it is
/// taken as given.
pub fn resolve_data_root(
    app_dir: &Path,
    vault_dir: &Path,
    override_path: Option<PathBuf>,
) -> Result<PathBuf, DeviceSyncError> {
    if let Some(explicit) = override_path {
        return Ok(explicit);
    }
    let current = owner_settings::default_data_root(app_dir);
    let legacy = owner_settings::legacy_data_root(vault_dir);
    if let DataRootMigration::Moved { from, to } =
        owner_settings::migrate_data_root(&legacy, &current)?
    {
        tracing::info!(
            from = %from.display(),
            to = %to.display(),
            "moved the Graphshell data root out of the Personae vault"
        );
    }
    Ok(current)
}

/// Start personal sync for a profile, or return `None` when the owner has not
/// enabled it.
pub async fn start<P: IdentityProvider + ?Sized>(
    identity: &P,
    app_dir: &Path,
    vault_dir: &Path,
    profile: &ProfileId,
    data_root_override: Option<PathBuf>,
    overrides: SyncOverrides,
    peer_tickets: Vec<String>,
    seed_notes: Vec<SeedNote>,
) -> Result<Option<DeviceSupplementalCards>, DeviceSyncError> {
    let settings_file = owner_settings::settings_path(app_dir, profile);
    let stored = OwnerSettings::load(&settings_file)?;
    tracing::info!(
        path = %settings_file.display(),
        configured = stored.sync.is_some(),
        "owner settings"
    );
    let Some(sync) = owner_settings::resolve_sync(stored.sync, overrides) else {
        return Ok(None);
    };

    let graph = personal_graph_id(&sync.graph);
    let data_root = resolve_data_root(app_dir, vault_dir, data_root_override)?;
    let store_path = sync.store_path.clone().unwrap_or_else(|| {
        data_root
            .join("personal-sync")
            .join(format!("{}.redb", owner_settings::hex32(&graph)))
    });
    let mut roots = sync.roster_root_keys()?;
    roots.push(identity.master_public_key().to_bytes());
    roots.sort_unstable();
    roots.dedup();
    let paired_nodes = sync.paired_node_keys()?;
    // A malformed relay url is refused rather than dropped: silently starting
    // LAN-only when the owner asked for a relay would look like the relay was
    // configured and simply not helping.
    let relays = sync
        .relay_urls
        .iter()
        .map(|url| {
            url.parse::<transport::p2panda_transport::RelayUrl>()
                .map_err(|error| {
                    DeviceSyncError::Host(PersonalSyncHostError::Transport(format!(
                        "relay url {url:?}: {error}"
                    )))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !relays.is_empty() {
        tracing::info!(relays = relays.len(), "personal sync will register relays");
    }
    let selection = SyncSelection::default()
        .with_facets(sync.lanes.facets.clone())
        .with_access_records(sync.lanes.access_records)
        .with_saved_scenes(sync.lanes.saved_scenes)
        .with_handler_preferences(sync.lanes.handler_preferences)
        .with_blob_availability(sync.lanes.blob_availability);

    let host = Arc::new(
        PersonalSyncHost::open(
            identity,
            PersonalSyncHostConfig {
                graph,
                store_path,
                roster: SyncRoster::new(roots),
                selection,
                peer_tickets,
                paired_nodes: paired_nodes.clone(),
                relay_urls: relays,
            },
        )
        .await?,
    );
    // node_id is per-graph derived, so it is unlinkable across graphs and safe
    // to leave in a log that sits on disk. The master root is NOT: it is the
    // one identifier common to every graph this profile joins, so it is
    // disclosed on request through `pairing_facts` rather than written here on
    // every start. A peer learns it from the attestation on the wire anyway;
    // that is a different surface from a plaintext file.
    tracing::info!(
        graph = %owner_settings::hex32(&graph),
        node_id = %owner_settings::hex32(&host.node_id()),
        paired = sync.paired_devices.len(),
        ticket = %host.ticket().await?,
        "personal graph sync listening"
    );
    // Name any device that can reach this graph but cannot write to it. The
    // failure it would otherwise produce is a stream of refused operations on
    // the far side, which is a confusing place to learn about a missing root.
    for device in sync.receive_only_devices() {
        tracing::warn!(
            node = %device.node_id,
            label = %device.label,
            "paired device has no roster root: it will receive this graph, and \
             its own writes will be refused"
        );
    }

    // Author before the watchers start, so the receipt of a seeded node is
    // never confused with something that arrived from a peer.
    //
    // This runs at start-up rather than as its own command because the store
    // is single-writer and this process holds its lock: authoring can only
    // enter through this process, by argv now or by a typed intent over the
    // admitted session later. There is no third door, and a second binary
    // writing to the same store would be the wrong answer to that.
    for note in seed_notes {
        host.author(vec![PersonalGraphEvent::AddNode {
            id: uuid::Uuid::new_v4(),
            address: note.address.clone(),
            title: note.title.clone(),
        }])
        .await?;
        tracing::info!(address = %note.address, title = %note.title, "authored a node");
    }

    spawn_pairing_watch(
        Arc::clone(&host),
        settings_file,
        paired_nodes,
        identity.master_public_key().to_bytes(),
    );
    let cards: DeviceSupplementalCards =
        Arc::new(tokio::sync::RwLock::new(host.supplemental_cards().await?));
    spawn_card_refresh(host, Arc::clone(&cards));
    Ok(Some(cards))
}

/// The pairing loop's second half: `pair_device` and `unpair_device` write the
/// file, and this reconciles the live overlay against it, so a device paired
/// or dropped now takes effect now.
///
/// This reconciles in both directions deliberately. An unpair that only took
/// effect on the next restart would leave a device the owner believes they
/// removed still receiving the graph until they happened to reboot.
fn spawn_pairing_watch(
    host: Arc<PersonalSyncHost>,
    settings_file: PathBuf,
    already_applied: Vec<[u8; 32]>,
    local_root: [u8; 32],
) {
    let mut applied: std::collections::HashSet<[u8; 32]> = already_applied.into_iter().collect();
    // Last reported reachability, so the log records transitions rather than
    // repeating the same line every poll.
    let mut reported: Option<Vec<(String, bool)>> = None;
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(PAIRING_POLL).await;
            let reloaded = match OwnerSettings::load(&settings_file) {
                Ok(settings) => settings,
                Err(error) => {
                    tracing::warn!(%error, "could not reload owner settings");
                    continue;
                }
            };
            // Absent sync means the owner turned it off in the file, which is
            // not the same as an empty roster; leave the live overlay alone
            // rather than tearing every device off it on a partial edit.
            let Some(sync) = reloaded.sync else { continue };
            let desired: std::collections::HashSet<[u8; 32]> = match sync.paired_node_keys() {
                Ok(nodes) => nodes.into_iter().collect(),
                Err(error) => {
                    tracing::warn!(%error, "owner settings hold an unusable node id");
                    continue;
                }
            };

            // Authority moves with reachability. Tagging a peer onto the
            // overlay without admitting its root produces a device that
            // connects and then has everything it sends refused, which is the
            // most confusing shape this can fail in.
            match sync.roster_root_keys() {
                Ok(mut roots) => {
                    roots.push(local_root);
                    roots.sort_unstable();
                    roots.dedup();
                    let roots_len = roots.len();
                    let next = SyncRoster::new(roots);
                    if host.roster().await != next {
                        host.set_roster(next).await;
                        tracing::info!(admitted = roots_len, "personal sync roster changed");
                    }
                }
                Err(error) => tracing::warn!(%error, "owner settings hold an unusable roster root"),
            }

            for node in desired.difference(&applied).copied().collect::<Vec<_>>() {
                match host.pair_node(node).await {
                    Ok(()) => {
                        applied.insert(node);
                        tracing::info!(
                            node = %owner_settings::hex32(&node),
                            "applied a newly paired device without a restart"
                        );
                    }
                    // Leave it unapplied so the next pass retries rather than
                    // silently dropping the pairing.
                    Err(error) => tracing::warn!(
                        %error,
                        node = %owner_settings::hex32(&node),
                        "could not apply a paired device"
                    ),
                }
            }

            for node in applied.difference(&desired).copied().collect::<Vec<_>>() {
                match host.unpair_node(node).await {
                    Ok(()) => {
                        applied.remove(&node);
                        tracing::info!(
                            node = %owner_settings::hex32(&node),
                            "dropped an unpaired device without a restart"
                        );
                    }
                    // Keep it in `applied` so the next pass retries: a device
                    // the owner removed must not stay on the overlay quietly.
                    Err(error) => tracing::warn!(
                        %error,
                        node = %owner_settings::hex32(&node),
                        "could not drop an unpaired device"
                    ),
                }
            }

            // Report reachability, not just membership. A paired device that
            // discovery has never resolved is silently doing nothing, and
            // "paired" alone cannot distinguish that from a working peer.
            match host.known_peers().await {
                Ok(peers) => {
                    let mut current: Vec<(String, bool)> = peers
                        .iter()
                        .map(|peer| (owner_settings::hex32(&peer.peer.to_bytes()), peer.reachable))
                        .collect();
                    current.sort();
                    if reported.as_ref() != Some(&current) {
                        let reachable = current.iter().filter(|(_, ok)| *ok).count();
                        tracing::info!(
                            peers = current.len(),
                            reachable,
                            detail = ?current,
                            "personal sync peer directory changed"
                        );
                        reported = Some(current);
                    }
                }
                Err(error) => tracing::warn!(%error, "could not read the peer directory"),
            }
        }
    });
}

fn spawn_card_refresh(host: Arc<PersonalSyncHost>, cards: DeviceSupplementalCards) {
    // The projection is otherwise only visible to an admitted browser session,
    // which means a graph arriving from a peer leaves no trace anywhere an
    // operator can see. Report the size when it changes, so convergence is
    // observable rather than merely asserted.
    let mut reported: Option<usize> = None;
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            match host.supplemental_cards().await {
                Ok(snapshot) => {
                    if reported != Some(snapshot.len()) {
                        tracing::info!(cards = snapshot.len(), "personal graph projection changed");
                        reported = Some(snapshot.len());
                    }
                    *cards.write().await = snapshot;
                }
                Err(error) => tracing::warn!(%error, "personal sync projection refresh failed"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_root_override_is_taken_as_given_and_skips_the_migration() {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("vault");
        std::fs::create_dir_all(owner_settings::legacy_data_root(&vault)).unwrap();
        let chosen = directory.path().join("elsewhere");

        let resolved = resolve_data_root(directory.path(), &vault, Some(chosen.clone())).unwrap();
        assert_eq!(resolved, chosen);
        assert!(
            owner_settings::legacy_data_root(&vault).exists(),
            "an explicit --data-root must not move anything behind the \
             owner's back"
        );
    }

    #[test]
    fn a_graph_name_maps_to_one_id_and_different_names_do_not_collide() {
        assert_eq!(personal_graph_id("personal"), personal_graph_id("personal"));
        assert_ne!(personal_graph_id("personal"), personal_graph_id("scratch"));
    }
}
