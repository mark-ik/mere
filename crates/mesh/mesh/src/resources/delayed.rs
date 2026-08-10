// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `mesh.delayed/v1` — the scheduler's rehearsal resource.
//!
//! A deterministic hash chain with a configurable pause between links. It
//! exists so lease lifecycle, cooperative cancellation, and checkpoint control
//! can be proven against a *real* adapter long before a GPU or remote resource
//! is allowed near the registry — which is the gate the lease plan sets.
//!
//! Its output depends only on the input and the unit count, never on how long
//! each unit took, so it still claims [`VerificationClass::ExactBytes`].

use crate::ident::{ImplementationId, ResourceId};
use crate::namespace::{BoxFuture, JobNamespaceView};
use crate::resource::{
    Checkpoint, ControlSignal, JobControl, MeshResource, Prepared, ResourceDescriptor,
    ResourceError,
};
use crate::spec::{ResourceRequirements, VerificationClass};

/// The granted input slot.
pub const PAYLOAD: &str = "payload";

/// A hash chain with cooperative pauses.
pub struct DelayedResource {
    descriptor: ResourceDescriptor,
    units: u64,
    unit_delay_ms: u64,
}

/// What preparation carried into execution.
struct Chain {
    seed: Vec<u8>,
    units: u64,
    unit_delay_ms: u64,
}

impl DelayedResource {
    /// `units` links, pausing `unit_delay_ms` between each. A zero delay still
    /// yields to the runtime at every boundary, so cancellation is observable
    /// without any wall-clock dependence.
    pub fn new(units: u64, unit_delay_ms: u64) -> Self {
        Self {
            descriptor: ResourceDescriptor {
                resource: ResourceId::parse("mesh.delayed/v1").expect("well-formed id"),
                implementation: ImplementationId::parse("mesh.delayed.blake3-chain/v1")
                    .expect("well-formed id"),
                requires: ResourceRequirements::cpu(),
                verification: VerificationClass::ExactBytes,
            },
            units,
            unit_delay_ms,
        }
    }

    /// The answer a run of `units` links over `seed` must produce. Verification
    /// re-runs the adapter; this is here so a test can state the expectation
    /// independently of the thing under test.
    pub fn expected(seed: &[u8], units: u64) -> Vec<u8> {
        let mut state = seed.to_vec();
        for _ in 0..units {
            state = blake3::hash(&state).as_bytes().to_vec();
        }
        state
    }
}

impl Default for DelayedResource {
    fn default() -> Self {
        Self::new(16, 0)
    }
}

impl MeshResource for DelayedResource {
    fn descriptor(&self) -> &ResourceDescriptor {
        &self.descriptor
    }

    fn prepare<'a>(
        &'a self,
        namespace: &'a JobNamespaceView<'a>,
    ) -> BoxFuture<'a, Result<Prepared, ResourceError>> {
        Box::pin(async move {
            Ok(Prepared::new(Chain {
                seed: namespace.read(PAYLOAD).await?,
                units: self.units,
                unit_delay_ms: self.unit_delay_ms,
            }))
        })
    }

    fn execute<'a>(
        &'a self,
        prepared: Prepared,
        control: &'a JobControl,
    ) -> BoxFuture<'a, Result<Vec<u8>, ResourceError>> {
        Box::pin(async move {
            let chain = prepared.take::<Chain>()?;
            let mut state = chain.seed;
            control.report(0, chain.units);
            for unit in 0..chain.units {
                // One cooperative point per link. A checkpoint request stops
                // here and says where; a cancel throws the work away.
                match control.signal() {
                    ControlSignal::Continue => {}
                    ControlSignal::Checkpoint => {
                        control.hold_checkpoint(true);
                        return Err(Checkpoint {
                            completed_units: unit,
                            total_units: chain.units,
                        }
                        .into());
                    }
                    ControlSignal::Cancel => return Err(crate::resource::Cancelled.into()),
                }
                state = blake3::hash(&state).as_bytes().to_vec();
                control.report(unit + 1, chain.units);
                tokio::time::sleep(std::time::Duration::from_millis(chain.unit_delay_ms)).await;
            }
            Ok(state)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::MemoryBlobSpace;
    use crate::registry::{ResourceRegistry, RunError, run_job};
    use crate::spec::{DeterminismClass, JobSpec};
    use std::sync::Arc;

    fn registry(units: u64) -> ResourceRegistry {
        let mut registry = ResourceRegistry::new();
        registry
            .register(Arc::new(DelayedResource::new(units, 0)))
            .unwrap();
        registry
    }

    fn spec(blob: proofs::BlobRef) -> JobSpec {
        JobSpec::simple(
            ResourceId::parse("mesh.delayed/v1").unwrap(),
            PAYLOAD,
            blob,
            "result",
            64,
            DeterminismClass::Exact,
        )
    }

    #[tokio::test]
    async fn the_chain_is_deterministic_and_reports_progress() {
        let registry = registry(8);
        let space = MemoryBlobSpace::in_memory();
        let blob = space.put(b"seed").await.unwrap();
        let (handle, control) = JobControl::new();

        let output = run_job(&registry, &spec(blob), &space, &space, &control)
            .await
            .unwrap();
        assert_eq!(
            space.get(&output.blob).await.unwrap(),
            Some(DelayedResource::expected(b"seed", 8))
        );
        let progress = handle.progress();
        assert_eq!((progress.done, progress.total), (8, 8));
        assert!(!progress.checkpoint_held);
    }

    #[tokio::test]
    async fn a_checkpoint_request_stops_at_a_boundary_and_says_where() {
        let registry = registry(8);
        let space = MemoryBlobSpace::in_memory();
        let blob = space.put(b"seed").await.unwrap();
        let (handle, control) = JobControl::new();
        handle.request_checkpoint();

        let err = run_job(&registry, &spec(blob), &space, &space, &control)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            RunError::Resource(ResourceError::Checkpointed(Checkpoint {
                completed_units: 0,
                total_units: 8
            }))
        ));
        assert!(
            handle.progress().checkpoint_held,
            "the heartbeat can now say a checkpoint exists here"
        );
        assert_eq!(
            space
                .get(&proofs::BlobRef::blake3(&DelayedResource::expected(
                    b"seed", 8
                )))
                .await
                .unwrap(),
            None,
            "an interrupted run commits nothing"
        );
    }

    #[tokio::test]
    async fn a_cancel_after_a_checkpoint_request_still_throws_the_work_away() {
        let registry = registry(8);
        let space = MemoryBlobSpace::in_memory();
        let blob = space.put(b"seed").await.unwrap();
        let (handle, control) = JobControl::new();
        handle.request_checkpoint();
        handle.cancel();

        assert!(matches!(
            run_job(&registry, &spec(blob), &space, &space, &control)
                .await
                .unwrap_err(),
            RunError::Resource(ResourceError::Cancelled(_))
        ));
    }
}
