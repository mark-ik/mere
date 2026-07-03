/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Browser Web Worker bindings for the content actor seam.

use serde::{Deserialize, Serialize};

use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
enum ContentWorkerEnvelope {
    Init {
        disabled: Vec<String>,
        auto_ingest: bool,
    },
    Command(Vec<u8>),
}

impl ContentWorkerEnvelope {
    fn into_transfer_buffer(self) -> Result<TransferBuffer, TransferError> {
        postcard::to_allocvec(&self)
            .map(TransferBuffer::from_bytes)
            .map_err(TransferError::Encode)
    }

    fn from_transfer_buffer(bytes: &[u8]) -> Result<Self, TransferError> {
        postcard::from_bytes(bytes).map_err(TransferError::Decode)
    }
}

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::sync::mpsc;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, closure::Closure, prelude::*};
#[cfg(target_arch = "wasm32")]
use web_sys::{DedicatedWorkerGlobalScope, ErrorEvent, MessageEvent};

#[cfg(target_arch = "wasm32")]
thread_local! {
    static CONTENT_WORKER_STATE: RefCell<Option<BrowserContentWorker>> = RefCell::new(None);
}

#[cfg(target_arch = "wasm32")]
pub(crate) struct WorkerContentHandle {
    worker: meerkat_browser_worker::WorkerHandle,
}

#[cfg(target_arch = "wasm32")]
impl WorkerContentHandle {
    pub(crate) fn command(&self, command: ContentCommand) -> bool {
        match command.into_transfer_buffer().and_then(|buffer| {
            ContentWorkerEnvelope::Command(buffer.into_bytes()).into_transfer_buffer()
        }) {
            Ok(buffer) => self.worker_post(buffer),
            Err(_) => false,
        }
    }

    fn worker_post(&self, buffer: TransferBuffer) -> bool {
        buffer.post_to_worker(self.worker.worker()).is_ok()
    }
}

#[cfg(target_arch = "wasm32")]
struct BrowserContentWorker {
    runtime: ContentRuntime,
    encoder: RefCell<SceneTransferEncoder>,
}

#[cfg(target_arch = "wasm32")]
impl BrowserContentWorker {
    fn new(disabled: std::collections::HashSet<String>, auto_ingest: bool) -> Self {
        Self {
            runtime: ContentRuntime::new(disabled, auto_ingest),
            encoder: RefCell::new(SceneTransferEncoder::default()),
        }
    }
}

#[cfg(target_arch = "wasm32")]
struct WorkerScopeContentSink<'a> {
    scope: DedicatedWorkerGlobalScope,
    encoder: &'a RefCell<SceneTransferEncoder>,
}

