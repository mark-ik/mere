//! The browser's durable byte store, as a [`Backend`].
//!
//! IndexedDB is the one storage API every browser exposes with a durable,
//! transactional, origin-scoped guarantee, which is the contract [`Backend`]
//! asks for. It sits beside [`RedbBackend`](crate::RedbBackend) and
//! [`ZipBackend`](crate::ZipBackend) as the third host realization: redb for
//! the desktop, zip for the portable archive, this for the browser.
//!
//! Keys are flat strings, so `list` and `scan` filter the key set rather than
//! using a cursor range. That is correct for stores of the size a browser tab
//! holds and keeps the transaction short; a cursor is the change to make if a
//! consumer grows a key set where enumerating it is the cost.
//!
//! Writes go through a transaction that this awaits to completion, so `put`,
//! `delete`, and `apply` resolve only once the browser has committed. `apply`
//! puts every op in ONE transaction, which is what makes a multi-key write
//! atomic: a tab closed mid-batch leaves the previous state, not half of it.

use std::cell::RefCell;
use std::rc::Rc;

use async_trait::async_trait;
use futures_channel::oneshot;
use js_sys::{Array, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::{Closure, JsValue};
use web_sys::{
    Event, IdbDatabase, IdbFactory, IdbObjectStore, IdbRequest, IdbTransaction, IdbTransactionMode,
};

use crate::{Backend, StoreError, WriteOp};

const DATABASE_VERSION: u32 = 1;

/// A cloneable handle to one IndexedDB object store.
#[derive(Clone)]
pub struct IndexedDbBackend {
    database: IdbDatabase,
    store_name: String,
}

impl IndexedDbBackend {
    /// Open or create a database whose one object store contains flat Muniment
    /// byte keys.
    pub async fn open(database_name: &str, store_name: &str) -> Result<Self, StoreError> {
        let factory = indexed_db_factory()?;
        let request = factory
            .open_with_u32(database_name, DATABASE_VERSION)
            .map_err(js_backend_error)?;

        let upgrade_request = request.clone();
        let upgrade_store = store_name.to_string();
        let on_upgrade = Closure::<dyn FnMut(Event)>::new(move |_| {
            let Ok(value) = upgrade_request.result() else {
                return;
            };
            let Ok(database) = value.dyn_into::<IdbDatabase>() else {
                return;
            };
            if !database.object_store_names().contains(&upgrade_store) {
                let _ = database.create_object_store(&upgrade_store);
            }
        });
        request.set_onupgradeneeded(Some(on_upgrade.as_ref().unchecked_ref()));

        let value = await_request(&request).await?;
        request.set_onupgradeneeded(None);
        let database = value
            .dyn_into::<IdbDatabase>()
            .map_err(|value| js_backend_error(value.into()))?;
        Ok(Self {
            database,
            store_name: store_name.to_string(),
        })
    }

    fn transaction(
        &self,
        mode: IdbTransactionMode,
    ) -> Result<(IdbTransaction, IdbObjectStore), StoreError> {
        let transaction = self
            .database
            .transaction_with_str_and_mode(&self.store_name, mode)
            .map_err(js_backend_error)?;
        let store = transaction
            .object_store(&self.store_name)
            .map_err(js_backend_error)?;
        Ok((transaction, store))
    }

    async fn all_keys(&self) -> Result<Vec<String>, StoreError> {
        let (_transaction, store) = self.transaction(IdbTransactionMode::Readonly)?;
        let request = store.get_all_keys().map_err(js_backend_error)?;
        let value = await_request(&request).await?;
        Ok(Array::from(&value)
            .iter()
            .filter_map(|key| key.as_string())
            .collect())
    }
}

fn indexed_db_factory() -> Result<IdbFactory, StoreError> {
    if let Some(window) = web_sys::window() {
        return window
            .indexed_db()
            .map_err(js_backend_error)?
            .ok_or_else(|| backend_error("IndexedDB is unavailable"));
    }

    let global = js_sys::global();
    let worker = global
        .dyn_into::<web_sys::WorkerGlobalScope>()
        .map_err(|_| backend_error("browser window or worker scope is unavailable"))?;
    worker
        .indexed_db()
        .map_err(js_backend_error)?
        .ok_or_else(|| backend_error("IndexedDB is unavailable"))
}

#[async_trait(?Send)]
impl Backend for IndexedDbBackend {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let (_transaction, store) = self.transaction(IdbTransactionMode::Readonly)?;
        let request = store
            .get(&JsValue::from_str(key))
            .map_err(js_backend_error)?;
        let value = await_request(&request).await?;
        if value.is_undefined() {
            return Ok(None);
        }
        let bytes = Uint8Array::new(&value);
        let mut output = vec![0; bytes.length() as usize];
        bytes.copy_to(&mut output);
        Ok(Some(output))
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let (transaction, store) = self.transaction(IdbTransactionMode::Readwrite)?;
        let value = Uint8Array::from(bytes);
        store
            .put_with_key(value.as_ref(), &JsValue::from_str(key))
            .map_err(js_backend_error)?;
        await_transaction(&transaction).await
    }

    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        let (transaction, store) = self.transaction(IdbTransactionMode::Readwrite)?;
        store
            .delete(&JsValue::from_str(key))
            .map_err(js_backend_error)?;
        await_transaction(&transaction).await
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        Ok(self
            .all_keys()
            .await?
            .into_iter()
            .filter(|key| key.starts_with(prefix))
            .collect())
    }

    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        let mut keys = self
            .all_keys()
            .await?
            .into_iter()
            .filter(|key| key.as_str() >= start && key.as_str() < end)
            .collect::<Vec<_>>();
        keys.sort();
        Ok(keys)
    }

    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        if ops.is_empty() {
            return Ok(());
        }
        let (transaction, store) = self.transaction(IdbTransactionMode::Readwrite)?;
        for op in ops {
            match op {
                WriteOp::Put { key, value } => {
                    let bytes = Uint8Array::from(value.as_slice());
                    store
                        .put_with_key(bytes.as_ref(), &JsValue::from_str(key))
                        .map_err(js_backend_error)?;
                }
                WriteOp::Delete { key } => {
                    store
                        .delete(&JsValue::from_str(key))
                        .map_err(js_backend_error)?;
                }
            }
        }
        await_transaction(&transaction).await
    }
}

