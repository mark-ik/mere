//! Standard vault opening: backend selection plus profile load-or-create.
//!
//! The bins and any resident host that serves the vault (per the
//! 2026-07-22 ruling, mere/Graphshell eventually) want the same opening
//! ceremony, so it lives here rather than being re-derived per consumer.
//!
//! Backend choice, in order:
//!
//! 1. [`Unlock::Passphrase`] (set `PERSONAE_PASSPHRASE`) opens the
//!    portable single-file [`crate::PassphraseEncryptedStorage`].
//! 2. [`Unlock::AutoOs`] opens [`crate::SealedProfileStorage`] over the
//!    OS-protected local root (DPAPI on Windows today).
//!
//! Where the `AutoOs` ladder does not exist yet, opening **fails** with a
//! message pointing at the passphrase option. It never falls back to
//! storing secrets in the clear.

use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

use crate::vault::{IdentityStorage, Profile, ProfileId};
use crate::{Ed25519Keypair, IdentityError, PassphraseEncryptedStorage, SealedProfileStorage};

/// Environment variable that selects the portable passphrase vault.
pub const PASSPHRASE_ENV: &str = "PERSONAE_PASSPHRASE";

/// How the vault's at-rest key is unlocked.
pub enum Unlock {
    /// OS-protected local root (Windows DPAPI today).
    AutoOs,
    /// Passphrase-derived KEK over a portable single-file vault.
    Passphrase(Zeroizing<Vec<u8>>),
}

impl Unlock {
    /// [`Unlock::Passphrase`] when [`PASSPHRASE_ENV`] is set, else
    /// [`Unlock::AutoOs`].
    pub fn from_env() -> Self {
        match std::env::var_os(PASSPHRASE_ENV) {
            Some(value) => Self::Passphrase(Zeroizing::new(
                value.to_string_lossy().as_bytes().to_vec(),
            )),
            None => Self::AutoOs,
        }
    }
}

/// An opened backend plus an honest one-line account of what protects it.
///
/// The description is meant to be shown to the user: they should never
/// have to guess whether their secrets are behind the OS keystore, a
/// passphrase, or nothing at all.
pub struct OpenedStorage {
    /// The backend, boxed so callers can choose at runtime.
    pub storage: Box<dyn IdentityStorage>,
    /// Human-readable description of the backend and its protection.
    pub description: String,
}

/// Per-user vault directory (`%LOCALAPPDATA%\personae\vault`, or
/// `$XDG_DATA_HOME/personae/vault`).
pub fn default_vault_dir() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("personae").join("vault")
}

/// Open (creating the directory if needed) the vault storage at `dir`.
pub fn open_storage(dir: &Path, unlock: Unlock) -> Result<OpenedStorage, IdentityError> {
    std::fs::create_dir_all(dir)
        .map_err(|err| IdentityError::Backend(format!("create vault dir {dir:?}: {err}")))?;
    match unlock {
        Unlock::Passphrase(passphrase) => {
            let path = dir.join("vault.json");
            let storage = PassphraseEncryptedStorage::open(&path, &passphrase)?;
            Ok(OpenedStorage {
                storage: Box::new(storage),
                description: format!("passphrase-encrypted vault at {}", path.display()),
            })
        }
        Unlock::AutoOs => match SealedProfileStorage::open_auto_os(dir)? {
            Some(storage) => Ok(OpenedStorage {
                storage: Box::new(storage),
                description: format!(
                    "OS auto-unlock sealed records at {} (DPAPI-wrapped root)",
                    dir.display()
                ),
            }),
            None => Err(IdentityError::Backend(format!(
                "no OS auto-unlock backend on this platform yet; set {PASSPHRASE_ENV} to use \
                 the portable passphrase vault instead"
            ))),
        },
    }
}

/// Load a profile, creating it (with a fresh master keypair) when absent.
///
/// Returns the profile and whether it was created, so callers can say so
/// rather than silently minting an identity.
pub fn load_or_create_profile(
    storage: &dyn IdentityStorage,
    id: &ProfileId,
) -> Result<(Profile, bool), IdentityError> {
    if storage
        .list_profiles()?
        .iter()
        .any(|summary| &summary.id == id)
    {
        return Ok((storage.load_profile(id)?, false));
    }
    let profile = Profile::new(
        id.clone(),
        id.0.clone(),
        Ed25519Keypair::generate(&mut rand_core::OsRng),
    );
    storage.save_profile(&profile)?;
    Ok((profile, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::InMemoryStorage;
    use tempfile::tempdir;

    #[test]
    fn passphrase_unlock_opens_a_portable_vault() {
        let dir = tempdir().unwrap();
        let opened = open_storage(
            dir.path(),
            Unlock::Passphrase(Zeroizing::new(b"pp".to_vec())),
        )
        .unwrap();
        assert!(opened.description.contains("passphrase-encrypted"));

        // The file appears on first write, not at open: opening only
        // derives the KEK and holds the salt (see PassphraseEncryptedStorage).
        assert!(!dir.path().join("vault.json").exists());
        load_or_create_profile(&*opened.storage, &ProfileId("default".into())).unwrap();
        assert!(dir.path().join("vault.json").exists());
    }

    #[test]
    fn load_or_create_mints_once_then_loads() {
        let storage = InMemoryStorage::new();
        let id = ProfileId("default".into());

        let (created, was_created) = load_or_create_profile(&storage, &id).unwrap();
        assert!(was_created);

        let (loaded, was_created) = load_or_create_profile(&storage, &id).unwrap();
        assert!(!was_created);
        assert_eq!(
            loaded.master.public_key().to_bytes(),
            created.master.public_key().to_bytes(),
            "reopening must not mint a new master key"
        );
    }

    #[test]
    fn default_vault_dir_is_under_a_personae_directory() {
        let dir = default_vault_dir();
        assert!(dir.ends_with("personae/vault") || dir.ends_with(r"personae\vault"));
    }
}
