//! Small sealed document vault built on Personae's record store.

use std::path::{Path, PathBuf};

use personae::{SealedRecordStorage, seal_bytes, unseal_bytes};
use serde::{Deserialize, Serialize};
use sibylla::VectorIndex;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const INDEX_PATH: &str = "knot/documents.json";
pub(crate) const SEARCH_INDEX_PATH: &str = "knot/search-index.json";
const INDEX_VERSION: u64 = 1;
const SYNC_KEY_CONTEXT: &str = "mere.knot.personal-vault-sync.v1";

/// One authored document in the sealed vault.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
pub struct VaultDocument {
    /// Stable caller-selected identity.
    pub id: String,
    /// Display title.
    pub title: String,
    /// Authored source bytes, unsealed only inside the endpoint.
    pub body: Vec<u8>,
    /// Source media type.
    pub media_type: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct VaultIndex {
    version: u64,
    revision: u64,
    documents: Vec<VaultDocument>,
}

/// An unlockable sealed document store.
pub struct KnotVault {
    root: PathBuf,
    store: Option<SealedRecordStorage>,
    sync_key: Option<Zeroizing<[u8; 32]>>,
    index: VaultIndex,
}

impl KnotVault {
    /// Open an unlocked vault with an already-recovered Personae root key.
    pub fn open(root: impl Into<PathBuf>, key: [u8; 32]) -> Result<Self, String> {
        let root = root.into();
        let sync_key = Zeroizing::new(blake3::derive_key(SYNC_KEY_CONTEXT, &key));
        let store = SealedRecordStorage::open_with_key(&root, key);
        let index = store
            .load_record::<VaultIndex>(INDEX_PATH)
            .map_err(|error| format!("could not load Knot vault: {error}"))?
            .unwrap_or_else(|| VaultIndex {
                version: INDEX_VERSION,
                revision: 0,
                documents: Vec::new(),
            });
        if index.version != INDEX_VERSION {
            return Err(format!(
                "unsupported Knot vault index version {}",
                index.version
            ));
        }
        Ok(Self {
            root,
            store: Some(store),
            sync_key: Some(sync_key),
            index,
        })
    }

    /// Root containing sealed record envelopes.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether document plaintext and the root key have been dropped.
    pub fn is_locked(&self) -> bool {
        self.store.is_none()
    }

    /// Current authored revision.
    pub fn revision(&self) -> u64 {
        self.index.revision
    }

    /// Unlocked documents. Locked vaults expose an empty iterator.
    pub fn documents(&self) -> impl Iterator<Item = &VaultDocument> {
        self.index.documents.iter()
    }

    /// Insert or replace one document and persist the sealed index.
    pub fn put(&mut self, document: VaultDocument) -> Result<(), String> {
        self.require_unlocked()?;
        if let Some(existing) = self
            .index
            .documents
            .iter_mut()
            .find(|existing| existing.id == document.id)
        {
            existing.zeroize();
            *existing = document;
        } else {
            self.index.documents.push(document);
        }
        self.index
            .documents
            .sort_by(|left, right| left.id.cmp(&right.id));
        self.index.revision = self.index.revision.saturating_add(1).max(1);
        self.save()
    }

    /// Read one source body while unlocked.
    pub fn body(&self, id: &str) -> Option<&[u8]> {
        if self.is_locked() {
            return None;
        }
        self.index
            .documents
            .iter()
            .find(|document| document.id == id)
            .map(|document| document.body.as_slice())
    }

    /// Seal the derived vault search index beside the document index.
    pub fn store_search_index(&self, index: &VectorIndex<String>) -> Result<(), String> {
        self.store
            .as_ref()
            .ok_or_else(|| "Knot vault is locked".to_string())?
            .save_record(SEARCH_INDEX_PATH, index)
            .map_err(|error| format!("could not save Knot vault search index: {error}"))
    }

