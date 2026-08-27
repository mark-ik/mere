// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The `app-core` component host — the ENVELOPE lane (ruled with Mark
//! 2026-07-23).
//!
//! A component that wants to act on the host app imports one interface:
//! `actions.emit(action-envelope)`, where the envelope is `{name, payload}`.
//! The app's WHOLE action surface is reachable through it — there is no
//! curated per-action interface, because a curated interface would be a
//! second authority that can drift from the gate's, and would change shape
//! every time an action is added (so a pack compiled against one install
//! could not run on another). One envelope is one stable ABI; the grant is
//! the only thing that varies.
//!
//! What decides an emission is therefore NOT this crate. This host is
//! app-agnostic: it moves the envelope across the boundary and hands it to a
//! host-supplied [`ActionSink`], which decodes it, classifies it into a
//! capability RING, and checks the emitting denizen's grant (turnstone's `ring`
//! module). A [`Refusal`] comes straight back to the guest as a typed error,
//! so a component learns "denied: session" synchronously rather than trapping
//! or silently no-op'ing.
//!
//! The sink COLLECTS; it never re-enters the app. A host function cannot hold
//! `&mut App` while the app is inside a wasm call, so an allowed emission is
//! queued in the sink and the app drains it after the turn returns — the same
//! shape the piccolo lane already uses (evaluate, collect Actions, lower them
//! through the ordinary spine under the denizen's author).
//!
//! Containment mirrors document-host: [`guarded_engine`] turns on epoch
//! interruption, [`Watchdog`] bumps the epoch so a runaway turn hits its
//! deadline, and `StoreLimits` caps allocation. A misbehaving component traps;
//! the host survives.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store, StoreLimits};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    // The shared `mere:script` contract (one WIT, many worlds) — the same
    // package document-host binds, so the two hosts never drift.
    path: "../wit",
    world: "app-core",
    exports: { default: async },
    imports: { default: async },
});

/// Why an emission did not become an action. Returned to the guest as the
/// WIT `emit-error`, so a refusal is part of the contract rather than a trap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The action's ring is not covered by this denizen's grant, or the ring
    /// is host-only (gate management: never grantable to anyone).
    Denied(String),
    /// No action by that name in this host build.
    Unknown(String),
    /// The name is known but the payload did not parse for it.
    Malformed(String),
}

impl Refusal {
    fn into_wit(self) -> crate::mere::script::actions::EmitError {
        use crate::mere::script::actions::EmitError as Wit;
        match self {
            Refusal::Denied(why) => Wit::Denied(why),
            Refusal::Unknown(name) => Wit::Unknown(name),
            Refusal::Malformed(why) => Wit::Malformed(why),
        }
    }
}

/// The host app's decision seam. The app implements this over its own action
/// vocabulary: decode the envelope, classify + authorize it, and QUEUE the
/// result for lowering after the turn. Returning `Ok` means "accepted for
/// lowering", not "already applied".
pub trait ActionSink: Send {
    fn emit(&mut self, name: &str, payload: &str) -> Result<(), Refusal>;
}

/// The per-instance store data: the app's sink, the guest's captured log
/// output, the grant names `caps.granted()` reports, and the WASI floor a std
/// guest needs.
pub struct AppHostState<S: ActionSink> {
    sink: S,
    logs: Vec<String>,
    granted: Vec<String>,
    wasi: WasiCtx,
    table: ResourceTable,
    limits: StoreLimits,
}

impl<S: ActionSink> crate::mere::script::log::Host for AppHostState<S> {
    async fn log(&mut self, message: String) {
        self.logs.push(message);
    }
}

impl<S: ActionSink> crate::mere::script::caps::Host for AppHostState<S> {
    async fn granted(&mut self) -> Vec<String> {
        self.granted.clone()
    }
}

impl<S: ActionSink> crate::mere::script::actions::Host for AppHostState<S> {
    async fn emit(
        &mut self,
        action: crate::mere::script::actions::ActionEnvelope,
    ) -> Result<(), crate::mere::script::actions::EmitError> {
        self.sink
            .emit(&action.name, &action.payload)
            .map_err(Refusal::into_wit)
    }
}

