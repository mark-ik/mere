// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

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
use std::sync::atomic::{AtomicBool, Ordering};

use crate::ident::{ImplementationId, ResourceId};
use crate::namespace::{BoxFuture, JobNamespaceView, NamespaceError};
use crate::spec::{ResourceRequirements, VerificationClass};

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

/// The host's side of cancellation: keep this, hand the [`JobControl`] to the
/// run. Dropping the handle does not cancel — a lost host is M3's owner-reclaim
/// problem, not something a worker should infer.
#[derive(Clone, Debug)]
pub struct JobControlHandle {
    cancelled: Arc<AtomicBool>,
}

impl JobControlHandle {
    /// Ask the run to stop at its next cooperative point.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
}

/// The run's side of cancellation. Cooperative: an adapter calls
/// [`check`](Self::check) between units of work.
#[derive(Clone, Debug)]
pub struct JobControl {
    cancelled: Arc<AtomicBool>,
}

impl JobControl {
    /// A fresh handle/control pair. The host keeps the handle.
    pub fn new() -> (JobControlHandle, Self) {
        let cancelled = Arc::new(AtomicBool::new(false));
        (
            JobControlHandle {
                cancelled: cancelled.clone(),
            },
            Self { cancelled },
        )
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// The cooperative cancellation point.
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            return Err(Cancelled);
        }
        Ok(())
    }
}

/// The run was asked to stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("job was cancelled")]
pub struct Cancelled;

/// Why an adapter could not produce a result.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ResourceError {
    #[error(transparent)]
    Namespace(#[from] NamespaceError),
    #[error("input {name:?} is not valid for this resource: {reason}")]
    Input { name: String, reason: String },
    #[error(transparent)]
    Cancelled(#[from] Cancelled),
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
    fn dropping_the_handle_does_not_cancel_the_run() {
        let (handle, control) = JobControl::new();
        drop(handle);
        assert!(control.check().is_ok());
    }
}
