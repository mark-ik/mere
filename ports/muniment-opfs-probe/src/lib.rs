// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! muniment OPFS feasibility probe.
//!
//! The question: can redb 4.2 run over an OPFS sync-access handle as a
//! muniment backend, inside a dedicated browser worker, while keeping redb's
//! storage contract and recovery guarantees, without an `unsafe` thread claim
//! and without browser authority leaking above the seam?
//!
//! The command and report vocabulary here compiles natively, so the
//! deterministic halves (the fault sweep, the generation invariant, the
//! workloads, the redb adapter) are ordinary `cargo test`s. The execution body
//! is wasm-only and runs inside a Web Worker; the page in `web/` drives it and
//! assembles the receipt.
//!
//! Lanes, per the 2026-08-22 feasibility plan:
//!
//! 1. WASM viability: [`ProbeCommand::InMemorySmoke`].
//! 2. OPFS storage device: [`ProbeCommand::OpfsRoundTrip`] then
//!    [`ProbeCommand::Reopen`] from a fresh worker.
//! 3. Recovery: [`ProbeCommand::Fault`] (injected errors and cuts over the
//!    real OPFS handle) and [`ProbeCommand::Churn`] terminated by the page
//!    mid-commit, each followed by a [`ProbeCommand::Reopen`] that checks the
//!    generation invariant.
//! 4. Ownership: [`ProbeCommand::Hold`] in one worker or tab while another
//!    runs [`ProbeCommand::TryOpen`].
//! 5. Portability and performance: [`ProbeCommand::Import`] /
//!    [`ProbeCommand::Export`] against the native fixture, and
//!    [`ProbeCommand::Bench`] across backends and workloads.

pub mod churn;
pub mod fault;
pub mod idb_keys;
#[cfg(target_arch = "wasm32")]
pub mod indexeddb_range;
#[cfg(target_arch = "wasm32")]
pub mod opfs_backend;
pub mod redb_backend;
#[cfg(target_arch = "wasm32")]
mod worker;
pub mod workload;

use serde::{Deserialize, Serialize};

pub use churn::{ChurnShape, GenerationCheck};
pub use fault::{FaultCounters, FaultPlan};
pub use workload::{BenchBackend, Phase, Workload, WorkloadOutcome};

/// Versioned report contract.
pub const SCHEMA: &str = "muniment.opfs-probe/v1";

/// One instruction to the worker. Serialized as `{"command": "...", ...}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProbeCommand {
    /// Lane 1: redb transactions against `InMemoryBackend` inside the worker.
    InMemorySmoke,
    /// Lane 2: create (or reset) a database at `path`, commit `commits`
    /// generations, close. Reopen from a fresh worker with [`Self::Reopen`].
    OpfsRoundTrip {
        path: String,
        reset: bool,
        commits: u32,
        #[serde(default)]
        shape: ChurnShape,
        #[serde(default)]
        two_phase_commit: bool,
    },
    /// Lane 3: commit generations, posting `progress` before and after every
    /// commit so the page can terminate the worker at a chosen point, and
    /// recording the same progress in an OPFS side file the page can read
    /// after the worker is dead.
    Churn {
        path: String,
        reset: bool,
        commits: u32,
        #[serde(default)]
        shape: ChurnShape,
        #[serde(default)]
        two_phase_commit: bool,
        /// Return to the event loop after every commit, so a `terminate()`
        /// is honored between commits (the cooperative app case) instead of
        /// by the browser's forcible delay.
        #[serde(default)]
        yield_between_commits: bool,
    },
    /// Lane 3: what the churn's side file says the worker was doing when it
    /// died: `(committed, committing)` generations.
    Progress { path: String },
    /// Open without reset, run the integrity check, and verify the generation
    /// invariant. The reopen half of every recovery trial.
    Reopen {
        path: String,
        #[serde(default)]
        shape: ChurnShape,
    },
    /// Lane 3: run generations under an injected fault plan over the real OPFS
    /// handle, then reopen cleanly in this same worker and verify.
    Fault {
        path: String,
        plan: FaultPlan,
        commits: u32,
        #[serde(default)]
        shape: ChurnShape,
        #[serde(default)]
        two_phase_commit: bool,
    },
    /// Lane 4: open, commit one generation, keep the handle for `hold_ms`,
    /// then close cleanly. The page kills this worker to test release.
    Hold {
        path: String,
        reset: bool,
        hold_ms: u32,
        #[serde(default)]
        shape: ChurnShape,
    },
    /// Lane 4: attempt to open an existing database and immediately close;
    /// report refusal or success with the browser's own error name.
    TryOpen { path: String },
    /// Lane 5: fetch `url` and write its bytes to `path` in OPFS.
    Import { path: String, url: String },
    /// Lane 5: length and blake3 of the file at `path`. The bytes themselves
    /// come through the worker's `export_file` export as a transferable.
    Export { path: String },
    /// Lane 5: open the redb database at `path` and digest its muniment table,
    /// the content oracle the native fixture manifest records.
    Digest { path: String },
    /// Lane 5: one workload against one backend. `name` is the OPFS path for
    /// redb, the database name for IndexedDB, ignored for memory.
    Bench {
        backend: BenchBackend,
        workload: Workload,
        name: String,
        reset: bool,
    },
    /// Delete the file at `path`. Absent is not an error.
    Remove { path: String },
    /// Lane 6: create a database at `path` through a **staging file**, commit
    /// `commits` generations, then promote the staging file onto `path`. Under
    /// a fault plan the creation is cut; the property under test is that
    /// `path` is then either absent or a valid database, never an unopenable
    /// stub (the §5.4 remedy).
    StagedCreate {
        path: String,
        #[serde(default)]
        plan: FaultPlan,
        commits: u32,
        #[serde(default)]
        shape: ChurnShape,
        #[serde(default)]
        two_phase_commit: bool,
        /// Cut the process after the staging file is committed but before the
        /// promotion runs.
        #[serde(default)]
        cut_before_promote: bool,
        /// Post a `promoting` state message immediately before calling
        /// `move()` and yield, so the page can `terminate()` this worker
        /// while the rename is in flight. That is the only way to test
        /// whether the promotion is crash-atomic; `cut_before_promote` only
        /// tests the window *before* it.
        #[serde(default)]
        announce_promote: bool,
    },
    /// Whether a file exists, and its length.
    Exists { path: String },
    /// Lane 5: exercise the range backend's ASCII key contract in a real
    /// browser — every key-bearing operation must refuse a non-ASCII key, and
    /// a refused `apply` must leave nothing behind.
    AsciiContract { name: String },
}

