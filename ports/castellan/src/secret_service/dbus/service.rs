use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use zbus::Connection;
use zbus::message::Header;
use zbus::object_server::{ObjectServer, SignalEmitter};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use super::state::{SecretServiceOperation, ServiceState};
use super::{
    COLLECTION_LABEL_PROPERTY, DbusSecret, SecretDbusError, collection_path, now_unix_secs,
    property_string, register_collection, root_path,
};

pub(super) struct ServiceInterface {
    state: Arc<ServiceState>,
}

impl ServiceInterface {
    pub(super) fn new(state: Arc<ServiceState>) -> Self {
        Self { state }
    }
}

#[zbus::interface(name = "org.freedesktop.Secret.Service")]
impl ServiceInterface {
    async fn open_session(
        &self,
        algorithm: &str,
        input: Value<'_>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> Result<(OwnedValue, OwnedObjectPath), SecretDbusError> {
        let caller = self
            .state
            .authorize(connection, &header, SecretServiceOperation::OpenSession)
            .await?;
        if algorithm != "plain" {
            return Err(SecretDbusError::NotSupported(format!(
                "unsupported Secret Service session algorithm {algorithm:?}"
            )));
        }
        let input = String::try_from(&input).map_err(|_| {
            SecretDbusError::InvalidArgs("plain session input must be a string".into())
        })?;
        if !input.is_empty() {
            return Err(SecretDbusError::InvalidArgs(
                "plain session input must be empty".into(),
            ));
        }
        let path = self.state.open_session(&caller.bus_name)?;
        if let Err(error) = server
            .at(
                path.clone(),
                SessionInterface::new(Arc::clone(&self.state), path.clone()),
            )
            .await
        {
            let _ = self.state.close_session(&path, &caller.bus_name);
            return Err(error.into());
        }
        let output = OwnedValue::try_from(Value::from(""))?;
        Ok((output, path))
    }

