//! Small sealed document vault built on Personae's record store.

use std::path::{Path, PathBuf};

use esp::embed::VectorIndex;
use personae::{SealedRecordStorage, seal_bytes, unseal_bytes};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const INDEX_PATH: &str = "knot/documents.json";
pub(crate) const SEARCH_INDEX_PATH: &str = "knot/search-index.json";
const DERIVED_CACHE_PATH_CONTEXT: &str = "mere.knot.derived-cache-path.v1";
const DERIVED_CACHE_VERSION: u64 = 1;
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

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DerivedCacheBlob {
    version: u64,
    bytes: Vec<u8>,
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

    /// Create another in-process read handle over the same unlocked vault.
    ///
    /// A retained publishing host only needs the sealed sync key to materialize
    /// signed source events. It must not take the authoring endpoint's mutable
    /// vault handle, but it does need an independently zeroized copy of the
    /// already-unlocked record-storage handle for the duration of the host.
    pub(crate) fn fork_read_handle(&self) -> Result<Self, String> {
        let store = self
            .store
            .as_ref()
            .ok_or_else(|| "cannot fork a locked Knot vault".to_string())?;
        let sync_key = self
            .sync_key
            .as_ref()
            .ok_or_else(|| "cannot fork a locked Knot vault".to_string())?;
        Ok(Self {
            root: self.root.clone(),
            store: Some(store.clone()),
            sync_key: Some(sync_key.clone()),
            index: self.index.clone(),
        })
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

    /// Replace the sealed local view with a projection of recorded sync
    /// operations.
    ///
    /// This is deliberately crate-private: callers author through
    /// `KnotSyncStore`; only the endpoint may rematerialize the derived vault
    /// index after that operation is accepted.
    pub(crate) fn replace_projection(
        &mut self,
        mut documents: Vec<VaultDocument>,
    ) -> Result<bool, String> {
        self.require_unlocked()?;
        documents.sort_by(|left, right| left.id.cmp(&right.id));
        if self.index.documents == documents {
            return Ok(false);
        }
        let removed = self
            .index
            .documents
            .iter()
            .filter(|current| {
                !documents
                    .iter()
                    .any(|replacement| replacement.id == current.id)
            })
            .map(|document| document.id.clone())
            .collect::<Vec<_>>();
        for id in removed {
            self.delete_derived_cache(&id)?;
        }
        self.index.documents.zeroize();
        self.index.documents = documents;
        self.index.revision = self.index.revision.saturating_add(1).max(1);
        self.save()?;
        Ok(true)
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

    /// Seal one non-authoritative derived-cache record beside the source
    /// index. The record id is keyed with vault material before it becomes a
    /// path, so it can neither escape the namespace nor act as a public
    /// dictionary oracle for document ids.
    pub(crate) fn store_derived_cache<T>(&self, id: &str, value: &T) -> Result<(), String>
    where
        T: Serialize,
    {
        let bytes = serde_json::to_vec(value)
            .map_err(|error| format!("could not encode Knot derived cache: {error}"))?;
        let path = self.derived_cache_path(id)?;
        self.store
            .as_ref()
            .ok_or_else(|| "Knot vault is locked".to_string())?
            .save_record(
                path,
                &DerivedCacheBlob {
                    version: DERIVED_CACHE_VERSION,
                    bytes,
                },
            )
            .map_err(|error| format!("could not seal Knot derived cache: {error}"))
    }

    /// Open one derived-cache record while the source vault is unlocked.
    pub(crate) fn load_derived_cache<T>(&self, id: &str) -> Result<Option<T>, String>
    where
        T: DeserializeOwned,
    {
        let path = self.derived_cache_path(id)?;
        let blob = self
            .store
            .as_ref()
            .ok_or_else(|| "Knot vault is locked".to_string())?
            .load_record::<DerivedCacheBlob>(path)
            .map_err(|error| format!("could not unseal Knot derived cache: {error}"))?;
        let Some(blob) = blob else {
            return Ok(None);
        };
        if blob.version != DERIVED_CACHE_VERSION {
            return Ok(None);
        }
        serde_json::from_slice(&blob.bytes)
            .map(Some)
            .map_err(|error| format!("could not decode Knot derived cache: {error}"))
    }

    fn delete_derived_cache(&self, id: &str) -> Result<(), String> {
        let path = self.derived_cache_path(id)?;
        self.store
            .as_ref()
            .ok_or_else(|| "Knot vault is locked".to_string())?
            .delete_record(path)
            .map_err(|error| format!("could not delete Knot derived cache: {error}"))
    }

    fn derived_cache_path(&self, id: &str) -> Result<PathBuf, String> {
        let key = self
            .sync_key
            .as_ref()
            .ok_or_else(|| "Knot vault is locked".to_string())?;
        let path_key = Zeroizing::new(blake3::derive_key(DERIVED_CACHE_PATH_CONTEXT, &**key));
        let digest = blake3::keyed_hash(&*path_key, id.as_bytes());
        Ok(PathBuf::from(format!(
            "knot/derived-cache/{}.json",
            digest.to_hex()
        )))
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

    #[test]
    fn removing_a_projected_document_collects_its_derived_cache() {
        let temp = tempdir().unwrap();
        let mut vault = KnotVault::open(temp.path(), [0x74; 32]).unwrap();
        vault.put(note()).unwrap();
        vault
            .store_derived_cache("field-note", &"cached result".to_string())
            .unwrap();
        let public_id_hash = blake3::hash(b"field-note").to_hex();
        assert!(
            !temp
                .path()
                .join(format!("knot/derived-cache/{public_id_hash}.json"))
                .exists(),
            "cache paths must not expose a public dictionary hash of the document id"
        );
        assert_eq!(
            vault.load_derived_cache::<String>("field-note").unwrap(),
            Some("cached result".into())
        );

        assert!(vault.replace_projection(Vec::new()).unwrap());
        assert_eq!(
            vault.load_derived_cache::<String>("field-note").unwrap(),
            None
        );
    }
}
