// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Background-worker orchestration trait per the framing brief §5.7.
//!
//! Sessions *declare* which workers they want running via
//! `GraphSessionManifest.active_workers: Vec<WorkerKind>`. A
//! [`SessionServiceRunner`] implementation owns the actual workers
//! and answers start / stop / list queries against them.
//!
//! v0a (this module) ships the trait, the worker-status vocabulary,
//! a [`NullRunner`] no-op, and an [`InMemoryRunner`] test double.
//! Real worker implementations (FetcherPool first) land as their
//! workloads materialise; see the
//! [SessionServiceRunner plan](../../../../design_docs/mere_docs/implementation_strategy/2026-05-14_session_service_runner_plan.md).
//!
//! The trait is intentionally synchronous and accepts owned ids so a
//! future `RemoteRunner` (over IPC, per framing brief §5.9) can match
//! the shape without rewriting session semantics — only the
//! transport.

use std::collections::HashMap;

use incipit::SessionId;

use crate::manifest::WorkerKind;

/// Opaque handle minted by a [`SessionServiceRunner`] when a worker
/// is started. Callers don't construct these; they identify a
/// running worker for later `stop_worker` / status queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WorkerHandle(pub u64);

/// Lifecycle stage a worker is in. v0a covers the common cases;
/// crash / restart semantics deferred to v0b (see plan §8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerState {
    /// Worker is actively servicing its workload.
    Running,
    /// Worker has been asked to stop but is still draining.
    Stopping,
    /// Worker has stopped cleanly. Status entries usually drop on
    /// stop; `Stopped` is exposed so a runner that retains history
    /// can report it.
    Stopped,
}

/// Snapshot of one worker for [`SessionServiceRunner::workers_for`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerStatus {
    pub session_id: SessionId,
    pub handle: WorkerHandle,
    pub kind: WorkerKind,
    pub state: WorkerState,
}

/// Why a runner refused to start a worker. Value enum (not freeform
/// strings) so a remote runner can wire failures across IPC without
/// freeform-text leaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerStartError {
    /// This runner doesn't know how to service the requested kind.
    /// The kernel-vocabulary `WorkerKind` lives ahead of every
    /// runner's coverage; `Unsupported` is the explicit way to say
    /// "the manifest asked for this, I can't provide it."
    Unsupported,
    /// The session already has a worker of this kind running and the
    /// runner enforces uniqueness.
    AlreadyRunning,
    /// Runner-specific resource constraint (worker pool saturated,
    /// model unavailable, etc.). Concrete runners refine this in
    /// v0b; v0a uses it as a catch-all.
    ResourceUnavailable,
}

/// Why a runner refused to stop a worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerStopError {
    /// The handle doesn't correspond to a known worker.
    UnknownHandle,
}

/// Background-worker orchestration. The host owns the runner; the
/// kernel never reaches for one.
///
/// The trait is sync — workers may run async internally, but
/// `start_worker` returns once the task is *spawned*, not when its
/// work completes. Real runners orchestrate Tokio tasks behind this
/// surface.
pub trait SessionServiceRunner {
    fn start_worker(
        &mut self,
        session_id: SessionId,
        kind: WorkerKind,
    ) -> Result<WorkerHandle, WorkerStartError>;

    fn stop_worker(&mut self, handle: WorkerHandle) -> Result<(), WorkerStopError>;

    fn workers_for(&self, session_id: SessionId) -> Vec<WorkerStatus>;
}

/// No-op runner. Threads through `HostRoot` before a real runner
/// lands. `start_worker` always rejects with `Unsupported`;
/// `workers_for` is empty.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullRunner;

impl SessionServiceRunner for NullRunner {
    fn start_worker(
        &mut self,
        _session_id: SessionId,
        _kind: WorkerKind,
    ) -> Result<WorkerHandle, WorkerStartError> {
        Err(WorkerStartError::Unsupported)
    }

    fn stop_worker(&mut self, _handle: WorkerHandle) -> Result<(), WorkerStopError> {
        Err(WorkerStopError::UnknownHandle)
    }

    fn workers_for(&self, _session_id: SessionId) -> Vec<WorkerStatus> {
        Vec::new()
    }
}

/// Test-double runner that ledgers start/stop calls in-memory and
/// hands out handles. Used by the trait tests and re-exported so
/// downstream test crates can share the shape when they write real
/// runner tests.
///
/// `supported_kinds` controls which `WorkerKind` values this runner
/// will service; everything else is `Unsupported`. Empty set ⇒
/// behaves like `NullRunner`.
#[derive(Clone, Debug, Default)]
pub struct InMemoryRunner {
    next_handle: u64,
    workers: HashMap<WorkerHandle, WorkerStatus>,
    supported_kinds: std::collections::HashSet<WorkerKind>,
}

impl InMemoryRunner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-declare a kind this runner will service. Calls to
    /// `start_worker` with a kind not in this set return
    /// `WorkerStartError::Unsupported`.
    pub fn with_supported(mut self, kind: WorkerKind) -> Self {
        self.supported_kinds.insert(kind);
        self
    }

    /// True when this runner is currently tracking the supplied
    /// worker handle. Test helper.
    pub fn knows(&self, handle: WorkerHandle) -> bool {
        self.workers.contains_key(&handle)
    }
}

