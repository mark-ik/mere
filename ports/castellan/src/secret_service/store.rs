use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use personae::{IdentityError, PersonaId, SealedRecordChange, SealedRecordStorage};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;
#[cfg(any(test, target_os = "linux"))]
use zeroize::Zeroizing;

const RECORD_VERSION: u8 = 1;
const RECORD_DIRECTORY: &str = "castellan/secret-service/v1";

mod persistence;

use persistence::{StoredCollection, StoredItem};

/// Resource limits applied before any Secret Service value reaches storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecretServiceLimits {
    /// Collections allowed for one persona.
    pub max_collections: usize,
    /// Items allowed in one collection.
    pub max_items_per_collection: usize,
    /// Lookup attributes allowed on one item.
    pub max_attributes: usize,
    /// Maximum UTF-8 bytes in a label, attribute key, or content type.
    pub max_name_bytes: usize,
    /// Maximum UTF-8 bytes in one attribute value.
    pub max_attribute_value_bytes: usize,
    /// Maximum bytes in one secret value.
    pub max_secret_bytes: usize,
    /// Concurrent D-Bus transfer sessions retained by the adapter.
    pub max_sessions: usize,
}

impl Default for SecretServiceLimits {
    fn default() -> Self {
        Self {
            max_collections: 32,
            max_items_per_collection: 4_096,
            max_attributes: 64,
            max_name_bytes: 1_024,
            max_attribute_value_bytes: 4_096,
            max_secret_bytes: 1024 * 1024,
            max_sessions: 1_024,
        }
    }
}

/// Stable identifier for one Secret Service collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SecretCollectionId(uuid::Uuid);

impl SecretCollectionId {
    fn mint() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Recover an identifier from a D-Bus object-path UUID.
    pub fn from_uuid(id: uuid::Uuid) -> Self {
        Self(id)
    }

    /// Return the UUID used in the D-Bus object path.
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}

impl fmt::Display for SecretCollectionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Stable identifier for one Secret Service item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SecretItemId(uuid::Uuid);

impl SecretItemId {
    fn mint() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Recover an identifier from a D-Bus object-path UUID.
    pub fn from_uuid(id: uuid::Uuid) -> Self {
        Self(id)
    }

    /// Return the UUID used in the D-Bus object path.
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}

impl fmt::Display for SecretItemId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Secret-free collection metadata exposed through D-Bus properties.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretCollection {
    /// Stable collection identifier.
    pub id: SecretCollectionId,
    /// User-visible collection label.
    pub label: String,
    /// Unix creation time in seconds.
    pub created: u64,
    /// Unix modification time in seconds.
    pub modified: u64,
}

/// Secret-free item metadata exposed through D-Bus properties and search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretItem {
    /// Stable item identifier.
    pub id: SecretItemId,
    /// Owning collection.
    pub collection: SecretCollectionId,
    /// User-visible item label.
    pub label: String,
    /// Exact-match lookup attributes.
    pub attributes: BTreeMap<String, String>,
    /// Unix creation time in seconds.
    pub created: u64,
    /// Unix modification time in seconds.
    pub modified: u64,
}

/// Values required to create or exact-attribute-replace one secret item.
pub struct NewSecretItem {
    /// Collection that will own the item.
    pub collection: SecretCollectionId,
    /// Human-readable item label.
    pub label: String,
    /// Exact-match lookup attributes.
    pub attributes: BTreeMap<String, String>,
    /// Secret bytes to seal.
    pub secret: Vec<u8>,
    /// MIME-style content type for the secret bytes.
    pub content_type: String,
    /// Replace an item in the collection with identical attributes.
    pub replace: bool,
    /// Host-supplied Unix timestamp for creation or replacement.
    pub unix_secs: u64,
}

