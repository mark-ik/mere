/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # mere-host-runtime
//!
//! Portable runtime state + types Mere hosts attach to. Per the
//! multiplexer framing thesis:
//!
//! > **Mere multiplexes durable graph sessions. Engines are
//! > replaceable content producers. Hosts are attach clients.
//! > Graph truth is never browser profile state.**
//!
//! This crate is the **state** half of that picture — the durable
//! session manifest, tile data model, action-bus / tear-out
//! payloads, and (eventually) the manifest store + action bus
//! dispatcher. Everything here compiles wasm32-clean: no gpui, no
//! iced, no winit, no wgpu.
//!
//! Sibling crates:
//!
//! - [`mere-host-contract`](../mere_host_contract/index.html) —
//!   the **vocabulary** half: port traits hosts satisfy
//!   (HostPaintPort, HostInputPort, HostSurfacePort, HostAccessibility
//!   Port, …) + frame-projection inputs.
//! - [`mere-host`](../mere_host/index.html) — the **gpui adapter**:
//!   `HostRoot` Render impl, window opening, mouse handler wiring,
//!   gpui Entity ownership. Consumes both contract + runtime.
//! - Future host adapters (iced, web, headless server) consume
//!   `mere-host-runtime` + `mere-host-contract` the same way
//!   `mere-host` does. The runtime is identical across them.
//!
//! See:
//! - `design_docs/mere_docs/research/2026-05-11_browser_multiplexer_framing.md`
//! - `design_docs/mere_docs/implementation_strategy/2026-05-11_graph_session_manifest_plan.md`

#![doc(html_root_url = "https://docs.rs/mere-host-runtime/0.0.1")]

pub mod action_bus;
pub mod manifest;
pub mod manifest_store;
pub mod session_graph_store;
pub mod tearout;
pub mod tiles;

pub use action_bus::{
    ActionKind, ActionTarget, BusAction, BusDispatchOutcome, DenyReason, PermissionDecision,
    PermissionGate, PermitEverythingGate, RefuseEverythingGate, SurfaceId, TearOutMode,
    check_permission,
};
pub use manifest::{
    EngineProfileBinding, EngramId, GraphSessionManifest, MANIFEST_SCHEMA_VERSION, PersonaId,
    SessionPolicy, SessionPolicyOverride, WorkerKind,
};
pub use manifest_store::{LoadFailure, LoadReport, MANIFEST_FILE, ManifestStore, TRASH_DIR};
pub use tearout::{PaneDragPayload, TileDragPayload};
pub use tiles::{HistoryEntry, NavigateMode, TileManager, TileState};
