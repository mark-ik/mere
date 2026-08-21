use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use zbus::message::{Header, Message};
use zbus::names::ErrorName;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{Connection, DBusError};

use super::{
    SecretCollectionId, SecretItemId, SecretServiceError, SecretServiceLimits, SecretServiceStore,
};

mod objects;
mod service;
mod state;

pub use state::{SecretServiceAccessPolicy, SecretServiceCaller, SecretServiceOperation};

use objects::{CollectionInterface, ItemInterface};
use service::ServiceInterface;
use state::ServiceState;

pub(super) const SERVICE_NAME: &str = "org.freedesktop.secrets";
pub(super) const SERVICE_PATH: &str = "/org/freedesktop/secrets";
pub(super) const ROOT_PATH: &str = "/";
pub(super) const COLLECTION_LABEL_PROPERTY: &str = "org.freedesktop.Secret.Collection.Label";
pub(super) const ITEM_LABEL_PROPERTY: &str = "org.freedesktop.Secret.Item.Label";
pub(super) const ITEM_ATTRIBUTES_PROPERTY: &str = "org.freedesktop.Secret.Item.Attributes";

pub(super) type DbusSecret = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);
pub(super) type DbusResult<T> = Result<T, SecretDbusError>;

/// Running Secret Service name and object tree.
pub struct SecretServiceServer {
    connection: Connection,
}

impl SecretServiceServer {
    /// Session-bus connection that owns `org.freedesktop.secrets`.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

/// Failure while starting the D-Bus adapter.
#[derive(Debug, thiserror::Error)]
pub enum SecretServiceStartError {
    /// The collection store could not initialize.
    #[error(transparent)]
    Storage(#[from] SecretServiceError),
    /// The session bus, name, or object server could not start.
    #[error("start Secret Service D-Bus adapter: {0}")]
    Dbus(#[from] zbus::Error),
}

/// Serve one persona's collections as Freedesktop Secret Service 0.2.
///
/// The standard bus name is requested without replacement. Startup therefore
/// fails when another keyring already owns the desktop surface.
pub async fn serve(
    store: SecretServiceStore,
    policy: Arc<dyn SecretServiceAccessPolicy>,
    default_collection_label: &str,
) -> Result<SecretServiceServer, SecretServiceStartError> {
    store.ensure_default_collection(default_collection_label, now_unix_secs())?;
    let limits = store.limits();
    let state = Arc::new(ServiceState::new(store, limits, policy));
    let connection = zbus::connection::Builder::session()?
        .name(SERVICE_NAME)?
        .serve_at(SERVICE_PATH, ServiceInterface::new(Arc::clone(&state)))?
        .build()
        .await?;

    for collection in state.store.collections()? {
        register_collection(&connection, Arc::clone(&state), collection.id).await?;
        for item in state.store.items(collection.id)? {
            register_item(&connection, Arc::clone(&state), item.id).await?;
        }
    }
    for (alias, collection) in state.store.aliases()? {
        register_alias(&connection, Arc::clone(&state), &alias, collection).await?;
    }

    Ok(SecretServiceServer { connection })
}

async fn register_collection(
    connection: &Connection,
    state: Arc<ServiceState>,
    id: SecretCollectionId,
) -> Result<(), zbus::Error> {
    connection
        .object_server()
        .at(collection_path(id), CollectionInterface::new(state, id))
        .await?;
    Ok(())
}

async fn register_item(
    connection: &Connection,
    state: Arc<ServiceState>,
    id: SecretItemId,
) -> Result<(), zbus::Error> {
    connection
        .object_server()
        .at(item_path(id), ItemInterface::new(state, id))
        .await?;
    Ok(())
}

async fn register_alias(
    connection: &Connection,
    state: Arc<ServiceState>,
    alias: &str,
    id: SecretCollectionId,
) -> Result<(), zbus::Error> {
    connection
        .object_server()
        .at(alias_path(alias)?, CollectionInterface::new(state, id))
        .await?;
    Ok(())
}

pub(super) fn collection_path(id: SecretCollectionId) -> OwnedObjectPath {
    OwnedObjectPath::try_from(format!(
        "{SERVICE_PATH}/collection/{}",
        id.as_uuid().simple()
    ))
    .expect("UUID collection path is valid")
}

pub(super) fn item_path(id: SecretItemId) -> OwnedObjectPath {
    OwnedObjectPath::try_from(format!("{SERVICE_PATH}/item/{}", id.as_uuid().simple()))
        .expect("UUID item path is valid")
}

pub(super) fn alias_path(alias: &str) -> DbusResult<OwnedObjectPath> {
    OwnedObjectPath::try_from(format!("{SERVICE_PATH}/aliases/{alias}")).map_err(|_| {
        SecretDbusError::InvalidArgs(format!(
            "collection alias {alias:?} is not a valid D-Bus object-path element"
        ))
    })
}

pub(super) fn root_path() -> OwnedObjectPath {
    OwnedObjectPath::try_from(ROOT_PATH).expect("root object path is valid")
}

pub(super) fn parse_collection_path(path: &OwnedObjectPath) -> Option<SecretCollectionId> {
    parse_uuid_path(path, &format!("{SERVICE_PATH}/collection/")).map(SecretCollectionId::from_uuid)
}

pub(super) fn parse_item_path(path: &OwnedObjectPath) -> Option<SecretItemId> {
    parse_uuid_path(path, &format!("{SERVICE_PATH}/item/")).map(SecretItemId::from_uuid)
}

fn parse_uuid_path(path: &OwnedObjectPath, prefix: &str) -> Option<uuid::Uuid> {
    path.as_str().strip_prefix(prefix)?.parse().ok()
}

pub(super) fn property_string(
    properties: &HashMap<String, OwnedValue>,
    key: &'static str,
) -> DbusResult<String> {
    properties
        .get(key)
        .ok_or_else(|| SecretDbusError::InvalidArgs(format!("missing property {key}")))
        .and_then(|value| {
            String::try_from(&**value).map_err(|_| {
                SecretDbusError::InvalidArgs(format!("property {key} must be a string"))
            })
        })
}

pub(super) fn property_attributes(
    properties: &HashMap<String, OwnedValue>,
) -> DbusResult<HashMap<String, String>> {
    let value = properties
        .get(ITEM_ATTRIBUTES_PROPERTY)
        .ok_or_else(|| {
            SecretDbusError::InvalidArgs(format!("missing property {ITEM_ATTRIBUTES_PROPERTY}"))
        })?
        .try_clone()
        .map_err(SecretDbusError::from)?;
    value.try_into().map_err(|_| {
        SecretDbusError::InvalidArgs(format!(
            "property {ITEM_ATTRIBUTES_PROPERTY} must be a string dictionary"
        ))
    })
}

pub(super) fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[derive(Debug)]
pub(super) enum SecretDbusError {
    IsLocked(String),
    NoSession(String),
    NoSuchObject(String),
    AccessDenied(String),
    NotSupported(String),
    InvalidArgs(String),
    LimitsExceeded(String),
    Failed(String),
}

impl fmt::Display for SecretDbusError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.description().unwrap_or("Secret Service error"))
    }
}

