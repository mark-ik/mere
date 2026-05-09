/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable shell session state for the Mere browser.
//!
//! This crate owns the host-neutral session-state types that the runtime
//! surfaces on `GraphshellRuntime` and the `Frame*` view-models: focus
//! authorities, command palette state, omnibar/toolbar drafts, frame
//! view-models, return-target routing, and UX diagnostics/observability/probes.
//!
//! Each module is WASM-clean (no egui, no servo, no tokio, no platform I/O)
//! and testable without booting a host. Consumers (mere-host-contract,
//! graphshell-side reducers, future host adapters) reach for these types
//! when they need to project, restore, or mutate session state.
//!
//! The vocabulary primitives (`geometry`, `time`, `pane`, `content`, etc.)
//! and the host-port trait definitions live in
//! [`mere-kernel`](https://crates.io/crates/mere-kernel); this crate
//! depends on those.

pub mod authorities;
pub mod command_palette;
pub mod command_surface_telemetry;
pub mod frame_model;
pub mod host_intent;
pub mod omnibar;
pub mod routing;
pub mod toolbar;
pub mod ux_diagnostics;
pub mod ux_observability;
pub mod ux_probes;