impl Drop for NewSecretItem {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

#[cfg(any(test, target_os = "linux"))]
pub(super) struct SecretValue {
    pub(super) bytes: Zeroizing<Vec<u8>>,
    pub(super) content_type: String,
}

/// Failure while applying a Secret Service storage operation.
#[derive(Debug, thiserror::Error)]
pub enum SecretServiceError {
    /// Personae could not authenticate or update a sealed record.
    #[error("sealed Secret Service storage: {0}")]
    Storage(#[from] IdentityError),
    /// A requested collection is absent.
    #[error("Secret Service collection {0} does not exist")]
    CollectionNotFound(SecretCollectionId),
    /// A requested item is absent.
    #[error("Secret Service item {0} does not exist")]
    ItemNotFound(SecretItemId),
    /// A sealed record was created by an unsupported format.
    #[error("unsupported Secret Service record version {0}")]
    UnsupportedRecordVersion(u8),
    /// Stored catalog state names an absent child record.
    #[error("Secret Service catalog is inconsistent: {0}")]
    InconsistentCatalog(String),
    /// A caller-provided value exceeds the configured resident limits.
    #[error("Secret Service input exceeds configured limit: {0}")]
    Limit(&'static str),
    /// A label, content type, or attribute key is empty or contains controls.
    #[error("Secret Service input is not a presentable {0}")]
    InvalidText(&'static str),
}

/// Persona-scoped Secret Service collections held by one resident authority.
#[derive(Clone)]
pub struct SecretServiceStore {
    storage: SealedRecordStorage,
    persona: PersonaId,
    limits: SecretServiceLimits,
    transaction: Arc<Mutex<()>>,
}

impl SecretServiceStore {
    pub(crate) fn new(
        storage: SealedRecordStorage,
        persona: PersonaId,
        limits: SecretServiceLimits,
        transaction: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            storage,
            persona,
            limits,
            transaction,
        }
    }

    /// Persona whose collections this store serves.
    pub fn persona(&self) -> PersonaId {
        self.persona
    }

    #[cfg(any(test, target_os = "linux"))]
    pub(super) fn limits(&self) -> SecretServiceLimits {
        self.limits
    }

    /// Ensure and return the collection behind the conventional `default` alias.
    pub fn ensure_default_collection(
        &self,
        label: &str,
        unix_secs: u64,
    ) -> Result<SecretCollection, SecretServiceError> {
        self.validate_name(label, "collection label")?;
        let _guard = self.lock();
        let catalog = self.load_catalog()?;
        if let Some(id) = catalog.aliases.get("default").copied() {
            return self.require_collection(id);
        }
        self.create_collection_locked(label, Some("default"), unix_secs)
    }

    /// Return every collection in stable identifier order.
    pub fn collections(&self) -> Result<Vec<SecretCollection>, SecretServiceError> {
        let _guard = self.lock();
        self.load_catalog()?
            .collections
            .into_iter()
            .map(|id| self.require_collection(id))
            .collect()
    }

    /// Read one collection's secret-free metadata.
    pub fn collection(
        &self,
        id: SecretCollectionId,
    ) -> Result<SecretCollection, SecretServiceError> {
        let _guard = self.lock();
        self.require_collection(id)
    }

    /// Create a collection, optionally binding one well-known alias.
    pub fn create_collection(
        &self,
        label: &str,
        alias: Option<&str>,
        unix_secs: u64,
    ) -> Result<SecretCollection, SecretServiceError> {
        self.validate_name(label, "collection label")?;
        if let Some(alias) = alias {
            self.validate_alias(alias)?;
        }
        let _guard = self.lock();
        let catalog = self.load_catalog()?;
        if let Some(existing) = alias.and_then(|alias| catalog.aliases.get(alias).copied()) {
            self.set_collection_label_locked(existing, label, unix_secs)?;
            return self.require_collection(existing);
        }
        self.create_collection_locked(label, alias, unix_secs)
    }

    /// Resolve a well-known alias.
    pub fn read_alias(
        &self,
        alias: &str,
    ) -> Result<Option<SecretCollectionId>, SecretServiceError> {
        let _guard = self.lock();
        Ok(self.load_catalog()?.aliases.get(alias).copied())
    }

    #[cfg(any(test, target_os = "linux"))]
    pub(super) fn aliases(
        &self,
    ) -> Result<BTreeMap<String, SecretCollectionId>, SecretServiceError> {
        let _guard = self.lock();
        Ok(self.load_catalog()?.aliases)
    }

    /// Point or remove a well-known alias.
    pub fn set_alias(
        &self,
        alias: &str,
        collection: Option<SecretCollectionId>,
    ) -> Result<(), SecretServiceError> {
        self.validate_alias(alias)?;
        let _guard = self.lock();
        if let Some(id) = collection {
            self.require_collection(id)?;
        }
        let mut catalog = self.load_catalog()?;
        match collection {
            Some(id) => {
                catalog.aliases.insert(alias.to_string(), id);
            }
            None => {
                catalog.aliases.remove(alias);
            }
        }
        self.save_catalog(&catalog)
    }

    /// Change a collection label.
    pub fn set_collection_label(
        &self,
        id: SecretCollectionId,
        label: &str,
        unix_secs: u64,
    ) -> Result<(), SecretServiceError> {
        self.validate_name(label, "collection label")?;
        let _guard = self.lock();
        self.set_collection_label_locked(id, label, unix_secs)
    }

    /// Delete a collection and tombstone every item it contained.
    pub fn delete_collection(&self, id: SecretCollectionId) -> Result<(), SecretServiceError> {
        let _guard = self.lock();
        let collection = self
            .load_collection_record(id)?
            .ok_or(SecretServiceError::CollectionNotFound(id))?;
        for item in collection.items {
            self.storage.delete_record(self.item_path(item))?;
        }
        self.storage.delete_record(self.collection_path(id))?;
        let mut catalog = self.load_catalog()?;
        catalog.collections.retain(|candidate| *candidate != id);
        catalog.aliases.retain(|_, candidate| *candidate != id);
        self.save_catalog(&catalog)
    }

    /// Return every item in a collection.
    pub fn items(
        &self,
        collection: SecretCollectionId,
    ) -> Result<Vec<SecretItem>, SecretServiceError> {
        let _guard = self.lock();
        let collection = self
            .load_collection_record(collection)?
            .ok_or(SecretServiceError::CollectionNotFound(collection))?;
        collection
            .items
            .into_iter()
            .map(|id| self.require_item(id))
            .collect()
    }

    /// Search all collections using exact matches for every supplied attribute.
    pub fn search(
        &self,
        attributes: &BTreeMap<String, String>,
    ) -> Result<Vec<SecretItem>, SecretServiceError> {
        self.validate_attributes(attributes)?;
        let _guard = self.lock();
        let mut found = Vec::new();
        for collection_id in self.load_catalog()?.collections {
            let collection = self.load_collection_record(collection_id)?.ok_or_else(|| {
                SecretServiceError::InconsistentCatalog(format!(
                    "collection {collection_id} is indexed but absent"
                ))
            })?;
            for id in collection.items {
                let item = self.require_item_record(id)?;
                if attributes
                    .iter()
                    .all(|(key, value)| item.attributes.get(key) == Some(value))
                {
                    found.push(item.metadata(id));
                }
            }
        }
        found.sort_by_key(|item| item.id);
        Ok(found)
    }

    /// Create or exact-attribute-replace one item.
    pub fn create_item(
        &self,
        mut request: NewSecretItem,
    ) -> Result<SecretItem, SecretServiceError> {
        self.validate_name(&request.label, "item label")?;
        self.validate_name(&request.content_type, "content type")?;
        self.validate_attributes(&request.attributes)?;
        self.validate_secret(&request.secret)?;
        let _guard = self.lock();
        let mut collection_record = self
            .load_collection_record(request.collection)?
            .ok_or(SecretServiceError::CollectionNotFound(request.collection))?;
        if request.replace {
            for id in &collection_record.items {
                let mut current = self.require_item_record(*id)?;
                if current.attributes == request.attributes {
                    current.label.clone_from(&request.label);
                    current.secret.zeroize();
                    current.secret = std::mem::take(&mut request.secret);
                    current.content_type.clone_from(&request.content_type);
                    current.modified = request.unix_secs;
                    let metadata = current.metadata(*id);
                    self.storage.save_record(self.item_path(*id), &current)?;
                    return Ok(metadata);
                }
            }
        }
        if collection_record.items.len() >= self.limits.max_items_per_collection {
            return Err(SecretServiceError::Limit("items per collection"));
        }
        let id = SecretItemId::mint();
        let record = StoredItem {
            version: RECORD_VERSION,
            collection: request.collection,
            label: std::mem::take(&mut request.label),
            attributes: std::mem::take(&mut request.attributes),
            secret: std::mem::take(&mut request.secret),
            content_type: std::mem::take(&mut request.content_type),
            created: request.unix_secs,
            modified: request.unix_secs,
        };
        let metadata = record.metadata(id);
        self.storage.save_record(self.item_path(id), &record)?;
        collection_record.items.push(id);
        collection_record.modified = request.unix_secs;
        self.storage
            .save_record(self.collection_path(request.collection), &collection_record)?;
        Ok(metadata)
    }

    /// Read one item's secret-free metadata.
    pub fn item(&self, id: SecretItemId) -> Result<SecretItem, SecretServiceError> {
        let _guard = self.lock();
        self.require_item(id)
    }

    /// Change an item's lookup attributes.
    pub fn set_item_attributes(
        &self,
        id: SecretItemId,
        attributes: BTreeMap<String, String>,
        unix_secs: u64,
    ) -> Result<(), SecretServiceError> {
        self.validate_attributes(&attributes)?;
        self.update_item(id, |item| {
            item.attributes = attributes;
            item.modified = unix_secs;
        })
    }

    /// Change an item's label.
    pub fn set_item_label(
        &self,
        id: SecretItemId,
        label: &str,
        unix_secs: u64,
    ) -> Result<(), SecretServiceError> {
        self.validate_name(label, "item label")?;
        self.update_item(id, |item| {
            item.label = label.to_string();
            item.modified = unix_secs;
        })
    }

    #[cfg(any(test, target_os = "linux"))]
    pub(super) fn set_secret(
        &self,
        id: SecretItemId,
        secret: Vec<u8>,
        content_type: &str,
        unix_secs: u64,
    ) -> Result<(), SecretServiceError> {
        self.validate_secret(&secret)?;
        self.validate_name(content_type, "content type")?;
        self.update_item(id, |item| {
            item.secret.zeroize();
            item.secret = secret;
            item.content_type = content_type.to_string();
            item.modified = unix_secs;
        })
    }

    #[cfg(any(test, target_os = "linux"))]
    pub(super) fn secret(&self, id: SecretItemId) -> Result<SecretValue, SecretServiceError> {
        let _guard = self.lock();
        let item = self.require_item_record(id)?;
        Ok(SecretValue {
            bytes: Zeroizing::new(item.secret.clone()),
            content_type: item.content_type.clone(),
        })
    }

    /// Delete an item and remove it from its collection index.
    pub fn delete_item(&self, id: SecretItemId) -> Result<(), SecretServiceError> {
        let _guard = self.lock();
        let item = self.require_item_record(id)?;
        self.storage.delete_record(self.item_path(id))?;
        let mut collection = self
            .load_collection_record(item.collection)?
            .ok_or_else(|| {
                SecretServiceError::InconsistentCatalog(format!(
                    "item {id} names absent collection {}",
                    item.collection
                ))
            })?;
        collection.items.retain(|candidate| *candidate != id);
        self.storage
            .save_record(self.collection_path(item.collection), &collection)
            .map_err(SecretServiceError::from)
    }

    fn create_collection_locked(
        &self,
        label: &str,
        alias: Option<&str>,
        unix_secs: u64,
    ) -> Result<SecretCollection, SecretServiceError> {
        let mut catalog = self.load_catalog()?;
        if catalog.collections.len() >= self.limits.max_collections {
            return Err(SecretServiceError::Limit("collections per persona"));
        }
        let id = SecretCollectionId::mint();
        let record = StoredCollection {
            version: RECORD_VERSION,
            label: label.to_string(),
            items: Vec::new(),
            created: unix_secs,
            modified: unix_secs,
        };
        self.storage
            .save_record(self.collection_path(id), &record)?;
        catalog.collections.push(id);
        catalog.collections.sort();
        if let Some(alias) = alias {
            catalog.aliases.insert(alias.to_string(), id);
        }
        self.save_catalog(&catalog)?;
        Ok(record.metadata(id))
    }

    fn set_collection_label_locked(
        &self,
        id: SecretCollectionId,
        label: &str,
        unix_secs: u64,
    ) -> Result<(), SecretServiceError> {
        let mut record = self
            .load_collection_record(id)?
            .ok_or(SecretServiceError::CollectionNotFound(id))?;
        record.label = label.to_string();
        record.modified = unix_secs;
        self.storage
            .save_record(self.collection_path(id), &record)
            .map_err(SecretServiceError::from)
    }

    fn update_item(
        &self,
        id: SecretItemId,
        update: impl FnOnce(&mut StoredItem),
    ) -> Result<(), SecretServiceError> {
        let _guard = self.lock();
        self.storage.update_record(
            self.item_path(id),
            |current: Option<StoredItem>| -> Result<_, SecretServiceError> {
                let mut current = current
                    .ok_or(SecretServiceError::ItemNotFound(id))?
                    .check_version()?;
                update(&mut current);
                Ok(((), SealedRecordChange::Replace(current)))
            },
        )
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
