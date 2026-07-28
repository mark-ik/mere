// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `kernel`: Portable identity, authority, and mutation kernel.
//!
//! This crate must compile to `wasm32-unknown-unknown` with zero errors.
//! It contains no egui, wgpu, Servo, or platform I/O dependencies.
//!
//! Forbidden in this crate:
//! - `Uuid::new_v4()` outside `#[cfg(not(target_arch = "wasm32"))]` (WASM hosts provide IDs)
//! - `std::time::{Instant, SystemTime}::now()` (panics on browser WASM — use
//!   `PortableInstant` or the crate's `web_time`-backed wall-clock helpers)
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
pub mod host_toolkit;
pub mod intents;
pub mod overlay;
pub mod paint;
pub mod pane;
/// Settings/permissions scope hierarchy + the narrowing rule (R0 invariant).
pub mod permissions;
pub mod persistence;
/// Edge persistence types (`PersistedEdge` et al.), re-exported through
/// [`persistence`]. Separate file to keep `persistence.rs` under the ceiling.
pub mod persistence_edge;
/// Field-layer persistence DTOs (`PersistedField`/`PersistedCoupling`),
/// re-exported through [`persistence`]. Separate file to keep `persistence.rs`
/// under the per-file ceiling.
pub mod persistence_fields;
pub mod signal_router;
/// Native on-disk graph persistence. Gated by the `store` feature so the kernel
/// stays portable-by-default; the native host enables it.
#[cfg(feature = "store")]
pub mod store;
pub mod time;
pub mod types;
pub mod verso_address;

#[cfg(test)]
mod tests;
