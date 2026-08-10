// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The two M1 kinds, behind adapters.
//!
//! `Echo` and `Blake3` used to be a `match` in the worker. They are resources
//! now, so the inline-payload wire and the V2 wire share one execution route
//! instead of two.

use crate::ident::{ImplementationId, ResourceId};
use crate::namespace::{BoxFuture, JobNamespaceView};
use crate::resource::{JobControl, MeshResource, Prepared, ResourceDescriptor, ResourceError};
use crate::spec::{ResourceRequirements, VerificationClass};

/// The slot both M1 adapters read. The legacy compatibility path binds the
/// inline payload to this name in an ephemeral namespace.
pub const PAYLOAD: &str = "payload";

/// Return the payload unchanged — the round-trip proof.
pub struct EchoResource {
    descriptor: ResourceDescriptor,
}

/// Return the BLAKE3 hash of the payload.
pub struct Blake3Resource {
    descriptor: ResourceDescriptor,
}

impl EchoResource {
    pub fn new() -> Self {
        Self {
            descriptor: descriptor("mesh.echo/v1", "mesh.echo.identity/v1"),
        }
    }
}

impl Default for EchoResource {
    fn default() -> Self {
        Self::new()
    }
}

impl Blake3Resource {
    pub fn new() -> Self {
        Self {
            descriptor: descriptor("mesh.blake3/v1", "mesh.blake3.blake3-1/v1"),
        }
    }
}

impl Default for Blake3Resource {
    fn default() -> Self {
        Self::new()
    }
}

fn descriptor(resource: &str, implementation: &str) -> ResourceDescriptor {
    ResourceDescriptor {
        resource: ResourceId::parse(resource).expect("built-in resource id is well formed"),
        implementation: ImplementationId::parse(implementation)
            .expect("built-in implementation id is well formed"),
        requires: ResourceRequirements::cpu(),
        // Pure byte functions: any conforming device reproduces the bytes.
        verification: VerificationClass::ExactBytes,
    }
}

/// Both adapters read the same single slot, so preparation is shared.
fn read_payload<'a>(
    namespace: &'a JobNamespaceView<'a>,
) -> BoxFuture<'a, Result<Prepared, ResourceError>> {
    Box::pin(async move { Ok(Prepared::new(namespace.read(PAYLOAD).await?)) })
}

impl MeshResource for EchoResource {
    fn descriptor(&self) -> &ResourceDescriptor {
        &self.descriptor
    }

    fn prepare<'a>(
        &'a self,
        namespace: &'a JobNamespaceView<'a>,
    ) -> BoxFuture<'a, Result<Prepared, ResourceError>> {
        read_payload(namespace)
    }

    fn execute<'a>(
        &'a self,
        prepared: Prepared,
        control: &'a JobControl,
    ) -> BoxFuture<'a, Result<Vec<u8>, ResourceError>> {
        Box::pin(async move {
            control.check()?;
            prepared.take::<Vec<u8>>()
        })
    }
}

impl MeshResource for Blake3Resource {
    fn descriptor(&self) -> &ResourceDescriptor {
        &self.descriptor
    }

    fn prepare<'a>(
        &'a self,
        namespace: &'a JobNamespaceView<'a>,
    ) -> BoxFuture<'a, Result<Prepared, ResourceError>> {
        read_payload(namespace)
    }

    fn execute<'a>(
        &'a self,
        prepared: Prepared,
        control: &'a JobControl,
    ) -> BoxFuture<'a, Result<Vec<u8>, ResourceError>> {
        Box::pin(async move {
            control.check()?;
            let payload = prepared.take::<Vec<u8>>()?;
            Ok(blake3::hash(&payload).as_bytes().to_vec())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::MemoryBlobSpace;
    use crate::registry::{ResourceRegistry, run_job};
    use crate::spec::{DeterminismClass, JobSpec};
    use std::sync::Arc;

    #[tokio::test]
    async fn the_adapters_reproduce_the_m1_executors() {
        let mut registry = ResourceRegistry::new();
        registry.register(Arc::new(EchoResource::new())).unwrap();
        registry.register(Arc::new(Blake3Resource::new())).unwrap();
        let space = MemoryBlobSpace::in_memory();
        let blob = space.put(b"abc").await.unwrap();
        let (_handle, control) = JobControl::new();

        for (resource, expected) in [
            ("mesh.echo/v1", b"abc".to_vec()),
            ("mesh.blake3/v1", blake3::hash(b"abc").as_bytes().to_vec()),
        ] {
            let spec = JobSpec::simple(
                ResourceId::parse(resource).unwrap(),
                PAYLOAD,
                blob.clone(),
                "result",
                64,
                DeterminismClass::Exact,
            );
            let output = run_job(&registry, &spec, &space, &space, &control)
                .await
                .unwrap();
            assert_eq!(
                space.get(&output.blob).await.unwrap(),
                Some(expected),
                "{resource}"
            );
        }
    }

    #[tokio::test]
    async fn an_adapter_cannot_read_a_slot_the_job_did_not_grant() {
        let mut registry = ResourceRegistry::new();
        registry.register(Arc::new(EchoResource::new())).unwrap();
        let space = MemoryBlobSpace::in_memory();
        let blob = space.put(b"abc").await.unwrap();
        let mut spec = JobSpec::simple(
            ResourceId::parse("mesh.echo/v1").unwrap(),
            "not-payload",
            blob,
            "result",
            64,
            DeterminismClass::Exact,
        );
        spec.resource = ResourceId::parse("mesh.echo/v1").unwrap();
        let (_handle, control) = JobControl::new();
        assert!(
            run_job(&registry, &spec, &space, &space, &control)
                .await
                .is_err(),
            "the adapter asks for `payload`; this job granted another name"
        );
    }
}
