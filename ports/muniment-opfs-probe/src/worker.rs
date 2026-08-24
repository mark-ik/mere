// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MPL-2.0

//! The wasm execution body: one `run_command` entry the worker script calls
//! with a JSON [`ProbeCommand`], answering a JSON [`ProbeReport`], plus the
//! `export_file` entry that hands the page a file's bytes as a transferable.
//! Everything browser-specific that is not the storage backend itself lives
//! here, in the probe, never in the adapter or the workloads.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use js_sys::Uint8Array;
use muniment::{Backend, IndexedDbBackend, MemoryBackend, WriteOp};
use redb::backends::InMemoryBackend;
use redb::{Database, ReadableDatabase, TableDefinition};
use serde_json::json;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{DedicatedWorkerGlobalScope, Response};

use crate::churn::{self, ChurnShape};
use crate::fault::{FaultBackend, FaultPlan, FaultWatch};
use crate::indexeddb_range::IndexedDbRangeBackend;
use crate::opfs_backend::{
    self as opfs, IoCounters, OpfsBackend, ProgressFile, dom_exception_name,
};
use crate::redb_backend::RedbBackend;
use crate::workload::{self, BenchBackend};
use crate::{
    AsciiContractReport, BenchReport, ChurnReport, DigestReport, ExistsReport, ExportReport,
    FaultReport, HoldReport, ImportReport, IoStats, OpenReport, ProbeCommand, ProbeReport,
    ProgressReport, ReopenReport, RoundTripReport, SmokeReport, StagedCreateReport, TryOpenReport,
};

/// redb defaults to a 1 GiB cache; a worker does not want that.
const CACHE_BYTES: usize = 32 << 20;

fn scope() -> DedicatedWorkerGlobalScope {
    js_sys::global().unchecked_into()
}

fn now_ms() -> f64 {
    scope()
        .performance()
        .map(|p| p.now())
        .unwrap_or_else(js_sys::Date::now)
}

fn post(kind: &str, mut body: serde_json::Value) -> Result<(), String> {
    body["kind"] = json!(kind);
    scope()
        .post_message(&JsValue::from_str(&body.to_string()))
        .map_err(|e| format!("postMessage: {e:?}"))
}

fn post_state(state: &str, detail: &str) -> Result<(), String> {
    post("state", json!({ "state": state, "detail": detail }))
}

/// Progress over `postMessage`, which the page stops receiving the moment it
/// calls `terminate()`; the side file (`ProgressFile`) is what survives.
fn post_progress(path: &str, phase: &str, generation: u64, io: &IoStats) -> Result<(), String> {
    post(
        "progress",
        json!({ "path": path, "phase": phase, "generation": generation, "io": io }),
    )
}

fn install_panic_hook() {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let _ = post_state("panicked", &info.to_string());
        }));
    });
}

async fn sleep_ms(ms: u32) -> Result<(), String> {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = scope().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32);
    });
    JsFuture::from(promise)
        .await
        .map(|_| ())
        .map_err(|e| format!("sleep: {e:?}"))
}

struct Opened {
    db: Database,
    counters: Arc<IoCounters>,
    watch: Option<FaultWatch>,
    report: OpenReport,
}

struct OpenFailure {
    report: OpenReport,
    dom_exception: Option<String>,
    /// The fault counters, when the open ran under a plan and got as far as
    /// redb (a cut inside creation is one of the lane-3 cases).
    watch: Option<FaultWatch>,
}

fn io_failure(err: std::io::Error, started: f64) -> OpenFailure {
    OpenFailure {
        watch: None,
        dom_exception: dom_exception_name(&err),
        report: OpenReport {
            opened: false,
            error: Some(err.to_string()),
            error_kind: Some(format!("{:?}", err.kind())),
            open_ms: now_ms() - started,
            repair_invoked: false,
            repair_callbacks: 0,
        },
    }
}

