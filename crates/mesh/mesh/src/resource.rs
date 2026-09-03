// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The resource adapter seam: what a mesh resource is, and how it is driven.
//!
//! An adapter supplies stable identities, the capability it needs, preparation
//! through the restricted namespace, execution under a host-owned control
//! handle, and the verification class its output may claim. It does **not**
//! read the mesh store, inspect the OS, choose a device, or mutate the board —
//! and it does not write its own output either: [`execute`](MeshResource::execute)
//! returns bytes, and the runner commits them through the grant.
//!
//! Both phases are async on purpose. Blob access is async, so a synchronous
//! seam would force a `block_on` inside an adapter; and a long-running resource
//! arriving with the M3 scheduler must not require a breaking rewrite here.

use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

use crate::ident::{ImplementationId, ResourceId};
use crate::lease::{LeaseActivity, LeaseProgress};
use crate::namespace::{BoxFuture, JobNamespaceView, NamespaceError};
use crate::spec::{ResourceRequirements, VerificationClass};
use crate::{JobId, LeaseId};

/// Who this resource is, what it needs, and what its output may claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceDescriptor {
    /// The resource a job asks for.
    pub resource: ResourceId,
    /// The build that answers it — the identity a verifier re-runs against.
    pub implementation: ImplementationId,
    /// What this implementation needs from a host, independent of what a
    /// poster guessed the job would need.
    pub requires: ResourceRequirements,
    /// The strongest claim this implementation's output can carry. Earned by
    /// receipt, not asserted (see the crate's lexical determinism test).
    pub verification: VerificationClass,
}

/// Mesh identity of one supervised resource run.
///
/// Most resources need only their granted namespace. A resident service such
/// as remote compute also has to bind an external session to the exact run the
/// host can cancel. The host supplies this context; the job never does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunContext {
    /// Job whose granted namespace is being executed.
    pub job: JobId,
    /// Live lease the host bound this run to, when the job is leased.
    pub lease: Option<LeaseId>,
}

/// Whatever an adapter carried from preparation into execution.
///
/// Opaque so the seam stays object-safe: the registry moves the box, only the
/// adapter that made it looks inside.
pub struct Prepared(Box<dyn Any + Send>);

impl Prepared {
    pub fn new<T: Any + Send>(value: T) -> Self {
        Self(Box::new(value))
    }

    /// Recover what [`new`](Self::new) stored. A mismatch means an adapter's
    /// two halves disagree — a bug in the adapter, not in a job.
    pub fn take<T: Any + Send>(self) -> Result<T, ResourceError> {
        self.0
            .downcast::<T>()
            .map(|boxed| *boxed)
            .map_err(|_| ResourceError::PreparedTypeMismatch)
    }
}

impl std::fmt::Debug for Prepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Prepared(..)")
    }
}

/// What the host is asking a running job to do. Escalating: once a run has been
/// cancelled, a later checkpoint request cannot walk it back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ControlSignal {
    /// Keep going.
    Continue,
    /// Reach a checkpoint boundary and stop there, reporting how far you got.
    Checkpoint,
    /// Stop now. Nothing is preserved.
    Cancel,
}

/// Shared between the host's handle and the run's control.
#[derive(Debug)]
struct ControlState {
    signal: AtomicU8,
    done: AtomicU64,
    total: AtomicU64,
    checkpoint_held: AtomicBool,
    activity: AtomicU8,
}

impl ControlState {
    fn escalate(&self, to: u8) {
        self.signal.fetch_max(to, Ordering::SeqCst);
    }

    fn signal(&self) -> ControlSignal {
        match self.signal.load(Ordering::SeqCst) {
            0 => ControlSignal::Continue,
            1 => ControlSignal::Checkpoint,
            _ => ControlSignal::Cancel,
        }
    }

