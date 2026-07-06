/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host wiring for the `>ask` omnibar verb: spawn the armillary inference
//! actor with a provider built on its own thread.
//!
//! The provider is chosen at build time and resolved at spawn time:
//!
//! - With the `local-inference` feature **and** `MERE_TINYLLAMA_DIR`
//!   pointing at a real checkpoint, the actor loads a
//!   `DecoderProvider<Wgpu>` (TinyLlama on the GPU). The 2.2GB load runs
//!   on the actor thread, never the UI thread.
//! - Otherwise it falls back to infer's deterministic `CannedProvider`
//!   stub, so `>ask` always answers (an echo) and the shell never depends
//!   on a model being present — the same "None disables the feature, the
//!   shell still runs" shape the fetch/store paths use.

use std::sync::mpsc::Receiver;

use armillary::{ActorHandle, Wake};
use infer::{CannedProvider, InferCommand, InferUpdate, InferenceProvider};

/// Spawn the inference actor. Returns the kernel's command handle and the
/// update receiver, folded into `Content` / `KernelInbox` beside the other
/// actors.
pub(crate) fn spawn_inference(
    wake: Wake,
) -> (ActorHandle<InferCommand>, Receiver<InferUpdate>) {
    infer::spawn_inference_actor(wake, build_provider)
}

/// Build the provider on the actor thread. Kept `Send` (no captured host
/// state) so armillary can move it across the boundary.
fn build_provider() -> Box<dyn InferenceProvider> {
    #[cfg(feature = "local-inference")]
    {
        if let Some(provider) = try_load_local_model() {
            return provider;
        }
    }
    // No model: the stub echoes the prompt (`echo: <prompt>`), which both
    // exercises the streaming path and makes it obvious no model is loaded.
    tracing::info!(target: "meerkat::infer", "inference provider = canned stub (no local model)");
    Box::new(CannedProvider::new())
}

/// Load a real llama-family checkpoint from `MERE_TINYLLAMA_DIR`, on the
/// wgpu backend. Returns `None` (→ canned fallback) when the env var is
/// unset or the load fails; the failure is traced, never a panic.
#[cfg(feature = "local-inference")]
fn try_load_local_model() -> Option<Box<dyn InferenceProvider>> {
    let dir = std::env::var("MERE_TINYLLAMA_DIR").ok()?;
    let dir = std::path::PathBuf::from(dir);
    tracing::info!(target: "meerkat::infer", dir = %dir.display(), "loading local model (reading artifact triple)");
    let read = |name: &str| std::fs::read(dir.join(name));
    let (config, tokenizer, weights) = match (
        read("config.json"),
        read("tokenizer.json"),
        read("model.safetensors"),
    ) {
        (Ok(c), Ok(t), Ok(w)) => (c, t, w),
        _ => {
            tracing::warn!(
                target: "meerkat::infer",
                dir = %dir.display(),
                "MERE_TINYLLAMA_DIR set but a model file is missing; using the canned stub"
            );
            return None;
        }
    };
    tracing::info!(
        target: "meerkat::infer",
        weight_bytes = weights.len(),
        "artifact triple read; building model on burn-wgpu (device init + tensor upload)"
    );
    match infer::decoder::load_wgpu_provider(
        &config,
        &tokenizer,
        &weights,
        "TinyLlama/TinyLlama-1.1B-Chat-v1.0",
    ) {
        Ok(p) => {
            tracing::info!(target: "meerkat::infer", "loaded local model on burn-wgpu");
            Some(Box::new(p))
        }
        Err(e) => {
            tracing::warn!(target: "meerkat::infer", error = %e, "local model load failed; using the canned stub");
            None
        }
    }
}
