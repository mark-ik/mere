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
    self, DataRootMigration, OwnerSettings, OwnerSettingsError, SyncOverrides, SyncSettings,
};
use crate::native::personal_sync_host::{
    PersonalSyncHost, PersonalSyncHostConfig, PersonalSyncHostError,
};
use crate::personal_sync::{SyncRoster, SyncSelection};

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

/// What a pairing attempt did.
#[derive(Debug, PartialEq, Eq)]
pub enum PairOutcome {
    Added { path: PathBuf },
    AlreadyPaired,
}

/// Record a paired device in a profile's settings.
///
/// This is the only writer of the settings file. The resident host reads it
/// and applies additions to its live transport but never writes back, so two
/// processes never race to rewrite it.
pub fn pair_device(
    app_dir: &Path,
    profile: &ProfileId,
    node_id: [u8; 32],
    label: &str,
    at_ms: u64,
) -> Result<PairOutcome, DeviceSyncError> {
    let path = owner_settings::settings_path(app_dir, profile);
    let mut settings = OwnerSettings::load(&path)?;
    let sync = settings.sync.get_or_insert_with(SyncSettings::default);
    // Refuse to record a peer for a graph that does not exist: the pairing
    // would look accepted and then never join anything.
    if sync.graph.trim().is_empty() {
        return Err(DeviceSyncError::NoGraphConfigured {
            profile: profile.0.clone(),
            path: path.display().to_string(),
        });
    }
    if !sync.pair(node_id, label, at_ms) {
        return Ok(PairOutcome::AlreadyPaired);
    }
    settings.save(&path)?;
    Ok(PairOutcome::Added { path })
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
            },
        )
        .await?,
    );
    // node_id is the durable half of this line: a peer pairs with it once and
    // it survives restarts. The ticket is logged too because it still
    // bootstraps across networks, where mDNS cannot reach.
    tracing::info!(
        graph = %owner_settings::hex32(&graph),
        node_id = %owner_settings::hex32(&host.node_id()),
        paired = sync.paired_devices.len(),
        ticket = %host.ticket().await?,
        "personal graph sync listening"
    );

    spawn_pairing_watch(Arc::clone(&host), settings_file, paired_nodes);
    let cards: DeviceSupplementalCards =
        Arc::new(tokio::sync::RwLock::new(host.supplemental_cards().await?));
    spawn_card_refresh(host, Arc::clone(&cards));
    Ok(Some(cards))
}

/// The pairing loop's second half: `pair_device` writes the file, and this
/// notices and tags the device onto the live overlay, so a device paired now
/// starts syncing now.
///
/// Additive only. The address book has no untag through this seam, so removing
/// a device takes effect on the next start.
fn spawn_pairing_watch(
    host: Arc<PersonalSyncHost>,
    settings_file: PathBuf,
    already_applied: Vec<[u8; 32]>,
) {
    let mut applied: std::collections::HashSet<[u8; 32]> = already_applied.into_iter().collect();
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
            let Some(sync) = reloaded.sync else { continue };
            let nodes = match sync.paired_node_keys() {
                Ok(nodes) => nodes,
                Err(error) => {
                    tracing::warn!(%error, "owner settings hold an unusable node id");
                    continue;
                }
            };
            for node in nodes {
                if !applied.insert(node) {
                    continue;
                }
                match host.pair_node(node).await {
                    Ok(()) => tracing::info!(
                        node = %owner_settings::hex32(&node),
                        "applied a newly paired device without a restart"
                    ),
                    Err(error) => {
                        // Leave it unapplied so the next pass retries rather
                        // than silently dropping the pairing.
                        applied.remove(&node);
                        tracing::warn!(
                            %error,
                            node = %owner_settings::hex32(&node),
                            "could not apply a paired device"
                        );
                    }
                }
            }
        }
    });
}

fn spawn_card_refresh(host: Arc<PersonalSyncHost>, cards: DeviceSupplementalCards) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            match host.supplemental_cards().await {
                Ok(snapshot) => *cards.write().await = snapshot,
                Err(error) => tracing::warn!(%error, "personal sync projection refresh failed"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> ProfileId {
        ProfileId("default".into())
    }

    #[test]
    fn pairing_refuses_when_no_graph_is_configured() {
        let directory = tempfile::tempdir().unwrap();
        let error = pair_device(directory.path(), &profile(), [0x31; 32], "qpc", 1).unwrap_err();
        assert!(
            matches!(error, DeviceSyncError::NoGraphConfigured { .. }),
            "pairing into a profile with no graph must fail rather than \
             record a peer that could never join anything: {error}"
        );
    }

    #[test]
    fn pairing_writes_once_and_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let path = owner_settings::settings_path(directory.path(), &profile());
        OwnerSettings {
            sync: Some(SyncSettings {
                graph: "personal".into(),
                ..SyncSettings::default()
            }),
        }
        .save(&path)
        .unwrap();

        assert!(matches!(
            pair_device(directory.path(), &profile(), [0x31; 32], "qpc", 100).unwrap(),
            PairOutcome::Added { .. }
        ));
        assert_eq!(
            pair_device(directory.path(), &profile(), [0x31; 32], "qpc", 200).unwrap(),
            PairOutcome::AlreadyPaired
        );

        let reloaded = OwnerSettings::load(&path).unwrap().sync.unwrap();
        assert_eq!(reloaded.paired_devices.len(), 1);
        assert_eq!(reloaded.paired_devices[0].label, "qpc");
        assert_eq!(reloaded.paired_devices[0].added_ms, 100);
        assert_eq!(reloaded.paired_node_keys().unwrap(), vec![[0x31; 32]]);
        assert_eq!(
            reloaded.graph, "personal",
            "pairing must not disturb the rest of the settings"
        );
    }

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
