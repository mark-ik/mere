// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Startup-unlocked personal Knot authority.
//!
//! This is the production seam between pandect's Personae wallet and
//! Knot's sealed, signed document store. Callers name a data root and persona;
//! recovered epoch bytes and every derived key stay inside Knot.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use p2panda_core::SigningKey;
use pandect::wallet_store;
use personae::{Ed25519Keypair, PersonaId};
use zeroize::{Zeroize, Zeroizing};

use crate::{
    KnotEndpoint, KnotPublishSource, KnotResidentSource, KnotSyncEvent, KnotSyncFileStore,
    KnotVault, KnotWriteGrant, VaultDocument,
};

const VAULT_KEY_CONTEXT: &str = "mere.knot.persona-vault.root.v1";
const SIGNING_KEY_CONTEXT: &str = "mere.knot.persona-vault.writer.v1";
const SPACE_ID_CONTEXT: &str = "mere.knot.persona-vault.space.v1";
const KNOT_VAULT_DIR: &str = "vault";
const KNOT_SYNC_FILE: &str = "knot/sync.redb";

struct StartupPersonalKeys {
    vault_key: Zeroizing<[u8; 32]>,
    signing_seed: Zeroizing<[u8; 32]>,
    legacy_writer: [u8; 32],
}

/// Unlocked authority held only long enough to seed or launch one endpoint.
pub struct StartupUnlockedPersonalVault {
    vault: KnotVault,
    store: KnotSyncFileStore,
    signing_seed: Zeroizing<[u8; 32]>,
}