impl SessionServiceRunner for InMemoryRunner {
    fn start_worker(
        &mut self,
        session_id: SessionId,
        kind: WorkerKind,
    ) -> Result<WorkerHandle, WorkerStartError> {
        if !self.supported_kinds.contains(&kind) {
            return Err(WorkerStartError::Unsupported);
        }
        // Test runner doesn't enforce per-session uniqueness — real
        // runners will refuse duplicates with AlreadyRunning, but
        // the contract test verifies that path with a separate
        // configurable double if needed.
        self.next_handle += 1;
        let handle = WorkerHandle(self.next_handle);
        self.workers.insert(
            handle,
            WorkerStatus {
                session_id,
                handle,
                kind,
                state: WorkerState::Running,
            },
        );
        Ok(handle)
    }

    fn stop_worker(&mut self, handle: WorkerHandle) -> Result<(), WorkerStopError> {
        if self.workers.remove(&handle).is_none() {
            return Err(WorkerStopError::UnknownHandle);
        }
        Ok(())
    }

    fn workers_for(&self, session_id: SessionId) -> Vec<WorkerStatus> {
        let mut out: Vec<WorkerStatus> = self
            .workers
            .values()
            .filter(|w| w.session_id == session_id)
            .cloned()
            .collect();
        out.sort_by_key(|w| w.handle);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn session(idx: u128) -> SessionId {
        SessionId(Uuid::from_u128(idx))
    }

    #[test]
    fn null_runner_rejects_everything() {
        let mut runner = NullRunner;
        assert_eq!(
            runner.start_worker(session(1), WorkerKind::FetcherPool),
            Err(WorkerStartError::Unsupported)
        );
        assert_eq!(
            runner.stop_worker(WorkerHandle(0)),
            Err(WorkerStopError::UnknownHandle)
        );
        assert!(runner.workers_for(session(1)).is_empty());
    }

    #[test]
    fn in_memory_runner_supports_explicitly_declared_kinds() {
        let mut runner = InMemoryRunner::new().with_supported(WorkerKind::FetcherPool);
        let h = runner
            .start_worker(session(1), WorkerKind::FetcherPool)
            .expect("FetcherPool is supported");
        assert!(runner.knows(h));
        assert_eq!(
            runner.start_worker(session(1), WorkerKind::Indexer),
            Err(WorkerStartError::Unsupported),
            "Indexer wasn't declared supported"
        );
    }

    #[test]
    fn start_then_stop_releases_handle() {
        let mut runner = InMemoryRunner::new().with_supported(WorkerKind::Embedder);
        let h = runner
            .start_worker(session(1), WorkerKind::Embedder)
            .unwrap();
        assert!(runner.knows(h));
        runner.stop_worker(h).unwrap();
        assert!(!runner.knows(h));
        assert!(runner.workers_for(session(1)).is_empty());
    }

    #[test]
    fn stop_unknown_handle_errors_cleanly() {
        let mut runner = InMemoryRunner::new();
        assert_eq!(
            runner.stop_worker(WorkerHandle(9999)),
            Err(WorkerStopError::UnknownHandle)
        );
    }

    #[test]
    fn workers_for_filters_by_session() {
        let mut runner = InMemoryRunner::new()
            .with_supported(WorkerKind::FetcherPool)
            .with_supported(WorkerKind::Indexer);
        let h1 = runner
            .start_worker(session(1), WorkerKind::FetcherPool)
            .unwrap();
        let h2 = runner
            .start_worker(session(2), WorkerKind::Indexer)
            .unwrap();
        let h3 = runner
            .start_worker(session(1), WorkerKind::Indexer)
            .unwrap();

        let s1 = runner.workers_for(session(1));
        assert_eq!(s1.len(), 2);
        assert!(
            s1.iter()
                .any(|w| w.handle == h1 && w.kind == WorkerKind::FetcherPool)
        );
        assert!(
            s1.iter()
                .any(|w| w.handle == h3 && w.kind == WorkerKind::Indexer)
        );

        let s2 = runner.workers_for(session(2));
        assert_eq!(s2.len(), 1);
        assert_eq!(s2[0].handle, h2);
    }

    #[test]
    fn two_sessions_can_run_the_same_kind() {
        let mut runner = InMemoryRunner::new().with_supported(WorkerKind::FetcherPool);
        let h1 = runner
            .start_worker(session(1), WorkerKind::FetcherPool)
            .unwrap();
        let h2 = runner
            .start_worker(session(2), WorkerKind::FetcherPool)
            .unwrap();
        assert_ne!(h1, h2);
        assert_eq!(runner.workers_for(session(1)).len(), 1);
        assert_eq!(runner.workers_for(session(2)).len(), 1);
    }

    #[test]
    fn worker_status_carries_state_running_after_start() {
        let mut runner = InMemoryRunner::new().with_supported(WorkerKind::Embedder);
        let h = runner
            .start_worker(session(1), WorkerKind::Embedder)
            .unwrap();
        let status = runner.workers_for(session(1))[0].clone();
        assert_eq!(status.handle, h);
        assert_eq!(status.state, WorkerState::Running);
        assert_eq!(status.kind, WorkerKind::Embedder);
        assert_eq!(status.session_id, session(1));
    }

    #[test]
    fn worker_kind_serde_round_trips_for_every_variant() {
        for kind in [
            WorkerKind::None,
            WorkerKind::FetcherPool,
            WorkerKind::Embedder,
            WorkerKind::Indexer,
            WorkerKind::IntelligenceSignalProducer,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            let back: WorkerKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, kind, "WorkerKind::{kind:?} did not round-trip");
        }
    }
}
