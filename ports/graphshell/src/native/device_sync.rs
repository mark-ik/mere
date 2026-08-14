//! Bring-up and pairing for the resident host's personal-graph sync.
//!
//! Split out of the device-host binary so the settings resolution, the data
//! root migration and the pairing write are ordinary library code with tests,
//! rather than logic reachable only by running the resident host.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use personae::{IdentityProvider, ProfileId};

use crate::identity_endpoint::TransferDecision;
use crate::native::device_broker::{DeviceSurface, DeviceSurfaceHandle};
use crate::native::owner_settings::{
    self, DataRootMigration, OwnerSettings, OwnerSettingsError, SyncOverrides,
};
use crate::native::personal_sync_host::{
    PersonalSyncHost, PersonalSyncHostConfig, PersonalSyncHostError,
};
use crate::native::transfer_staging::{receive_transfer, released_blobs_for};
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

/// How long a startup fetch waits for the graph to name a holder. Long enough
/// for a first sync on a cold join, short enough that a genuinely absent
/// advertisement is reported while the operator is still watching.
const BLOB_FETCH_WAIT_TICK: std::time::Duration = std::time::Duration::from_secs(2);
const BLOB_FETCH_WAIT_TICKS: u64 = 30;

/// A blob operation to run once as the host starts.
///
/// Here for the same reason [`SeedNote`] is: the blob store and the graph
/// that advertises it are owned by this process, so a second binary cannot do
/// this without taking the lock away from the resident host. Stage-then-serve
/// in particular has to keep running afterwards, which a command that exits
/// cannot do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlobAction {
    /// Read a local file, hold its bytes, and tell paired devices this one has
    /// them. Logs the hash a sibling needs in order to ask.
    Stage { path: PathBuf },
    /// Fetch a blob some paired device has advertised, into this device's
    /// store. Logs which device supplied it.
    Fetch { blob: [u8; 32] },
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
    blob_actions: Vec<BlobAction>,
) -> Result<Option<DeviceSurfaceHandle>, DeviceSyncError> {
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
    let paired_devices = paired_nodes_with_roots(&sync)?;
    let paired_nodes = paired_devices.keys().copied().collect::<Vec<_>>();
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
    // The receipt lane rides alongside whatever the owner selected. Its facets
    // are added rather than configured because a device that ingests receipts
    // always wants their provenance: a receipt whose `receipt.run` facet was
    // filtered out replicates as a bare title, which is the failure this lane
    // exists to prevent. Blob availability comes on with it, since a receipt
    // without its captures is half a fact.
    let mut facets = sync.lanes.facets.clone();
    for facet in crate::receipts::sync_facets() {
        if !facets.iter().any(|selected| selected == facet) {
            facets.push(facet.to_string());
        }
    }
    let selection = SyncSelection::default()
        .with_facets(facets)
        .with_access_records(sync.lanes.access_records)
        .with_saved_scenes(sync.lanes.saved_scenes)
        .with_handler_preferences(sync.lanes.handler_preferences)
        // Left as the owner set it. With the lane off, a receipt still
        // replicates whole — its artifacts facet carries every blake3 hash —
        // and only *which device holds the bytes* goes unsaid, which is a
        // separate thing an owner may reasonably decline.
        .with_blob_availability(sync.lanes.blob_availability)
        // The only synthetic rule the resident host installs today. If a
        // second lane ever needs one, extend this list rather than calling
        // the setter twice: it replaces rather than appends.
        .with_synthetic_addresses([crate::receipts::sync_address_rule()]);

    // The cached-address rung: every hint recorded by a previous run rides in
    // as a best-effort address, so a device that has connected once can redial
    // through the relay after both ends restart, with no discovery working.
    // This is what turns the paired-device record from a name into a route.
    let peer_hints: Vec<String> = sync
        .paired_devices
        .iter()
        .filter_map(|device| device.last_endpoint.clone())
        .collect();
    if !peer_hints.is_empty() {
        tracing::info!(hints = peer_hints.len(), "seeding stored dial hints");
    }
    let host = Arc::new(
        PersonalSyncHost::open(
            identity,
            PersonalSyncHostConfig {
                graph,
                store_path,
                roster: SyncRoster::new(roots),
                selection,
                peer_tickets,
                peer_hints,
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

    // After the seed notes, and still before the watchers, so a staged blob's
    // advertisement is authored in the same quiet window.
    //
    // A failure here is reported and does not stop the host. Staging is a
    // request to hold and offer bytes; if it fails the host is still a working
    // sync engine, and exiting would take the graph down over a file that
    // could not be read. A fetch failure is more often "the holder is not up
    // yet" than anything permanent, and the record stays, so asking again
    // later is the recovery.
    for action in blob_actions {
        match action {
            BlobAction::Stage { path } => match std::fs::read(&path) {
                Ok(bytes) => {
                    let byte_len = bytes.len();
                    let container = uuid::Uuid::new_v4();
                    match host.stage_blob(container, bytes).await {
                        Ok(blob) => tracing::info!(
                            path = %path.display(),
                            byte_len,
                            container = %container,
                            blob = %owner_settings::hex32(&blob),
                            "staged a blob; a paired device can now fetch it by this hash"
                        ),
                        Err(error) => {
                            tracing::error!(path = %path.display(), %error, "could not stage the blob")
                        }
                    }
                }
                Err(error) => {
                    tracing::error!(path = %path.display(), %error, "could not read the file to stage")
                }
            },
            BlobAction::Fetch { blob } => {
                // Wait for the graph to say who holds this before asking.
                //
                // These actions run before the watchers start, which is early:
                // on a host that has only just joined, the advertisement has
                // not arrived yet, so an immediate attempt would report "no
                // paired device has advertised" for a blob that is on its way.
                // Waiting for the record is what a fetch means here, since the
                // graph is how this device learns where bytes are.
                let mut holders = Vec::new();
                for _ in 0..BLOB_FETCH_WAIT_TICKS {
                    holders = host.blob_holders(blob).await.unwrap_or_default();
                    if !holders.is_empty() {
                        break;
                    }
                    tokio::time::sleep(BLOB_FETCH_WAIT_TICK).await;
                }
                if holders.is_empty() {
                    // Say which of the two causes it is. The first version of
                    // this blamed the lane, and cost an hour on a device whose
                    // lane was on and which simply had no route to its peer:
                    // no relay configured, and mDNS not announcing. Those need
                    // opposite fixes, so guessing between them is worse than
                    // saying nothing.
                    //
                    // `known_peers` reports address-book membership rather than
                    // a live connection, so it cannot promise reachability; it
                    // can still separate "no peer is even configured" from
                    // "a peer is configured and told us nothing".
                    let peers = host.known_peers().await.unwrap_or_default();
                    let connected = peers.iter().filter(|peer| peer.connected).count();
                    let waited_s = BLOB_FETCH_WAIT_TICKS * BLOB_FETCH_WAIT_TICK.as_secs();
                    if peers.is_empty() {
                        tracing::error!(
                            blob = %owner_settings::hex32(&blob),
                            waited_s,
                            "no device is paired onto this graph's overlay, so \
                             nothing could advertise this blob"
                        );
                    } else if connected == 0 {
                        tracing::error!(
                            blob = %owner_settings::hex32(&blob),
                            waited_s,
                            peers = peers.len(),
                            "no paired device has a live path, so no \
                             advertisement could arrive. This is a connectivity \
                             failure, not a missing blob: check a firewall on \
                             either end, then whether a relay is configured and \
                             reachable"
                        );
                    } else {
                        tracing::error!(
                            blob = %owner_settings::hex32(&blob),
                            waited_s,
                            peers = peers.len(),
                            connected,
                            "a device is connected but none advertised this \
                             blob. The holder most likely never staged it, or \
                             staged it with the blob-availability lane disabled"
                        );
                    }
                    continue;
                }
                match host.fetch_blob_by_availability(blob).await {
                    Ok(supplier) => tracing::info!(
                        blob = %owner_settings::hex32(&blob),
                        supplier = %owner_settings::hex32(&supplier),
                        "fetched a blob from a paired device"
                    ),
                    Err(error) => tracing::error!(
                        blob = %owner_settings::hex32(&blob),
                        %error,
                        "could not fetch the blob"
                    ),
                }
            }
        }
    }

    spawn_pairing_watch(
        Arc::clone(&host),
        settings_file,
        paired_devices,
        identity.master_public_key().to_bytes(),
    );
    let surface: DeviceSurfaceHandle = Arc::new(tokio::sync::RwLock::new(DeviceSurface {
        cards: host.supplemental_cards().await?,
        released_blobs: Vec::new(),
        decisions: Default::default(),
    }));
    spawn_card_refresh(Arc::clone(&host), Arc::clone(&surface));
    spawn_receipt_intake(Arc::clone(&host), crate::receipts::inbox_dir(&data_root));
    spawn_accept_watch(host, Arc::clone(&surface));
    Ok(Some(surface))
}

/// The pairing loop's second half: `pair_device` and `unpair_device` write the
/// file, and this reconciles the live overlay against it, so a device paired
/// or dropped now takes effect now.
///
/// This reconciles in both directions deliberately. An unpair that only took
/// effect on the next restart would leave a device the owner believes they
/// removed still receiving the graph until they happened to reboot.
/// Paired devices as node id to Personae root.
///
/// Both halves together because unpairing needs them together: the node id is
/// what the overlay drops, and the root is what resolves the device's seat in
/// the key group. A receive-only device has no root recorded and cannot join
/// the key group, so `None` is a real answer rather than missing data.
fn paired_nodes_with_roots(
    sync: &owner_settings::SyncSettings,
) -> Result<std::collections::BTreeMap<[u8; 32], Option<[u8; 32]>>, OwnerSettingsError> {
    let mut paired = std::collections::BTreeMap::new();
    for device in &sync.paired_devices {
        let node = owner_settings::parse_hex32(&device.node_id)?;
        let root = match device.root.as_deref() {
            Some(root) => Some(owner_settings::parse_hex32(root)?),
            None => None,
        };
        paired.insert(node, root);
    }
    Ok(paired)
}

fn spawn_pairing_watch(
    host: Arc<PersonalSyncHost>,
    settings_file: PathBuf,
    already_applied: std::collections::BTreeMap<[u8; 32], Option<[u8; 32]>>,
    local_root: [u8; 32],
) {
    // Node id to Personae root, not just node ids. Unpairing has to revoke the
    // departed device's key, the key group knows devices by a recipient
    // derived from their root, and the settings record is gone by the time the
    // removal is noticed. Remembering the root here is what makes revocation
    // possible at all; `None` is a receive-only device, which cannot join the
    // key group and so has nothing to revoke.
    let mut applied = already_applied;
    // Last reported reachability, so the log records transitions rather than
    // repeating the same line every poll.
    // (node, has an address, has a live path). Connectivity is in the compared
    // tuple deliberately: a peer going silent while keeping its address is a
    // change worth logging, and was previously invisible.
    let mut reported: Option<Vec<(String, bool, bool)>> = None;
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
            let desired: std::collections::BTreeMap<[u8; 32], Option<[u8; 32]>> =
                match paired_nodes_with_roots(&sync) {
                    Ok(paired) => paired,
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

            // A node can stay paired while its Personae authority changes,
            // especially when a receive-only pairing is promoted. Reconcile
            // that value as well as map membership. When a root is removed or
            // replaced, finish its key revocation before forgetting it so a
            // transient lane-write failure remains retryable on the next pass.
            let authority_changes: Vec<([u8; 32], Option<[u8; 32]>, Option<[u8; 32]>)> = desired
                .iter()
                .filter_map(|(node, next)| {
                    let previous = applied.get(node)?;
                    (previous != next).then_some((*node, *previous, *next))
                })
                .collect();
            for (node, previous, next) in authority_changes {
                if let Some(root) = previous {
                    match host.retire_reader(root).await {
                        Ok(()) => tracing::info!(
                            node = %owner_settings::hex32(&node),
                            "retired the device's previous readership"
                        ),
                        Err(error) => {
                            tracing::warn!(
                                %error,
                                node = %owner_settings::hex32(&node),
                                "could not revoke changed key authority; will retry"
                            );
                            continue;
                        }
                    }
                }
                applied.insert(node, next);
            }

            let relayed: std::collections::BTreeMap<[u8; 32], Option<String>> = sync
                .paired_devices
                .iter()
                .filter_map(|device| {
                    let node = owner_settings::parse_hex32(&device.node_id).ok()?;
                    Some((node, device.prekey.clone()))
                })
                .collect();
            let arrivals: Vec<([u8; 32], Option<[u8; 32]>)> = desired
                .iter()
                .filter(|(node, _)| !applied.contains_key(*node))
                .map(|(node, root)| (*node, *root))
                .collect();
            for (node, root) in arrivals {
                match host.pair_node(node).await {
                    Ok(()) => {
                        applied.insert(node, root);
                        // Reachability and readership are separate grants and
                        // stay separate: pairing says where a device is, this
                        // says it may read. A receive-only device has no root
                        // and gets neither, which is what receive-only means.
                        if let Some(root) = root {
                            if let Err(error) = host.admit_reader(root, "paired device").await {
                                tracing::warn!(
                                    %error,
                                    node = %owner_settings::hex32(&node),
                                    "could not admit the paired device as a reader"
                                );
                            }
                        }
                        // Relay a pre-key its owner could not publish. The
                        // bundle authenticates its own subject, so carrying it
                        // asserts nothing on that device's behalf beyond what
                        // it already signed.
                        if let Some(prekey) = relayed.get(&node).cloned().flatten() {
                            match owner_settings::parse_hex(&prekey) {
                                Ok(bundle) => {
                                    if let Err(error) = host
                                        .author(vec![PersonalGraphEvent::PublishPrekey { bundle }])
                                        .await
                                    {
                                        tracing::warn!(
                                            %error,
                                            node = %owner_settings::hex32(&node),
                                            "could not relay the paired device's pre-key; it                                              stays reachable but unreadable until this succeeds"
                                        );
                                    }
                                }
                                Err(error) => tracing::warn!(
                                    %error,
                                    node = %owner_settings::hex32(&node),
                                    "the paired device's recorded pre-key is unreadable"
                                ),
                            }
                        }
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

            let departures: Vec<([u8; 32], Option<[u8; 32]>)> = applied
                .iter()
                .filter(|(node, _)| !desired.contains_key(*node))
                .map(|(node, root)| (*node, *root))
                .collect();
            for (node, root) in departures {
                // Revoke while the departure remains in `applied`. A failed
                // lane write then retries next pass instead of leaving a local
                // epoch turn that the rest of the graph never received.
                if let Some(root) = root {
                    match host.retire_reader(root).await {
                        Ok(()) => tracing::info!(
                            node = %owner_settings::hex32(&node),
                            "retired the unpaired device's readership"
                        ),
                        Err(error) => {
                            tracing::warn!(
                                %error,
                                node = %owner_settings::hex32(&node),
                                "could not retire the unpaired device's readership; will retry"
                            );
                            continue;
                        }
                    }
                }
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

            // Turn encryption on if the owner asked and nobody has yet.
            //
            // Checked against the lane, not against this device: seeing a
            // group means some device already created one, and starting a
            // second would split the graph into two halves that cannot read
            // each other. So this creates at most once per graph, and only
            // from the device whose settings say so.
            if sync.encrypted && !host.is_keyed().await {
                match host.key_group_exists().await {
                    Ok(false) => match host.enable_encryption().await {
                        Ok(()) => tracing::info!("encryption is on for this graph"),
                        Err(error) => tracing::warn!(
                            %error,
                            "could not turn encryption on; this graph stays readable to every                              admitted device"
                        ),
                    },
                    // Somebody created it. This device waits to be added,
                    // which is the ordinary path for every device but one.
                    Ok(true) => {}
                    Err(error) => {
                        tracing::warn!(%error, "could not tell whether this graph has a key group")
                    }
                }
            }

            // Key whatever has published since the last pass, and learn any
            // epoch this device has been given. Paired with the revocation
            // above deliberately: a host that keys devices automatically but
            // waits for someone to revoke would widen the reader set on its
            // own and narrow it only when asked.
            //
            // A no-op on a graph where encryption was never turned on, which
            // is why it is unconditional rather than gated on a setting.
            match host.key_paired_devices().await {
                Ok(0) => {}
                Ok(keyed) => tracing::info!(keyed, "keyed newly paired devices"),
                Err(error) => tracing::warn!(%error, "could not key paired devices this pass"),
            }

            // Report connectedness, not just membership or a known address. A
            // paired device discovery never resolved is silently doing
            // nothing, and one with an address it cannot actually reach looks
            // identical unless the two are named apart.
            //
            // They were not, and it cost hours: a firewall dropped every
            // inbound packet to a peer while its address stayed in the book,
            // so this line read `reachable=1` the whole time and the host
            // looked healthy while nothing replicated at all (2026-08-03).
            match host.known_peers().await {
                Ok(peers) => {
                    // Refresh the cached dial hints while the truth is live.
                    // Only connected peers: an address the endpoint holds for
                    // a peer it is NOT talking to may be exactly the stale
                    // route a working hint would replace, so writing it back
                    // would overwrite good information with bad.
                    for peer in peers.iter().filter(|peer| peer.connected) {
                        let node = peer.peer.to_bytes();
                        let ticket = match host.peer_ticket(node).await {
                            Ok(Some(ticket)) => ticket,
                            Ok(None) => continue,
                            Err(error) => {
                                tracing::warn!(%error, "could not read a peer's current address");
                                continue;
                            }
                        };
                        let stored = sync
                            .paired_devices
                            .iter()
                            .find(|device| {
                                device
                                    .node_id
                                    .eq_ignore_ascii_case(&owner_settings::hex32(&node))
                            })
                            .and_then(|device| device.last_endpoint.as_deref());
                        if stored == Some(ticket.as_str()) {
                            continue;
                        }
                        // Load-modify-save through the same atomic path every
                        // other settings write uses, so a concurrent pair or
                        // unpair edit is not clobbered by this refresh.
                        match OwnerSettings::load(&settings_file) {
                            Ok(mut latest) => {
                                let Some(live) = latest.sync.as_mut() else {
                                    continue;
                                };
                                if live.record_endpoint(&node, &ticket) {
                                    match latest.save(&settings_file) {
                                        Ok(()) => tracing::info!(
                                            node = %owner_settings::hex32(&node),
                                            "recorded a fresh dial hint for a connected device"
                                        ),
                                        Err(error) => tracing::warn!(
                                            %error,
                                            "could not persist a refreshed dial hint"
                                        ),
                                    }
                                }
                            }
                            Err(error) => {
                                tracing::warn!(%error, "could not reload settings to refresh a hint");
                            }
                        }
                    }
                    let mut current: Vec<(String, bool, bool)> = peers
                        .iter()
                        .map(|peer| {
                            (
                                owner_settings::hex32(&peer.peer.to_bytes()),
                                peer.reachable,
                                peer.connected,
                            )
                        })
                        .collect();
                    current.sort();
                    if reported.as_ref() != Some(&current) {
                        let addressed = current.iter().filter(|(_, ok, _)| *ok).count();
                        let connected = current.iter().filter(|(_, _, live)| *live).count();
                        tracing::info!(
                            peers = current.len(),
                            addressed,
                            connected,
                            detail = ?current,
                            "personal sync peer directory changed"
                        );
                        // The state that looks fine and is not: peers exist,
                        // every one has an address, and not one is talking.
                        // Say it plainly rather than leaving it to be inferred
                        // from a count nobody reads as connectivity.
                        if connected == 0 && !current.is_empty() {
                            tracing::warn!(
                                peers = current.len(),
                                addressed,
                                "no paired device has a live path: this host is \
                                 replicating nothing. Check a firewall on either \
                                 end, then whether a relay is configured and \
                                 reachable"
                            );
                        }
                        reported = Some(current);
                    }
                }
                Err(error) => tracing::warn!(%error, "could not read the peer directory"),
            }
        }
    });
}

/// Act on transfers a person has accepted.
///
/// The gesture is answered synchronously by the endpoint, which records it and
/// returns; fetching the bytes it implies cannot happen there. This is where
/// awaiting is possible, so this is where the decision becomes bytes.
///
/// A failed accept is logged and dropped rather than retried forever: the
/// offer stays on the graph, so the person can accept again once whatever
/// blocked it (a peer that is offline, bytes that are too large) has changed.
fn spawn_accept_watch(host: Arc<PersonalSyncHost>, surface: DeviceSurfaceHandle) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            // Take the queue handle out first, so the surface lock is not held
            // while the decision lock is. Two locks held in one order here and
            // the other order anywhere else is how a deadlock is written.
            let decisions = surface.read().await.decisions.clone();
            let accepted = match decisions.lock() {
                Ok(mut queue) => std::mem::take(&mut *queue),
                Err(_) => {
                    tracing::warn!("transfer decisions are unreadable; accepts will not be served");
                    continue;
                }
            };
            for decision in accepted {
                match serve_accepted_transfer(&host, &decision).await {
                    Ok(released) => {
                        let blobs = released.len();
                        surface.write().await.released_blobs = released;
                        tracing::info!(
                            transfer = %decision.transfer_id,
                            blobs,
                            "accepted transfer is staged and released to the browser"
                        );
                    }
                    Err(error) => tracing::warn!(
                        transfer = %decision.transfer_id,
                        %error,
                        "accepted transfer could not be served; the offer remains"
                    ),
                }
            }
        }
    });
}

/// Fetch one accepted transfer and produce the blobs a browser may pull.
async fn serve_accepted_transfer(
    host: &PersonalSyncHost,
    decision: &TransferDecision,
) -> Result<Vec<(graphshell_protocol::ContentHash, Vec<u8>)>, DeviceSyncError> {
    let offers = host.offers().await.map_err(DeviceSyncError::Host)?;
    let offer = offers
        .into_iter()
        .find(|offer| offer.transfer_id.to_string() == decision.transfer_id)
        .ok_or_else(|| {
            DeviceSyncError::Host(PersonalSyncHostError::Transport(format!(
                "no offer named {} is addressed to this device",
                decision.transfer_id
            )))
        })?;
    // Staging writes into the host's own store, which is also what apply will
    // read. A browser-side apply reads the released bytes instead, so the
    // product store here is the same one either path uses.
    let staging = muniment::BlobStore::new(muniment::MemoryBackend::new());
    let manifest = receive_transfer(host, &staging, &offer)
        .await
        .map_err(|error| {
            DeviceSyncError::Host(PersonalSyncHostError::Transport(error.to_string()))
        })?;
    released_blobs_for(host, &manifest)
        .await
        .map_err(|error| DeviceSyncError::Host(PersonalSyncHostError::Transport(error.to_string())))
}

fn spawn_card_refresh(host: Arc<PersonalSyncHost>, surface: DeviceSurfaceHandle) {
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
                    // Only the cards: released blobs are granted by
                    // accepting a transfer, not derived from the projection,
                    // so a refresh must not revoke them.
                    surface.write().await.cards = snapshot;
                }
                Err(error) => tracing::warn!(%error, "personal sync projection refresh failed"),
            }
        }
    });
}

