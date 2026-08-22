// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MPL-2.0

//! The generation workload shared by the native fault sweep, the browser
//! termination trials, and the portability fixture.
//!
//! Every commit writes a generation counter and that generation's keys in ONE
//! transaction, and deletes the keys two generations back (so pages are freed
//! and reused, which is where torn writes bite). A reopen is then checkable
//! against one invariant, [`verify`]: the counter names generation `g`, every
//! `g` key is present with the right bytes, nothing from `g + 1` is visible,
//! and nothing older than `g - 1` survives. "Either the preceding commit or
//! the completed commit" is exactly `g ∈ {completed, completed + 1}` with the
//! invariant holding.

use redb::{Database, ReadOnlyTable, ReadableDatabase, TableDefinition, TableError};
use serde::{Deserialize, Serialize};

/// Generation counter table.
pub const META: TableDefinition<&str, u64> = TableDefinition::new("probe_meta");
/// Generation payload table.
pub const DATA: TableDefinition<&str, &[u8]> = TableDefinition::new("probe_data");
/// The key in [`META`] that names the latest committed generation.
pub const GENERATION_KEY: &str = "generation";

/// How much each generation writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChurnShape {
    pub keys_per_commit: u32,
    pub value_bytes: usize,
}

impl Default for ChurnShape {
    fn default() -> Self {
        Self {
            keys_per_commit: 16,
            value_bytes: 4096,
        }
    }
}

/// The key of entry `index` in `generation`. Fixed-width so generations sort.
pub fn key(generation: u64, index: u32) -> String {
    format!("g/{generation:08}/{index:04}")
}

/// The prefix every key of `generation` shares.
pub fn prefix(generation: u64) -> String {
    format!("g/{generation:08}/")
}

/// Deterministic bytes for entry `index` of `generation`.
pub fn value(generation: u64, index: u32, len: usize) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&generation.to_le_bytes());
    hasher.update(&index.to_le_bytes());
    let mut out = vec![0u8; len];
    hasher.finalize_xof().fill(&mut out);
    out
}

/// Materialize both tables so a read on a fresh database finds them.
pub fn materialize(db: &Database) -> Result<(), redb::Error> {
    let txn = db.begin_write()?;
    txn.open_table(META)?;
    txn.open_table(DATA)?;
    txn.commit()?;
    Ok(())
}

/// The latest committed generation, 0 on a fresh or table-less database.
pub fn current_generation(db: &Database) -> Result<u64, redb::Error> {
    let read = db.begin_read()?;
    let table = match read.open_table(META) {
        Ok(table) => table,
        Err(TableError::TableDoesNotExist(_)) => return Ok(0),
        Err(err) => return Err(err.into()),
    };
    Ok(table.get(GENERATION_KEY)?.map(|g| g.value()).unwrap_or(0))
}

/// Commit one generation: counter plus keys in one transaction, the keys two
/// generations back removed in the same transaction.
pub fn commit_generation(
    db: &Database,
    generation: u64,
    shape: ChurnShape,
    two_phase_commit: bool,
) -> Result<(), redb::Error> {
    let mut txn = db.begin_write()?;
    txn.set_two_phase_commit(two_phase_commit);
    {
        let mut meta = txn.open_table(META)?;
        meta.insert(GENERATION_KEY, generation)?;
        let mut data = txn.open_table(DATA)?;
        for index in 0..shape.keys_per_commit {
            data.insert(
                key(generation, index).as_str(),
                value(generation, index, shape.value_bytes).as_slice(),
            )?;
        }
        if generation >= 2 {
            for index in 0..shape.keys_per_commit {
                data.remove(key(generation - 2, index).as_str())?;
            }
        }
    }
    txn.commit()?;
    Ok(())
}

