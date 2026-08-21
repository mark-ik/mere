use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use zbus::Connection;
use zbus::message::Header;
use zbus::names::BusName;
use zbus::zvariant::OwnedObjectPath;

use super::{
    DbusResult, SecretDbusError, SecretServiceLimits, SecretServiceStore, item_path,
    parse_collection_path, parse_item_path,
};
use crate::secret_service::{SecretCollectionId, SecretItemId};

/// Bus-authenticated caller facts supplied to the resident access policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretServiceCaller {
    /// Unique D-Bus connection name from the method header.
    pub bus_name: String,
    /// Unix uid reported by the session bus, when available.
    pub unix_user_id: Option<u32>,
    /// Process id reported by the session bus, when available.
    pub process_id: Option<u32>,
    /// Current `/proc/<pid>/exe` target, when resolvable.
    pub executable: Option<PathBuf>,
    /// Linux security label bytes reported by the bus, when available.
    pub linux_security_label: Option<Vec<u8>>,
}

/// Security-relevant operation presented to the host policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SecretServiceOperation {
    OpenSession,
    ListCollections,
    CreateCollection,
    Search,
    Lock,
    Unlock,
    ReadAlias,
    SetAlias,
    ReadCollection(SecretCollectionId),
    ChangeCollection(SecretCollectionId),
    DeleteCollection(SecretCollectionId),
    CreateItem(SecretCollectionId),
    ReadItem(SecretItemId),
    ReadSecret(SecretItemId),
    ChangeItem(SecretItemId),
    DeleteItem(SecretItemId),
}

/// Host-owned policy deciding which bus-authenticated callers may use the adapter.
pub trait SecretServiceAccessPolicy: Send + Sync + 'static {
    /// Return true only when this exact caller may perform the operation.
    fn allows(&self, caller: &SecretServiceCaller, operation: &SecretServiceOperation) -> bool;
}

impl<F> SecretServiceAccessPolicy for F
where
    F: Fn(&SecretServiceCaller, &SecretServiceOperation) -> bool + Send + Sync + 'static,
{
    fn allows(&self, caller: &SecretServiceCaller, operation: &SecretServiceOperation) -> bool {
        self(caller, operation)
    }
}

pub(super) struct ServiceState {
    pub(super) store: SecretServiceStore,
    limits: SecretServiceLimits,
    policy: Arc<dyn SecretServiceAccessPolicy>,
    sessions: Mutex<HashMap<String, String>>,
    locked_collections: Mutex<BTreeSet<SecretCollectionId>>,
    locked_items: Mutex<BTreeSet<SecretItemId>>,
}

impl ServiceState {
    pub(super) fn new(
        store: SecretServiceStore,
        limits: SecretServiceLimits,
        policy: Arc<dyn SecretServiceAccessPolicy>,
    ) -> Self {
        Self {
            store,
            limits,
            policy,
            sessions: Mutex::new(HashMap::new()),
            locked_collections: Mutex::new(BTreeSet::new()),
            locked_items: Mutex::new(BTreeSet::new()),
        }
    }

    pub(super) async fn authorize(
        &self,
        connection: &Connection,
        header: &Header<'_>,
        operation: SecretServiceOperation,
    ) -> DbusResult<SecretServiceCaller> {
        let caller = caller(connection, header).await?;
        if self.policy.allows(&caller, &operation) {
            Ok(caller)
        } else {
            Err(SecretDbusError::AccessDenied(format!(
                "caller {} is not admitted for {operation:?}",
                caller.bus_name
            )))
        }
    }

    pub(super) fn open_session(&self, owner: &str) -> DbusResult<OwnedObjectPath> {
        let mut sessions = self.sessions.lock().unwrap();
        if sessions.len() >= self.limits.max_sessions {
            return Err(SecretDbusError::LimitsExceeded(format!(
                "the resident retains at most {} Secret Service sessions",
                self.limits.max_sessions
            )));
        }
        let id = uuid::Uuid::new_v4();
        let path =
            OwnedObjectPath::try_from(format!("/org/freedesktop/secrets/session/{}", id.simple()))
                .expect("UUID session path is valid");
        sessions.insert(path.as_str().to_owned(), owner.to_string());
        Ok(path)
    }

