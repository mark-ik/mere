//! Recording, forgetting and disclosing paired devices.
//!
//! These edit the settings file and compute public key material; none of them
//! open the personal-graph store. That separation is deliberate, and it is why
//! they live apart from [`crate::native::device_sync`], which brings the store
//! and the transport up: the store is single-writer and the resident host
//! holds its lock, so a management verb that needed it could not run while the
//! host was up. Everything here works with the host running, which is when you
//! actually want it.

use std::path::{Path, PathBuf};

use personae::{IdentityProvider, ProfileId};

use crate::native::device_sync::{DeviceSyncError, personal_graph_id};
use crate::native::owner_settings::{self, OwnerSettings, SyncSettings};
use crate::native::personal_sync_host::PersonalSyncHostError;

/// What a pairing attempt did.
#[derive(Debug, PartialEq, Eq)]
pub enum PairOutcome {
    Added {
        path: PathBuf,
        /// True when no roster root was supplied, so the device will receive
        /// this graph but its writes will be refused.
        receive_only: bool,
    },
    AlreadyPaired,
}

/// What an unpairing attempt did.
#[derive(Debug, PartialEq, Eq)]
pub enum UnpairOutcome {
    Removed { path: PathBuf },
    NotPaired,
}

/// Record a paired device in a profile's settings.
///
/// This and [`unpair_device`] are the only writers of the settings file. The
/// resident host reads it and reconciles its live transport against it but
/// never writes back, so two processes never race to rewrite it.
pub fn pair_device(
    app_dir: &Path,
    profile: &ProfileId,
    node_id: [u8; 32],
    root: Option<[u8; 32]>,
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
    if !sync.pair(node_id, root, label, at_ms) {
        return Ok(PairOutcome::AlreadyPaired);
    }
    settings.save(&path)?;
    Ok(PairOutcome::Added {
        path,
        receive_only: root.is_none(),
    })
}

/// Forget a paired device.
///
/// The resident host notices the removal and drops the device from the live
/// overlay, so unpairing does not wait for a restart.
pub fn unpair_device(
    app_dir: &Path,
    profile: &ProfileId,
    node_id: [u8; 32],
) -> Result<UnpairOutcome, DeviceSyncError> {
    let path = owner_settings::settings_path(app_dir, profile);
    let mut settings = OwnerSettings::load(&path)?;
    let Some(sync) = settings.sync.as_mut() else {
        return Ok(UnpairOutcome::NotPaired);
    };
    if !sync.unpair(node_id) {
        return Ok(UnpairOutcome::NotPaired);
    }
    settings.save(&path)?;
    Ok(UnpairOutcome::Removed { path })
}

/// What the other device needs in order to pair with this one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingFacts {
    pub graph: [u8; 32],
    /// Reachability. Derived per graph, so it is unlinkable across graphs.
    pub node_id: [u8; 32],
    /// Authority. Common to every graph this profile joins, so it is disclosed
    /// on request rather than logged.
    pub root: [u8; 32],
    /// Readability: this device's group pre-key bundle, hex encoded.
    ///
    /// Here because a receive-only device cannot put it on the lane itself. It
    /// has no roster root, so every operation it authors is refused, including
    /// the one that would announce it. Without somebody carrying this, such a
    /// device could be admitted as a reader and never actually keyed, which
    /// would make read-without-write a promise the design could not keep.
    ///
    /// Safe to paste. The bundle attests back to `root`, so relaying it proves
    /// nothing about the relay and everything about its subject; a forged one
    /// is refused when the lane admits it.
    ///
    /// Empty when this device has no key group session yet.
    pub prekey: String,
}

/// Compute this device's pairing facts without opening the store.
pub fn pairing_facts<P: IdentityProvider + ?Sized>(
    identity: &P,
    app_dir: &Path,
    profile: &ProfileId,
    data_root: Option<&Path>,
) -> Result<Option<PairingFacts>, DeviceSyncError> {
    let stored = OwnerSettings::load(&owner_settings::settings_path(app_dir, profile))?;
    let Some(sync) = stored.sync.filter(|sync| !sync.graph.trim().is_empty()) else {
        return Ok(None);
    };
    let graph = personal_graph_id(&sync.graph);
    let transport_key = identity
        .derive_keypair(&crate::personal_sync::personal_graph_identity_salt(graph))
        .map_err(|error| {
            DeviceSyncError::Host(PersonalSyncHostError::Transport(error.to_string()))
        })?;
    // Read from the session this device already has rather than minting one:
    // creating a second session would orphan the first, and this device's seat
    // is whichever one the lane has heard of.
    let prekey = match data_root {
        Some(root) => {
            let key_root = key_group_root(root, &sync, graph);
            match crate::native::graph_keys::GraphKeyGroup::open(identity, graph, &key_root) {
                Ok(opened) => hex(opened.group.published_bundle()),
                Err(error) => {
                    tracing::warn!(
                        %error,
                        "could not read this device's group pre-key; a device pairing with                          these facts will be reachable but not readable"
                    );
                    String::new()
                }
            }
        }
        None => String::new(),
    };
    Ok(Some(PairingFacts {
        graph,
        node_id: transport_key.public_key().to_bytes(),
        root: identity.master_public_key().to_bytes(),
        prekey,
    }))
}