/// What [`verify`] found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationCheck {
    pub generation: u64,
    pub keys_present: u32,
    pub keys_expected: u32,
    pub values_ok: bool,
    /// Keys of the previous generation still present (informational).
    pub previous_keys_present: u32,
    /// Keys of `generation + 1` visible: must be 0 (atomicity).
    pub next_generation_keys: u32,
    /// Keys older than `generation - 1` surviving: must be 0.
    pub stale_keys: u32,
    pub ok: bool,
}

fn count_range(
    table: &ReadOnlyTable<&'static str, &'static [u8]>,
    lo: &str,
    hi: &str,
) -> Result<u32, redb::Error> {
    let mut n = 0u32;
    for entry in table.range(lo..hi)? {
        entry?;
        n += 1;
    }
    Ok(n)
}

/// Check the generation invariant on an open database.
pub fn verify(db: &Database, shape: ChurnShape) -> Result<GenerationCheck, redb::Error> {
    let generation = current_generation(db)?;
    let keys_expected = if generation == 0 {
        0
    } else {
        shape.keys_per_commit
    };
    let read = db.begin_read()?;
    let data = match read.open_table(DATA) {
        Ok(table) => table,
        Err(TableError::TableDoesNotExist(_)) => {
            return Ok(GenerationCheck {
                generation,
                keys_present: 0,
                keys_expected,
                values_ok: generation == 0,
                previous_keys_present: 0,
                next_generation_keys: 0,
                stale_keys: 0,
                ok: generation == 0,
            });
        }
        Err(err) => return Err(err.into()),
    };
    let mut keys_present = 0;
    let mut values_ok = true;
    if generation > 0 {
        for index in 0..shape.keys_per_commit {
            if let Some(found) = data.get(key(generation, index).as_str())? {
                keys_present += 1;
                if found.value() != value(generation, index, shape.value_bytes).as_slice() {
                    values_ok = false;
                }
            }
        }
    }
    let previous_keys_present = if generation >= 2 {
        count_range(&data, &prefix(generation - 1), &prefix(generation))?
    } else {
        0
    };
    let next_generation_keys =
        count_range(&data, &prefix(generation + 1), &prefix(generation + 2))?;
    let stale_keys = if generation >= 2 {
        count_range(&data, "g/", &prefix(generation - 1))?
    } else {
        0
    };
    let ok =
        keys_present == keys_expected && values_ok && next_generation_keys == 0 && stale_keys == 0;
    Ok(GenerationCheck {
        generation,
        keys_present,
        keys_expected,
        values_ok,
        previous_keys_present,
        next_generation_keys,
        stale_keys,
        ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::backends::InMemoryBackend;

    fn db() -> Database {
        Database::builder()
            .create_with_backend(InMemoryBackend::new())
            .unwrap()
    }

    #[test]
    fn fresh_database_is_generation_zero_and_ok() {
        let db = db();
        assert_eq!(current_generation(&db).unwrap(), 0);
        let check = verify(&db, ChurnShape::default()).unwrap();
        assert!(check.ok, "{check:?}");
    }

    #[test]
    fn each_generation_verifies_and_drops_two_back() {
        let db = db();
        materialize(&db).unwrap();
        let shape = ChurnShape {
            keys_per_commit: 4,
            value_bytes: 100,
        };
        for generation in 1..=5 {
            commit_generation(&db, generation, shape, false).unwrap();
            let check = verify(&db, shape).unwrap();
            assert!(check.ok, "generation {generation}: {check:?}");
            assert_eq!(check.generation, generation);
            assert_eq!(check.keys_present, 4);
            assert_eq!(
                check.previous_keys_present,
                if generation >= 2 { 4 } else { 0 }
            );
        }
    }

    #[test]
    fn values_are_deterministic_and_distinct() {
        assert_eq!(value(3, 1, 64), value(3, 1, 64));
        assert_ne!(value(3, 1, 64), value(3, 2, 64));
        assert_ne!(value(3, 1, 64), value(4, 1, 64));
        assert_eq!(key(12, 7), "g/00000012/0007");
    }
}
