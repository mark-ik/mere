//! Persisted owner settings for the resident Graphshell host.
//!
//! Durable configuration used to live only in the launcher's argument list,
//! which made every lasting choice (which graph, which lanes, which paired
//! devices) a property of a shell script rather than of the owner's profile.
//! Reinstalling the host, or editing the wrong wrapper, silently changed what
//! the device synchronised.
//!
//! Three boundaries hold here:
//!
//! - **Not in the credential vault.** Graphshell's storage lives under its own
//!   application directory. The Personae vault holds secrets and performs
//!   cryptographic operations; the two do not merge storage domains, even
//!   though the vault directory happened to be where the data root landed
//!   first. [`migrate_data_root`] moves that history forward.
//! - **One file per Personae profile.** A work face and a burner face do not
//!   share a paired-device roster or a set of enabled lanes, so cross-persona
//!   linkage stays opt-in.
//! - **Nothing secret.** Node ids and roster roots are public keys. Seeds,
//!   private keys, vault roots and decrypted slot payloads stay in Personae and
//!   never reach this file.

use std::path::{Path, PathBuf};

use personae::ProfileId;
use serde::{Deserialize, Serialize};

/// Graphshell's own application directory.
///
/// Mirrors the platform choice personae makes for its vault, but deliberately
/// does not nest under it.
pub fn default_app_dir() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    // The installed Windows layout is already `%LOCALAPPDATA%\Graphshell`
    // (bin, NativeMessagingHosts, the host log), so match it rather than
    // introduce a second spelling beside it.
    #[cfg(windows)]
    let name = "Graphshell";
    #[cfg(not(windows))]
    let name = "graphshell";
    base.join(name)
}

/// Where the personal-graph store and other Graphshell data belong.
pub fn default_data_root(app_dir: &Path) -> PathBuf {
    app_dir.join("data")
}

/// Where the data root used to live: inside the Personae vault directory,
/// beside `profiles/` and the wrapped auto-unlock root.
pub fn legacy_data_root(vault_dir: &Path) -> PathBuf {
    vault_dir.join("graphshell-data")
}

/// The settings file for one profile.
pub fn settings_path(app_dir: &Path, profile: &ProfileId) -> PathBuf {
    app_dir
        .join("settings")
        .join(format!("{}.json", sanitize_profile(&profile.0)))
}

/// A profile id reaches this process from an argument or an environment
/// variable and is about to become a filename, so it is constrained to
/// characters that cannot escape the settings directory.
fn sanitize_profile(profile: &str) -> String {
    let cleaned: String = profile
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "default".to_string()
    } else {
        cleaned
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OwnerSettingsError {
    #[error("owner settings at {path}: {message}")]
    File { path: String, message: String },
    #[error("owner settings: {value:?} is not a 64-character hex key")]
    NotHex { value: String },
    #[error("both {legacy} and {current} exist; refusing to guess which personal graph is current")]
    AmbiguousDataRoot { legacy: String, current: String },
}

/// Everything the resident host reads at start and may write back.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct OwnerSettings {
    /// Absent means personal sync stays off for this profile.
    pub sync: Option<SyncSettings>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct SyncSettings {
    /// Names the personal graph. Both devices must use the same name: it is
    /// hashed with a domain separator into the graph id.
    pub graph: String,
    /// Overrides the store location. Normally left unset.
    pub store_path: Option<PathBuf>,
    /// Additional 64-hex public roster roots. The local Personae root joins
    /// automatically and does not need listing.
    pub roster_roots: Vec<String>,
    pub paired_devices: Vec<PairedDevice>,
    pub lanes: LaneSettings,
    /// iroh relay urls to register.
    ///
    /// Empty means this device is LAN-only: p2panda registers no relay by
    /// default, and without one a peer is reachable only at a directly
    /// routable address the other side already knows, which mDNS supplies on a
    /// shared link and nothing supplies off one.
    ///
    /// A relay sees which devices talk to each other and when, even though it
    /// cannot read the contents, so which relay to trust is the owner's
    /// decision. That is why this is a setting with no default rather than a
    /// public relay wired in.
    pub relay_urls: Vec<String>,
}

/// A device this profile syncs with.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PairedDevice {
    /// Reachability: the peer's 64-hex per-graph transport node id.
    ///
    /// Deliberately not a ticket. A ticket carries the peer's address as of its
    /// last bind and goes stale when that device restarts; a node id is derived
    /// from the peer's master seed and the graph salt, so it keeps working.
    pub node_id: String,
    /// Authority: the peer's 64-hex Personae master public root.
    ///
    /// Reachability and authority are separate facts and a node id cannot
    /// carry both. The roster is what admits a writer, so a device recorded
    /// without a root can still receive this graph while its own operations
    /// are refused as `WriterNotAdmitted`.
    ///
    /// That is a real configuration, a device that reads without
    /// contributing, so it stays representable. It is reported at start-up so
    /// it cannot become a silent accident.
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub added_ms: u64,
}

