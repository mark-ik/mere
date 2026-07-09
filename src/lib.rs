//! vates — a local-model inference seam.
//!
//! One trait ([`InferenceProvider`]), a capability descriptor
//! ([`ModelCapability`]) so a caller matches a model to a runtime, and a
//! deterministic dependency-light stub ([`CannedProvider`]) so the whole
//! pipeline (seam, streaming, prompt plumbing) tests on machines with no GPU
//! and no model. Model-backed backends land behind features: the own
//! burn-wgpu decoder first, then external endpoints (Ollama, llama.cpp) and
//! native runtimes (mistral.rs), each behind this same trait, selected by
//! capability, never bound as universal.
//!
//! Streaming is the primary call:
//! [`InferenceProvider::generate_streaming`] pushes each text fragment through
//! a callback as it is produced; [`InferenceProvider::generate`] is the
//! provided collect-it-all wrapper.
//!
//! Promoted from mere's `intel/infer` seam. The decoder (own burn llama-family
//! decoder, `decoder` / `decoder-wgpu`) and the streaming actor (`actor`, on
//! armillary threads) landed 2026-07-09; the external-endpoint backend
//! (`endpoint`) remains the roadmap in `design_docs/`.

#[cfg(feature = "actor")]
pub mod actor;
pub mod canned;
#[cfg(feature = "decoder")]
pub mod decoder;
pub mod provider;

#[cfg(feature = "actor")]
pub use actor::{InferCommand, InferUpdate, spawn_inference_actor};
pub use canned::CannedProvider;
pub use provider::{
    CapabilityQuery, GenerationRequest, InferError, InferenceProvider, ModelCapability,
};