impl StartupUnlockedPersonalVault {
    /// Recover the current private epoch through the configured startup-unlock
    /// policy and open this persona's sealed vault plus signed operation store.
    ///
    /// `device_root` is this machine's Personae master public key. It is what
    /// makes the writer identity device-distinct, and carrying the persona
    /// epoch to a second device is why that matters: the vault key and space
    /// must be identical across devices so both can decrypt the same space,
    /// but the writer must not be, because its public half is also the node
    /// identity. Two devices deriving one writer would be one node on the
    /// network and one author in a per-author log, and neither works.
    ///
    /// `admitted` carries the other devices' writer keys, which is how a
    /// second device's operations pass admission.
    pub fn open(
        data_root: impl AsRef<Path>,
        persona: PersonaId,
        device_root: [u8; 32],
        admitted: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, String> {
        let data_root = data_root.as_ref();
        let keys = unlock_personal_keys(data_root, persona, device_root)?;

        let vault_root = persona_vault_root(data_root, persona);
        fs::create_dir_all(vault_root.join("knot"))
            .map_err(|error| format!("could not create Knot persona vault: {error}"))?;
        let vault = KnotVault::open(&vault_root, *keys.vault_key)?;
        let space_id = blake3::derive_key(SPACE_ID_CONTEXT, persona.as_uuid().as_bytes());
        let writer = *SigningKey::from_bytes(&keys.signing_seed)
            .verifying_key()
            .as_bytes();
        let mut writers = vec![writer, keys.legacy_writer];
        writers.extend(admitted);
        writers.sort_unstable();
        writers.dedup();
        let store = KnotSyncFileStore::open(vault_root.join(KNOT_SYNC_FILE), space_id, writers)
            .map_err(|error| {
                format!(
                    "could not open Knot persona sync store; another resident may already own this persona: {error}"
                )
            })?;

        let authority = Self {
            vault,
            store,
            signing_seed: keys.signing_seed,
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

    /// This device's writer key, and so also its transport node id: what the
    /// other devices must admit before its operations will fold.
    pub fn writer(&self) -> [u8; 32] {
        *SigningKey::from_bytes(&self.signing_seed)
            .verifying_key()
            .as_bytes()
    }

    /// The signed operation store, for binding a transport to it.
    pub fn store(&self) -> &KnotSyncFileStore {
        &self.store
    }

    /// The seed this device signs and binds its transport with.
    pub fn signing_seed(&self) -> [u8; 32] {
        *self.signing_seed
    }

    /// Consume the recovered authority into a writable Graphshell endpoint.
    pub fn into_endpoint(self, grant: KnotWriteGrant) -> Result<KnotEndpoint, String> {
        Ok(self.into_resident_source()?.session(Some(grant)))
    }

    /// Consume the startup unlock into one cloneable resident source.
    pub fn into_resident_source(self) -> Result<KnotResidentSource, String> {
        KnotResidentSource::from_synced_vault(self.vault, self.store, *self.signing_seed)
    }

    /// Split one startup unlock between the mutable Graphshell editor endpoint
    /// and the independently retained read-only publishing host. Both handles
    /// retain the same synced source key, but only the endpoint receives the
    /// mutable vault handle and write grant.
    pub fn into_endpoint_and_publish_source(
        self,
        grant: KnotWriteGrant,
    ) -> Result<(KnotEndpoint, KnotPublishSource), String> {
        let (source, publish) = self.into_resident_source_and_publish_source()?;
        Ok((source.session(Some(grant)), publish))
    }

    /// Split one startup unlock between a cloneable authoring source and the
    /// independently retained read-only publishing source.
    pub fn into_resident_source_and_publish_source(
        self,
    ) -> Result<(KnotResidentSource, KnotPublishSource), String> {
        let publish_vault = Arc::new(self.vault.fork_read_handle()?);
        let publish_store = self.store.clone();
        let publish_identity = Ed25519Keypair::from_seed(*self.signing_seed);
        let source =
            KnotResidentSource::from_synced_vault(self.vault, self.store, *self.signing_seed)?;
        Ok((
            source,
            KnotPublishSource::from_unlocked(publish_identity, publish_store, publish_vault),
        ))
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

/// Derive this device's Knot writer identity without opening the Knot vault or
/// signed-operation store.
///
/// Pairing tools may run while the resident owns those files. Personae still
/// performs the configured startup unlock because the writer is derived from
/// the current persona epoch, but the management read cannot become a second
/// Knot store owner.
pub fn personal_vault_writer(
    data_root: impl AsRef<Path>,
    persona: PersonaId,
    device_root: [u8; 32],
) -> Result<[u8; 32], String> {
    let keys = unlock_personal_keys(data_root.as_ref(), persona, device_root)?;
    Ok(*SigningKey::from_bytes(&keys.signing_seed)
        .verifying_key()
        .as_bytes())
}

fn unlock_personal_keys(
    data_root: &Path,
    persona: PersonaId,
    device_root: [u8; 32],
) -> Result<StartupPersonalKeys, String> {
    let mut epoch = wallet_store::load_current_private_epoch(data_root, persona)
        .map_err(|error| format!("could not load Knot persona epoch: {error}"))?
        .ok_or_else(|| {
            "Knot persona vault is locked or has no current private epoch".to_string()
        })?;
    let vault_key = Zeroizing::new(blake3::derive_key(VAULT_KEY_CONTEXT, &epoch.epoch_secret));
    // The pre-device-scoped writer. Admitted, never authored with, so
    // operations written before this derivation existed still fold rather
    // than becoming an unreadable log signed by nobody admitted.
    let legacy_writer = *SigningKey::from_bytes(&blake3::derive_key(
        SIGNING_KEY_CONTEXT,
        &epoch.epoch_secret,
    ))
    .verifying_key()
    .as_bytes();
    let mut material = Zeroizing::new(Vec::with_capacity(64));
    material.extend_from_slice(&epoch.epoch_secret);
    material.extend_from_slice(&device_root);
    let signing_seed = Zeroizing::new(blake3::derive_key(SIGNING_KEY_CONTEXT, &material));
    epoch.epoch_secret.zeroize();
    Ok(StartupPersonalKeys {
        vault_key,
        signing_seed,
        legacy_writer,
    })
}

/// This machine's public device key, minted once and reused thereafter.
///
/// The device component of the writer derivation. It is public on purpose: it
/// only has to be *distinct* per device, since the secrecy of the writer comes
/// from the persona epoch it is mixed with. Reusing pandect's local
/// device identity rather than minting a Knot-private one keeps a device one
/// device across the whole system.
pub fn local_device_root(data_root: &Path, label: &str) -> Result<[u8; 32], String> {
    let identity = wallet_store::ensure_local_device_identity(data_root, label)
        .map_err(|error| format!("could not open this device's identity: {error}"))?;
    Ok(*SigningKey::from_bytes(&identity.device_seed)
        .verifying_key()
        .as_bytes())
}

pub fn persona_vault_root(data_root: &Path, persona: PersonaId) -> PathBuf {
    data_root
        .join(pandect::PERSONAS_DIR)
        .join(persona.as_uuid().to_string())
        .join(KNOT_VAULT_DIR)
}

#[cfg(all(test, windows))]
mod tests {
    use graphshell_endpoint::{ProjectionCatalog, ProjectionSource};
    use pandect::{DeviceSettings, save_device_settings};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn auto_os_unlock_opens_signed_sealed_persona_truth() {
        let root = tempdir().unwrap();
        let persona = PersonaId::new();
        let settings = DeviceSettings {
            startup_unlock_mode: personae::StartupUnlockMode::AutoOs,
            ..Default::default()
        };
        save_device_settings(root.path(), &settings).unwrap();
        wallet_store::ensure_wallet_state(root.path(), persona, "Knot receipt").unwrap();

        let authority = StartupUnlockedPersonalVault::open(
            root.path(),
            persona,
            local_device_root(root.path(), "knot receipt").unwrap(),
            [],
        )
        .unwrap();
        let duplicate = match StartupUnlockedPersonalVault::open(
            root.path(),
            persona,
            local_device_root(root.path(), "knot receipt").unwrap(),
            [],
        ) {
            Ok(_) => panic!("a second persona owner must be refused promptly"),
            Err(error) => error,
        };
        assert!(duplicate.contains("another resident may already own this persona"));
        assert_eq!(
            personal_vault_writer(
                root.path(),
                persona,
                local_device_root(root.path(), "knot receipt").unwrap(),
            )
            .unwrap(),
            authority.writer(),
            "pairing facts derive beside the resident without reopening its stores",
        );
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
        let reopened = StartupUnlockedPersonalVault::open(
            root.path(),
            persona,
            local_device_root(root.path(), "knot receipt").unwrap(),
            [],
        )
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
