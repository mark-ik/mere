// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Fault injection around a redb [`StorageBackend`]: call-indexed errors,
//! short writes, a quota ceiling, and a cut (the process dies inside write
//! `k` or before resize `k`, so nothing after it reaches the file). Generic
//! over the inner backend, so the same plans run over [`SharedMemory`]
//! natively (the sweep in the tests below) and over the real OPFS handle in
//! the browser.
//!
//! The cut models worker termination faithfully: a terminated worker is a
//! process kill, not a power loss. Every write before the kill is in the
//! browser's file; the fatal write may be torn; nothing after it exists.
//! Power loss (what `flush` buys against the OS) is the browser's promise
//! and is out of this probe's reach from inside a worker.

use std::io::{self, ErrorKind};
use std::sync::{Arc, Mutex};

use redb::StorageBackend;
use serde::{Deserialize, Serialize};

/// Which calls fail, and how. Call indices are 1-based per call kind.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FaultPlan {
    /// The write that fails outright, no bytes applied.
    pub fail_write_at: Option<u64>,
    /// The write that applies only its first half, then reports a short write.
    pub short_write_at: Option<u64>,
    /// The `sync_data` that fails.
    pub fail_sync_at: Option<u64>,
    /// The `set_len` that fails, no resize applied.
    pub fail_set_len_at: Option<u64>,
    /// A `set_len` above this, or a write ending past it, reports
    /// `QuotaExceeded` with nothing applied.
    pub quota_bytes: Option<u64>,
    /// The write inside which the process dies: `torn_bytes` of it land, then
    /// every later call fails with `BrokenPipe` (except `close`).
    pub cut_at_write: Option<u64>,
    /// With `cut_at_write`: how much of the fatal write lands.
    pub torn_bytes: usize,
    /// The `set_len` before which the process dies: the resize never lands.
    pub cut_at_set_len: Option<u64>,
}

/// What the wrapper observed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaultCounters {
    pub reads: u64,
    pub writes: u64,
    pub syncs: u64,
    pub set_lens: u64,
    pub bytes_written: u64,
    /// Human-readable record of each injected fault, in order.
    pub injected: Vec<String>,
    /// Whether a cut fired.
    pub dead: bool,
}

#[derive(Debug, Default)]
struct FaultState {
    counters: FaultCounters,
}

/// A handle onto the wrapper's counters that outlives handing the wrapper to
/// redb.
#[derive(Clone, Debug)]
pub struct FaultWatch(Arc<Mutex<FaultState>>);

impl FaultWatch {
    pub fn counters(&self) -> FaultCounters {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .counters
            .clone()
    }
}

/// The fault-injecting wrapper.
#[derive(Debug)]
pub struct FaultBackend<B> {
    inner: B,
    plan: FaultPlan,
    state: Arc<Mutex<FaultState>>,
}

impl<B: StorageBackend> FaultBackend<B> {
    pub fn new(inner: B, plan: FaultPlan) -> Self {
        Self {
            inner,
            plan,
            state: Arc::new(Mutex::new(FaultState::default())),
        }
    }

    pub fn watch(&self) -> FaultWatch {
        FaultWatch(self.state.clone())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FaultState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn dead() -> io::Error {
        io::Error::new(ErrorKind::BrokenPipe, "injected cut: the process is dead")
    }
}

impl<B: StorageBackend> StorageBackend for FaultBackend<B> {
    fn len(&self) -> io::Result<u64> {
        if self.lock().counters.dead {
            return Err(Self::dead());
        }
        self.inner.len()
    }

    fn read(&self, offset: u64, out: &mut [u8]) -> io::Result<()> {
        {
            let mut state = self.lock();
            if state.counters.dead {
                return Err(Self::dead());
            }
            state.counters.reads += 1;
        }
        self.inner.read(offset, out)
    }