    async fn create_collection(
        &self,
        properties: HashMap<String, OwnedValue>,
        alias: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(OwnedObjectPath, OwnedObjectPath), SecretDbusError> {
        self.state
            .authorize(
                connection,
                &header,
                SecretServiceOperation::CreateCollection,
            )
            .await?;
        let label = property_string(&properties, COLLECTION_LABEL_PROPERTY)?;
        let existing = if alias.is_empty() {
            None
        } else {
            self.state.store.read_alias(alias)?
        };
        let collection = self.state.store.create_collection(
            &label,
            (!alias.is_empty()).then_some(alias),
            now_unix_secs(),
        )?;
        let path = collection_path(collection.id);
        if existing.is_none() {
            register_collection(connection, Arc::clone(&self.state), collection.id).await?;
            emitter.collection_created(path.clone()).await?;
        } else {
            emitter.collection_changed(path.clone()).await?;
        }
        Ok((path, root_path()))
    }

    async fn search_items(
        &self,
        attributes: HashMap<String, String>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<(Vec<OwnedObjectPath>, Vec<OwnedObjectPath>), SecretDbusError> {
        self.state
            .authorize(connection, &header, SecretServiceOperation::Search)
            .await?;
        let attributes = attributes.into_iter().collect::<BTreeMap<_, _>>();
        let items = self.state.store.search(&attributes)?;
        self.state
            .split_locked(items.into_iter().map(|item| item.id))
    }

    async fn unlock(
        &self,
        objects: Vec<OwnedObjectPath>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<(Vec<OwnedObjectPath>, OwnedObjectPath), SecretDbusError> {
        self.state
            .authorize(connection, &header, SecretServiceOperation::Unlock)
            .await?;
        let mut unlocked = Vec::new();
        for object in objects {
            if self.state.unlock_object(&object)? {
                unlocked.push(object);
            }
        }
        Ok((unlocked, root_path()))
    }

    async fn lock(
        &self,
        objects: Vec<OwnedObjectPath>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<(Vec<OwnedObjectPath>, OwnedObjectPath), SecretDbusError> {
        self.state
            .authorize(connection, &header, SecretServiceOperation::Lock)
            .await?;
        let mut locked = Vec::new();
        for object in objects {
            if self.state.lock_object(&object)? {
                locked.push(object);
            }
        }
        Ok((locked, root_path()))
    }

    async fn get_secrets(
        &self,
        items: Vec<OwnedObjectPath>,
        session: OwnedObjectPath,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<HashMap<OwnedObjectPath, DbusSecret>, SecretDbusError> {
        let caller = self
            .state
            .authorize(connection, &header, SecretServiceOperation::Search)
            .await?;
        self.state.require_session(&session, &caller.bus_name)?;
        let mut secrets = HashMap::new();
        for path in items {
            let id = super::parse_item_path(&path).ok_or_else(|| {
                SecretDbusError::NoSuchObject(format!("{path} is not a Secret Service item"))
            })?;
            self.state
                .authorize(connection, &header, SecretServiceOperation::ReadSecret(id))
                .await?;
            self.state.require_item_unlocked(id)?;
            let secret = self.state.store.secret(id)?;
            secrets.insert(
                path,
                (
                    session.clone(),
                    Vec::new(),
                    secret.bytes.to_vec(),
                    secret.content_type,
                ),
            );
        }
        Ok(secrets)
    }

    async fn read_alias(
        &self,
        name: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<OwnedObjectPath, SecretDbusError> {
        self.state
            .authorize(connection, &header, SecretServiceOperation::ReadAlias)
            .await?;
        Ok(self
            .state
            .store
            .read_alias(name)?
            .map(collection_path)
            .unwrap_or_else(root_path))
    }

    async fn set_alias(
        &self,
        name: &str,
        collection: OwnedObjectPath,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<(), SecretDbusError> {
        self.state
            .authorize(connection, &header, SecretServiceOperation::SetAlias)
            .await?;
        let collection = if collection.as_str() == "/" {
            None
        } else {
            Some(super::parse_collection_path(&collection).ok_or_else(|| {
                SecretDbusError::NoSuchObject("alias target is not a collection".into())
            })?)
        };
        self.state.store.set_alias(name, collection)?;
        Ok(())
    }

    #[zbus(property)]
    async fn collections(
        &self,
        #[zbus(header)] header: Option<Header<'_>>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<Vec<OwnedObjectPath>> {
        let header = header.ok_or_else(|| {
            zbus::fdo::Error::AccessDenied("property read has no authenticated header".into())
        })?;
        self.state
            .authorize(connection, &header, SecretServiceOperation::ListCollections)
            .await?;
        Ok(self
            .state
            .store
            .collections()?
            .into_iter()
            .map(|collection| collection_path(collection.id))
            .collect())
    }

    #[zbus(signal)]
    pub(super) async fn collection_created(
        emitter: &SignalEmitter<'_>,
        collection: OwnedObjectPath,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub(super) async fn collection_deleted(
        emitter: &SignalEmitter<'_>,
        collection: OwnedObjectPath,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub(super) async fn collection_changed(
        emitter: &SignalEmitter<'_>,
        collection: OwnedObjectPath,
    ) -> zbus::Result<()>;
}

struct SessionInterface {
    state: Arc<ServiceState>,
    path: OwnedObjectPath,
}

impl SessionInterface {
    fn new(state: Arc<ServiceState>, path: OwnedObjectPath) -> Self {
        Self { state, path }
    }
}

#[zbus::interface(name = "org.freedesktop.Secret.Session")]
impl SessionInterface {
    async fn close(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> Result<(), SecretDbusError> {
        let caller = self
            .state
            .authorize(connection, &header, SecretServiceOperation::OpenSession)
            .await?;
        self.state.close_session(&self.path, &caller.bus_name)
    }
}
