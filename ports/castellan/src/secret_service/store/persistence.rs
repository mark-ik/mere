// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::collections::BTreeMap;
use std::sync::MutexGuard;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use super::{
    RECORD_DIRECTORY, RECORD_VERSION, SecretCollection, SecretCollectionId, SecretItem,
    SecretItemId, SecretServiceError, SecretServiceStore,
};

impl SecretServiceStore {
    pub(super) fn load_catalog(&self) -> Result<StoredCatalog, SecretServiceError> {
        self.storage
            .load_record(self.catalog_path())?
            .map(StoredCatalog::check_version)
            .transpose()
            .map(|catalog| catalog.unwrap_or_default())
    }

    pub(super) fn save_catalog(&self, catalog: &StoredCatalog) -> Result<(), SecretServiceError> {
        self.storage
            .save_record(self.catalog_path(), catalog)
            .map_err(SecretServiceError::from)
    }

    pub(super) fn load_collection_record(
        &self,
        id: SecretCollectionId,
    ) -> Result<Option<StoredCollection>, SecretServiceError> {
        self.storage
            .load_record(self.collection_path(id))?
            .map(StoredCollection::check_version)
            .transpose()
    }

    pub(super) fn require_collection(
        &self,
        id: SecretCollectionId,
    ) -> Result<SecretCollection, SecretServiceError> {
        self.load_collection_record(id)?
            .map(|record| record.metadata(id))
            .ok_or(SecretServiceError::CollectionNotFound(id))
    }

    pub(super) fn require_item_record(
        &self,
        id: SecretItemId,
    ) -> Result<StoredItem, SecretServiceError> {
        self.storage
            .load_record(self.item_path(id))?
            .ok_or(SecretServiceError::ItemNotFound(id))
            .and_then(StoredItem::check_version)
    }

    pub(super) fn require_item(&self, id: SecretItemId) -> Result<SecretItem, SecretServiceError> {
        self.require_item_record(id)
            .map(|record| record.metadata(id))
    }

    pub(super) fn validate_name(
        &self,
        text: &str,
        kind: &'static str,
    ) -> Result<(), SecretServiceError> {
        if text.is_empty() || text.chars().any(char::is_control) {
            return Err(SecretServiceError::InvalidText(kind));
        }
        if text.len() > self.limits.max_name_bytes {
            return Err(SecretServiceError::Limit(kind));
        }
        Ok(())
    }

    pub(super) fn validate_attributes(
        &self,
        attributes: &BTreeMap<String, String>,
    ) -> Result<(), SecretServiceError> {
        if attributes.len() > self.limits.max_attributes {
            return Err(SecretServiceError::Limit("attributes per item"));
        }
        for (key, value) in attributes {
            self.validate_name(key, "attribute key")?;
            if value.len() > self.limits.max_attribute_value_bytes
                || value.chars().any(char::is_control)
            {
                return Err(SecretServiceError::Limit("attribute value"));
            }
        }
        Ok(())
    }

    pub(super) fn validate_secret(&self, secret: &[u8]) -> Result<(), SecretServiceError> {
        if secret.len() > self.limits.max_secret_bytes {
            Err(SecretServiceError::Limit("secret bytes"))
        } else {
            Ok(())
        }
    }

    pub(super) fn validate_alias(&self, alias: &str) -> Result<(), SecretServiceError> {
        self.validate_name(alias, "collection alias")?;
        if alias
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            Ok(())
        } else {
            Err(SecretServiceError::InvalidText("collection alias"))
        }
    }

    fn base_path(&self) -> String {
        format!("{RECORD_DIRECTORY}/{}", self.persona.as_uuid())
    }

    pub(super) fn catalog_path(&self) -> String {
        format!("{}/catalog.json", self.base_path())
    }

    pub(super) fn collection_path(&self, id: SecretCollectionId) -> String {
        format!("{}/collections/{id}.json", self.base_path())
    }

    pub(super) fn item_path(&self, id: SecretItemId) -> String {
        format!("{}/items/{id}.json", self.base_path())
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, ()> {
        self.transaction
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct StoredCatalog {
    pub(super) version: u8,
    pub(super) collections: Vec<SecretCollectionId>,
    pub(super) aliases: BTreeMap<String, SecretCollectionId>,
}

impl Default for StoredCatalog {
    fn default() -> Self {
        Self {
            version: RECORD_VERSION,
            collections: Vec::new(),
            aliases: BTreeMap::new(),
        }
    }
}

impl StoredCatalog {
    fn check_version(self) -> Result<Self, SecretServiceError> {
        check_version(self.version)?;
        Ok(self)
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct StoredCollection {
    pub(super) version: u8,
    pub(super) label: String,
    pub(super) items: Vec<SecretItemId>,
    pub(super) created: u64,
    pub(super) modified: u64,
}

impl StoredCollection {
    fn check_version(self) -> Result<Self, SecretServiceError> {
        check_version(self.version)?;
        Ok(self)
    }

    pub(super) fn metadata(&self, id: SecretCollectionId) -> SecretCollection {
        SecretCollection {
            id,
            label: self.label.clone(),
            created: self.created,
            modified: self.modified,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct StoredItem {
    pub(super) version: u8,
    pub(super) collection: SecretCollectionId,
    pub(super) label: String,
    pub(super) attributes: BTreeMap<String, String>,
    pub(super) secret: Vec<u8>,
    pub(super) content_type: String,
    pub(super) created: u64,
    pub(super) modified: u64,
}

impl Drop for StoredItem {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

impl StoredItem {
    pub(super) fn check_version(self) -> Result<Self, SecretServiceError> {
        check_version(self.version)?;
        Ok(self)
    }

    pub(super) fn metadata(&self, id: SecretItemId) -> SecretItem {
        SecretItem {
            id,
            collection: self.collection,
            label: self.label.clone(),
            attributes: self.attributes.clone(),
            created: self.created,
            modified: self.modified,
        }
    }
}

fn check_version(version: u8) -> Result<(), SecretServiceError> {
    if version == RECORD_VERSION {
        Ok(())
    } else {
        Err(SecretServiceError::UnsupportedRecordVersion(version))
    }
}