/// Take the OPFS handle and open a redb database over it, optionally through
/// a fault plan. `create` false refuses a missing file with `NotFound`.
async fn open_db(
    path: &str,
    reset: bool,
    create: bool,
    plan: Option<FaultPlan>,
) -> Result<Opened, OpenFailure> {
    let started = now_ms();
    if reset {
        opfs::remove(path)
            .await
            .map_err(|e| io_failure(e, started))?;
    }
    let backend = OpfsBackend::open(path, create)
        .await
        .map_err(|e| io_failure(e, started))?;
    let counters = backend.counters();
    let repairs = Rc::new(Cell::new(0u32));
    let mut builder = Database::builder();
    builder.set_cache_size(CACHE_BYTES);
    let counted = repairs.clone();
    builder.set_repair_callback(move |_| counted.set(counted.get() + 1));
    let (outcome, watch) = match plan {
        None => (builder.create_with_backend(backend), None),
        Some(plan) => {
            let faulty = FaultBackend::new(backend, plan);
            let watch = faulty.watch();
            (builder.create_with_backend(faulty), Some(watch))
        }
    };
    // On failure redb has already called `close()`: the handle is released.
    let db = outcome.map_err(|err| OpenFailure {
        dom_exception: None,
        watch: watch.clone(),
        report: OpenReport {
            opened: false,
            error: Some(err.to_string()),
            error_kind: None,
            open_ms: now_ms() - started,
            repair_invoked: repairs.get() > 0,
            repair_callbacks: repairs.get(),
        },
    })?;
    Ok(Opened {
        db,
        counters,
        watch,
        report: OpenReport {
            opened: true,
            error: None,
            error_kind: None,
            open_ms: now_ms() - started,
            repair_invoked: repairs.get() > 0,
            repair_callbacks: repairs.get(),
        },
    })
}

fn in_memory_smoke() -> Result<SmokeReport, String> {
    const T: TableDefinition<&str, &[u8]> = TableDefinition::new("smoke");
    let started = now_ms();
    let run = || -> Result<bool, redb::Error> {
        let mut builder = Database::builder();
        builder.set_cache_size(CACHE_BYTES);
        let db = builder.create_with_backend(InMemoryBackend::new())?;
        let w = db.begin_write()?;
        {
            let mut t = w.open_table(T)?;
            t.insert("k", b"v".as_slice())?;
        }
        w.commit()?;
        let w = db.begin_write()?;
        {
            let mut t = w.open_table(T)?;
            let blob = vec![0xABu8; 64 * 1024];
            for i in 0..64u32 {
                t.insert(format!("blob/{i:04}").as_str(), blob.as_slice())?;
            }
        }
        w.commit()?;
        let r = db.begin_read()?;
        let t = r.open_table(T)?;
        let v = t.get("k")?.map(|g| g.value().to_vec());
        let n = t.range("blob/".."blob0")?.count();
        Ok(v.as_deref() == Some(b"v".as_slice()) && n == 64)
    };
    let ok = run().map_err(|e| format!("in-memory smoke: {e}"))?;
    Ok(SmokeReport {
        ok,
        blobs: 64,
        ms: now_ms() - started,
    })
}

async fn reopen(path: &str, shape: ChurnShape) -> ReopenReport {
    let started = now_ms();
    match open_db(path, false, false, None).await {
        Err(failure) => ReopenReport {
            open: failure.report,
            integrity_ok: None,
            integrity_error: None,
            check: None,
            file_len: opfs::size(path).await.unwrap_or(0),
            io: IoStats::default(),
            ms: now_ms() - started,
        },
        Ok(mut opened) => {
            let (integrity_ok, integrity_error) = match opened.db.check_integrity() {
                Ok(ok) => (Some(ok), None),
                Err(err) => (None, Some(err.to_string())),
            };
            let check = churn::verify(&opened.db, shape).ok();
            let io = opened.counters.snapshot();
            drop(opened.db);
            ReopenReport {
                open: opened.report,
                integrity_ok,
                integrity_error,
                check,
                file_len: opfs::size(path).await.unwrap_or(0),
                io,
                ms: now_ms() - started,
            }
        }
    }
}