    pub(super) fn require_session(&self, path: &OwnedObjectPath, owner: &str) -> DbusResult<()> {
        match self.sessions.lock().unwrap().get(path.as_str()) {
            Some(session_owner) if session_owner == owner => Ok(()),
            _ => Err(SecretDbusError::NoSession(
                "session is absent or belongs to another D-Bus connection".into(),
            )),
        }
    }

    pub(super) fn close_session(&self, path: &OwnedObjectPath, owner: &str) -> DbusResult<()> {
        self.require_session(path, owner)?;
        self.sessions.lock().unwrap().remove(path.as_str());
        Ok(())
    }

    pub(super) fn collection_locked(&self, id: SecretCollectionId) -> bool {
        self.locked_collections.lock().unwrap().contains(&id)
    }

    pub(super) fn item_locked(&self, id: SecretItemId) -> DbusResult<bool> {
        if self.locked_items.lock().unwrap().contains(&id) {
            return Ok(true);
        }
        let item = self.store.item(id)?;
        Ok(self.collection_locked(item.collection))
    }

    pub(super) fn require_collection_unlocked(&self, id: SecretCollectionId) -> DbusResult<()> {
        if self.collection_locked(id) {
            Err(SecretDbusError::IsLocked(format!(
                "collection {id} is locked"
            )))
        } else {
            Ok(())
        }
    }

    pub(super) fn require_item_unlocked(&self, id: SecretItemId) -> DbusResult<()> {
        if self.item_locked(id)? {
            Err(SecretDbusError::IsLocked(format!("item {id} is locked")))
        } else {
            Ok(())
        }
    }

    pub(super) fn lock_object(&self, path: &OwnedObjectPath) -> DbusResult<bool> {
        if let Some(id) = parse_collection_path(path) {
            self.store.collection(id)?;
            self.locked_collections.lock().unwrap().insert(id);
            return Ok(true);
        }
        if let Some(id) = parse_item_path(path) {
            self.store.item(id)?;
            self.locked_items.lock().unwrap().insert(id);
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn unlock_object(&self, path: &OwnedObjectPath) -> DbusResult<bool> {
        if let Some(id) = parse_collection_path(path) {
            self.store.collection(id)?;
            self.locked_collections.lock().unwrap().remove(&id);
            return Ok(true);
        }
        if let Some(id) = parse_item_path(path) {
            self.store.item(id)?;
            self.locked_items.lock().unwrap().remove(&id);
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn split_locked(
        &self,
        items: impl IntoIterator<Item = SecretItemId>,
    ) -> DbusResult<(Vec<OwnedObjectPath>, Vec<OwnedObjectPath>)> {
        let mut unlocked = Vec::new();
        let mut locked = Vec::new();
        for id in items {
            if self.item_locked(id)? {
                locked.push(item_path(id));
            } else {
                unlocked.push(item_path(id));
            }
        }
        Ok((unlocked, locked))
    }
}

async fn caller(connection: &Connection, header: &Header<'_>) -> DbusResult<SecretServiceCaller> {
    let sender = header
        .sender()
        .ok_or_else(|| {
            SecretDbusError::AccessDenied("method has no bus-authenticated sender".into())
        })?
        .to_owned();
    let proxy = zbus::fdo::DBusProxy::new(connection).await?;
    let credentials = proxy
        .get_connection_credentials(BusName::from(sender.clone()))
        .await?;
    let process_id = credentials.process_id();
    let executable = process_id.and_then(|pid| std::fs::read_link(format!("/proc/{pid}/exe")).ok());
    Ok(SecretServiceCaller {
        bus_name: sender.to_string(),
        unix_user_id: credentials.unix_user_id(),
        process_id,
        executable,
        linux_security_label: credentials.linux_security_label().cloned(),
    })
}
