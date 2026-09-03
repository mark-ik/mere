// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

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
use crate::spec::{ComputeClass, ResourceRequirements, VerificationClass};

/// Working-memory floor this adapter advertises, in MiB.
///
/// A full batch at the codec's ceilings is roughly 80 MiB live — up to 64 MiB
/// of decoded input text plus a 16 MiB vector block, before `Vec<Vec<f32>>`
/// overhead. 128 MiB is the conservative round number over that. It is a
/// *constant* because [`ResourceRequirements`] is per-adapter, not per-job;
/// when requirements become input-derived this becomes a function of the
/// batch's declared dimensions and text count.
pub const MEMORY_FLOOR_MIB: u32 = 128;

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
                requires: ResourceRequirements {
                    memory_mib: MEMORY_FLOOR_MIB,
                    compute: ComputeClass::Cpu,
                },
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
            let vectors =
                embed_batch(&batch).map_err(|err| ResourceError::Backend(err.to_string()))?;
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
        let bytes = space.get(&output.blob).await.unwrap().unwrap();
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
            Verdict::Reproduced {
                by: ImplementationId::parse("mesh.lexical.fnv1a/v1").unwrap()
            }
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
    /// The argument is only a receipt because a second target ran it:
    /// `crates/probes/mesh-lexical-wasm` compiles this exact codec source and
    /// the real ESP provider to wasm32-unknown-unknown and produced the same
    /// digest under Node on 2026-08-09 (801 canonical bytes, digest below).
    /// If this pin ever moves, re-run that probe before re-pinning it — a
    /// native-only change to the digest says nothing about the class.
    #[test]
    fn lexical_determinism_receipt() {
        let canonical = run_canonical(&batch().encode()).unwrap();
        let digest = blake3::hash(&canonical);
        assert_eq!(
            digest.to_hex().as_str(),
            "4d0f647fa7f15639aad9591ae9e466d48d9d029ef581d3a6d25ca10e5258aa01",
            "canonical lexical output digest moved; \
             re-run the wasm probe before re-pinning it"
        );
        assert_eq!(canonical.len(), 24 + 9 + 3 * 64 * 4);
    }
}