    /// Unseal the derived vault search index while the vault is unlocked.
    pub fn load_search_index(&self) -> Result<Option<VectorIndex<String>>, String> {
        self.store
            .as_ref()
            .ok_or_else(|| "Knot vault is locked".to_string())?
            .load_record(SEARCH_INDEX_PATH)
            .map_err(|error| format!("could not load Knot vault search index: {error}"))
    }

    pub(crate) fn seal_sync_payload(
        &self,
        aad: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, String> {
        let key = self
            .sync_key
            .as_ref()
            .ok_or_else(|| "Knot vault is locked".to_string())?;
        seal_bytes(key, aad, plaintext)
            .map_err(|error| format!("could not seal Knot sync payload: {error}"))
    }

    pub(crate) fn unseal_sync_payload(
        &self,
        aad: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, String> {
        let key = self
            .sync_key
            .as_ref()
            .ok_or_else(|| "Knot vault is locked".to_string())?;
        unseal_bytes(key, aad, ciphertext)
            .map_err(|error| format!("could not unseal Knot sync payload: {error}"))
    }

    /// Drop decrypted document state and the root key.
    pub fn lock(&mut self) {
        self.index.zeroize();
        self.index = VaultIndex::default();
        self.store = None;
        self.sync_key = None;
    }

    /// Re-open a locked vault with an already-recovered root key.
    pub fn unlock(&mut self, key: [u8; 32]) -> Result<(), String> {
        if !self.is_locked() {
            return Ok(());
        }
        let store = SealedRecordStorage::open_with_key(&self.root, key);
        let index = store
            .load_record::<VaultIndex>(INDEX_PATH)
            .map_err(|error| format!("could not unlock Knot vault: {error}"))?
            .ok_or_else(|| "Knot vault index is absent".to_string())?;
        if index.version != INDEX_VERSION {
            return Err(format!(
                "unsupported Knot vault index version {}",
                index.version
            ));
        }
        self.store = Some(store);
        self.sync_key = Some(Zeroizing::new(blake3::derive_key(SYNC_KEY_CONTEXT, &key)));
        self.index = index;
        Ok(())
    }

    fn require_unlocked(&self) -> Result<(), String> {
        if self.is_locked() {
            Err("Knot vault is locked".into())
        } else {
            Ok(())
        }
    }

    fn save(&self) -> Result<(), String> {
        self.store
            .as_ref()
            .ok_or_else(|| "Knot vault is locked".to_string())?
            .save_record(INDEX_PATH, &self.index)
            .map_err(|error| format!("could not save Knot vault: {error}"))
    }
}

impl Drop for KnotVault {
    fn drop(&mut self) {
        self.index.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    fn note() -> VaultDocument {
        VaultDocument {
            id: "field-note".into(),
            title: "Field note".into(),
            body: b"private field observation".to_vec(),
            media_type: "text/vnd.knot".into(),
        }
    }

    #[test]
    fn sealed_document_survives_reopen_without_plaintext_on_disk() {
        let temp = tempdir().unwrap();
        let key = [0x71; 32];
        let mut vault = KnotVault::open(temp.path(), key).unwrap();
        vault.put(note()).unwrap();
        drop(vault);

        let sealed = fs::read(temp.path().join(INDEX_PATH)).unwrap();
        assert!(
            !sealed
                .windows(b"private field observation".len())
                .any(|window| window == b"private field observation")
        );

        let vault = KnotVault::open(temp.path(), key).unwrap();
        assert_eq!(
            vault.body("field-note"),
            Some(&b"private field observation"[..])
        );
    }

    #[test]
    fn lock_drops_documents_and_wrong_key_cannot_unlock() {
        let temp = tempdir().unwrap();
        let key = [0x72; 32];
        let mut vault = KnotVault::open(temp.path(), key).unwrap();
        vault.put(note()).unwrap();
        vault.lock();
        assert!(vault.is_locked());
        assert!(vault.documents().next().is_none());
        assert!(vault.body("field-note").is_none());
        assert!(vault.unlock([0x73; 32]).is_err());
        assert!(vault.unlock(key).is_ok());
        assert_eq!(
            vault.body("field-note"),
            Some(&b"private field observation"[..])
        );
    }
}