#[cfg(target_arch = "wasm32")]
impl ContentUpdateSink for WorkerScopeContentSink<'_> {
    fn emit_update(&self, update: ContentUpdate) {
        match update.into_transfer_buffer(&mut self.encoder.borrow_mut()) {
            Ok(buffer) => {
                let _ = buffer.post_to_worker_scope(&self.scope);
            }
            Err(err) => {
                if let Ok(buffer) = TransferBuffer::from_transport_error(err.to_string()) {
                    let _ = buffer.post_to_worker_scope(&self.scope);
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn init_content_worker(
    worker: &web_sys::Worker,
    disabled: std::collections::HashSet<String>,
    auto_ingest: bool,
) -> Result<(), JsValue> {
    ContentWorkerEnvelope::Init {
        disabled: disabled.into_iter().collect(),
        auto_ingest,
    }
    .into_transfer_buffer()
    .map_err(|err| JsValue::from_str(&err.to_string()))?
    .post_to_worker(worker)
}

#[cfg(target_arch = "wasm32")]
fn emit_transport_error(scope: &DedicatedWorkerGlobalScope, reason: impl Into<String>) {
    if let Ok(buffer) = TransferBuffer::from_transport_error(reason.into()) {
        let _ = buffer.post_to_worker_scope(scope);
    }
}

#[cfg(target_arch = "wasm32")]
fn on_worker_message(scope: &DedicatedWorkerGlobalScope, event: MessageEvent) {
    let Ok(array_buffer) = event.data().dyn_into::<js_sys::ArrayBuffer>() else {
        emit_transport_error(scope, "content worker received non-ArrayBuffer message");
        return;
    };
    let buffer = TransferBuffer::from_array_buffer(array_buffer);
    let envelope = match ContentWorkerEnvelope::from_transfer_buffer(buffer.as_bytes()) {
        Ok(envelope) => envelope,
        Err(err) => {
            emit_transport_error(scope, err.to_string());
            return;
        }
    };
    match envelope {
        ContentWorkerEnvelope::Init {
            disabled,
            auto_ingest,
        } => {
            CONTENT_WORKER_STATE.with(|slot| {
                *slot.borrow_mut() = Some(BrowserContentWorker::new(
                    disabled.into_iter().collect(),
                    auto_ingest,
                ));
            });
        }
        ContentWorkerEnvelope::Command(bytes) => {
            CONTENT_WORKER_STATE.with(|slot| {
                let mut state = slot.borrow_mut();
                let Some(state) = state.as_mut() else {
                    emit_transport_error(scope, "content worker command arrived before init");
                    return;
                };
                let command = match ContentCommand::from_transfer_buffer(&bytes) {
                    Ok(command) => command,
                    Err(err) => {
                        emit_transport_error(scope, err.to_string());
                        return;
                    }
                };
                let sink = WorkerScopeContentSink {
                    scope: scope.clone(),
                    encoder: &state.encoder,
                };
                state.runtime.handle_command(command, &sink);
            });
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn spawn_content_worker(
    wake: Wake,
    disabled: std::collections::HashSet<String>,
    auto_ingest: bool,
) -> (ContentHandle, ContentUpdateStream) {
    let worker = meerkat_browser_worker::create_worker()
        .expect("content worker factory must return a Worker");
    let (updates_tx, updates_rx) = mpsc::channel();
    let error_tx = updates_tx.clone();

    let wake_on_message = wake.clone();
    let onmessage = Closure::wrap(Box::new(move |event: MessageEvent| {
        let buffer = match event.data().dyn_into::<js_sys::ArrayBuffer>() {
            Ok(buffer) => TransferBuffer::from_array_buffer(buffer),
            Err(_) => match TransferBuffer::from_transport_error(
                "content worker posted non-ArrayBuffer update",
            ) {
                Ok(buffer) => buffer,
                Err(_) => return,
            },
        };
        if updates_tx.send(buffer).is_ok() {
            (wake_on_message)();
        }
    }) as Box<dyn FnMut(_)>);
    let wake_on_error = wake;
    let onerror = Closure::wrap(Box::new(move |event: ErrorEvent| {
        if let Ok(buffer) = TransferBuffer::from_transport_error(format!(
            "content worker error: {}",
            event.message()
        )) {
            if error_tx.send(buffer).is_ok() {
                (wake_on_error)();
            }
        }
    }) as Box<dyn FnMut(_)>);
    init_content_worker(&worker, disabled, auto_ingest)
        .expect("content worker init message should post");
    let worker = meerkat_browser_worker::bind_worker(worker, onmessage, onerror);

    (
        ContentHandle::Worker(WorkerContentHandle { worker }),
        ContentUpdateStream::Transfer {
            updates: updates_rx,
            decoder: SceneTransferDecoder::default(),
        },
    )
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn set_content_worker_factory(factory: js_sys::Function) {
    meerkat_browser_worker::set_worker_factory(factory);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn install_content_worker() -> Result<(), JsValue> {
    let scope = js_sys::global().dyn_into::<DedicatedWorkerGlobalScope>()?;
    let handler_scope = scope;
    let handler = Closure::wrap(Box::new(move |event: MessageEvent| {
        on_worker_message(&handler_scope, event);
    }) as Box<dyn FnMut(_)>);
    meerkat_browser_worker::install_worker_listener(handler)
}
