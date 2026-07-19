// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! infer — mere's glue over the standalone `vates` inference crate.
//!
//! The seam (`InferenceProvider` / `ModelCapability` / `CannedProvider`),
//! the burn llama-family decoder, and the armillary streaming actor all
//! live in [vates](https://github.com/mark-ik/vates) since 2026-07-09
//! (boundary pass slice B, mirroring `embed` over `sibylla`). This crate
//! re-exports that surface under the established `infer::` paths and
//! keeps only the mere-side pieces: the eidetic model corridor
//! (`tests/eidetic_corridor.rs`, proving model loading needs no
//! filesystem convention, only a `ManifestId`).
//!
//! Features forward one-to-one: `actor` -> `vates/actor`,
//! `decoder` -> `vates/decoder`, `decoder-wgpu` -> `vates/decoder-wgpu`.

pub use vates::{
    CannedProvider, CapabilityQuery, GenerationRequest, InferError, InferenceProvider,
    ModelCapability,
};

#[cfg(feature = "actor")]
pub use vates::actor;
#[cfg(feature = "actor")]
pub use vates::{InferCommand, InferUpdate, spawn_inference_actor};

#[cfg(feature = "decoder")]
pub use vates::decoder;
