// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `esp.embed.lexical/v1` — the first useful mesh resource.
//!
//! Feature-hashed lexical embeddings from
//! [`esp::embed::LexicalEmbeddingProvider`]: a cheap shared-vocabulary signal
//! for clustering and recall rehearsals, with no Burn, no GPU, no weights, and
//! no tokenizer assets. ESP owns the embedding behaviour; this file owns only
//! the namespace-shaped seam around it.
//!
//! The declared [`VerificationClass::ExactBytes`] is a claim about *every*
//! device, so it is earned by receipt rather than asserted — see
//! `lexical_determinism_receipt` below and the wasm probe it pins.

use crate::ident::{ImplementationId, ResourceId};
use crate::namespace::{BoxFuture, JobNamespaceView};
use crate::resource::{JobControl, MeshResource, Prepared, ResourceDescriptor, ResourceError};
use crate::resources::lexical_codec::{LexicalBatch, embed_batch};
use crate::spec::{ResourceRequirements, VerificationClass};

/// The granted input slot: a canonical [`LexicalBatch`].
pub const TEXTS: &str = "texts";
/// The conventional output slot for a canonical vector block.
pub const VECTORS: &str = "vectors";

/// The lexical embedding adapter.
pub struct LexicalEmbedResource {
    descriptor: ResourceDescriptor,
}

impl LexicalEmbedResource {
    pub fn new() -> Self {
        Self {
            descriptor: ResourceDescriptor {
                resource: ResourceId::parse("esp.embed.lexical/v1").expect("well-formed id"),
                implementation: ImplementationId::parse("mesh.lexical.fnv1a/v1")
                    .expect("well-formed id"),
                requires: ResourceRequirements::cpu(),
                verification: VerificationClass::ExactBytes,
            },
        }
    }
}

impl Default for LexicalEmbedResource {
    fn default() -> Self {
        Self::new()
    }
}

impl MeshResource for LexicalEmbedResource {
    fn descriptor(&self) -> &ResourceDescriptor {
        &self.descriptor
    }

    fn prepare<'a>(
        &'a self,
        namespace: &'a JobNamespaceView<'a>,
    ) -> BoxFuture<'a, Result<Prepared, ResourceError>> {
        Box::pin(async move {
            let bytes = namespace.read(TEXTS).await?;
            let batch = LexicalBatch::decode(&bytes)
                .map_err(|err| ResourceError::input(TEXTS, err.to_string()))?;
            Ok(Prepared::new(batch))
        })
    }

    fn execute<'a>(
        &'a self,
        prepared: Prepared,
        control: &'a JobControl,
    ) -> BoxFuture<'a, Result<Vec<u8>, ResourceError>> {
        Box::pin(async move {
            control.check()?;
            let batch = prepared.take::<LexicalBatch>()?;
            // One cooperative point per batch: a bounded, allocation-light run
            // this size has no useful interior. A resource that does have one
            // (M3's long jobs) checks inside its own loop; the seam is the same.
            let vectors = embed_batch(&batch)
                .map_err(|err| ResourceError::Backend(err.to_string()))?;
            control.check()?;
            Ok(vectors.encode())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::MemoryBlobSpace;
    use crate::registry::{ResourceRegistry, Verdict, run_job, verify_output};
    use crate::resources::lexical_codec::{LexicalVectors, run_canonical};
    use crate::spec::{DeterminismClass, JobSpec};
    use std::sync::Arc;

    fn batch() -> LexicalBatch {
        LexicalBatch::new(
            64,
            vec![
                "async rust programming".to_string(),
                "rust runtime internals".to_string(),
                "italian dinner recipes".to_string(),
            ],
        )
    }

    fn spec(blob: proofs::BlobRef) -> JobSpec {
        JobSpec::simple(
            ResourceId::parse("esp.embed.lexical/v1").unwrap(),
            TEXTS,
            blob,
            VECTORS,
            64 * 1024,
            DeterminismClass::Exact,
        )
    }

    fn registry() -> ResourceRegistry {
        let mut registry = ResourceRegistry::new();
        registry
            .register(Arc::new(LexicalEmbedResource::new()))
            .unwrap();
        registry
    }

    #[tokio::test]
    async fn a_batch_embeds_in_input_order_and_reruns_identically() {
        let space = MemoryBlobSpace::in_memory();
        let blob = space.put(&batch().encode()).await.unwrap();
        let spec = spec(blob);
        let (_handle, control) = JobControl::new();

        let output = run_job(&registry(), &spec, &space, &space, &control)
            .await
            .unwrap();
        let bytes = space.fetch(&output.blob).await.unwrap().unwrap();
        let vectors = LexicalVectors::decode(&bytes).unwrap();
        assert_eq!(vectors.dimensions, 64);
        assert_eq!(vectors.vectors.len(), 3);
        assert!(vectors.vectors.iter().all(|v| v.len() == 64));

        // The product claim: shared vocabulary scores above disjoint vocabulary.
        let cosine = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
        assert!(
            cosine(&vectors.vectors[0], &vectors.vectors[1])
                > cosine(&vectors.vectors[0], &vectors.vectors[2])
        );

        assert_eq!(
            verify_output(&registry(), &spec, &output, &space, &space, &control)
                .await
                .unwrap(),
            Verdict::Reproduced
        );
    }

    #[tokio::test]
    async fn a_non_canonical_input_is_refused_as_input_not_as_a_crash() {
        let space = MemoryBlobSpace::in_memory();
        let blob = space.put(b"not a canonical batch").await.unwrap();
        let (_handle, control) = JobControl::new();
        let err = run_job(&registry(), &spec(blob), &space, &space, &control)
            .await
            .unwrap_err();
        assert!(
            format!("{err}").contains("texts"),
            "the failure names the offending slot: {err}"
        );
    }

    /// The exactness receipt, pinned by value.
    ///
    /// `VerificationClass::ExactBytes` claims that *any* conforming device
    /// reproduces these bytes, which is only defensible because every step is
    /// IEEE-754-determined: token hashing is integer, bucket accumulation is
    /// exact in `f32` (whole numbers far below 2^24), and the L2 normalization
    /// is one correctly-rounded `sqrt` plus correctly-rounded divisions. No
    /// FMA contraction, no x87 excess precision, no reduction-order freedom.
    ///
    /// The digest below is what this pipeline produces. `crates/probes/
    /// mesh-lexical-wasm` compiles the same codec source to wasm32 and asserts
    /// the same digest, which is what turns the argument into a receipt.
    #[test]
    fn lexical_determinism_receipt() {
        let canonical = run_canonical(&batch().encode()).unwrap();
        let digest = blake3::hash(&canonical);
        assert_eq!(
            digest.to_hex().as_str(),
            "0000000000000000000000000000000000000000000000000000000000000000",
            "canonical lexical output digest moved; \
             re-run the wasm probe before re-pinning it"
        );
        assert_eq!(canonical.len(), 24 + 9 + 3 * 64 * 4);
    }
}
