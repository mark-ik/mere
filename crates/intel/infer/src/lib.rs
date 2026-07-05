/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! infer — the inference-provider seam (local-models harness §2).
//!
//! Sibling of `embed`'s `EmbeddingProvider`, same shape: one trait
//! ([`InferenceProvider`]), a capability descriptor ([`ModelCapability`])
//! so callers match a model to a runtime, and a deterministic
//! dependency-light stub ([`CannedProvider`]) so the whole pipeline
//! (seam, actor, RAG plumbing) tests on machines with no GPU and no
//! model. Model-backed providers land behind features, burn-wgpu first;
//! native heavy runtimes (mistral.rs, llama.cpp) are future backends
//! behind this same trait, selected by capability, never bound as
//! universal.
//!
//! Streaming is the primary call: [`InferenceProvider::generate_streaming`]
//! pushes each text fragment through a callback as it is produced (the
//! armillary inference actor forwards fragments as messages);
//! [`InferenceProvider::generate`] is the provided collect-it-all wrapper.

pub mod canned;
pub mod provider;

pub use canned::CannedProvider;
pub use provider::{
    CapabilityQuery, GenerationRequest, InferError, InferenceProvider, ModelCapability,
};