    fn set_len(&self, len: u64) -> io::Result<()> {
        {
            let mut state = self.lock();
            if state.counters.dead {
                return Err(Self::dead());
            }
            state.counters.set_lens += 1;
            let n = state.counters.set_lens;
            if self.plan.cut_at_set_len == Some(n) {
                state.counters.dead = true;
                state
                    .counters
                    .injected
                    .push(format!("cut before set_len #{n} ({len})"));
                return Err(io::Error::new(
                    ErrorKind::BrokenPipe,
                    format!("injected cut before set_len #{n}"),
                ));
            }
            if self.plan.fail_set_len_at == Some(n) {
                state
                    .counters
                    .injected
                    .push(format!("set_len #{n} ({len}) failed"));
                return Err(io::Error::other(format!("injected set_len #{n} failure")));
            }
            if let Some(quota) = self.plan.quota_bytes
                && len > quota
            {
                state
                    .counters
                    .injected
                    .push(format!("set_len #{n} ({len}) exceeded quota {quota}"));
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    format!("injected quota: set_len {len} > {quota}"),
                ));
            }
        }
        self.inner.set_len(len)
    }

    fn sync_data(&self) -> io::Result<()> {
        {
            let mut state = self.lock();
            if state.counters.dead {
                return Err(Self::dead());
            }
            state.counters.syncs += 1;
            let n = state.counters.syncs;
            if self.plan.fail_sync_at == Some(n) {
                state.counters.injected.push(format!("sync #{n} failed"));
                return Err(io::Error::other(format!("injected sync #{n} failure")));
            }
        }
        self.inner.sync_data()
    }

    fn write(&self, offset: u64, data: &[u8]) -> io::Result<()> {
        let len = data.len();
        let n = {
            let mut state = self.lock();
            if state.counters.dead {
                return Err(Self::dead());
            }
            state.counters.writes += 1;
            let n = state.counters.writes;
            if let Some(quota) = self.plan.quota_bytes
                && offset + len as u64 > quota
            {
                state.counters.injected.push(format!(
                    "write #{n} ({offset}+{len}) exceeded quota {quota}"
                ));
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    format!("injected quota: write {offset}+{len} > {quota}"),
                ));
            }
            if self.plan.fail_write_at == Some(n) {
                state
                    .counters
                    .injected
                    .push(format!("write #{n} ({offset}+{len}) failed"));
                return Err(io::Error::other(format!("injected write #{n} failure")));
            }
            n
        };
        if self.plan.cut_at_write == Some(n) {
            let keep = self.plan.torn_bytes.min(len);
            if keep > 0 {
                self.inner.write(offset, &data[..keep])?;
            }
            let mut state = self.lock();
            state.counters.dead = true;
            state.counters.injected.push(format!(
                "cut inside write #{n} ({offset}+{len}), {keep} bytes landed"
            ));
            return Err(io::Error::new(
                ErrorKind::BrokenPipe,
                format!("injected cut inside write #{n}"),
            ));
        }
        if self.plan.short_write_at == Some(n) {
            let half = len / 2;
            self.inner.write(offset, &data[..half])?;
            let mut state = self.lock();
            state.counters.bytes_written += half as u64;
            state
                .counters
                .injected
                .push(format!("write #{n} ({offset}+{len}) short: {half} landed"));
            return Err(io::Error::new(
                ErrorKind::WriteZero,
                format!("injected short write #{n}: {half} of {len}"),
            ));
        }
        self.inner.write(offset, data)?;
        self.lock().counters.bytes_written += len as u64;
        Ok(())
    }

    /// Always forwarded: redb calls it exactly once, and a dead backend still
    /// releases its handle.
    fn close(&self) -> io::Result<()> {
        self.inner.close()
    }
}

/// A byte image shared between opens, so a database "killed" under a fault
/// plan can be reopened over the same bytes natively. Extends on write past
/// the end, as a file (and an OPFS sync-access handle) does.
#[derive(Clone, Debug, Default)]
pub struct SharedMemory(Arc<Mutex<Vec<u8>>>);

