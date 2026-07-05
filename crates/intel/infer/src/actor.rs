/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The armillary inference actor (local-models harness §3).
//!
//! Holds a loaded [`InferenceProvider`] on its own thread, answers
//! generation requests, and streams fragments back as typed updates —
//! the host never blocks its loop on a token. The provider is built *on
//! the actor thread* via the build closure, per armillary doctrine, so
//! model load cost never lands on the kernel thread either.
//!
//! Feature-gated (`actor`) because armillary spawns OS threads; the
//! seam's core stays thread-free so it keeps compiling for
//! wasm32-unknown-unknown.
//!
//! Cancellation of an *in-flight* generation is not yet possible: the
//! provider callback has no control-flow return. When a real model body
//! lands (P1) the callback signature grows a `ControlFlow` so a long
//! generation can be stopped mid-stream; the update vocabulary below
//! already leaves room for it.

use std::sync::mpsc::Receiver;

use armillary::actor::spawn_named;
use armillary::{ActorHandle, Wake};

use crate::provider::{GenerationRequest, InferError, InferenceProvider, ModelCapability};

/// Commands the kernel sends the inference actor.
#[derive(Debug, Clone, PartialEq)]
pub enum InferCommand {
    /// Run one generation. `id` is the kernel's correlation key; every
    /// update for this request carries it back.
    Generate {
        id: u64,
        request: GenerationRequest,
    },
}

/// Updates the actor streams back to the kernel.
#[derive(Debug, Clone, PartialEq)]
pub enum InferUpdate {
    /// The provider is loaded and ready; sent once at actor startup so
    /// status surfaces can show the real capability, not a guess.
    Ready { capability: ModelCapability },
    /// Generation `id` began.
    Started { id: u64 },
    /// One streamed fragment of generation `id`, in production order.
    Fragment { id: u64, text: String },
    /// Generation `id` completed; `text` is the full assembled output.
    Finished { id: u64, text: String },
    /// Generation `id` failed.
    Failed { id: u64, error: InferError },
}

/// Spawn the inference actor. `build` runs on the actor thread and
/// constructs (or loads) the provider there; the kernel gets the usual
/// armillary pair of a `Send` handle and an update receiver.
pub fn spawn_inference_actor<F>(wake: Wake, build: F) -> (ActorHandle<InferCommand>, Receiver<InferUpdate>)
where
    F: FnOnce() -> Box<dyn InferenceProvider> + Send + 'static,
{
    spawn_named::<InferCommand, InferUpdate, _>("infer", wake, move |commands, out| {
        let provider = build();
        out.emit(InferUpdate::Ready {
            capability: provider.capability().clone(),
        });
        while let Ok(command) = commands.recv() {
            match command {
                InferCommand::Generate { id, request } => {
                    out.emit(InferUpdate::Started { id });
                    let streamed = out.clone();
                    let result = provider.generate_streaming(&request, &mut |fragment| {
                        streamed.emit(InferUpdate::Fragment {
                            id,
                            text: fragment.to_string(),
                        });
                    });
                    match result {
                        Ok(text) => out.emit(InferUpdate::Finished { id, text }),
                        Err(error) => out.emit(InferUpdate::Failed { id, error }),
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::canned::CannedProvider;

    fn no_wake() -> Wake {
        Arc::new(|| {})
    }

    #[test]
    fn actor_streams_ready_started_fragments_finished_in_order() {
        let (handle, updates) = spawn_inference_actor(no_wake(), || {
            Box::new(CannedProvider::new().with_response("ping", "pong pang"))
        });
        assert!(handle.command(InferCommand::Generate {
            id: 7,
            request: GenerationRequest {
                prompt: "ping".to_string(),
                ..Default::default()
            },
        }));
        handle.join();

        let got: Vec<InferUpdate> = updates.iter().collect();
        assert!(
            matches!(&got[0], InferUpdate::Ready { capability } if capability.loader == "canned")
        );
        assert_eq!(got[1], InferUpdate::Started { id: 7 });
        assert_eq!(
            got[2],
            InferUpdate::Fragment {
                id: 7,
                text: "pong".to_string()
            }
        );
        assert_eq!(
            got[3],
            InferUpdate::Fragment {
                id: 7,
                text: " pang".to_string()
            }
        );
        assert_eq!(
            got[4],
            InferUpdate::Finished {
                id: 7,
                text: "pong pang".to_string()
            }
        );
        assert_eq!(got.len(), 5);
    }

    #[test]
    fn failed_generation_reports_error_with_id() {
        let (handle, updates) = spawn_inference_actor(no_wake(), || Box::new(CannedProvider::new()));
        assert!(handle.command(InferCommand::Generate {
            id: 1,
            request: GenerationRequest::default(), // empty prompt → InvalidRequest
        }));
        handle.join();

        let got: Vec<InferUpdate> = updates.iter().collect();
        assert!(matches!(
            got.last(),
            Some(InferUpdate::Failed {
                id: 1,
                error: InferError::InvalidRequest(_)
            })
        ));
    }

    #[test]
    fn sequential_requests_keep_their_correlation_ids() {
        let (handle, updates) = spawn_inference_actor(no_wake(), || {
            Box::new(
                CannedProvider::new()
                    .with_response("a", "alpha")
                    .with_response("b", "beta"),
            )
        });
        for (id, prompt) in [(1u64, "a"), (2u64, "b")] {
            handle.command(InferCommand::Generate {
                id,
                request: GenerationRequest {
                    prompt: prompt.to_string(),
                    ..Default::default()
                },
            });
        }
        handle.join();

        let got: Vec<InferUpdate> = updates.iter().collect();
        let finished: Vec<(u64, String)> = got
            .iter()
            .filter_map(|u| match u {
                InferUpdate::Finished { id, text } => Some((*id, text.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            finished,
            vec![(1, "alpha".to_string()), (2, "beta".to_string())]
        );
    }
}
