// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use zbus::Connection;
use zbus::message::Header;
use zbus::object_server::{ObjectServer, SignalEmitter};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use super::service::ServiceInterface;
use super::state::{SecretServiceOperation, ServiceState};
use super::{
    DbusSecret, ITEM_LABEL_PROPERTY, SERVICE_PATH, SecretDbusError, alias_path, collection_path,
    item_path, now_unix_secs, property_attributes, property_string, register_item, root_path,
};
use crate::secret_service::{SecretCollectionId, SecretItemId};

pub(super) struct CollectionInterface {
    state: Arc<ServiceState>,
    id: SecretCollectionId,
}

impl CollectionInterface {
    pub(super) fn new(state: Arc<ServiceState>, id: SecretCollectionId) -> Self {
        Self { state, id }
    }

    async fn authorize_read(
        &self,
        header: Option<Header<'_>>,
        connection: &Connection,
    ) -> zbus::fdo::Result<()> {
        let header = required_header(header)?;
        self.state
            .authorize(
                connection,
                &header,
                SecretServiceOperation::ReadCollection(self.id),
            )
            .await?;
        Ok(())
    }
}

#[zbus::interface(name = "org.freedesktop.Secret.Collection")]
impl CollectionInterface {
    async fn delete(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> Result<OwnedObjectPath, SecretDbusError> {
        self.state
            .authorize(
                connection,
                &header,
                SecretServiceOperation::DeleteCollection(self.id),
            )
            .await?;
        self.state.require_collection_unlocked(self.id)?;
        let items: Vec<_> = self
            .state
            .store
            .items(self.id)?
            .into_iter()
            .map(|item| item.id)
            .collect();
        let aliases: Vec<_> = self
            .state
            .store
            .aliases()?
            .into_iter()
            .filter_map(|(alias, id)| (id == self.id).then_some(alias))
            .collect();
        self.state.store.delete_collection(self.id)?;
        let emitter = SignalEmitter::new(connection, SERVICE_PATH)?;
        ServiceInterface::collection_deleted(&emitter, collection_path(self.id)).await?;
        for item in &items {
            server.remove::<ItemInterface, _>(item_path(*item)).await?;
        }
        for alias in aliases {
            server
                .remove::<CollectionInterface, _>(alias_path(&alias)?)
                .await?;
        }
        server
            .remove::<CollectionInterface, _>(collection_path(self.id))
            .await?;
        self.state.forget_collection(self.id, items);
        Ok(root_path())
    }

    async fn search_items(
        &self,
        attributes: HashMap<String, String>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<Vec<OwnedObjectPath>, SecretDbusError> {
        self.state
            .authorize(connection, &header, SecretServiceOperation::Search)
            .await?;
        let attributes = attributes.into_iter().collect::<BTreeMap<_, _>>();
        Ok(self
            .state
            .store
            .search(&attributes)?
            .into_iter()
            .filter(|item| item.collection == self.id)
            .map(|item| item_path(item.id))
            .collect())
    }

    async fn create_item(
        &self,
        properties: HashMap<String, OwnedValue>,
        secret: DbusSecret,
        replace: bool,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(OwnedObjectPath, OwnedObjectPath), SecretDbusError> {
        let caller = self
            .state
            .authorize(
                connection,
                &header,
                SecretServiceOperation::CreateItem(self.id),
            )
            .await?;
        self.state.require_collection_unlocked(self.id)?;
        self.state.require_session(&secret.0, &caller.bus_name)?;
        if !secret.1.is_empty() {
            return Err(SecretDbusError::InvalidArgs(
                "plain Secret parameters must be empty".into(),
            ));
        }
        let label = property_string(&properties, ITEM_LABEL_PROPERTY)?;
        let attributes = property_attributes(&properties)?
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        let before = if replace {
            self.state
                .store
                .search(&attributes)?
                .into_iter()
                .find(|item| item.collection == self.id)
                .map(|item| item.id)
        } else {
            None
        };
        let item = self.state.store.create_item(super::super::NewSecretItem {
            collection: self.id,
            label,
            attributes,
            secret: secret.2,
            content_type: secret.3,
            replace,
            unix_secs: now_unix_secs(),
        })?;
        let path = item_path(item.id);
        if before.is_some() {
            emitter.item_changed(path.clone()).await?;
        } else {
            register_item(connection, Arc::clone(&self.state), item.id).await?;
            emitter.item_created(path.clone()).await?;
        }
        Ok((path, root_path()))
    }

    #[zbus(property)]
    async fn items(
        &self,
        #[zbus(header)] header: Option<Header<'_>>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<Vec<OwnedObjectPath>> {
        let header = required_header(header)?;
        self.state
            .authorize(
                connection,
                &header,
                SecretServiceOperation::ReadCollection(self.id),
            )
            .await?;
        Ok(self
            .state
            .store
            .items(self.id)?
            .into_iter()
            .map(|item| item_path(item.id))
            .collect())
    }

    #[zbus(property)]
    async fn label(
        &self,
        #[zbus(header)] header: Option<Header<'_>>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<String> {
        let header = required_header(header)?;
        self.state
            .authorize(
                connection,
                &header,
                SecretServiceOperation::ReadCollection(self.id),
            )
            .await?;
        Ok(self.state.store.collection(self.id)?.label)
    }

    #[zbus(property)]
    async fn set_label(
        &self,
        label: &str,
        #[zbus(header)] header: Option<Header<'_>>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<()> {
        let header = required_header(header)?;
        self.state
            .authorize(
                connection,
                &header,
                SecretServiceOperation::ChangeCollection(self.id),
            )
            .await?;
        self.state.require_collection_unlocked(self.id)?;
        self.state
            .store
            .set_collection_label(self.id, label, now_unix_secs())?;
        let emitter = SignalEmitter::new(connection, SERVICE_PATH)?;
        ServiceInterface::collection_changed(&emitter, collection_path(self.id)).await?;
        Ok(())
    }

    #[zbus(property)]
    async fn locked(
        &self,
        #[zbus(header)] header: Option<Header<'_>>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<bool> {
        self.authorize_read(header, connection).await?;
        Ok(self.state.collection_locked(self.id))
    }

    #[zbus(property)]
    async fn created(
        &self,
        #[zbus(header)] header: Option<Header<'_>>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<u64> {
        self.authorize_read(header, connection).await?;
        Ok(self.state.store.collection(self.id)?.created)
    }

    #[zbus(property)]
    async fn modified(
        &self,
        #[zbus(header)] header: Option<Header<'_>>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<u64> {
        self.authorize_read(header, connection).await?;
        Ok(self.state.store.collection(self.id)?.modified)
    }

    #[zbus(signal)]
    pub(super) async fn item_created(
        emitter: &SignalEmitter<'_>,
        item: OwnedObjectPath,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub(super) async fn item_deleted(
        emitter: &SignalEmitter<'_>,
        item: OwnedObjectPath,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub(super) async fn item_changed(
        emitter: &SignalEmitter<'_>,
        item: OwnedObjectPath,
    ) -> zbus::Result<()>;
}

pub(super) struct ItemInterface {
    state: Arc<ServiceState>,
    id: SecretItemId,
}

impl ItemInterface {
    pub(super) fn new(state: Arc<ServiceState>, id: SecretItemId) -> Self {
        Self { state, id }
    }

    async fn emit_changed(&self, connection: &Connection) -> Result<(), SecretDbusError> {
        let collection = self.state.store.item(self.id)?.collection;
        let emitter = SignalEmitter::new(connection, collection_path(collection))?;
        CollectionInterface::item_changed(&emitter, item_path(self.id)).await?;
        Ok(())
    }

    async fn authorize_read(
        &self,
        header: Option<Header<'_>>,
        connection: &Connection,
    ) -> zbus::fdo::Result<()> {
        let header = required_header(header)?;
        self.state
            .authorize(
                connection,
                &header,
                SecretServiceOperation::ReadItem(self.id),
            )
            .await?;
        Ok(())
    }
}

#[zbus::interface(name = "org.freedesktop.Secret.Item")]
impl ItemInterface {
    async fn delete(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> Result<OwnedObjectPath, SecretDbusError> {
        self.state
            .authorize(
                connection,
                &header,
                SecretServiceOperation::DeleteItem(self.id),
            )
            .await?;
        self.state.require_item_unlocked(self.id)?;
        let collection = self.state.store.item(self.id)?.collection;
        self.state.store.delete_item(self.id)?;
        let emitter = SignalEmitter::new(connection, collection_path(collection))?;
        CollectionInterface::item_deleted(&emitter, item_path(self.id)).await?;
        server
            .remove::<ItemInterface, _>(item_path(self.id))
            .await?;
        self.state.forget_item(self.id);
        Ok(root_path())
    }

    async fn get_secret(
        &self,
        session: OwnedObjectPath,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<DbusSecret, SecretDbusError> {
        let caller = self
            .state
            .authorize(
                connection,
                &header,
                SecretServiceOperation::ReadSecret(self.id),
            )
            .await?;
        self.state.require_session(&session, &caller.bus_name)?;
        self.state.require_item_unlocked(self.id)?;
        let secret = self.state.store.secret(self.id)?;
        Ok((
            session,
            Vec::new(),
            secret.bytes.to_vec(),
            secret.content_type,
        ))
    }

    async fn set_secret(
        &self,
        secret: DbusSecret,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<(), SecretDbusError> {
        let caller = self
            .state
            .authorize(
                connection,
                &header,
                SecretServiceOperation::ChangeItem(self.id),
            )
            .await?;
        self.state.require_session(&secret.0, &caller.bus_name)?;
        self.state.require_item_unlocked(self.id)?;
        if !secret.1.is_empty() {
            return Err(SecretDbusError::InvalidArgs(
                "plain Secret parameters must be empty".into(),
            ));
        }
        self.state
            .store
            .set_secret(self.id, secret.2, &secret.3, now_unix_secs())?;
        self.emit_changed(connection).await
    }

    #[zbus(property)]
    async fn locked(
        &self,
        #[zbus(header)] header: Option<Header<'_>>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<bool> {
        self.authorize_read(header, connection).await?;
        Ok(self.state.item_locked(self.id)?)
    }

    #[zbus(property)]
    async fn attributes(
        &self,
        #[zbus(header)] header: Option<Header<'_>>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<HashMap<String, String>> {
        self.authorize_read(header, connection).await?;
        Ok(self
            .state
            .store
            .item(self.id)?
            .attributes
            .into_iter()
            .collect())
    }

    #[zbus(property)]
    async fn set_attributes(
        &self,
        attributes: HashMap<String, String>,
        #[zbus(header)] header: Option<Header<'_>>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<()> {
        let header = required_header(header)?;
        self.state
            .authorize(
                connection,
                &header,
                SecretServiceOperation::ChangeItem(self.id),
            )
            .await?;
        self.state.require_item_unlocked(self.id)?;
        self.state.store.set_item_attributes(
            self.id,
            attributes.into_iter().collect(),
            now_unix_secs(),
        )?;
        Ok(self.emit_changed(connection).await?)
    }

    #[zbus(property)]
    async fn label(
        &self,
        #[zbus(header)] header: Option<Header<'_>>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<String> {
        self.authorize_read(header, connection).await?;
        Ok(self.state.store.item(self.id)?.label)
    }

    #[zbus(property)]
    async fn set_label(
        &self,
        label: &str,
        #[zbus(header)] header: Option<Header<'_>>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<()> {
        let header = required_header(header)?;
        self.state
            .authorize(
                connection,
                &header,
                SecretServiceOperation::ChangeItem(self.id),
            )
            .await?;
        self.state.require_item_unlocked(self.id)?;
        self.state
            .store
            .set_item_label(self.id, label, now_unix_secs())?;
        Ok(self.emit_changed(connection).await?)
    }

    #[zbus(property)]
    async fn created(
        &self,
        #[zbus(header)] header: Option<Header<'_>>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<u64> {
        self.authorize_read(header, connection).await?;
        Ok(self.state.store.item(self.id)?.created)
    }

    #[zbus(property)]
    async fn modified(
        &self,
        #[zbus(header)] header: Option<Header<'_>>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<u64> {
        self.authorize_read(header, connection).await?;
        Ok(self.state.store.item(self.id)?.modified)
    }
}

fn required_header(header: Option<Header<'_>>) -> zbus::fdo::Result<Header<'_>> {
    header.ok_or_else(|| {
        zbus::fdo::Error::AccessDenied("property operation has no authenticated header".into())
    })
}