/// Which lanes leave this device.
///
/// The structural graph lane is always present once sync is on. Every lane
/// here is off until the owner turns it on; `access_records` in particular
/// carries chronology, including the addresses actually used.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct LaneSettings {
    pub facets: Vec<String>,
    pub access_records: bool,
    pub saved_scenes: bool,
    pub handler_preferences: bool,
    pub blob_availability: bool,
}

impl OwnerSettings {
    /// Read the settings for a profile. A missing file is not an error: it
    /// means this profile has no stored preferences yet. Malformed content is
    /// an error, because silently falling back to defaults would quietly turn
    /// lanes off or drop paired devices.
    pub fn load(path: &Path) -> Result<Self, OwnerSettingsError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(OwnerSettingsError::File {
                    path: path.display().to_string(),
                    message: error.to_string(),
                });
            }
        };
        serde_json::from_str(&text).map_err(|error| OwnerSettingsError::File {
            path: path.display().to_string(),
            message: error.to_string(),
        })
    }

    /// Write the settings by rename, so a crash mid-write leaves the previous
    /// file intact rather than a truncated one.
    pub fn save(&self, path: &Path) -> Result<(), OwnerSettingsError> {
        let fail = |message: String| OwnerSettingsError::File {
            path: path.display().to_string(),
            message,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| fail(error.to_string()))?;
        }
        let mut text =
            serde_json::to_string_pretty(self).map_err(|error| fail(error.to_string()))?;
        text.push('\n');
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text.as_bytes()).map_err(|error| fail(error.to_string()))?;
        // Windows rename fails onto an existing path, so clear the destination
        // first. The temp file is still complete, so the window where neither
        // is readable is the rename itself.
        if path.exists() {
            std::fs::remove_file(path).map_err(|error| fail(error.to_string()))?;
        }
        std::fs::rename(&temporary, path).map_err(|error| fail(error.to_string()))
    }
}

impl SyncSettings {
    /// Every root admitted to write this graph: the standalone roots plus the
    /// root recorded against each paired device.
    ///
    /// Pairing records both facts together so that one unpair removes both. A
    /// root left behind after its device was dropped would be write authority
    /// the owner believes they revoked.
    pub fn roster_root_keys(&self) -> Result<Vec<[u8; 32]>, OwnerSettingsError> {
        self.roster_roots
            .iter()
            .map(|root| parse_hex32(root))
            .chain(
                self.paired_devices
                    .iter()
                    .filter_map(|device| device.root.as_deref())
                    .map(parse_hex32),
            )
            .collect()
    }

    /// Paired devices with no roster root. They receive this graph; their own
    /// writes are refused.
    pub fn receive_only_devices(&self) -> Vec<&PairedDevice> {
        self.paired_devices
            .iter()
            .filter(|device| device.root.is_none())
            .collect()
    }

    /// The paired devices' node ids as raw keys.
    pub fn paired_node_keys(&self) -> Result<Vec<[u8; 32]>, OwnerSettingsError> {
        self.paired_devices
            .iter()
            .map(|device| parse_hex32(&device.node_id))
            .collect()
    }