/// How often the host looks for receipts deposited by `receipt_ingest`.
///
/// Slow on purpose: a receipt arrives when a person runs a scenario on another
/// machine, which is minutes apart at best, and the intake is a directory scan
/// rather than something worth spinning on.
const RECEIPT_POLL: std::time::Duration = std::time::Duration::from_secs(10);

/// Author receipts that ingest left in the inbox.
///
/// The resident host is the only writer for this graph, so ingest deposits and
/// this authors. One turn per receipt: a run is one fact, and batching two
/// runs into a turn would make a later reader unable to tell which events
/// belonged to which.
fn spawn_receipt_intake(host: Arc<PersonalSyncHost>, inbox: PathBuf) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(RECEIPT_POLL).await;
            let waiting = match crate::receipts::pending(&inbox) {
                Ok(waiting) if waiting.is_empty() => continue,
                Ok(waiting) => waiting,
                Err(error) => {
                    tracing::warn!(%error, inbox = %inbox.display(), "receipt intake scan failed");
                    continue;
                }
            };
            for receipt in waiting {
                let events = receipt.events.len();
                match host.author(receipt.events).await {
                    Ok(()) => {
                        // Only now: a file cleared before the turn succeeded
                        // would lose the receipt entirely, whereas one cleared
                        // after a failure simply gets retried next poll.
                        if let Err(error) = crate::receipts::mark_applied(&receipt.path) {
                            tracing::warn!(
                                %error,
                                path = %receipt.path.display(),
                                "authored a receipt but could not clear its file; \
                                 it will be authored again next poll"
                            );
                        }
                        tracing::info!(
                            events,
                            path = %receipt.path.display(),
                            "authored a receipt into the personal graph"
                        );
                    }
                    Err(error) => tracing::warn!(
                        %error,
                        path = %receipt.path.display(),
                        "could not author a receipt; leaving it pending"
                    ),
                }
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

    /// The pairing watcher starts from this value. Losing the root here means
    /// an ordinary restart followed by unpair can drop reachability while
    /// silently leaving the departed device in the graph's key group.
    #[test]
    fn initial_pairing_watch_state_retains_each_device_root() {
        let node = [0xd4; 32];
        let root = [0xd5; 32];
        let mut sync = owner_settings::SyncSettings::default();
        assert!(sync.pair(node, Some(root), "sibling", 1));

        let applied = paired_nodes_with_roots(&sync).unwrap();
        assert_eq!(applied.get(&node), Some(&Some(root)));
    }
}