impl std::error::Error for SecretDbusError {}

impl DBusError for SecretDbusError {
    fn create_reply(&self, call: &Header<'_>) -> zbus::Result<Message> {
        Message::error(call, self.name())?.build(&(self.description().unwrap_or_default(),))
    }

    fn name(&self) -> ErrorName<'_> {
        let name = match self {
            Self::IsLocked(_) => "org.freedesktop.Secret.Error.IsLocked",
            Self::NoSession(_) => "org.freedesktop.Secret.Error.NoSession",
            Self::NoSuchObject(_) => "org.freedesktop.Secret.Error.NoSuchObject",
            Self::AccessDenied(_) => "org.freedesktop.DBus.Error.AccessDenied",
            Self::NotSupported(_) => "org.freedesktop.DBus.Error.NotSupported",
            Self::InvalidArgs(_) => "org.freedesktop.DBus.Error.InvalidArgs",
            Self::LimitsExceeded(_) => "org.freedesktop.DBus.Error.LimitsExceeded",
            Self::Failed(_) => "org.freedesktop.DBus.Error.Failed",
        };
        ErrorName::from_static_str_unchecked(name)
    }

    fn description(&self) -> Option<&str> {
        let description = match self {
            Self::IsLocked(description)
            | Self::NoSession(description)
            | Self::NoSuchObject(description)
            | Self::AccessDenied(description)
            | Self::NotSupported(description)
            | Self::InvalidArgs(description)
            | Self::LimitsExceeded(description)
            | Self::Failed(description) => description,
        };
        Some(description)
    }
}

impl From<SecretServiceError> for SecretDbusError {
    fn from(error: SecretServiceError) -> Self {
        match error {
            SecretServiceError::CollectionNotFound(_) | SecretServiceError::ItemNotFound(_) => {
                Self::NoSuchObject(error.to_string())
            }
            SecretServiceError::Limit(_) => Self::LimitsExceeded(error.to_string()),
            SecretServiceError::InvalidText(_) => Self::InvalidArgs(error.to_string()),
            _ => Self::Failed(error.to_string()),
        }
    }
}

impl From<zbus::Error> for SecretDbusError {
    fn from(error: zbus::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

impl From<zbus::fdo::Error> for SecretDbusError {
    fn from(error: zbus::fdo::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

impl From<zbus::zvariant::Error> for SecretDbusError {
    fn from(error: zbus::zvariant::Error) -> Self {
        Self::InvalidArgs(error.to_string())
    }
}

impl From<SecretDbusError> for zbus::fdo::Error {
    fn from(error: SecretDbusError) -> Self {
        let description = error.to_string();
        match error {
            SecretDbusError::AccessDenied(_) => Self::AccessDenied(description),
            SecretDbusError::NotSupported(_) => Self::NotSupported(description),
            SecretDbusError::InvalidArgs(_) => Self::InvalidArgs(description),
            SecretDbusError::LimitsExceeded(_) => Self::LimitsExceeded(description),
            SecretDbusError::IsLocked(_)
            | SecretDbusError::NoSession(_)
            | SecretDbusError::NoSuchObject(_)
            | SecretDbusError::Failed(_) => Self::Failed(description),
        }
    }
}

impl From<SecretServiceError> for zbus::fdo::Error {
    fn from(error: SecretServiceError) -> Self {
        SecretDbusError::from(error).into()
    }
}

impl From<SecretDbusError> for zbus::Error {
    fn from(error: SecretDbusError) -> Self {
        Self::FDO(Box::new(error.into()))
    }
}
