// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MPL-2.0

//! Representative muniment workloads, run through the [`Backend`] trait so
//! redb-on-OPFS, IndexedDB, and memory are measured on the same code: small
//! mutable slots, an ordered log, atomic batches, and large blobs. Each
//! workload ends by digesting every key and value in the store, so two
//! backends that report the same digest stored the same bytes.

use muniment::{Backend, StoreError, WriteOp};
use serde::{Deserialize, Serialize};

/// Which backend a benchmark row ran on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchBackend {
    /// redb 4.2 over the probe's OPFS storage backend (browser) or a native
    /// file (fixture).
    RedbOpfs,
    /// muniment's production `IndexedDbBackend` (browser only). Its `scan`
    /// and `list` fetch every key and filter in Rust, so this row measures
    /// the shipping adapter, not IndexedDB's range performance.
    IndexedDb,
    /// The same store with `scan`/`list` on `IDBKeyRange` + `getAllKeys`, so
    /// IndexedDB does the selection. The fair read baseline.
    IndexedDbRange,
    /// muniment's `MemoryBackend`, the floor.
    Memory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Workload {
    /// 200 slots of 256 bytes: put, get, overwrite. The session/settings shape.
    SmallSlots,
    /// 1000 fixed-width log keys inserted out of order **one `put` at a time**,
    /// then the full ordered scan and 50 window scans. Deliberately unbatched:
    /// it is the worst case for a store that commits durably per call, and it
    /// is NOT how the shipping log writes. Compare against [`Self::LogBatched`].
    OrderedLog,
    /// The **shipping** log shape: 1000 operations, each one `apply` of the
    /// 2–3 keys `stickleback::MunimentStore::insert_operation` writes together
    /// (log entry, id pointer, payload reference), then the same scans as
    /// [`Self::OrderedLog`]. This is the row to read for a real write cost.
    LogBatched,
    /// 100 `apply` batches of 8 ops (header + payload + index + a delete).
    AtomicBatches,
    /// 8 blobs of 1 MiB and 2 of 8 MiB, content-addressed, put then read back
    /// and verified.
    LargeBlobs,
}

impl Workload {
    pub const ALL: [Workload; 5] = [
        Workload::SmallSlots,
        Workload::OrderedLog,
        Workload::LogBatched,
        Workload::AtomicBatches,
        Workload::LargeBlobs,
    ];
}

/// One timed phase of a workload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Phase {
    pub name: String,
    pub ops: u64,
    pub bytes: u64,
    pub ms: f64,
}

/// What a workload produced.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkloadOutcome {
    pub ops: u64,
    pub bytes: u64,
    /// blake3 over every (key, value) in the store, sorted by key.
    pub digest: String,
    pub keys: u64,
    /// Every read-back check passed.
    pub checks_ok: bool,
    pub note: Option<String>,
}

/// Deterministic bytes for a seed.
pub fn fill(seed: &str, len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    blake3::Hasher::new()
        .update(seed.as_bytes())
        .finalize_xof()
        .fill(&mut out);
    out
}

/// A monotonic millisecond clock supplied by the host.
pub type Clock<'a> = &'a dyn Fn() -> f64;

struct Timer<'a> {
    clock: Clock<'a>,
    phases: Vec<Phase>,
    started: f64,
    ops: u64,
    bytes: u64,
}

impl<'a> Timer<'a> {
    fn new(clock: Clock<'a>) -> Self {
        Self {
            clock,
            phases: Vec::new(),
            started: clock(),
            ops: 0,
            bytes: 0,
        }
    }

    fn op(&mut self, bytes: usize) {
        self.ops += 1;
        self.bytes += bytes as u64;
    }

    fn phase(&mut self, name: &str) {
        let now = (self.clock)();
        self.phases.push(Phase {
            name: name.to_string(),
            ops: self.ops,
            bytes: self.bytes,
            ms: now - self.started,
        });
        self.started = now;
        self.ops = 0;
        self.bytes = 0;
    }
}

/// blake3 over every (key, value) in the store, sorted by key, plus the key
/// count. The cross-backend (and cross-host) content oracle.
pub async fn digest<B: Backend>(backend: &B) -> Result<(String, u64), StoreError> {
    let mut keys = backend.list("").await?;
    keys.sort();
    let mut hasher = blake3::Hasher::new();
    for key in &keys {
        let value = backend.get(key).await?.unwrap_or_default();
        hasher.update(&(key.len() as u64).to_le_bytes());
        hasher.update(key.as_bytes());
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(&value);
    }
    Ok((hasher.finalize().to_hex().to_string(), keys.len() as u64))
}

