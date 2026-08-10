// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The one registry, and the one route from a job to its committed output.
//!
//! [`ResourceRegistry`] maps a [`ResourceId`] to the adapter that answers it.
//! It lives above wire/store/sync and below the host actor: the board never
//! learns that adapters exist, and adding a resource touches neither `wire.rs`
//! nor `JobBoard::fold`.
//!
//! [`run_job`] is the whole execution path — validate, match, grant a
//! namespace, prepare, execute under cancellation, commit through the grant.
//! [`run_legacy`] routes M1's inline-payload jobs through that same path over
//! an ephemeral one-input namespace, so the executors have one route rather
//! than two.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::ident::ResourceId;
use crate::namespace::{BlobSink, BlobSource, JobNamespaceView, MemoryBlobSpace, NamespaceError};
use crate::resource::{JobControl, MeshResource, ResourceError};
use crate::resources::{legacy_resource_id, register_builtin};
use crate::spec::{
    DeterminismClass, HostFacts, JobOutput, JobSpec, OutputError, SpecError, VerificationClass,
};
use crate::wire::JobKind;

/// Which resources this device can run.
#[derive(Clone, Default)]
pub struct ResourceRegistry {
    adapters: BTreeMap<ResourceId, Arc<dyn MeshResource>>,
}

impl ResourceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// The registry a device ships with: the two M1 kinds behind adapters, plus
    /// the lexical embedding resource.
    pub fn builtin() -> Self {
        let mut registry = Self::new();
        register_builtin(&mut registry).expect("built-in adapters register cleanly");
        registry
    }

    /// Register one adapter under the resource id it declares.
    pub fn register(&mut self, adapter: Arc<dyn MeshResource>) -> Result<(), RegistryError> {
        let descriptor = adapter.descriptor();
        descriptor.resource.validate()?;
        descriptor.implementation.validate()?;
        let id = descriptor.resource.clone();
        if self.adapters.contains_key(&id) {
            return Err(RegistryError::Duplicate(id));
        }
        self.adapters.insert(id, adapter);
        Ok(())
    }

    pub fn get(&self, resource: &ResourceId) -> Option<&Arc<dyn MeshResource>> {
        self.adapters.get(resource)
    }

    /// Every registered resource, in id order.
    pub fn resources(&self) -> impl Iterator<Item = &ResourceId> {
        self.adapters.keys()
    }

    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }

    /// Whether this device would take the job: it has the adapter, the host
    /// satisfies both the poster's stated requirement and the adapter's own,
    /// and the adapter's verification class is at least what was asked for.
    pub fn offers(&self, spec: &JobSpec, facts: &HostFacts) -> bool {
        let Some(adapter) = self.get(&spec.resource) else {
            return false;
        };
        let descriptor = adapter.descriptor();
        spec.requirements.satisfied_by(facts)
            && descriptor.requires.satisfied_by(facts)
            && descriptor.verification.satisfies(spec.determinism)
    }
}

/// Why an adapter could not be registered.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    #[error("adapter identity: {0}")]
    Ident(#[from] crate::ident::IdentError),
    #[error("resource {0} is already registered")]
    Duplicate(ResourceId),
}