    /// Record a paired device, with its write authority when known. Returns
    /// false when that node id was already present, so re-pairing does not
    /// accumulate duplicates.
    pub fn pair(
        &mut self,
        node_id: [u8; 32],
        root: Option<[u8; 32]>,
        label: &str,
        at_ms: u64,
    ) -> bool {
        let node_id = hex32(&node_id);
        if self
            .paired_devices
            .iter()
            .any(|device| device.node_id.eq_ignore_ascii_case(&node_id))
        {
            return false;
        }
        self.paired_devices.push(PairedDevice {
            node_id,
            root: root.map(|root| hex32(&root)),
            label: label.to_string(),
            added_ms: at_ms,
        });
        true
    }

    /// Forget a paired device. Returns false when it was not paired, so
    /// unpairing twice is not an error.
    pub fn unpair(&mut self, node_id: [u8; 32]) -> bool {
        let node_id = hex32(&node_id);
        let before = self.paired_devices.len();
        self.paired_devices
            .retain(|device| !device.node_id.eq_ignore_ascii_case(&node_id));
        self.paired_devices.len() != before
    }
}

/// Command-line overrides for the sync section.
///
/// Peer tickets are absent on purpose. A ticket is only good until the peer
/// rebinds, so storing one would rebuild the staleness the node-id roster
/// exists to avoid; tickets stay a command-line bootstrap for the across-network
/// case that mDNS cannot serve.
#[derive(Clone, Debug, Default)]
pub struct SyncOverrides {
    pub graph: Option<String>,
    pub store_path: Option<PathBuf>,
    pub roster_roots: Vec<String>,
    pub paired_nodes: Vec<String>,
    pub relay_urls: Vec<String>,
    pub facets: Vec<String>,
    pub access_records: bool,
    pub saved_scenes: bool,
    pub handler_preferences: bool,
    pub blob_availability: bool,
}

impl SyncOverrides {
    /// Whether any override was supplied at all.
    pub fn is_empty(&self) -> bool {
        self.graph.is_none()
            && self.store_path.is_none()
            && self.roster_roots.is_empty()
            && self.paired_nodes.is_empty()
            && self.relay_urls.is_empty()
            && self.facets.is_empty()
            && !self.access_records
            && !self.saved_scenes
            && !self.handler_preferences
            && !self.blob_availability
    }
}

impl SyncSettings {
    /// Fold command-line overrides over the stored settings.
    ///
    /// A supplied value wins. A lane flag can only turn a lane on, because the
    /// flags have no negative form; to turn a lane off, edit the file. Lists
    /// replace rather than merge, so an explicit run is exactly reproducible
    /// from its own arguments.
    pub fn with_overrides(mut self, overrides: SyncOverrides) -> Self {
        if let Some(graph) = overrides.graph {
            self.graph = graph;
        }
        if overrides.store_path.is_some() {
            self.store_path = overrides.store_path;
        }
        if !overrides.roster_roots.is_empty() {
            self.roster_roots = overrides.roster_roots;
        }
        if !overrides.relay_urls.is_empty() {
            self.relay_urls = overrides.relay_urls;
        }
        if !overrides.paired_nodes.is_empty() {
            self.paired_devices = overrides
                .paired_nodes
                .into_iter()
                // --sync-peer-node carries reachability only, so an override
                // pairs receive-only; write authority comes from --sync-root
                // or the stored roster.
                .map(|node_id| PairedDevice {
                    node_id,
                    root: None,
                    label: String::new(),
                    added_ms: 0,
                })
                .collect();
        }
        if !overrides.facets.is_empty() {
            self.lanes.facets = overrides.facets;
        }
        self.lanes.access_records |= overrides.access_records;
        self.lanes.saved_scenes |= overrides.saved_scenes;
        self.lanes.handler_preferences |= overrides.handler_preferences;
        self.lanes.blob_availability |= overrides.blob_availability;
        self
    }
}