async fn await_request(request: &IdbRequest) -> Result<JsValue, StoreError> {
    let (sender, receiver) = oneshot::channel::<Result<(), StoreError>>();
    let sender = Rc::new(RefCell::new(Some(sender)));
    let success_sender = sender.clone();
    let success = Closure::<dyn FnMut(Event)>::new(move |_| {
        if let Some(sender) = success_sender.borrow_mut().take() {
            let _ = sender.send(Ok(()));
        }
    });
    let error_sender = sender;
    let error_request = request.clone();
    let error = Closure::<dyn FnMut(Event)>::new(move |_| {
        if let Some(sender) = error_sender.borrow_mut().take() {
            let message = error_request
                .error()
                .ok()
                .flatten()
                .map(|error| error.message())
                .unwrap_or_else(|| "IndexedDB request failed".to_string());
            let _ = sender.send(Err(backend_error(message)));
        }
    });
    request.set_onsuccess(Some(success.as_ref().unchecked_ref()));
    request.set_onerror(Some(error.as_ref().unchecked_ref()));
    let outcome = receiver
        .await
        .map_err(|_| backend_error("IndexedDB request was cancelled"))?;
    request.set_onsuccess(None);
    request.set_onerror(None);
    outcome?;
    request.result().map_err(js_backend_error)
}

async fn await_transaction(transaction: &IdbTransaction) -> Result<(), StoreError> {
    let (sender, receiver) = oneshot::channel::<Result<(), StoreError>>();
    let sender = Rc::new(RefCell::new(Some(sender)));
    let complete_sender = sender.clone();
    let complete = Closure::<dyn FnMut(Event)>::new(move |_| {
        if let Some(sender) = complete_sender.borrow_mut().take() {
            let _ = sender.send(Ok(()));
        }
    });
    let failed_sender = sender.clone();
    let failed_transaction = transaction.clone();
    let failed = Closure::<dyn FnMut(Event)>::new(move |_| {
        if let Some(sender) = failed_sender.borrow_mut().take() {
            let message = failed_transaction
                .error()
                .map(|error| error.message())
                .unwrap_or_else(|| "IndexedDB transaction failed".to_string());
            let _ = sender.send(Err(backend_error(message)));
        }
    });
    let aborted_sender = sender;
    let aborted_transaction = transaction.clone();
    let aborted = Closure::<dyn FnMut(Event)>::new(move |_| {
        if let Some(sender) = aborted_sender.borrow_mut().take() {
            let message = aborted_transaction
                .error()
                .map(|error| error.message())
                .unwrap_or_else(|| "IndexedDB transaction was aborted".to_string());
            let _ = sender.send(Err(backend_error(message)));
        }
    });
    transaction.set_oncomplete(Some(complete.as_ref().unchecked_ref()));
    transaction.set_onerror(Some(failed.as_ref().unchecked_ref()));
    transaction.set_onabort(Some(aborted.as_ref().unchecked_ref()));
    let outcome = receiver
        .await
        .map_err(|_| backend_error("IndexedDB transaction was cancelled"))?;
    transaction.set_oncomplete(None);
    transaction.set_onerror(None);
    transaction.set_onabort(None);
    outcome
}

fn backend_error(message: impl Into<String>) -> StoreError {
    StoreError::Backend(message.into())
}

fn js_backend_error(value: JsValue) -> StoreError {
    backend_error(value.as_string().unwrap_or_else(|| format!("{value:?}")))
}
