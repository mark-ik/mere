// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MPL-2.0

//! A muniment [`Backend`] on IndexedDB whose `scan` and `list` use a **real
//! indexed range query** (`IDBKeyRange` + `getAllKeys`) instead of fetching
//! every key and filtering in Rust.
//!
//! Why this exists: production `muniment::IndexedDbBackend` implements `scan`
//! and `list` by calling `getAllKeys()` with no query and filtering the whole
//! key set. That is a documented, deliberate choice ("correct for stores of
//! the size a browser tab holds"), but it means a benchmark against it
//! measures **that adapter**, not IndexedDB's indexed-range performance. A
//! read comparison against redb is only fair against a backend that asks
//! IndexedDB for the range it wants.
//!
//! This backend is otherwise identical to the production one: same database
//! shape, same single object store, same transaction discipline. Only `scan`
//! and `list` differ. It lives in the probe, not in muniment — the point is a
//! fair baseline, not a production change.

use std::cell::RefCell;
use std::rc::Rc;

use async_trait::async_trait;
use futures_channel::oneshot;
use js_sys::{Array, Uint8Array};
use muniment::{Backend, StoreError, WriteOp};

use crate::idb_keys::{ascii_prefix_upper_bound, require_ascii};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::{Closure, JsValue};
use web_sys::{
    Event, IdbDatabase, IdbFactory, IdbKeyRange, IdbObjectStore, IdbRequest, IdbTransaction,
    IdbTransactionMode,
};

const DATABASE_VERSION: u32 = 1;

/// A cloneable handle to one IndexedDB object store, with range-query reads.
#[derive(Clone)]
pub struct IndexedDbRangeBackend {
    database: IdbDatabase,
    store_name: String,
}

impl IndexedDbRangeBackend {
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

    /// Keys in `range`, straight from the index. The whole point of this
    /// backend: IndexedDB does the selection, not Rust.
    async fn keys_in(&self, range: &IdbKeyRange) -> Result<Vec<String>, StoreError> {
        let (_transaction, store) = self.transaction(IdbTransactionMode::Readonly)?;
        let request = store
            .get_all_keys_with_key(range.as_ref())
            .map_err(js_backend_error)?;
        let value = await_request(&request).await?;
        Ok(Array::from(&value)
            .iter()
            .filter_map(|key| key.as_string())
            .collect())
    }
}

/// The half-open range `[start, end)` IndexedDB understands.
fn bounded(start: &str, end: &str) -> Result<IdbKeyRange, StoreError> {
    IdbKeyRange::bound_with_lower_open_and_upper_open(
        &JsValue::from_str(start),
        &JsValue::from_str(end),
        false,
        true,
    )
    .map_err(js_backend_error)
}

/// Every key carrying `prefix`, as a range: `[prefix, successor(prefix))`.
///
/// The upper bound is the prefix's immediate ASCII successor, not a sentinel
/// character appended to it. An earlier version appended `U+10FFFF` on the
/// theory that nothing sorts above it — false under IndexedDB's UTF-16
/// ordering, where a supplementary character is a surrogate pair starting
/// below `U+FFFF`, so `prefix + U+FFFF` sorted *above* the bound and was
/// silently dropped. See [`crate::idb_keys`] for the ordering divergence this
/// backend's ASCII contract exists to avoid.
fn prefixed(prefix: &str) -> Result<IdbKeyRange, StoreError> {
    match ascii_prefix_upper_bound(prefix) {
        Some(upper) => IdbKeyRange::bound_with_lower_open_and_upper_open(
            &JsValue::from_str(prefix),
            &JsValue::from_str(&upper),
            false,
            true,
        )
        .map_err(js_backend_error),
        // Nothing sorts above the prefix, so an open-ended lower bound is the
        // whole matching set.
        None => IdbKeyRange::lower_bound(&JsValue::from_str(prefix)).map_err(js_backend_error),
    }
}

/// The ASCII contract, enforced on **every key-bearing operation**.
///
/// Checking only `scan`/`list` was not enough: a non-ASCII key admitted
/// through `put` or `apply` would sit in the store and then surface through
/// `list("")` — which has no bounds to validate and returns whatever
/// IndexedDB holds, in **IndexedDB's** UTF-16 order. The contract has to be
/// enforced at the door, not at the range query, or the store can reach a
/// state the range query cannot describe correctly.
fn contract(what: &'static str, key: &str) -> Result<(), StoreError> {
    require_ascii(what, key).map_err(|e| StoreError::Backend(e.to_string()))
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
impl Backend for IndexedDbRangeBackend {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        contract("get key", key)?;
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
        contract("put key", key)?;
        let (transaction, store) = self.transaction(IdbTransactionMode::Readwrite)?;
        let value = Uint8Array::from(bytes);
        store
            .put_with_key(value.as_ref(), &JsValue::from_str(key))
            .map_err(js_backend_error)?;
        await_transaction(&transaction).await
    }

    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        contract("delete key", key)?;
        let (transaction, store) = self.transaction(IdbTransactionMode::Readwrite)?;
        store
            .delete(&JsValue::from_str(key))
            .map_err(js_backend_error)?;
        await_transaction(&transaction).await
    }

    /// Indexed prefix range, not a filtered full listing.
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        contract("list prefix", prefix)?;
        if prefix.is_empty() {
            // An empty prefix is "everything"; getAllKeys with no query is
            // the right call for that and needs no range.
            let (_transaction, store) = self.transaction(IdbTransactionMode::Readonly)?;
            let request = store.get_all_keys().map_err(js_backend_error)?;
            let value = await_request(&request).await?;
            return Ok(Array::from(&value)
                .iter()
                .filter_map(|key| key.as_string())
                .collect());
        }
        self.keys_in(&prefixed(prefix)?).await
    }

    /// Indexed half-open range. IndexedDB returns keys in ascending order —
    /// its own, which matches Rust's only for ASCII keys, hence the contract.
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        contract("scan start", start)?;
        contract("scan end", end)?;
        if start >= end {
            return Ok(Vec::new());
        }
        self.keys_in(&bounded(start, end)?).await
    }

    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        if ops.is_empty() {
            return Ok(());
        }
        // Prevalidate the WHOLE batch before opening the transaction. Checking
        // per-op inside the loop would let the ops before a bad key land, and
        // `apply`'s contract is all-or-nothing.
        for op in ops {
            match op {
                WriteOp::Put { key, .. } => contract("apply put key", key)?,
                WriteOp::Delete { key } => contract("apply delete key", key)?,
            }
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