async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let value = JsFuture::from(scope().fetch_with_str(url))
        .await
        .map_err(|e| format!("fetch {url}: {e:?}"))?;
    let response: Response = value
        .dyn_into()
        .map_err(|_| format!("fetch {url}: response had the wrong type"))?;
    if !response.ok() {
        return Err(format!(
            "fetch {url}: HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }
    let buffer = JsFuture::from(
        response
            .array_buffer()
            .map_err(|e| format!("read {url}: {e:?}"))?,
    )
    .await
    .map_err(|e| format!("read {url}: {e:?}"))?;
    let bytes = Uint8Array::new(&buffer);
    let mut out = vec![0; bytes.length() as usize];
    bytes.copy_to(&mut out);
    Ok(out)
}

async fn execute(command: ProbeCommand) -> Result<ProbeReport, String> {
    match command {
        ProbeCommand::InMemorySmoke => {
            post_state(
                "smoke",
                "redb transactions against InMemoryBackend in this worker",
            )?;
            Ok(ProbeReport::InMemorySmoke(in_memory_smoke()?))
        }
        ProbeCommand::OpfsRoundTrip {
            path,
            reset,
            commits,
            shape,
            two_phase_commit,
        } => {
            let started = now_ms();
            post_state("opening", &format!("taking the OPFS handle for {path}"))?;
            let opened = open_db(&path, reset, true, None)
                .await
                .map_err(|f| f.report.error.unwrap_or_default())?;
            churn::materialize(&opened.db).map_err(|e| e.to_string())?;
            let start = churn::current_generation(&opened.db).map_err(|e| e.to_string())?;
            post_state("committing", &format!("{commits} generations from {start}"))?;
            for generation in start + 1..=start + commits as u64 {
                churn::commit_generation(&opened.db, generation, shape, two_phase_commit)
                    .map_err(|e| format!("generation {generation}: {e}"))?;
            }
            let generations_committed =
                churn::current_generation(&opened.db).map_err(|e| e.to_string())?;
            let io = opened.counters.snapshot();
            post_state("closing", "dropping the database releases the handle")?;
            drop(opened.db);
            Ok(ProbeReport::OpfsRoundTrip(RoundTripReport {
                open: opened.report,
                generations_committed,
                file_len: opfs::size(&path).await.map_err(|e| e.to_string())?,
                io,
                ms: now_ms() - started,
            }))
        }
        ProbeCommand::Churn {
            path,
            reset,
            commits,
            shape,
            two_phase_commit,
            yield_between_commits,
        } => {
            let started = now_ms();
            let opened = open_db(&path, reset, true, None)
                .await
                .map_err(|f| f.report.error.unwrap_or_default())?;
            let progress = ProgressFile::open(&path)
                .await
                .map_err(|e| format!("progress file: {e}"))?;
            churn::materialize(&opened.db).map_err(|e| e.to_string())?;
            let start = churn::current_generation(&opened.db).map_err(|e| e.to_string())?;
            progress.record(start, start).map_err(|e| e.to_string())?;
            let mut completed = 0u32;
            let mut last = start;
            for generation in start + 1..=start + commits as u64 {
                progress
                    .record(last, generation)
                    .map_err(|e| e.to_string())?;
                post_progress(&path, "committing", generation, &opened.counters.snapshot())?;
                churn::commit_generation(&opened.db, generation, shape, two_phase_commit)
                    .map_err(|e| format!("generation {generation}: {e}"))?;
                completed += 1;
                last = generation;
                progress
                    .record(last, generation)
                    .map_err(|e| e.to_string())?;
                post_progress(&path, "committed", generation, &opened.counters.snapshot())?;
                if yield_between_commits {
                    sleep_ms(0).await?;
                }
            }
            let io = opened.counters.snapshot();
            drop(progress);
            drop(opened.db);
            Ok(ProbeReport::Churn(ChurnReport {
                open: opened.report,
                start_generation: start,
                commits_completed: completed,
                last_generation: last,
                file_len: opfs::size(&path).await.map_err(|e| e.to_string())?,
                io,
                ms: now_ms() - started,
            }))
        }
        ProbeCommand::Progress { path } => {
            let found = ProgressFile::read(&path)
                .await
                .map_err(|e| format!("progress file: {e}"))?;
            Ok(ProbeReport::Progress(ProgressReport {
                path,
                committed: found.map(|(c, _)| c),
                committing: found.map(|(_, c)| c),
            }))
        }
        ProbeCommand::Reopen { path, shape } => {
            post_state(
                "reopening",
                &format!("{path}: integrity check and invariant"),
            )?;
            Ok(ProbeReport::Reopen(reopen(&path, shape).await))
        }
        ProbeCommand::Fault {
            path,
            plan,
            commits,
            shape,
            two_phase_commit,
        } => {
            post_state("faulting", &format!("{path} under {plan:?}"))?;
            // A plan can cut creation itself; that is a lane-3 case, not a
            // harness failure, so an open failure still reaches the reopen.
            let (open, watch, db) = match open_db(&path, true, true, Some(plan.clone())).await {
                Ok(opened) => (opened.report, opened.watch, Some(opened.db)),
                Err(failure) => (failure.report, failure.watch, None),
            };
            let mut completed = 0u32;
            let mut failed_generation = None;
            let mut error = open.error.clone();
            let mut further_commit_refused = None;
            if let Some(db) = db {
                if let Err(err) = churn::materialize(&db) {
                    error = Some(format!("materialize: {err}"));
                } else {
                    for generation in 1..=commits as u64 {
                        match churn::commit_generation(&db, generation, shape, two_phase_commit) {
                            Ok(()) => completed += 1,
                            Err(err) => {
                                failed_generation = Some(generation);
                                error = Some(err.to_string());
                                break;
                            }
                        }
                    }
                }
                further_commit_refused = failed_generation.map(|g| {
                    churn::commit_generation(&db, g + 1, shape, two_phase_commit).is_err()
                });
                drop(db);
            }
            let counters = watch.map(|w| w.counters()).unwrap_or_default();
            post_state("recovering", "reopening the faulted store cleanly")?;
            let reopen = reopen(&path, shape).await;
            Ok(ProbeReport::Fault(FaultReport {
                plan,
                open,
                commits_completed: completed,
                failed_generation,
                error,
                further_commit_refused,
                counters,
                reopen,
            }))
        }
        ProbeCommand::Hold {
            path,
            reset,
            hold_ms,
            shape,
        } => {
            let opened = open_db(&path, reset, true, None)
                .await
                .map_err(|f| f.report.error.unwrap_or_default())?;
            churn::materialize(&opened.db).map_err(|e| e.to_string())?;
            let generation = churn::current_generation(&opened.db).map_err(|e| e.to_string())? + 1;
            churn::commit_generation(&opened.db, generation, shape, false)
                .map_err(|e| e.to_string())?;
            let started = now_ms();
            post_state(
                "holding",
                &format!("{path} at generation {generation} for {hold_ms} ms"),
            )?;
            sleep_ms(hold_ms).await?;
            let held_ms = now_ms() - started;
            drop(opened.db);
            post_state("released", "closed cleanly")?;
            Ok(ProbeReport::Hold(HoldReport {
                open: opened.report,
                generation,
                held_ms,
                closed_cleanly: true,
            }))
        }
        ProbeCommand::TryOpen { path } => match open_db(&path, false, false, None).await {
            Ok(opened) => {
                let generation = churn::current_generation(&opened.db).ok();
                drop(opened.db);
                Ok(ProbeReport::TryOpen(TryOpenReport {
                    open: opened.report,
                    dom_exception: None,
                    generation,
                }))
            }
            Err(failure) => Ok(ProbeReport::TryOpen(TryOpenReport {
                open: failure.report,
                dom_exception: failure.dom_exception,
                generation: None,
            })),
        },
        ProbeCommand::Import { path, url } => {
            let started = now_ms();
            post_state("importing", &format!("{url} into {path}"))?;
            let bytes = fetch_bytes(&url).await?;
            opfs::write_all(&path, &bytes)
                .await
                .map_err(|e| e.to_string())?;
            Ok(ProbeReport::Import(ImportReport {
                path,
                url,
                bytes: bytes.len() as u64,
                blake3: blake3::hash(&bytes).to_hex().to_string(),
                ms: now_ms() - started,
            }))
        }
        ProbeCommand::Export { path } => {
            let bytes = opfs::read_all(&path).await.map_err(|e| e.to_string())?;
            Ok(ProbeReport::Export(ExportReport {
                path,
                bytes: bytes.len() as u64,
                blake3: blake3::hash(&bytes).to_hex().to_string(),
            }))
        }
        ProbeCommand::Digest { path } => {
            let mut opened = open_db(&path, false, false, None)
                .await
                .map_err(|f| f.report.error.unwrap_or_default())?;
            let integrity_ok = opened.db.check_integrity().ok();
            let generation = churn::current_generation(&opened.db).map_err(|e| e.to_string())?;
            let store = RedbBackend::from_database(opened.db).map_err(|e| e.to_string())?;
            let (digest, keys) = workload::digest(&store).await.map_err(|e| e.to_string())?;
            drop(store);
            Ok(ProbeReport::Digest(DigestReport {
                open: opened.report,
                integrity_ok,
                generation,
                digest,
                keys,
                file_len: opfs::size(&path).await.map_err(|e| e.to_string())?,
            }))
        }
        ProbeCommand::Bench {
            backend,
            workload,
            name,
            reset,
        } => {
            post_state("bench", &format!("{workload:?} on {backend:?}"))?;
            let clock = now_ms;
            let started = now_ms();
            let (outcome, phases, io) = match backend {
                BenchBackend::RedbOpfs => {
                    let opened = open_db(&name, reset, true, None)
                        .await
                        .map_err(|f| f.report.error.unwrap_or_default())?;
                    let counters = opened.counters.clone();
                    let store = RedbBackend::from_database(opened.db).map_err(|e| e.to_string())?;
                    let (outcome, phases) = workload::run(&store, workload, &clock)
                        .await
                        .map_err(|e| e.to_string())?;
                    let io = counters.snapshot();
                    drop(store);
                    (outcome, phases, Some(io))
                }
                BenchBackend::IndexedDb => {
                    let store = IndexedDbBackend::open(&name, "muniment")
                        .await
                        .map_err(|e| e.to_string())?;
                    let (outcome, phases) = workload::run(&store, workload, &clock)
                        .await
                        .map_err(|e| e.to_string())?;
                    (outcome, phases, None)
                }
                BenchBackend::IndexedDbRange => {
                    let store = IndexedDbRangeBackend::open(&name, "muniment")
                        .await
                        .map_err(|e| e.to_string())?;
                    let (outcome, phases) = workload::run(&store, workload, &clock)
                        .await
                        .map_err(|e| e.to_string())?;
                    (outcome, phases, None)
                }
                BenchBackend::Memory => {
                    let store = MemoryBackend::new();
                    let (outcome, phases) = workload::run(&store, workload, &clock)
                        .await
                        .map_err(|e| e.to_string())?;
                    (outcome, phases, None)
                }
            };
            Ok(ProbeReport::Bench(BenchReport {
                backend,
                workload,
                outcome,
                phases,
                total_ms: now_ms() - started,
                io,
            }))
        }
        ProbeCommand::Remove { path } => {
            let existed = opfs::remove(&path).await.map_err(|e| e.to_string())?;
            Ok(ProbeReport::Removed { path, existed })
        }
        ProbeCommand::AsciiContract { name } => {
            post_state("contract", "ASCII key contract on the range backend")?;
            let store = IndexedDbRangeBackend::open(&name, "muniment")
                .await
                .map_err(|e| e.to_string())?;
            // Two keys the UTF-16/code-point orders disagree about, plus an
            // ordinary non-ASCII one.
            let bad = "\u{FFFF}";
            let bad2 = "k/\u{10000}";
            let mut refused = Vec::new();
            refused.push(("put".into(), store.put(bad, b"v").await.is_err()));
            refused.push(("get".into(), store.get(bad).await.is_err()));
            refused.push(("delete".into(), store.delete(bad).await.is_err()));
            refused.push(("list".into(), store.list(bad).await.is_err()));
            refused.push(("scan_start".into(), store.scan(bad, "zzz").await.is_err()));
            refused.push(("scan_end".into(), store.scan("a", bad).await.is_err()));
            // A batch whose LAST op is bad must be refused whole, leaving the
            // earlier ops unapplied.
            let batch = vec![
                WriteOp::Put {
                    key: "ok/1".into(),
                    value: b"a".to_vec(),
                },
                WriteOp::Put {
                    key: "ok/2".into(),
                    value: b"b".to_vec(),
                },
                WriteOp::Put {
                    key: bad2.into(),
                    value: b"c".to_vec(),
                },
            ];
            refused.push(("apply".into(), store.apply(&batch).await.is_err()));
            // Fail CLOSED. An earlier version read `get` errors as "absent"
            // and `list` errors as "empty", so a backend that errored on
            // every call would have scored a perfect pass. These reads are
            // ASCII and must genuinely succeed; a failure is a failure.
            let left_1 = store
                .get("ok/1")
                .await
                .map_err(|e| format!("post-apply get(ok/1): {e}"))?;
            let left_2 = store
                .get("ok/2")
                .await
                .map_err(|e| format!("post-apply get(ok/2): {e}"))?;
            let apply_left_nothing = left_1.is_none() && left_2.is_none();
            // ASCII still works.
            store
                .put("ascii/key", b"v")
                .await
                .map_err(|e| format!("ascii put: {e}"))?;
            let ascii_still_works = store
                .get("ascii/key")
                .await
                .map_err(|e| format!("ascii get: {e}"))?
                == Some(b"v".to_vec())
                && store
                    .scan("ascii/", "ascii0")
                    .await
                    .map_err(|e| format!("ascii scan: {e}"))?
                    .len()
                    == 1;
            let final_keys = store
                .list("")
                .await
                .map_err(|e| format!("final list: {e}"))?;
            // Exactly the one key the ASCII writes put there — not merely
            // "everything present happens to be ASCII", which an empty or
            // partially-written store would also satisfy.
            let ok = refused.iter().all(|(_, r)| *r)
                && apply_left_nothing
                && ascii_still_works
                && final_keys == vec!["ascii/key".to_string()];
            Ok(ProbeReport::AsciiContract(AsciiContractReport {
                refused,
                ascii_still_works,
                apply_left_nothing,
                final_keys,
                ok,
            }))
        }
        ProbeCommand::Exists { path } => {
            let exists = opfs::exists(&path).await.map_err(|e| e.to_string())?;
            let len = if exists {
                opfs::size(&path).await.unwrap_or(0)
            } else {
                0
            };
            Ok(ProbeReport::Exists(ExistsReport { path, exists, len }))
        }
        ProbeCommand::StagedCreate {
            path,
            plan,
            commits,
            shape,
            two_phase_commit,
            cut_before_promote,
            announce_promote,
        } => {
            let staging_path = format!("{path}.staging");
            post_state("staging", &format!("creating {staging_path}"))?;
            // A fresh trial owns both names.
            opfs::remove(&path).await.map_err(|e| e.to_string())?;
            opfs::remove(&staging_path)
                .await
                .map_err(|e| e.to_string())?;

            // Build the database under the staging name, under the fault plan.
            let (open, watch, db) =
                match open_db(&staging_path, false, true, Some(plan.clone())).await {
                    Ok(opened) => (opened.report, opened.watch, Some(opened.db)),
                    Err(failure) => (failure.report, failure.watch, None),
                };
            let mut staged_ok = false;
            let mut completed = 0u32;
            let mut error = open.error.clone();
            if let Some(db) = db {
                match churn::materialize(&db) {
                    Err(err) => error = Some(format!("materialize: {err}")),
                    Ok(()) => {
                        staged_ok = true;
                        for generation in 1..=commits as u64 {
                            match churn::commit_generation(&db, generation, shape, two_phase_commit)
                            {
                                Ok(()) => completed += 1,
                                Err(err) => {
                                    staged_ok = false;
                                    error = Some(err.to_string());
                                    break;
                                }
                            }
                        }
                    }
                }
                drop(db);
            }
            let counters = watch.map(|w| w.counters()).unwrap_or_default();

            // Promote only a staging file that was fully built. A cut creation
            // leaves the staging file behind and the final path untouched,
            // which is exactly the property under test.
            let mut atomic_move = None;
            let mut promoted = false;
            if staged_ok && completed == commits && !cut_before_promote {
                post_state("promoting", &format!("{staging_path} -> {path}"))?;
                if announce_promote {
                    // Hand the page a scheduling slot to call terminate() in,
                    // so the kill can land while move() is in flight.
                    sleep_ms(0).await?;
                }
                match opfs::promote(&staging_path, &path).await {
                    Ok(atomic) => {
                        atomic_move = Some(atomic);
                        promoted = true;
                    }
                    Err(err) => error = Some(format!("promote: {err}")),
                }
            }

            let final_exists = opfs::exists(&path).await.map_err(|e| e.to_string())?;
            let final_len = if final_exists {
                opfs::size(&path).await.unwrap_or(0)
            } else {
                0
            };
            let reopen = if final_exists {
                Some(reopen(&path, shape).await)
            } else {
                None
            };
            let staging_left = opfs::exists(&staging_path)
                .await
                .map_err(|e| e.to_string())?;
            let staging_len = if staging_left {
                opfs::size(&staging_path).await.unwrap_or(0)
            } else {
                0
            };
            // The property: the final path is absent, or it opens soundly at
            // the full committed generation. An unopenable stub fails.
            let ok = match &reopen {
                None => !final_exists,
                Some(r) => {
                    r.open.opened
                        && r.integrity_ok == Some(true)
                        && r.check.as_ref().map(|c| c.ok).unwrap_or(false)
                        && r.check.as_ref().map(|c| c.generation) == Some(commits as u64)
                }
            };
            Ok(ProbeReport::StagedCreate(StagedCreateReport {
                path,
                staging_path,
                plan,
                atomic_move,
                staged_ok,
                commits_completed: completed,
                counters,
                error,
                promoted,
                final_exists,
                final_len,
                reopen,
                staging_left,
                staging_len,
                ok,
            }))
        }
    }
}

/// Run one command inside the dedicated worker; the answer is a JSON report.
#[wasm_bindgen]
pub async fn run_command(command_json: String) -> Result<String, JsValue> {
    install_panic_hook();
    let command: ProbeCommand = serde_json::from_str(&command_json)
        .map_err(|e| JsValue::from_str(&format!("probe command: {e}")))?;
    match execute(command).await {
        Ok(report) => serde_json::to_string(&report)
            .map_err(|e| JsValue::from_str(&format!("serialize report: {e}"))),
        Err(error) => {
            let _ = post_state("failed", &error);
            Err(JsValue::from_str(&error))
        }
    }
}

/// Build provenance baked in by run-probe.ps1, for the receipt.
///
/// Every field that can change behaviour: the probe's own tree, the
/// compiled-in muniment path dependency (whose lockfile version is only
/// "0.1.1", so the source is what identifies it), and the real lockfile,
/// hashed whole so sources and checksums are covered. `--locked` in the build
/// proves that lockfile is the one that was used. The page adds hashes of the
/// wasm and JS it actually loaded.
///
/// The hashes are **SHA-256**, and the field names say so; an earlier version
/// called them `blake3` while computing SHA-256.
#[wasm_bindgen]
pub fn build_info() -> String {
    json!({
        "schema": crate::SCHEMA,
        "redb": "4.2.0",
        "commit": option_env!("MUNIMENT_OPFS_PROBE_COMMIT").unwrap_or("unknown"),
        "dirty": option_env!("MUNIMENT_OPFS_PROBE_DIRTY").and_then(|v| v.parse::<bool>().ok()),
        "probe_source_sha256": option_env!("MUNIMENT_OPFS_PROBE_SOURCE_SHA256").unwrap_or("unknown"),
        "muniment_source_sha256": option_env!("MUNIMENT_OPFS_PROBE_MUNIMENT_SHA256").unwrap_or("unknown"),
        "cargo_lock_sha256": option_env!("MUNIMENT_OPFS_PROBE_LOCK_SHA256").unwrap_or("unknown"),
        "locked_build": option_env!("MUNIMENT_OPFS_PROBE_LOCK_SHA256").is_some(),
        "built_at": option_env!("MUNIMENT_OPFS_PROBE_BUILT_AT").unwrap_or("unknown"),
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "target_feature_atomics": cfg!(target_feature = "atomics"),
    })
    .to_string()
}

/// Browser storage capabilities that are only visible inside a worker.
#[wasm_bindgen]
pub async fn opfs_capabilities() -> String {
    json!({
        "sync_access_handle": true,
        "atomic_move": opfs::move_supported().await,
    })
    .to_string()
}

/// Every byte of an OPFS file, as a fresh `Uint8Array` the worker can
/// transfer to the page.
#[wasm_bindgen]
pub async fn export_file(path: String) -> Result<Uint8Array, JsValue> {
    install_panic_hook();
    let bytes = opfs::read_all(&path)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(Uint8Array::from(bytes.as_slice()))
}
