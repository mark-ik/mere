/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `graphshell-core`: Portable identity, authority, and mutation kernel.
//!
//! This crate must compile to `wasm32-unknown-unknown` with zero errors.
//! It contains no egui, wgpu, Servo, or platform I/O dependencies.
//!
//! Forbidden in this crate:
//! - `Uuid::new_v4()` outside `#[cfg(not(target_arch = "wasm32"))]` (WASM hosts provide IDs)
//! - `std::time::Instant` (panics on WASM — use `SystemTime` or accept time from host)
//! - `#[wasm_bindgen]` / UniFFI annotations (belong in wrapper crates)
//! - any platform I/O (file, network, browser APIs)

pub mod accessibility;
pub mod actions;
pub mod address;
pub mod async_host;
pub mod async_request;
pub mod color;
pub mod content;
pub mod geometry;
pub mod graph;
pub mod host_event;
pub mod intents;
pub mod overlay;
pub mod pane;
pub mod persistence;
pub mod routing;
pub mod shell_state;
pub mod signal_router;
pub mod time;
pub mod types;
pub mod ux_diagnostics;
pub mod ux_observability;
pub mod ux_probes;
pub mod verso_address;
pub mod viewer_host;