/// Storage-call counters collected by the OPFS backend for one open.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IoStats {
    pub reads: u64,
    pub writes: u64,
    pub set_lens: u64,
    pub syncs: u64,
    pub bytes_read: u64,
    pub bytes_written: u64,
}

/// How one open went.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OpenReport {
    pub opened: bool,
    /// Stringified error when `opened` is false.
    pub error: Option<String>,
    /// The `std::io::ErrorKind` the browser error mapped to, when one did.
    pub error_kind: Option<String>,
    /// Wall time from the OPFS handle request to a usable `Database`.
    pub open_ms: f64,
    /// Whether redb invoked the repair callback (an unclean prior shutdown).
    pub repair_invoked: bool,
    pub repair_callbacks: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SmokeReport {
    pub ok: bool,
    pub blobs: u32,
    pub ms: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoundTripReport {
    pub open: OpenReport,
    pub generations_committed: u64,
    pub file_len: u64,
    pub io: IoStats,
    pub ms: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChurnReport {
    pub open: OpenReport,
    pub start_generation: u64,
    pub commits_completed: u32,
    pub last_generation: u64,
    pub file_len: u64,
    pub io: IoStats,
    pub ms: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProgressReport {
    pub path: String,
    /// Absent when no side file exists.
    pub committed: Option<u64>,
    pub committing: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReopenReport {
    pub open: OpenReport,
    /// `Some(true)`: passed; `Some(false)`: problems found and repaired;
    /// `None`: the check itself errored (see `integrity_error`).
    pub integrity_ok: Option<bool>,
    pub integrity_error: Option<String>,
    pub check: Option<GenerationCheck>,
    pub file_len: u64,
    pub io: IoStats,
    pub ms: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FaultReport {
    pub plan: FaultPlan,
    pub open: OpenReport,
    pub commits_completed: u32,
    /// The generation whose commit returned the error, if one did.
    pub failed_generation: Option<u64>,
    pub error: Option<String>,
    /// Whether a further commit after the error was refused (redb poisons the
    /// database after a failed commit until it is reopened).
    pub further_commit_refused: Option<bool>,
    pub counters: FaultCounters,
    pub reopen: ReopenReport,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HoldReport {
    pub open: OpenReport,
    pub generation: u64,
    pub held_ms: f64,
    pub closed_cleanly: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TryOpenReport {
    pub open: OpenReport,
    /// The DOMException name on refusal. `NoModificationAllowedError` is the
    /// sync-access-handle exclusivity refusal.
    pub dom_exception: Option<String>,
    pub generation: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImportReport {
    pub path: String,
    pub url: String,
    pub bytes: u64,
    pub blake3: String,
    pub ms: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExportReport {
    pub path: String,
    pub bytes: u64,
    pub blake3: String,
}

/// Lane 6: one staged-creation trial.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StagedCreateReport {
    pub path: String,
    pub staging_path: String,
    pub plan: FaultPlan,
    /// Whether this browser implements `FileSystemFileHandle.move()`, so the
    /// promotion was a rename rather than a copy + delete.
    pub atomic_move: Option<bool>,
    /// Whether the staged database was created and committed at all.
    pub staged_ok: bool,
    pub commits_completed: u32,
    /// Storage calls the staged creation actually made. The cut sweep is
    /// derived from these, so "every write index" means every write index.
    pub counters: FaultCounters,
    pub error: Option<String>,
    /// Whether the promotion ran (false when the creation was cut first).
    pub promoted: bool,
    /// State of the FINAL path after the trial. This is the property under
    /// test: `absent` or a sound database, never an unopenable stub.
    pub final_exists: bool,
    pub final_len: u64,
    /// Present when the final path exists: the reopen that must succeed.
    pub reopen: Option<ReopenReport>,
    /// The staging file left behind, which a host discards on next open.
    pub staging_left: bool,
    pub staging_len: u64,
    /// The whole point: absent, or present-and-sound.
    pub ok: bool,
}

/// Lane 5: the ASCII contract, checked in the browser.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AsciiContractReport {
    /// Operation name → refused as required.
    pub refused: Vec<(String, bool)>,
    /// ASCII operations still work (the contract is not just "refuse
    /// everything").
    pub ascii_still_works: bool,
    /// A refused `apply` left no key behind — the batch is all-or-nothing.
    pub apply_left_nothing: bool,
    /// Keys present at the end; must contain only what ASCII writes put there.
    pub final_keys: Vec<String>,
    pub ok: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExistsReport {
    pub path: String,
    pub exists: bool,
    pub len: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DigestReport {
    pub open: OpenReport,
    pub integrity_ok: Option<bool>,
    pub generation: u64,
    pub digest: String,
    pub keys: u64,
    pub file_len: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BenchReport {
    pub backend: BenchBackend,
    pub workload: Workload,
    pub outcome: WorkloadOutcome,
    pub phases: Vec<Phase>,
    pub total_ms: f64,
    /// Present for the redb-on-OPFS backend only.
    pub io: Option<IoStats>,
}

/// The worker's answer. Serialized as `{"report": "...", ...}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "report", rename_all = "snake_case")]
pub enum ProbeReport {
    InMemorySmoke(SmokeReport),
    OpfsRoundTrip(RoundTripReport),
    Churn(ChurnReport),
    Progress(ProgressReport),
    Reopen(ReopenReport),
    Fault(FaultReport),
    Hold(HoldReport),
    TryOpen(TryOpenReport),
    Import(ImportReport),
    Export(ExportReport),
    Digest(DigestReport),
    Bench(BenchReport),
    StagedCreate(StagedCreateReport),
    Exists(ExistsReport),
    AsciiContract(AsciiContractReport),
    Removed { path: String, existed: bool },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_round_trip_through_json() {
        let commands = vec![
            ProbeCommand::InMemorySmoke,
            ProbeCommand::Reopen {
                path: "muniment-probe/a.redb".into(),
                shape: ChurnShape::default(),
            },
            ProbeCommand::Fault {
                path: "p".into(),
                plan: FaultPlan {
                    short_write_at: Some(3),
                    ..FaultPlan::default()
                },
                commits: 4,
                shape: ChurnShape::default(),
                two_phase_commit: true,
            },
            ProbeCommand::Bench {
                backend: BenchBackend::IndexedDb,
                workload: Workload::LargeBlobs,
                name: "bench".into(),
                reset: true,
            },
        ];
        for command in commands {
            let json = serde_json::to_string(&command).unwrap();
            assert_eq!(
                serde_json::from_str::<ProbeCommand>(&json).unwrap(),
                command
            );
        }
    }

    #[test]
    fn commands_reject_unknown_controls() {
        let json = r#"{"command":"reopen","path":"p","product_default":true}"#;
        assert!(serde_json::from_str::<ProbeCommand>(json).is_err());
    }

    #[test]
    fn reopen_defaults_its_shape() {
        let json = r#"{"command":"reopen","path":"p"}"#;
        let command: ProbeCommand = serde_json::from_str(json).unwrap();
        assert_eq!(
            command,
            ProbeCommand::Reopen {
                path: "p".into(),
                shape: ChurnShape::default()
            }
        );
    }
}