/// Where the key group's sealed session lives, which is beside the graph
/// store and named for the same graph.
fn key_group_root(
    data_root: &Path,
    sync: &owner_settings::SyncSettings,
    graph: [u8; 32],
) -> std::path::PathBuf {
    sync.store_path
        .clone()
        .unwrap_or_else(|| {
            data_root
                .join("personal-sync")
                .join(format!("{}.redb", owner_settings::hex32(&graph)))
        })
        .with_extension("keys")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> ProfileId {
        ProfileId("default".into())
    }

    fn configured(directory: &Path) -> PathBuf {
        let path = owner_settings::settings_path(directory, &profile());
        OwnerSettings {
            sync: Some(SyncSettings {
                graph: "personal".into(),
                ..SyncSettings::default()
            }),
        }
        .save(&path)
        .unwrap();
        path
    }

    #[test]
    fn pairing_refuses_when_no_graph_is_configured() {
        let directory = tempfile::tempdir().unwrap();
        let error =
            pair_device(directory.path(), &profile(), [0x31; 32], None, "qpc", 1).unwrap_err();
        assert!(
            matches!(error, DeviceSyncError::NoGraphConfigured { .. }),
            "pairing into a profile with no graph must fail rather than \
             record a peer that could never join anything: {error}"
        );
    }

    #[test]
    fn pairing_writes_once_and_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let path = configured(directory.path());

        assert!(matches!(
            pair_device(
                directory.path(),
                &profile(),
                [0x31; 32],
                Some([0xa1; 32]),
                "qpc",
                100
            )
            .unwrap(),
            PairOutcome::Added { .. }
        ));
        assert_eq!(
            pair_device(
                directory.path(),
                &profile(),
                [0x31; 32],
                Some([0xa1; 32]),
                "qpc",
                200
            )
            .unwrap(),
            PairOutcome::AlreadyPaired
        );

        let reloaded = OwnerSettings::load(&path).unwrap().sync.unwrap();
        assert_eq!(reloaded.paired_devices.len(), 1);
        assert_eq!(reloaded.paired_devices[0].label, "qpc");
        assert_eq!(reloaded.paired_devices[0].added_ms, 100);
        assert_eq!(
            reloaded.graph, "personal",
            "pairing must not disturb the rest of the settings"
        );
    }

    #[test]
    fn unpairing_revokes_write_authority_along_with_reachability() {
        let directory = tempfile::tempdir().unwrap();
        let path = owner_settings::settings_path(directory.path(), &profile());
        OwnerSettings {
            sync: Some(SyncSettings {
                graph: "personal".into(),
                roster_roots: vec![owner_settings::hex32(&[0xc0; 32])],
                ..SyncSettings::default()
            }),
        }
        .save(&path)
        .unwrap();
        pair_device(
            directory.path(),
            &profile(),
            [0x51; 32],
            Some([0xd1; 32]),
            "thinkpad",
            1,
        )
        .unwrap();

        let paired = OwnerSettings::load(&path).unwrap().sync.unwrap();
        let mut roster = paired.roster_root_keys().unwrap();
        roster.sort_unstable();
        assert_eq!(
            roster,
            vec![[0xc0; 32], [0xd1; 32]],
            "a paired device's root must join the roster, or its writes are \
             refused while it looks paired"
        );

        unpair_device(directory.path(), &profile(), [0x51; 32]).unwrap();
        let dropped = OwnerSettings::load(&path).unwrap().sync.unwrap();
        assert_eq!(
            dropped.roster_root_keys().unwrap(),
            vec![[0xc0; 32]],
            "unpairing must revoke write authority too: a root left behind is \
             authority the owner believes they removed"
        );
        assert!(
            dropped.paired_node_keys().unwrap().is_empty(),
            "and reachability goes with it"
        );
    }

    #[test]
    fn a_device_paired_without_a_root_is_receive_only_and_says_so() {
        let directory = tempfile::tempdir().unwrap();
        let path = configured(directory.path());

        assert_eq!(
            pair_device(directory.path(), &profile(), [0x61; 32], None, "reader", 1).unwrap(),
            PairOutcome::Added {
                path: path.clone(),
                receive_only: true
            }
        );
        let stored = OwnerSettings::load(&path).unwrap().sync.unwrap();
        assert!(
            stored.roster_root_keys().unwrap().is_empty(),
            "no root was given, so nothing joins the roster"
        );
        assert_eq!(
            stored.receive_only_devices().len(),
            1,
            "and the device is reportable as receive-only rather than looking \
             like a fully paired peer that mysteriously cannot write"
        );
    }

    #[test]
    fn unpairing_twice_and_unpairing_an_unconfigured_profile_are_not_errors() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            unpair_device(directory.path(), &profile(), [0x43; 32]).unwrap(),
            UnpairOutcome::NotPaired,
            "a profile with no sync section has nothing to forget"
        );
        configured(directory.path());
        pair_device(
            directory.path(),
            &profile(),
            [0x41; 32],
            Some([0xa1; 32]),
            "imac",
            1,
        )
        .unwrap();
        unpair_device(directory.path(), &profile(), [0x41; 32]).unwrap();
        assert_eq!(
            unpair_device(directory.path(), &profile(), [0x41; 32]).unwrap(),
            UnpairOutcome::NotPaired
        );
    }

    #[test]
    fn a_device_can_be_paired_again_after_being_unpaired() {
        let directory = tempfile::tempdir().unwrap();
        let path = configured(directory.path());

        pair_device(
            directory.path(),
            &profile(),
            [0x44; 32],
            Some([0xa4; 32]),
            "imac",
            1,
        )
        .unwrap();
        unpair_device(directory.path(), &profile(), [0x44; 32]).unwrap();
        assert!(
            matches!(
                pair_device(
                    directory.path(),
                    &profile(),
                    [0x44; 32],
                    Some([0xa4; 32]),
                    "imac again",
                    3
                )
                .unwrap(),
                PairOutcome::Added { .. }
            ),
            "unpairing must not leave a tombstone that blocks re-pairing"
        );
        let again = OwnerSettings::load(&path).unwrap().sync.unwrap();
        assert_eq!(again.paired_devices.len(), 1);
        assert_eq!(again.paired_devices[0].added_ms, 3);
    }
}