/// Run `workload` against `backend`, timing each phase with `clock`.
pub async fn run<B: Backend>(
    backend: &B,
    workload: Workload,
    clock: Clock<'_>,
) -> Result<(WorkloadOutcome, Vec<Phase>), StoreError> {
    let mut timer = Timer::new(clock);
    let mut checks_ok = true;
    let mut note = None;
    match workload {
        Workload::SmallSlots => {
            for i in 0..200 {
                let value = fill(&format!("slot/{i}/v1"), 256);
                backend.put(&format!("slot/{i:04}"), &value).await?;
                timer.op(256);
            }
            timer.phase("put");
            for i in 0..200 {
                let got = backend.get(&format!("slot/{i:04}")).await?;
                checks_ok &= got.as_deref() == Some(fill(&format!("slot/{i}/v1"), 256).as_slice());
                timer.op(256);
            }
            timer.phase("get");
            for i in 0..200 {
                let value = fill(&format!("slot/{i}/v2"), 256);
                backend.put(&format!("slot/{i:04}"), &value).await?;
                timer.op(256);
            }
            timer.phase("overwrite");
        }
        Workload::OrderedLog => {
            // A fixed permutation: stride through the sequence so insertion
            // order is far from key order.
            let n = 1000u64;
            for j in 0..n {
                let seq = (j * 617) % n;
                let value = fill(&format!("log/{seq}"), 128);
                backend.put(&format!("log/a/0/{seq:016}"), &value).await?;
                timer.op(128);
            }
            timer.phase("put_shuffled");
            let all = backend
                .scan("log/a/0/0000000000000000", &format!("log/a/0/{n:016}"))
                .await?;
            checks_ok &= all.len() == n as usize && all.windows(2).all(|w| w[0] < w[1]);
            timer.op(all.len() * 32);
            timer.phase("scan_full");
            for w in 0..50u64 {
                let lo = w * 20;
                let keys = backend
                    .scan(
                        &format!("log/a/0/{lo:016}"),
                        &format!("log/a/0/{:016}", lo + 20),
                    )
                    .await?;
                checks_ok &= keys.len() == 20;
                timer.op(keys.len() * 32);
            }
            timer.phase("scan_windows");
        }
        Workload::LogBatched => {
            // stickleback::MunimentStore::insert_operation writes the log
            // entry, the id → log-key pointer, and (when the header carries a
            // payload hash) a payload reference, in ONE `apply` so a reader
            // never sees one without the others. Same 1000 operations and the
            // same scans as OrderedLog, so the two rows differ only in
            // batching.
            let n = 1000u64;
            for j in 0..n {
                let seq = (j * 617) % n;
                let entry = fill(&format!("log/{seq}"), 128);
                let log_key = format!("log/a/0/{seq:016}");
                let mut ops = vec![
                    WriteOp::Put {
                        key: log_key.clone(),
                        value: entry,
                    },
                    WriteOp::Put {
                        key: format!("op/{seq:016}"),
                        value: log_key.clone().into_bytes(),
                    },
                ];
                // Two operations in three carry a payload, as a mixed log does.
                if seq % 3 != 0 {
                    ops.push(WriteOp::Put {
                        key: format!("payload/{seq:016}"),
                        value: log_key.into_bytes(),
                    });
                }
                backend.apply(&ops).await?;
                timer.op(128 + 32 + 32);
            }
            timer.phase("apply_shuffled");
            let all = backend
                .scan("log/a/0/0000000000000000", &format!("log/a/0/{n:016}"))
                .await?;
            checks_ok &= all.len() == n as usize && all.windows(2).all(|w| w[0] < w[1]);
            timer.op(all.len() * 32);
            timer.phase("scan_full");
            for w in 0..50u64 {
                let lo = w * 20;
                let keys = backend
                    .scan(
                        &format!("log/a/0/{lo:016}"),
                        &format!("log/a/0/{:016}", lo + 20),
                    )
                    .await?;
                checks_ok &= keys.len() == 20;
                timer.op(keys.len() * 32);
            }
            timer.phase("scan_windows");
            note = Some(
                "one `apply` per operation, matching stickleback's insert_operation; compare with ordered_log's one `put` per key"
                    .into(),
            );
        }
        Workload::AtomicBatches => {
            for b in 0..100 {
                let payload = fill(&format!("batch/{b}/payload"), 2048);
                let mut ops = vec![
                    WriteOp::Put {
                        key: format!("op/{b:04}"),
                        value: fill(&format!("batch/{b}/header"), 96),
                    },
                    WriteOp::Put {
                        key: format!("op/{b:04}/payload"),
                        value: payload,
                    },
                    WriteOp::Put {
                        key: format!("idx/{b:04}"),
                        value: fill(&format!("batch/{b}/index"), 32),
                    },
                ];
                for k in 0..4 {
                    ops.push(WriteOp::Put {
                        key: format!("tmp/{b:04}/{k}"),
                        value: fill(&format!("batch/{b}/tmp/{k}"), 64),
                    });
                }
                if b > 0 {
                    ops.push(WriteOp::Delete {
                        key: format!("tmp/{:04}/0", b - 1),
                    });
                }
                backend.apply(&ops).await?;
                timer.op(2048 + 96 + 32 + 4 * 64);
            }
            timer.phase("apply");
            for b in 0..100 {
                let header = backend.get(&format!("op/{b:04}")).await?;
                let payload = backend.get(&format!("op/{b:04}/payload")).await?;
                checks_ok &= header.is_some() && payload.is_some();
                timer.op(2048 + 96);
            }
            timer.phase("verify");
        }
        Workload::LargeBlobs => {
            let sizes: Vec<(usize, usize)> = (0..8)
                .map(|i| (i, 1 << 20))
                .chain((8..10).map(|i| (i, 8 << 20)))
                .collect();
            let mut hashes = Vec::new();
            for (i, size) in &sizes {
                let bytes = fill(&format!("blob/{i}"), *size);
                let hash = blake3::hash(&bytes).to_hex().to_string();
                backend.put(&format!("blob/{hash}"), &bytes).await?;
                hashes.push((hash, *size));
                timer.op(*size);
            }
            timer.phase("put");
            for (hash, size) in &hashes {
                let got = backend.get(&format!("blob/{hash}")).await?;
                checks_ok &= got
                    .as_ref()
                    .map(|b| b.len() == *size && blake3::hash(b).to_hex().as_str() == hash)
                    .unwrap_or(false);
                timer.op(*size);
            }
            timer.phase("get_verify");
            note = Some(
                "two 8 MiB values sit far under redb's 3 GiB value ceiling; IndexedDB stores each as one Uint8Array"
                    .into(),
            );
        }
    }
    let total_ops = timer.phases.iter().map(|p| p.ops).sum();
    let total_bytes = timer.phases.iter().map(|p| p.bytes).sum();
    let (digest, keys) = digest(backend).await?;
    timer.phase("digest");
    Ok((
        WorkloadOutcome {
            ops: total_ops,
            bytes: total_bytes,
            digest,
            keys,
            checks_ok,
            note,
        },
        timer.phases,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redb_backend::RedbBackend;
    use muniment::MemoryBackend;
    use redb::Database;
    use redb::backends::InMemoryBackend;
    use std::time::Instant;

    fn redb_memory() -> RedbBackend {
        RedbBackend::from_database(
            Database::builder()
                .create_with_backend(InMemoryBackend::new())
                .unwrap(),
        )
        .unwrap()
    }

    /// The same workload on muniment's memory floor and on redb-in-memory
    /// must store the same bytes: the digest is the cross-backend oracle.
    #[test]
    fn every_workload_digests_identically_across_backends() {
        let started = Instant::now();
        let clock = move || started.elapsed().as_secs_f64() * 1000.0;
        for workload in Workload::ALL {
            let (memory, _) =
                pollster::block_on(run(&MemoryBackend::new(), workload, &clock)).unwrap();
            let (redb, phases) = pollster::block_on(run(&redb_memory(), workload, &clock)).unwrap();
            assert!(memory.checks_ok, "{workload:?} memory checks");
            assert!(redb.checks_ok, "{workload:?} redb checks");
            assert_eq!(memory.digest, redb.digest, "{workload:?} digests differ");
            assert_eq!(memory.keys, redb.keys);
            assert!(!phases.is_empty());
            eprintln!(
                "{workload:?}: {} ops, {} bytes, {} keys, phases {:?}",
                redb.ops,
                redb.bytes,
                redb.keys,
                phases
                    .iter()
                    .map(|p| (p.name.as_str(), p.ms as u64))
                    .collect::<Vec<_>>()
            );
        }
    }
}