/// Decide the sync configuration in force for this run.
///
/// Sync is on when either the file configures it or an argument asks for it.
/// `None` means the owner has not enabled it for this profile.
pub fn resolve_sync(
    stored: Option<SyncSettings>,
    overrides: SyncOverrides,
) -> Option<SyncSettings> {
    match (stored, overrides.is_empty()) {
        (None, true) => None,
        (stored, _) => {
            let resolved = stored.unwrap_or_default().with_overrides(overrides);
            // A graph name is what identifies the shared graph; without one
            // there is nothing to join.
            if resolved.graph.trim().is_empty() {
                None
            } else {
                Some(resolved)
            }
        }
    }
}

/// Move the data root out of the Personae vault, once.
///
/// Returns the path actually in force. A failed move is an error rather than a
/// silent fresh start: the store holds the personal graph, and beginning again
/// with an empty one would look exactly like data loss.
pub fn migrate_data_root(
    legacy: &Path,
    current: &Path,
) -> Result<DataRootMigration, OwnerSettingsError> {
    if !legacy.exists() {
        return Ok(DataRootMigration::NothingToDo);
    }
    if current.exists() {
        return Err(OwnerSettingsError::AmbiguousDataRoot {
            legacy: legacy.display().to_string(),
            current: current.display().to_string(),
        });
    }
    if let Some(parent) = current.parent() {
        std::fs::create_dir_all(parent).map_err(|error| OwnerSettingsError::File {
            path: parent.display().to_string(),
            message: error.to_string(),
        })?;
    }
    std::fs::rename(legacy, current).map_err(|error| OwnerSettingsError::File {
        path: legacy.display().to_string(),
        message: format!("move to {}: {error}", current.display()),
    })?;
    Ok(DataRootMigration::Moved {
        from: legacy.to_path_buf(),
        to: current.to_path_buf(),
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum DataRootMigration {
    NothingToDo,
    Moved { from: PathBuf, to: PathBuf },
}

pub fn parse_hex32(value: &str) -> Result<[u8; 32], OwnerSettingsError> {
    let value = value.trim();
    if value.len() != 64 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(OwnerSettingsError::NotHex {
            value: value.to_string(),
        });
    }
    let mut out = [0u8; 32];
    for (index, slot) in out.iter_mut().enumerate() {
        let byte = &value[index * 2..index * 2 + 2];
        *slot = u8::from_str_radix(byte, 16).map_err(|_| OwnerSettingsError::NotHex {
            value: value.to_string(),
        })?;
    }
    Ok(out)
}

pub fn hex32(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_empty_settings_but_a_malformed_one_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings").join("default.json");
        assert_eq!(
            OwnerSettings::load(&path).unwrap(),
            OwnerSettings::default()
        );

        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ not json").unwrap();
        assert!(
            OwnerSettings::load(&path).is_err(),
            "a malformed file must not silently read as defaults: that would \
             turn lanes off and drop paired devices without saying so"
        );
    }

    #[test]
    fn a_typo_in_a_lane_name_is_refused_rather_than_ignored() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("default.json");
        std::fs::write(
            &path,
            br#"{"sync":{"graph":"personal","lanes":{"access_record":true}}}"#,
        )
        .unwrap();
        assert!(
            OwnerSettings::load(&path).is_err(),
            "a misspelled lane key must fail loudly; silently ignoring it \
             would leave the owner believing a lane was configured"
        );
    }

    #[test]
    fn settings_round_trip_and_the_write_replaces_the_previous_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings").join("work.json");
        let mut settings = OwnerSettings {
            sync: Some(SyncSettings {
                graph: "personal".into(),
                lanes: LaneSettings {
                    blob_availability: true,
                    ..LaneSettings::default()
                },
                ..SyncSettings::default()
            }),
        };
        settings.save(&path).unwrap();
        assert_eq!(OwnerSettings::load(&path).unwrap(), settings);

        settings.sync.as_mut().unwrap().graph = "second".into();
        settings.save(&path).unwrap();
        assert_eq!(OwnerSettings::load(&path).unwrap(), settings);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "the temporary file must not survive a successful write"
        );
    }

    #[test]
    fn pairing_is_idempotent_and_keyed_on_the_node_id() {
        let mut sync = SyncSettings::default();
        assert!(sync.pair([0x11; 32], Some([0xb1; 32]), "qpc", 100));
        assert!(
            !sync.pair([0x11; 32], Some([0xb1; 32]), "qpc renamed", 200),
            "re-pairing the same device must not add a second entry"
        );
        assert!(sync.pair([0x22; 32], None, "laptop", 300));
        assert_eq!(sync.paired_devices.len(), 2);
        assert_eq!(
            sync.paired_node_keys().unwrap(),
            vec![[0x11; 32], [0x22; 32]]
        );
    }

    #[test]
    fn a_profile_id_cannot_escape_the_settings_directory() {
        let app = Path::new("/app");
        let escaped = settings_path(app, &ProfileId("../../evil".into()));
        assert_eq!(
            escaped,
            app.join("settings").join("______evil.json"),
            "path separators and dots must not survive into the filename"
        );
        assert_eq!(
            settings_path(app, &ProfileId(String::new())),
            app.join("settings").join("default.json")
        );
    }

    #[test]
    fn the_data_root_moves_once_and_refuses_to_guess_when_both_exist() {
        let directory = tempfile::tempdir().unwrap();
        let legacy = directory.path().join("vault").join("graphshell-data");
        let current = directory.path().join("Graphshell").join("data");
        assert_eq!(
            migrate_data_root(&legacy, &current).unwrap(),
            DataRootMigration::NothingToDo
        );

        std::fs::create_dir_all(legacy.join("personal-sync")).unwrap();
        std::fs::write(legacy.join("personal-sync").join("graph.redb"), b"store").unwrap();
        let moved = migrate_data_root(&legacy, &current).unwrap();
        assert!(matches!(moved, DataRootMigration::Moved { .. }));
        assert!(current.join("personal-sync").join("graph.redb").exists());
        assert!(!legacy.exists());

        std::fs::create_dir_all(&legacy).unwrap();
        assert!(
            migrate_data_root(&legacy, &current).is_err(),
            "two candidate stores must stop the host rather than have it pick \
             one and appear to lose the other"
        );
    }

    #[test]
    fn an_argument_wins_over_the_file_but_a_lane_flag_only_turns_a_lane_on() {
        let stored = SyncSettings {
            graph: "personal".into(),
            lanes: LaneSettings {
                access_records: true,
                blob_availability: false,
                ..LaneSettings::default()
            },
            ..SyncSettings::default()
        };
        let resolved = resolve_sync(
            Some(stored),
            SyncOverrides {
                graph: Some("scratch".into()),
                blob_availability: true,
                ..SyncOverrides::default()
            },
        )
        .unwrap();

        assert_eq!(resolved.graph, "scratch", "an argument overrides the file");
        assert!(resolved.lanes.blob_availability, "a flag turns a lane on");
        assert!(
            resolved.lanes.access_records,
            "a lane enabled in the file stays on: the flags have no negative \
             form, so absence of a flag must not silently disable a lane"
        );
    }

    #[test]
    fn sync_stays_off_without_a_graph_from_either_source() {
        assert!(resolve_sync(None, SyncOverrides::default()).is_none());
        assert!(
            resolve_sync(
                None,
                SyncOverrides {
                    blob_availability: true,
                    ..SyncOverrides::default()
                }
            )
            .is_none(),
            "a lane flag alone must not start sync: there is no graph to join"
        );
        assert!(
            resolve_sync(
                None,
                SyncOverrides {
                    graph: Some("personal".into()),
                    ..SyncOverrides::default()
                }
            )
            .is_some(),
            "a graph named on the command line enables sync with no file"
        );
    }

    #[test]
    fn hex_round_trips_and_rejects_the_wrong_length() {
        assert_eq!(parse_hex32(&hex32(&[0xab; 32])).unwrap(), [0xab; 32]);
        assert!(parse_hex32("abcd").is_err());
        assert!(parse_hex32(&"z".repeat(64)).is_err());
    }
}