impl<S: ActionSink> WasiView for AppHostState<S> {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

/// An engine with epoch interruption on, so a runaway turn can be trapped.
/// Pair it with a [`Watchdog`] and `Store::set_epoch_deadline`.
pub fn guarded_engine() -> wasmtime::Result<Engine> {
    let mut config = Config::new();
    config.epoch_interruption(true);
    Engine::new(&config)
}

/// Bumps an engine's epoch on a fixed tick so a deadline is actually reached.
/// Stops and joins on drop, so a run can never leak the thread.
pub struct Watchdog {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Watchdog {
    pub fn start(engine: Engine, tick: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let watch_stop = stop.clone();
        let handle = std::thread::spawn(move || {
            while !watch_stop.load(Ordering::Relaxed) {
                std::thread::sleep(tick);
                engine.increment_epoch();
            }
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// One live `app-core` instance: the store (holding the sink) plus bindings.
/// Drive it with [`activate`](Self::activate) / [`on_event`](Self::on_event) /
/// [`deactivate`](Self::deactivate), draining the sink between turns.
pub struct AppScript<S: ActionSink + 'static> {
    store: Store<AppHostState<S>>,
    bindings: AppCore,
}

impl<S: ActionSink + 'static> AppScript<S> {
    /// Instantiate the component at `path` on `engine` with `sink` backing
    /// `emit`. `granted` is what `caps.granted()` reports (the grant is
    /// authoritative elsewhere; this is the guest's read-only window onto it,
    /// so a component can skip a feature instead of emitting into a denial).
    /// `epoch_deadline` of `Some(ticks)` arms epoch interruption — the engine
    /// must come from [`guarded_engine`] and a [`Watchdog`] must be running.
    pub async fn attach(
        engine: &Engine,
        path: &Path,
        sink: S,
        granted: Vec<String>,
        limits: StoreLimits,
        epoch_deadline: Option<u64>,
    ) -> wasmtime::Result<Self> {
        let component = Component::from_file(engine, path)?;
        let mut linker: Linker<AppHostState<S>> = Linker::new(engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
        // All three imports are intrinsic to the world: `log` and `caps` name
        // no authority, and `actions` is gated per EMISSION by the sink (the
        // ring check), not by linking. Unlinking it would only make a
        // component fail to instantiate rather than learn why it was refused.
        crate::mere::script::log::add_to_linker::<_, HasSelf<AppHostState<S>>>(&mut linker, |s| s)?;
        crate::mere::script::caps::add_to_linker::<_, HasSelf<AppHostState<S>>>(
            &mut linker,
            |s| s,
        )?;
        crate::mere::script::actions::add_to_linker::<_, HasSelf<AppHostState<S>>>(
            &mut linker,
            |s| s,
        )?;

        let mut store = Store::new(
            engine,
            AppHostState {
                sink,
                logs: Vec::new(),
                granted,
                wasi: WasiCtxBuilder::new().build(),
                table: ResourceTable::new(),
                limits,
            },
        );
        store.limiter(|s| &mut s.limits);
        if let Some(ticks) = epoch_deadline {
            store.set_epoch_deadline(ticks);
        }
        let bindings = AppCore::instantiate_async(&mut store, &component, &linker).await?;
        Ok(Self { store, bindings })
    }

    pub async fn activate(&mut self) -> wasmtime::Result<()> {
        self.bindings.call_activate(&mut self.store).await
    }

    /// Deliver one host event. The component emits during this call; the
    /// accepted emissions land in the sink, which the caller drains after.
    pub async fn on_event(&mut self, kind: &str, payload: &str) -> wasmtime::Result<()> {
        self.bindings
            .call_on_event(&mut self.store, kind, payload)
            .await
    }

    pub async fn deactivate(&mut self) -> wasmtime::Result<()> {
        self.bindings.call_deactivate(&mut self.store).await
    }

    pub fn sink(&self) -> &S {
        &self.store.data().sink
    }

    pub fn sink_mut(&mut self) -> &mut S {
        &mut self.store.data_mut().sink
    }

    /// The guest's captured `log` output, oldest first.
    pub fn logs(&self) -> &[String] {
        &self.store.data().logs
    }
}

/// The blocking face of [`AppScript`] for sync hosts (turnstone's spine is
/// sync): a minimal `block_on` with no global runtime, matching
/// document-host's `WasmModRuntime` bridge. With no async import in this
/// world, every call completes in a single poll.
impl<S: ActionSink + 'static> AppScript<S> {
    pub fn attach_blocking(
        engine: &Engine,
        path: &Path,
        sink: S,
        granted: Vec<String>,
        limits: StoreLimits,
        epoch_deadline: Option<u64>,
    ) -> wasmtime::Result<Self> {
        pollster::block_on(Self::attach(
            engine,
            path,
            sink,
            granted,
            limits,
            epoch_deadline,
        ))
    }

    pub fn activate_blocking(&mut self) -> wasmtime::Result<()> {
        pollster::block_on(self.activate())
    }

    pub fn on_event_blocking(&mut self, kind: &str, payload: &str) -> wasmtime::Result<()> {
        pollster::block_on(self.on_event(kind, payload))
    }

    pub fn deactivate_blocking(&mut self) -> wasmtime::Result<()> {
        pollster::block_on(self.deactivate())
    }
}
