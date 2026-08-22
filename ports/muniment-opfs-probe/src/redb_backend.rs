// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MPL-2.0

//! A muniment [`Backend`] over a redb 4.2 [`Database`], whatever storage the
//! database was opened on. Production muniment's `RedbBackend` is the same
//! shape on redb 2; this copy exists only because the probe pins 4.2 and the
//! two majors must not meet in one graph. Nothing here is browser-specific:
//! the OPFS authority stays below `Database`, inside the storage backend.

use std::sync::Arc;

use async_trait::async_trait;
use muniment::{Backend, StoreError, WriteOp};
use redb::{Database, ReadableDatabase, TableDefinition};

/// The single key/value table: opaque string keys, raw byte values.
const TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("muniment");

fn backend(err: impl std::fmt::Display) -> StoreError {
    StoreError::Backend(err.to_string())
}

/// Cheap to clone; the database is shared behind an `Arc`.
#[derive(Clone)]
pub struct RedbBackend {
    db: Arc<Database>,
}

impl RedbBackend {
    /// Wrap an open database, materializing the table so a read on a fresh
    /// database sees an empty table rather than "table does not exist".
    pub fn from_database(db: Database) -> Result<Self, StoreError> {
        let txn = db.begin_write().map_err(backend)?;
        txn.open_table(TABLE).map_err(backend)?;
        txn.commit().map_err(backend)?;
        Ok(Self { db: Arc::new(db) })
    }

    /// The database, for callers that need redb directly. `check_integrity`
    /// needs `&mut Database`, so run it before wrapping.
    pub fn database(&self) -> &Database {
        &self.db
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Backend for RedbBackend {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let read = self.db.begin_read().map_err(backend)?;
        let table = read.open_table(TABLE).map_err(backend)?;
        match table.get(key).map_err(backend)? {
            Some(value) => Ok(Some(value.value().to_vec())),
            None => Ok(None),
        }
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let txn = self.db.begin_write().map_err(backend)?;
        {
            let mut table = txn.open_table(TABLE).map_err(backend)?;
            table.insert(key, bytes).map_err(backend)?;
        }
        txn.commit().map_err(backend)?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        let txn = self.db.begin_write().map_err(backend)?;
        {
            let mut table = txn.open_table(TABLE).map_err(backend)?;
            table.remove(key).map_err(backend)?;
        }
        txn.commit().map_err(backend)?;
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let read = self.db.begin_read().map_err(backend)?;
        let table = read.open_table(TABLE).map_err(backend)?;
        let mut keys = Vec::new();
        for entry in table.range(prefix..).map_err(backend)? {
            let (key, _value) = entry.map_err(backend)?;
            let key = key.value();
            if !key.starts_with(prefix) {
                break;
            }
            keys.push(key.to_string());
        }
        Ok(keys)
    }

    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        let read = self.db.begin_read().map_err(backend)?;
        let table = read.open_table(TABLE).map_err(backend)?;
        let mut keys = Vec::new();
        for entry in table.range(start..end).map_err(backend)? {
            let (key, _value) = entry.map_err(backend)?;
            keys.push(key.value().to_string());
        }
        Ok(keys)
    }

    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        let txn = self.db.begin_write().map_err(backend)?;
        {
            let mut table = txn.open_table(TABLE).map_err(backend)?;
            for op in ops {
                match op {
                    WriteOp::Put { key, value } => {
                        table
                            .insert(key.as_str(), value.as_slice())
                            .map_err(backend)?;
                    }
                    WriteOp::Delete { key } => {
                        table.remove(key.as_str()).map_err(backend)?;
                    }
                }
            }
        }
        txn.commit().map_err(backend)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::backends::InMemoryBackend;

    fn memory() -> RedbBackend {
        let db = Database::builder()
            .create_with_backend(InMemoryBackend::new())
            .unwrap();
        RedbBackend::from_database(db).unwrap()
    }

    #[test]
    fn put_get_delete_list_scan_apply() {
        pollster::block_on(async {
            let b = memory();
            assert_eq!(b.get("k").await.unwrap(), None);
            b.put("k", b"v").await.unwrap();
            assert_eq!(b.get("k").await.unwrap(), Some(b"v".to_vec()));
            for (k, v) in [
                ("log/a/0/0000000000000002", "two"),
                ("log/a/0/0000000000000000", "zero"),
                ("log/a/0/0000000000000001", "one"),
                ("log/b/0/0000000000000000", "other"),
            ] {
                b.put(k, v.as_bytes()).await.unwrap();
            }
            assert_eq!(
                b.list("log/b/").await.unwrap(),
                vec!["log/b/0/0000000000000000"]
            );
            assert_eq!(
                b.scan("log/a/0/0000000000000000", "log/a/0/0000000000000003")
                    .await
                    .unwrap(),
                vec![
                    "log/a/0/0000000000000000",
                    "log/a/0/0000000000000001",
                    "log/a/0/0000000000000002"
                ]
            );
            b.apply(&[
                WriteOp::Put {
                    key: "op/h".into(),
                    value: b"header".to_vec(),
                },
                WriteOp::Delete { key: "k".into() },
            ])
            .await
            .unwrap();
            assert_eq!(b.get("op/h").await.unwrap(), Some(b"header".to_vec()));
            assert_eq!(b.get("k").await.unwrap(), None);
            b.delete("absent").await.unwrap();
        });
    }
}
