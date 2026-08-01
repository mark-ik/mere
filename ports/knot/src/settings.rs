//! Persisted sync settings for one persona's Knot vault.
//!
//! Scoped to the persona rather than to a machine or a profile, because that
//! is what a Knot space is scoped to: the space id derives from the persona
//! uuid, so every device carrying that persona's epoch addresses the same
//! space and needs the same answer to "who else writes here".
//!
//! Deliberately not `session_runtime::settings_store`, which is the app's
//! surface preferences (tab cap, theme, shellbar). Deliberately not
//! Graphshell's owner settings either: that file is keyed by Personae profile
//! and carries a graph name, lane selection, and paired *node* ids, none of
//! which mean the same thing here. The two share a shape, not a subject; if a
//! third consumer appears, the atomic-write mechanism is what to extract, not
//! the schema.
//!
//! Nothing secret lands here. Writer keys are public, and the epoch that makes
//! them useful never leaves the wallet.

use std::path::{Path, PathBuf};

use personae::PersonaId;
use serde::{Deserialize, Serialize};

/// Where a persona's Knot sync settings live: beside the vault they configure.
pub fn knot_settings_path(data_root: &Path, persona: PersonaId) -> PathBuf {
    data_root
        .join(session_runtime::PERSONAS_DIR)
        .join(persona.as_uuid().to_string())
        .join("knot-sync.json")
}

#[derive(Debug, thiserror::Error)]
pub enum KnotSettingsError {
    #[error("Knot sync settings at {path}: {message}")]
    File { path: String, message: String },
    #[error("Knot sync settings: {value:?} is not a 64-character hex key")]
    NotHex { value: String },
}

/// How this persona's devices reach and admit each other.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct KnotSyncSettings {
    /// The other devices' writer keys, 64-hex.
    ///
    /// One value serves twice: it is the key admitted to write this space and
    /// the transport node id dialled to reach that device. Knot binds its
    /// transport with the writer seed, so the two cannot drift apart the way
    /// they can in the personal graph.
    pub paired_writers: Vec<String>,
    /// iroh relay urls. Empty leaves this device LAN-only, since p2panda
    /// registers no relay by default.
    pub relay_urls: Vec<String>,
    /// Label recorded for this machine's own device identity.
    pub device_label: String,
}

/// Everything the resident Knot host reads at start.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct KnotSettings {
    /// Absent means this persona's Knot vault does not sync on this device.
    pub sync: Option<KnotSyncSettings>,
}

impl KnotSettings {
    /// A missing file means "not configured", which is not an error. Malformed
    /// content is one: falling back to defaults would quietly unpair every
    /// device and drop the relay, and present as a device that simply stopped
    /// syncing.
    pub fn load(path: &Path) -> Result<Self, KnotSettingsError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(KnotSettingsError::File {
                    path: path.display().to_string(),
                    message: error.to_string(),
                });
            }
        };
        serde_json::from_str(&text).map_err(|error| KnotSettingsError::File {
            path: path.display().to_string(),
            message: error.to_string(),
        })
    }

    /// Write by rename, so a crash mid-write leaves the previous file rather
    /// than a truncated one.
    pub fn save(&self, path: &Path) -> Result<(), KnotSettingsError> {
        let fail = |message: String| KnotSettingsError::File {
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
        if path.exists() {
            std::fs::remove_file(path).map_err(|error| fail(error.to_string()))?;
        }
        std::fs::rename(&temporary, path).map_err(|error| fail(error.to_string()))
    }
}

impl KnotSyncSettings {
    /// The paired writers as raw keys.
    pub fn paired_writer_keys(&self) -> Result<Vec<[u8; 32]>, KnotSettingsError> {
        self.paired_writers
            .iter()
            .map(|key| parse_hex32(key))
            .collect()
    }

    /// Record another device. False when already present, so re-pairing does
    /// not accumulate duplicates.
    pub fn pair(&mut self, writer: [u8; 32]) -> bool {
        let writer = hex32(&writer);
        if self
            .paired_writers
            .iter()
            .any(|known| known.eq_ignore_ascii_case(&writer))
        {
            return false;
        }
        self.paired_writers.push(writer);
        true
    }

    /// Forget a device. False when it was not paired, so unpairing twice is
    /// not an error.
    pub fn unpair(&mut self, writer: [u8; 32]) -> bool {
        let writer = hex32(&writer);
        let before = self.paired_writers.len();
        self.paired_writers
            .retain(|known| !known.eq_ignore_ascii_case(&writer));
        self.paired_writers.len() != before
    }
}

pub fn parse_hex32(value: &str) -> Result<[u8; 32], KnotSettingsError> {
    let value = value.trim();
    if value.len() != 64 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(KnotSettingsError::NotHex {
            value: value.to_string(),
        });
    }
    let mut out = [0u8; 32];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|_| {
            KnotSettingsError::NotHex {
                value: value.to_string(),
            }
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
    fn a_missing_file_is_unconfigured_but_a_malformed_one_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("knot-sync.json");
        assert_eq!(KnotSettings::load(&path).unwrap(), KnotSettings::default());

        std::fs::write(&path, b"{ not json").unwrap();
        assert!(
            KnotSettings::load(&path).is_err(),
            "reading a malformed file as defaults would unpair every device \
             and present as a machine that just stopped syncing"
        );
    }

    #[test]
    fn a_misspelled_key_is_refused_rather_than_ignored() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("knot-sync.json");
        std::fs::write(&path, br#"{"sync":{"paired_writer":[]}}"#).unwrap();
        assert!(KnotSettings::load(&path).is_err());
    }

    #[test]
    fn pairing_round_trips_and_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("knot-sync.json");
        let mut sync = KnotSyncSettings {
            relay_urls: vec!["https://relay.example/".into()],
            device_label: "o-pc".into(),
            ..KnotSyncSettings::default()
        };
        assert!(sync.pair([0x11; 32]));
        assert!(!sync.pair([0x11; 32]));
        assert!(sync.pair([0x22; 32]));

        KnotSettings {
            sync: Some(sync.clone()),
        }
        .save(&path)
        .unwrap();
        let reloaded = KnotSettings::load(&path).unwrap().sync.unwrap();
        assert_eq!(reloaded, sync);
        assert_eq!(
            reloaded.paired_writer_keys().unwrap(),
            vec![[0x11; 32], [0x22; 32]]
        );
        assert!(
            !path.with_extension("json.tmp").exists(),
            "the temporary file must not survive a successful write"
        );
    }

    #[test]
    fn unpairing_removes_one_and_twice_is_not_an_error() {
        let mut sync = KnotSyncSettings::default();
        sync.pair([0x31; 32]);
        sync.pair([0x32; 32]);
        assert!(sync.unpair([0x31; 32]));
        assert!(!sync.unpair([0x31; 32]));
        assert_eq!(sync.paired_writer_keys().unwrap(), vec![[0x32; 32]]);
    }

    #[test]
    fn settings_sit_beside_the_vault_they_configure() {
        let persona = PersonaId::new();
        let path = knot_settings_path(Path::new("/data"), persona);
        assert_eq!(
            path.parent(),
            crate::persona_vault_root(Path::new("/data"), persona).parent(),
            "a persona's Knot settings and its vault must not drift apart"
        );
    }
}