    fn progress(&self) -> LeaseProgress {
        LeaseProgress {
            done: self.done.load(Ordering::SeqCst),
            total: self.total.load(Ordering::SeqCst),
            checkpoint_held: self.checkpoint_held.load(Ordering::SeqCst),
            activity: match self.activity.load(Ordering::SeqCst) {
                0 => LeaseActivity::Fetching,
                1 => LeaseActivity::Preparing,
                3 => LeaseActivity::Checkpointing,
                _ => LeaseActivity::Running,
            },
        }
    }
}

/// The host's side of execution control: keep this, hand the [`JobControl`] to
/// the run. Dropping the handle does not cancel — a lost host is an owner-reclaim
/// problem, not something a worker should infer from a dropped value.
#[derive(Clone, Debug)]
pub struct JobControlHandle {
    state: Arc<ControlState>,
}

impl JobControlHandle {
    /// Stop the run at its next cooperative point, preserving nothing.
    pub fn cancel(&self) {
        self.state.escalate(2);
    }

    /// Ask the run to stop at a checkpoint boundary instead. Ignored if the run
    /// has already been cancelled.
    pub fn request_checkpoint(&self) {
        self.state.escalate(1);
    }

    pub fn signal(&self) -> ControlSignal {
        self.state.signal()
    }

    /// What the run last reported — the heartbeat's payload.
    pub fn progress(&self) -> LeaseProgress {
        self.state.progress()
    }
}

/// The run's side of execution control. Cooperative: an adapter calls
/// [`check`](Self::check) or [`signal`](Self::signal) between units of work.
#[derive(Clone, Debug)]
pub struct JobControl {
    state: Arc<ControlState>,
}

impl JobControl {
    /// A fresh handle/control pair. The host keeps the handle.
    pub fn new() -> (JobControlHandle, Self) {
        let state = Arc::new(ControlState {
            signal: AtomicU8::new(0),
            done: AtomicU64::new(0),
            total: AtomicU64::new(0),
            checkpoint_held: AtomicBool::new(false),
            activity: AtomicU8::new(2),
        });
        (
            JobControlHandle {
                state: state.clone(),
            },
            Self { state },
        )
    }

    pub fn signal(&self) -> ControlSignal {
        self.state.signal()
    }

    pub fn is_cancelled(&self) -> bool {
        self.signal() == ControlSignal::Cancel
    }

    /// The cooperative cancellation point, for a resource with nothing to
    /// preserve. A checkpoint request is not an error here — a resource that
    /// cannot checkpoint simply keeps going until cancelled.
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            return Err(Cancelled);
        }
        Ok(())
    }

    /// Publish progress for the host to heartbeat.
    pub fn report(&self, done: u64, total: u64) {
        self.state.done.store(done, Ordering::SeqCst);
        self.state.total.store(total, Ordering::SeqCst);
    }

    /// Say which phase this run is in. A device fetching a job's inputs is not
    /// a device that has stalled, and only the run knows the difference.
    pub fn set_activity(&self, activity: LeaseActivity) {
        let code = match activity {
            LeaseActivity::Fetching => 0,
            LeaseActivity::Preparing => 1,
            LeaseActivity::Running => 2,
            LeaseActivity::Checkpointing => 3,
        };
        self.state.activity.store(code, Ordering::SeqCst);
    }

    /// Declare whether a resumable checkpoint now exists **on this device**.
    pub fn hold_checkpoint(&self, held: bool) {
        self.state.checkpoint_held.store(held, Ordering::SeqCst);
    }

    pub fn progress(&self) -> LeaseProgress {
        self.state.progress()
    }
}

/// The run was asked to stop, with nothing preserved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("job was cancelled")]
pub struct Cancelled;

/// The run stopped at a checkpoint boundary it can name. The checkpoint itself
/// is local to this device; another device still starts from nothing until a
/// blob lane exists to carry it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("job stopped at checkpoint {completed_units}/{total_units}")]
pub struct Checkpoint {
    pub completed_units: u64,
    pub total_units: u64,
}