/// Why a run did not produce a committed output.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Spec(#[from] SpecError),
    #[error("no adapter registered for resource {0}")]
    UnknownResource(ResourceId),
    #[error("resource declares {declared:?} against a {asked:?} ask")]
    VerificationTooWeak {
        declared: DeterminismClass,
        asked: DeterminismClass,
    },
    #[error(transparent)]
    Resource(#[from] ResourceError),
    #[error(transparent)]
    Namespace(#[from] NamespaceError),
    #[error(transparent)]
    Output(#[from] OutputError),
}

/// Run one V2 job and commit its output through the granted slot.
///
/// The caller is the host: constructing the namespace here *is* the grant, so
/// authorization must already have been resolved.
pub async fn run_job(
    registry: &ResourceRegistry,
    spec: &JobSpec,
    source: &dyn BlobSource,
    sink: &dyn BlobSink,
    control: &JobControl,
) -> Result<JobOutput, RunError> {
    spec.validate()?;
    let adapter = registry
        .get(&spec.resource)
        .ok_or_else(|| RunError::UnknownResource(spec.resource.clone()))?;
    let descriptor = adapter.descriptor().clone();
    if !descriptor.verification.satisfies(spec.determinism) {
        return Err(RunError::VerificationTooWeak {
            declared: descriptor.verification.class(),
            asked: spec.determinism,
        });
    }

    let view = JobNamespaceView::grant(spec, source, sink);
    let prepared = adapter.prepare(&view).await?;
    control.check().map_err(ResourceError::from)?;
    let bytes = adapter.execute(prepared, control).await?;
    let commit = view.commit(&spec.output.name, &bytes).await?;

    let output = JobOutput {
        name: commit.name,
        blob: commit.blob,
        resource: descriptor.resource,
        implementation: descriptor.implementation,
        verification: descriptor.verification,
    };
    output.validate_against(spec)?;
    Ok(output)
}

/// What a local re-run says about a committed output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The re-run reproduced the committed bytes exactly.
    Reproduced,
    /// The re-run produced different bytes under an exact claim.
    Diverged { rerun: proofs::BlobRef },
    /// The declared class makes no byte-level claim. A tolerant comparison
    /// needs a resource-supplied decoder; the first resource that cannot claim
    /// [`VerificationClass::ExactBytes`] brings one.
    NotCheckable { class: DeterminismClass },
}

/// Re-run a job locally and judge its committed output under the class the
/// producing resource declared.
pub async fn verify_output(
    registry: &ResourceRegistry,
    spec: &JobSpec,
    output: &JobOutput,
    source: &dyn BlobSource,
    sink: &dyn BlobSink,
    control: &JobControl,
) -> Result<Verdict, RunError> {
    output.validate_against(spec)?;
    if output.verification != VerificationClass::ExactBytes {
        return Ok(Verdict::NotCheckable {
            class: output.verification.class(),
        });
    }
    let rerun = run_job(registry, spec, source, sink, control).await?;
    if rerun.blob == output.blob {
        Ok(Verdict::Reproduced)
    } else {
        Ok(Verdict::Diverged { rerun: rerun.blob })
    }
}

/// Run an M1 inline-payload job through the V2 route.
///
/// The payload becomes the single input of an ephemeral, device-local
/// namespace; the committed output is read back out and returned inline so the
/// legacy `JobDone` wire is unchanged. Nothing about this path reaches the
/// device's real blob space.
pub async fn run_legacy(
    registry: &ResourceRegistry,
    kind: JobKind,
    payload: &[u8],
    control: &JobControl,
) -> Result<Vec<u8>, RunError> {
    let scratch = MemoryBlobSpace::in_memory();
    let blob = scratch.put(payload).await?;
    let spec = JobSpec::simple(
        legacy_resource_id(kind),
        "payload",
        blob,
        "result",
        crate::spec::MAX_OUTPUT_BYTES,
        DeterminismClass::Exact,
    );
    let output = run_job(registry, &spec, &scratch, &scratch, control).await?;
    scratch
        .get(&output.blob)
        .await?
        .ok_or_else(|| RunError::Namespace(NamespaceError::MissingBlob("result".to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ident::ImplementationId;
    use crate::namespace::BoxFuture;
    use crate::resource::{Prepared, ResourceDescriptor};
    use crate::spec::{ComputeClass, ResourceRequirements};
    use proofs::BlobRef;

    /// A resource that exists only in this test file — the "adding a resource
    /// changes neither wire.rs nor the fold" receipt.
    struct ReverseResource {
        descriptor: ResourceDescriptor,
    }

    impl ReverseResource {
        fn new() -> Self {
            Self {
                descriptor: ResourceDescriptor {
                    resource: ResourceId::parse("test.reverse/v1").unwrap(),
                    implementation: ImplementationId::parse("test.reverse.std/v1").unwrap(),
                    requires: ResourceRequirements::cpu(),
                    verification: VerificationClass::ExactBytes,
                },
            }
        }
    }

    impl MeshResource for ReverseResource {
        fn descriptor(&self) -> &ResourceDescriptor {
            &self.descriptor
        }

        fn prepare<'a>(
            &'a self,
            namespace: &'a JobNamespaceView<'a>,
        ) -> BoxFuture<'a, Result<Prepared, ResourceError>> {
            Box::pin(async move { Ok(Prepared::new(namespace.read("payload").await?)) })
        }

        fn execute<'a>(
            &'a self,
            prepared: Prepared,
            control: &'a JobControl,
        ) -> BoxFuture<'a, Result<Vec<u8>, ResourceError>> {
            Box::pin(async move {
                control.check()?;
                let mut bytes = prepared.take::<Vec<u8>>()?;
                bytes.reverse();
                Ok(bytes)
            })
        }
    }

    /// A resource that never yields a cross-device claim.
    struct ObservedResource {
        descriptor: ResourceDescriptor,
    }

    impl MeshResource for ObservedResource {
        fn descriptor(&self) -> &ResourceDescriptor {
            &self.descriptor
        }

        fn prepare<'a>(
            &'a self,
            _namespace: &'a JobNamespaceView<'a>,
        ) -> BoxFuture<'a, Result<Prepared, ResourceError>> {
            Box::pin(async move { Ok(Prepared::new(())) })
        }

        fn execute<'a>(
            &'a self,
            _prepared: Prepared,
            _control: &'a JobControl,
        ) -> BoxFuture<'a, Result<Vec<u8>, ResourceError>> {
            Box::pin(async move { Ok(b"whatever this device saw".to_vec()) })
        }
    }

    fn observed() -> ObservedResource {
        ObservedResource {
            descriptor: ResourceDescriptor {
                resource: ResourceId::parse("test.observed/v1").unwrap(),
                implementation: ImplementationId::parse("test.observed.std/v1").unwrap(),
                requires: ResourceRequirements::cpu(),
                verification: VerificationClass::ProducerOnly,
            },
        }
    }

    fn reverse_spec(blob: BlobRef) -> JobSpec {
        JobSpec::simple(
            ResourceId::parse("test.reverse/v1").unwrap(),
            "payload",
            blob,
            "result",
            64,
            DeterminismClass::Exact,
        )
    }

    #[tokio::test]
    async fn a_test_resource_runs_end_to_end_without_touching_wire_or_fold() {
        let mut registry = ResourceRegistry::new();
        registry.register(Arc::new(ReverseResource::new())).unwrap();
        let space = MemoryBlobSpace::in_memory();
        let blob = space.put(b"abcdef").await.unwrap();
        let spec = reverse_spec(blob);
        let (_handle, control) = JobControl::new();

        let output = run_job(&registry, &spec, &space, &space, &control)
            .await
            .unwrap();
        assert_eq!(output.name, "result");
        assert_eq!(output.blob, BlobRef::blake3(b"fedcba"));
        assert_eq!(
            space.get(&output.blob).await.unwrap(),
            Some(b"fedcba".to_vec())
        );

        let verdict = verify_output(&registry, &spec, &output, &space, &space, &control)
            .await
            .unwrap();
        assert_eq!(verdict, Verdict::Reproduced);
    }

    #[tokio::test]
    async fn an_unknown_resource_and_a_duplicate_registration_are_refused() {
        let mut registry = ResourceRegistry::new();
        registry.register(Arc::new(ReverseResource::new())).unwrap();
        assert_eq!(
            registry.register(Arc::new(ReverseResource::new())),
            Err(RegistryError::Duplicate(
                ResourceId::parse("test.reverse/v1").unwrap()
            ))
        );

        let space = MemoryBlobSpace::in_memory();
        let blob = space.put(b"x").await.unwrap();
        let mut spec = reverse_spec(blob);
        spec.resource = ResourceId::parse("test.absent/v1").unwrap();
        let (_handle, control) = JobControl::new();
        assert!(matches!(
            run_job(&registry, &spec, &space, &space, &control).await,
            Err(RunError::UnknownResource(_))
        ));
    }

    #[tokio::test]
    async fn a_weaker_resource_may_not_answer_a_stronger_ask() {
        let mut registry = ResourceRegistry::new();
        registry.register(Arc::new(observed())).unwrap();
        let space = MemoryBlobSpace::in_memory();
        let blob = space.put(b"x").await.unwrap();
        let mut spec = reverse_spec(blob);
        spec.resource = ResourceId::parse("test.observed/v1").unwrap();
        let (_handle, control) = JobControl::new();

        assert!(matches!(
            run_job(&registry, &spec, &space, &space, &control).await,
            Err(RunError::VerificationTooWeak { .. })
        ));
        assert!(!registry.offers(&spec, &HostFacts::cpu(1024)));

        spec.determinism = DeterminismClass::Observed;
        assert!(registry.offers(&spec, &HostFacts::cpu(1024)));
        let output = run_job(&registry, &spec, &space, &space, &control)
            .await
            .unwrap();
        assert_eq!(
            verify_output(&registry, &spec, &output, &space, &space, &control)
                .await
                .unwrap(),
            Verdict::NotCheckable {
                class: DeterminismClass::Observed
            }
        );
    }

    #[tokio::test]
    async fn a_cancelled_run_commits_nothing() {
        let mut registry = ResourceRegistry::new();
        registry.register(Arc::new(ReverseResource::new())).unwrap();
        let space = MemoryBlobSpace::in_memory();
        let blob = space.put(b"abcdef").await.unwrap();
        let spec = reverse_spec(blob);
        let (handle, control) = JobControl::new();
        handle.cancel();

        assert!(matches!(
            run_job(&registry, &spec, &space, &space, &control).await,
            Err(RunError::Resource(ResourceError::Cancelled(_)))
        ));
        assert_eq!(
            space.get(&BlobRef::blake3(b"fedcba")).await.unwrap(),
            None,
            "a cancelled run leaves no output in the blob space"
        );
    }

    #[tokio::test]
    async fn host_capability_gates_selection() {
        let registry = ResourceRegistry::builtin();
        let space = MemoryBlobSpace::in_memory();
        let blob = space.put(b"x").await.unwrap();
        let mut spec = JobSpec::simple(
            legacy_resource_id(JobKind::Echo),
            "payload",
            blob,
            "result",
            64,
            DeterminismClass::Exact,
        );
        assert!(registry.offers(&spec, &HostFacts::cpu(1024)));

        spec.requirements = ResourceRequirements {
            memory_mib: 0,
            compute: ComputeClass::Gpu,
        };
        assert!(
            !registry.offers(&spec, &HostFacts::cpu(1024)),
            "a CPU-only device does not take a GPU job"
        );
    }

    #[tokio::test]
    async fn the_m1_kinds_run_through_the_v2_route() {
        let registry = ResourceRegistry::builtin();
        let (_handle, control) = JobControl::new();
        assert_eq!(
            run_legacy(&registry, JobKind::Echo, b"abc", &control)
                .await
                .unwrap(),
            b"abc".to_vec()
        );
        let hashed = run_legacy(&registry, JobKind::Blake3, b"abc", &control)
            .await
            .unwrap();
        assert_eq!(hashed, blake3::hash(b"abc").as_bytes().to_vec());
    }
}
