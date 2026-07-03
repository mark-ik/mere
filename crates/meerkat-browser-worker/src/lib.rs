/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Browser Worker bootstrap utilities shared by Meerkat's wasm-side worker seams.

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, closure::Closure, prelude::*};
#[cfg(target_arch = "wasm32")]
use web_sys::{
    DedicatedWorkerGlobalScope, ErrorEvent, MessageEvent, Worker, WorkerOptions, WorkerType,
};

#[cfg(target_arch = "wasm32")]
thread_local! {
    static WORKER_FACTORY: RefCell<Option<js_sys::Function>> = RefCell::new(None);
    static WORKER_MESSAGE_HANDLER: RefCell<Option<Closure<dyn FnMut(MessageEvent)>>> =
        RefCell::new(None);
}

#[cfg(target_arch = "wasm32")]
pub struct WorkerHandle {
    worker: Worker,
    onmessage: Closure<dyn FnMut(MessageEvent)>,
    onerror: Closure<dyn FnMut(ErrorEvent)>,
}

#[cfg(target_arch = "wasm32")]
impl WorkerHandle {
    pub fn worker(&self) -> &Worker {
        &self.worker
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for WorkerHandle {
    fn drop(&mut self) {
        self.worker.set_onmessage(None);
        self.worker.set_onerror(None);
        self.worker.terminate();
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn set_worker_factory(factory: js_sys::Function) {
    WORKER_FACTORY.with(|slot| {
        *slot.borrow_mut() = Some(factory);
    });
}

#[cfg(target_arch = "wasm32")]
fn module_worker(script_url: String) -> Result<Worker, JsValue> {
    let options = WorkerOptions::new();
    options.set_type(WorkerType::Module);
    Worker::new_with_options(&script_url, &options)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn set_module_worker_factory(script_url: String) {
    let factory = Closure::wrap(Box::new(move || -> Result<Worker, JsValue> {
        module_worker(script_url.clone())
    }) as Box<dyn FnMut() -> Result<Worker, JsValue>>);
    set_worker_factory(factory.as_ref().unchecked_ref::<js_sys::Function>().clone());
    factory.forget();
}

#[cfg(target_arch = "wasm32")]
pub fn create_worker() -> Result<Worker, JsValue> {
    WORKER_FACTORY.with(|slot| {
        let binding = slot.borrow();
        let factory = binding
            .as_ref()
            .ok_or_else(|| JsValue::from_str("worker factory not set"))?;
        factory
            .call0(&JsValue::UNDEFINED)?
            .dyn_into::<Worker>()
            .map_err(Into::into)
    })
}

#[cfg(target_arch = "wasm32")]
pub fn bind_worker(
    worker: Worker,
    onmessage: Closure<dyn FnMut(MessageEvent)>,
    onerror: Closure<dyn FnMut(ErrorEvent)>,
) -> WorkerHandle {
    worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    worker.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    WorkerHandle {
        worker,
        onmessage,
        onerror,
    }
}

#[cfg(target_arch = "wasm32")]
pub fn install_worker_listener(handler: Closure<dyn FnMut(MessageEvent)>) -> Result<(), JsValue> {
    let scope = js_sys::global().dyn_into::<DedicatedWorkerGlobalScope>()?;
    scope.set_onmessage(Some(handler.as_ref().unchecked_ref()));
    WORKER_MESSAGE_HANDLER.with(|slot| {
        *slot.borrow_mut() = Some(handler);
    });
    Ok(())
}