impl SharedMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len_bytes(&self) -> usize {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).len()
    }

    pub fn snapshot(&self) -> Vec<u8> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

impl StorageBackend for SharedMemory {
    fn len(&self) -> io::Result<u64> {
        Ok(self.len_bytes() as u64)
    }

    fn read(&self, offset: u64, out: &mut [u8]) -> io::Result<()> {
        let bytes = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let offset = usize::try_from(offset).map_err(|_| ErrorKind::InvalidInput)?;
        let end = offset
            .checked_add(out.len())
            .ok_or(ErrorKind::InvalidInput)?;
        if end > bytes.len() {
            return Err(io::Error::new(
                ErrorKind::UnexpectedEof,
                format!("read {offset}+{} past length {}", out.len(), bytes.len()),
            ));
        }
        out.copy_from_slice(&bytes[offset..end]);
        Ok(())
    }

    fn set_len(&self, len: u64) -> io::Result<()> {
        let len = usize::try_from(len).map_err(|_| ErrorKind::InvalidInput)?;
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .resize(len, 0);
        Ok(())
    }

    fn sync_data(&self) -> io::Result<()> {
        Ok(())
    }

    fn write(&self, offset: u64, data: &[u8]) -> io::Result<()> {
        let mut bytes = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let offset = usize::try_from(offset).map_err(|_| ErrorKind::InvalidInput)?;
        let end = offset
            .checked_add(data.len())
            .ok_or(ErrorKind::InvalidInput)?;
        if end > bytes.len() {
            bytes.resize(end, 0);
        }
        bytes[offset..end].copy_from_slice(data);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::churn::{ChurnShape, GenerationCheck, commit_generation, materialize, verify};
    use redb::{Database, DatabaseError, StorageError};

    const SHAPE: ChurnShape = ChurnShape {
        keys_per_commit: 8,
        value_bytes: 1024,
    };
    const GENERATIONS: u64 = 12;

    struct Run {
        image: SharedMemory,
        /// Whether `create_with_backend` itself succeeded.
        created: bool,
        /// Generation commits that returned Ok.
        completed: u64,
        error: Option<String>,
        counters: FaultCounters,
    }

    /// Run the churn under `plan` over a fresh image.
    fn run(plan: FaultPlan, two_phase: bool) -> Run {
        let image = SharedMemory::new();
        let backend = FaultBackend::new(image.clone(), plan);
        let watch = backend.watch();
        let mut created = false;
        let mut completed = 0;
        let mut error = None;
        match Database::builder().create_with_backend(backend) {
            Ok(db) => {
                created = true;
                if let Err(err) = materialize(&db) {
                    error = Some(format!("materialize: {err}"));
                } else {
                    for generation in 1..=GENERATIONS {
                        match commit_generation(&db, generation, SHAPE, two_phase) {
                            Ok(()) => completed = generation,
                            Err(err) => {
                                error = Some(format!("generation {generation}: {err}"));
                                break;
                            }
                        }
                    }
                }
                drop(db);
            }
            Err(err) => error = Some(format!("open: {err}")),
        }
        Run {
            image,
            created,
            completed,
            error,
            counters: watch.counters(),
        }
    }

    /// What reopening an image after a fault produced.
    enum Recovery {
        Reopened {
            integrity: bool,
            check: GenerationCheck,
        },
        /// redb refused the bytes as not a database. The finding recorded in
        /// the plan: a cut between the initial resize and the first complete
        /// header write leaves a non-empty file redb will not open. It is only
        /// acceptable when creation itself was cut, before anything existed.
        Uninitialized(String),
        Failed(String),
    }

    fn reopen(image: &SharedMemory) -> Recovery {
        match Database::builder().create_with_backend(image.clone()) {
            Ok(mut db) => {
                let integrity = db.check_integrity().expect("integrity check must run");
                let check = verify(&db, SHAPE).expect("the invariant must be checkable");
                Recovery::Reopened { integrity, check }
            }
            Err(DatabaseError::Storage(StorageError::Io(err)))
                if err.kind() == ErrorKind::InvalidData =>
            {
                Recovery::Uninitialized(err.to_string())
            }
            Err(err) => Recovery::Failed(err.to_string()),
        }
    }

    /// The lane-3 invariant: a reopened store is sound and sits at the
    /// preceding or the completed commit. `None` is the uninitialized case,
    /// allowed only when creation itself was cut.
    fn assert_recovered(label: &str, run: &Run) -> Option<GenerationCheck> {
        match reopen(&run.image) {
            Recovery::Reopened { integrity, check } => {
                assert!(
                    integrity,
                    "{label}: integrity check found and repaired problems: {check:?}"
                );
                assert!(check.ok, "{label}: invariant violated: {check:?}");
                assert!(
                    check.generation == run.completed || check.generation == run.completed + 1,
                    "{label}: reopened generation {} is neither {} nor {}",
                    check.generation,
                    run.completed,
                    run.completed + 1
                );
                Some(check)
            }
            Recovery::Uninitialized(message) => {
                assert!(
                    !run.created && run.completed == 0,
                    "{label}: redb refused a store it had created ({} commits): {message}",
                    run.completed
                );
                None
            }
            Recovery::Failed(message) => panic!("{label}: unrecoverable: {message}"),
        }
    }

    /// A fault either surfaced as an error from create / materialize / a
    /// commit, or it fell in redb's shutdown writes during `drop`, in which
    /// case every generation had already committed.
    fn assert_fault_observed(label: &str, run: &Run) {
        if run.error.is_none() {
            assert_eq!(
                run.completed, GENERATIONS,
                "{label}: no error surfaced, yet not every generation committed"
            );
        }
    }

    fn assert_cut_observed(label: &str, run: &Run) {
        assert!(run.counters.dead, "{label}: the cut must fire");
        assert_fault_observed(label, run);
    }

    #[test]
    fn no_faults_completes_every_generation() {
        let run = run(FaultPlan::default(), false);
        assert_eq!(run.error, None);
        assert_eq!(run.completed, GENERATIONS);
        assert!(run.counters.writes > GENERATIONS, "{:?}", run.counters);
        assert!(assert_recovered("no faults", &run).is_some());
    }

    /// Lane 3, native half: die inside every write of the workload (the fatal
    /// write landing none, half, or all of its bytes) and before every resize,
    /// in both commit modes. Every reopen must yield the preceding or the
    /// completed commit with the invariant intact, or be the uninitialized
    /// case with creation itself cut.
    #[test]
    fn cut_sweep_every_write_and_resize_recovers_a_committed_generation() {
        for two_phase in [false, true] {
            let baseline = run(FaultPlan::default(), two_phase).counters;
            assert!(baseline.writes > 20, "the sweep needs a real write count");
            assert!(baseline.set_lens > 0, "the workload must resize");
            let mut outcomes = [0u64; 3];
            let mut trials = 0u64;
            let mut uninitialized_cuts = Vec::new();
            for cut in 1..=baseline.writes {
                for torn in [0usize, usize::MAX / 2, usize::MAX] {
                    let plan = FaultPlan {
                        cut_at_write: Some(cut),
                        torn_bytes: torn,
                        ..FaultPlan::default()
                    };
                    let run = run(plan, two_phase);
                    let label = format!("two_phase={two_phase} cut_at_write={cut} torn={torn}");
                    assert_cut_observed(&label, &run);
                    match assert_recovered(&label, &run) {
                        Some(check) => outcomes[(check.generation - run.completed) as usize] += 1,
                        None => {
                            outcomes[2] += 1;
                            uninitialized_cuts.push(label);
                        }
                    }
                    trials += 1;
                }
            }
            for cut in 1..=baseline.set_lens {
                let plan = FaultPlan {
                    cut_at_set_len: Some(cut),
                    ..FaultPlan::default()
                };
                let run = run(plan, two_phase);
                let label = format!("two_phase={two_phase} cut_at_set_len={cut}");
                assert_cut_observed(&label, &run);
                match assert_recovered(&label, &run) {
                    Some(check) => outcomes[(check.generation - run.completed) as usize] += 1,
                    None => {
                        outcomes[2] += 1;
                        uninitialized_cuts.push(label);
                    }
                }
                trials += 1;
            }
            eprintln!(
                "two_phase={two_phase}: {trials} cut trials over {} writes and {} resizes; reopened at the preceding commit {} times, at the completed commit {} times; uninitialized (creation cut) {} times: {:?}",
                baseline.writes,
                baseline.set_lens,
                outcomes[0],
                outcomes[1],
                outcomes[2],
                uninitialized_cuts
            );
        }
    }

    /// Lane 3, native half: short writes, failed writes, failed syncs, failed
    /// resizes, and a quota ceiling, each at every call index of the workload.
    #[test]
    fn error_faults_at_every_index_leave_a_reopenable_store() {
        let baseline = run(FaultPlan::default(), false).counters;
        let kinds: Vec<(&str, u64, Box<dyn Fn(u64) -> FaultPlan>)> = vec![
            (
                "fail_write_at",
                baseline.writes,
                Box::new(|n| FaultPlan {
                    fail_write_at: Some(n),
                    ..FaultPlan::default()
                }),
            ),
            (
                "short_write_at",
                baseline.writes,
                Box::new(|n| FaultPlan {
                    short_write_at: Some(n),
                    ..FaultPlan::default()
                }),
            ),
            (
                "fail_sync_at",
                baseline.syncs,
                Box::new(|n| FaultPlan {
                    fail_sync_at: Some(n),
                    ..FaultPlan::default()
                }),
            ),
            (
                "fail_set_len_at",
                baseline.set_lens,
                Box::new(|n| FaultPlan {
                    fail_set_len_at: Some(n),
                    ..FaultPlan::default()
                }),
            ),
        ];
        let mut uninitialized = Vec::new();
        for (name, total, make) in kinds {
            assert!(total > 0, "{name}: the workload must exercise this call");
            for n in 1..=total {
                let run = run(make(n), false);
                let label = format!("{name} #{n}");
                assert_eq!(
                    run.counters.injected.len(),
                    1,
                    "{label}: {:?}",
                    run.counters
                );
                assert_fault_observed(&label, &run);
                if assert_recovered(&label, &run).is_none() {
                    uninitialized.push(label);
                }
            }
        }
        // Quota: every 4 KiB ceiling from the first page up to the final size.
        let final_len = run(FaultPlan::default(), false).image.len_bytes() as u64;
        let mut ceiling = 4096;
        while ceiling < final_len {
            let plan = FaultPlan {
                quota_bytes: Some(ceiling),
                ..FaultPlan::default()
            };
            let run = run(plan, false);
            let label = format!("quota {ceiling}");
            assert_fault_observed(&label, &run);
            if assert_recovered(&label, &run).is_none() {
                uninitialized.push(label);
            }
            ceiling += 4096;
        }
        eprintln!("error faults: uninitialized (creation cut) cases: {uninitialized:?}");
    }

    #[test]
    fn shared_memory_extends_on_write_and_refuses_short_reads() {
        let image = SharedMemory::new();
        image.write(8, &[1, 2, 3]).unwrap();
        assert_eq!(image.len().unwrap(), 11);
        let mut out = [0u8; 3];
        image.read(8, &mut out).unwrap();
        assert_eq!(out, [1, 2, 3]);
        let mut past = [0u8; 4];
        assert_eq!(
            image.read(8, &mut past).unwrap_err().kind(),
            ErrorKind::UnexpectedEof
        );
        image.set_len(4).unwrap();
        assert_eq!(image.snapshot().len(), 4);
    }
}
