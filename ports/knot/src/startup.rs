//! Startup-unlocked personal Knot authority.
//!
//! This is the production seam between session-runtime's Personae wallet and
//! Knot's sealed, signed document store. Callers name a data root and persona;
//! recovered epoch bytes and every derived key stay inside Knot.

use std::fs;
use std::path::{Path, PathBuf};

use p2panda_core::SigningKey;
use personae::PersonaId;
use session_runtime::wallet_store;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    KnotEndpoint, KnotSyncEvent, KnotSyncFileStore, KnotVault, KnotWriteGrant, VaultDocument,
};

const VAULT_KEY_CONTEXT: &str = "mere.knot.persona-vault.root.v1";
const SIGNING_KEY_CONTEXT: &str = "mere.knot.persona-vault.writer.v1";
const SPACE_ID_CONTEXT: &str = "mere.knot.persona-vault.space.v1";
const KNOT_VAULT_DIR: &str = "vault";
const KNOT_SYNC_FILE: &str = "knot/sync.redb";

/// Unlocked authority held only long enough to seed or launch one endpoint.
pub struct StartupUnlockedPersonalVault {
    vault: KnotVault,
    store: KnotSyncFileStore,
    signing_seed: Zeroizing<[u8; 32]>,
}

impl StartupUnlockedPersonalVault {
    /// Recover the current private epoch through the configured startup-unlock
    /// policy and open this persona's sealed vault plus signed operation store.
    pub fn open(data_root: impl AsRef<Path>, persona: PersonaId) -> Result<Self, String> {
        let data_root = data_root.as_ref();
        let mut epoch = wallet_store::load_current_private_epoch(data_root, persona)
            .map_err(|error| format!("could not load Knot persona epoch: {error}"))?
            .ok_or_else(|| {
                "Knot persona vault is locked or has no current private epoch".to_string()
            })?;
        let vault_key = Zeroizing::new(blake3::derive_key(VAULT_KEY_CONTEXT, &epoch.epoch_secret));
        let signing_seed =
            Zeroizing::new(blake3::derive_key(SIGNING_KEY_CONTEXT, &epoch.epoch_secret));
        epoch.epoch_secret.zeroize();

        let vault_root = persona_vault_root(data_root, persona);
        fs::create_dir_all(vault_root.join("knot"))
            .map_err(|error| format!("could not create Knot persona vault: {error}"))?;
        let vault = KnotVault::open(&vault_root, *vault_key)?;
        let space_id = blake3::derive_key(SPACE_ID_CONTEXT, persona.as_uuid().as_bytes());
        let writer = *SigningKey::from_bytes(&*signing_seed)
            .verifying_key()
            .as_bytes();
        let store = KnotSyncFileStore::open(vault_root.join(KNOT_SYNC_FILE), space_id, [writer])
            .map_err(|error| format!("could not open Knot persona sync store: {error}"))?;

        let authority = Self {
            vault,
            store,
            signing_seed,
        };
        authority.migrate_unsynced_vault()?;
        Ok(authority)
    }

    /// Author one seed/import document through the same signed event path Save
    /// uses. This is an endpoint-owned setup seam, not a cleartext file write.
    pub fn author_document(&self, document: VaultDocument) -> Result<(), String> {
        pollster::block_on(self.store.author(
            *self.signing_seed,
            &self.vault,
            &KnotSyncEvent::Put(document),
        ))
        .map(|_| ())
        .map_err(|error| format!("could not author Knot persona document: {error}"))
    }

    /// Consume the recovered authority into a writable Graphshell endpoint.
    pub fn into_endpoint(self, grant: KnotWriteGrant) -> Result<KnotEndpoint, String> {
        KnotEndpoint::from_synced_vault(self.vault, self.store, *self.signing_seed, grant)
    }

    fn migrate_unsynced_vault(&self) -> Result<(), String> {
        let projection = pollster::block_on(self.store.projection(&self.vault))
            .map_err(|error| format!("could not inspect Knot persona sync store: {error}"))?;
        if !projection.documents.is_empty()
            || !projection.conflicts.is_empty()
            || !projection.pending.is_empty()
        {
            return Ok(());
        }
        let documents = self.vault.documents().cloned().collect::<Vec<_>>();
        for document in documents {
            self.author_document(document)?;
        }
        Ok(())
    }
}

pub fn persona_vault_root(data_root: &Path, persona: PersonaId) -> PathBuf {
    data_root
        .join(session_runtime::PERSONAS_DIR)
        .join(persona.as_uuid().to_string())
        .join(KNOT_VAULT_DIR)
}

#[cfg(all(test, windows))]
mod tests {
    use graphshell_endpoint::{ProjectionCatalog, ProjectionSource};
    use session_runtime::settings_store::{PersistedSettings, save_settings};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn auto_os_unlock_opens_signed_sealed_persona_truth() {
        let root = tempdir().unwrap();
        let persona = PersonaId::new();
        let settings = PersistedSettings {
            startup_unlock_mode: personae::StartupUnlockMode::AutoOs,
            ..PersistedSettings::default()
        };
        save_settings(root.path(), &settings).unwrap();
        wallet_store::ensure_wallet_state(root.path(), persona, "Knot receipt").unwrap();

        let authority = StartupUnlockedPersonalVault::open(root.path(), persona).unwrap();
        authority
            .author_document(VaultDocument {
                id: "field-note".into(),
                title: "Field note".into(),
                body: b"# Private\n".to_vec(),
                media_type: "text/vnd.knot".into(),
            })
            .unwrap();
        let mut endpoint = authority.into_endpoint(KnotWriteGrant::new(4096)).unwrap();
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();
        assert!(!snapshot.scene.tables.items.is_empty());

        drop(endpoint);
        let reopened = StartupUnlockedPersonalVault::open(root.path(), persona)
            .unwrap()
            .into_endpoint(KnotWriteGrant::new(4096))
            .unwrap();
        drop(reopened);

        let clear = b"# Private\n";
        let mut found_cleartext = false;
        for entry in walk_files(root.path()) {
            let bytes = fs::read(entry).unwrap();
            found_cleartext |= bytes.windows(clear.len()).any(|window| window == clear);
        }
        assert!(!found_cleartext, "persona truth must remain opaque at rest");
    }

    fn walk_files(root: &Path) -> Vec<PathBuf> {
        let mut pending = vec![root.to_path_buf()];
        let mut files = Vec::new();
        while let Some(path) = pending.pop() {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    files.push(path);
                }
            }
        }
        files
    }
}