/// Why an adapter could not produce a result.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ResourceError {
    #[error(transparent)]
    Namespace(#[from] NamespaceError),
    #[error("input {name:?} is not valid for this resource: {reason}")]
    Input { name: String, reason: String },
    #[error(transparent)]
    Cancelled(#[from] Cancelled),
    #[error(transparent)]
    Checkpointed(#[from] Checkpoint),
    #[error("resource backend: {0}")]
    Backend(String),
    #[error("adapter's prepared state does not match its execute half")]
    PreparedTypeMismatch,
}

impl ResourceError {
    pub fn input(name: &str, reason: impl Into<String>) -> Self {
        Self::Input {
            name: name.to_string(),
            reason: reason.into(),
        }
    }
}

/// One resource the mesh can run.
pub trait MeshResource: Send + Sync {
    fn descriptor(&self) -> &ResourceDescriptor;

    /// Read and validate everything the run needs — through the namespace and
    /// nowhere else.
    fn prepare<'a>(
        &'a self,
        namespace: &'a JobNamespaceView<'a>,
    ) -> BoxFuture<'a, Result<Prepared, ResourceError>>;

    /// Compute the output bytes, yielding to `control` between units of work.
    /// The runner commits the return value through the granted output slot.
    fn execute<'a>(
        &'a self,
        prepared: Prepared,
        control: &'a JobControl,
    ) -> BoxFuture<'a, Result<Vec<u8>, ResourceError>>;

    /// Execute with the host-authored identity of this run.
    ///
    /// Ordinary adapters inherit the namespace-only behavior. Resources that
    /// supervise an external session override this method so the session and
    /// the host cancellation handle name the same job and lease.
    fn execute_for<'a>(
        &'a self,
        prepared: Prepared,
        control: &'a JobControl,
        _context: RunContext,
    ) -> BoxFuture<'a, Result<Vec<u8>, ResourceError>> {
        self.execute(prepared, control)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_round_trips_its_own_type_and_refuses_another() {
        let prepared = Prepared::new(vec![1u8, 2, 3]);
        assert_eq!(prepared.take::<Vec<u8>>().unwrap(), vec![1, 2, 3]);
        let prepared = Prepared::new(7u32);
        assert_eq!(
            prepared.take::<String>().unwrap_err(),
            ResourceError::PreparedTypeMismatch
        );
    }

    #[test]
    fn cancellation_is_visible_to_the_run_and_only_the_host_sets_it() {
        let (handle, control) = JobControl::new();
        assert!(control.check().is_ok());
        assert!(!control.is_cancelled());
        handle.cancel();
        assert!(control.is_cancelled());
        assert_eq!(control.check(), Err(Cancelled));
        // A clone of the control observes the same flag: the host cancels one
        // run, not one copy of it.
        assert!(control.clone().is_cancelled());
    }

    #[test]
    fn signals_escalate_and_never_walk_back() {
        let (handle, control) = JobControl::new();
        assert_eq!(control.signal(), ControlSignal::Continue);
        handle.request_checkpoint();
        assert_eq!(control.signal(), ControlSignal::Checkpoint);
        assert!(
            control.check().is_ok(),
            "a checkpoint request is not a cancellation"
        );
        handle.cancel();
        assert_eq!(control.signal(), ControlSignal::Cancel);
        handle.request_checkpoint();
        assert_eq!(
            control.signal(),
            ControlSignal::Cancel,
            "a late checkpoint request cannot un-cancel a run"
        );
    }

    #[test]
    fn the_run_reports_progress_the_host_can_heartbeat() {
        let (handle, control) = JobControl::new();
        assert_eq!(handle.progress(), LeaseProgress::default());
        control.report(3, 10);
        control.hold_checkpoint(true);
        assert_eq!(
            handle.progress(),
            LeaseProgress {
                done: 3,
                total: 10,
                checkpoint_held: true,
                activity: LeaseActivity::Running,
            }
        );
    }

    #[test]
    fn dropping_the_handle_does_not_cancel_the_run() {
        let (handle, control) = JobControl::new();
        drop(handle);
        assert!(control.check().is_ok());
    }
}
