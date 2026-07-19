// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # chrome
//!
//! Graphshell domain module — root toolbar, app menu, address/omnibar,
//! window chrome view-models for the Mere browser.
//!
//! Each module is WASM-clean (no egui, no servo, no tokio, no platform
//! I/O) and testable without booting a host.

#![doc(html_root_url = "https://docs.rs/chrome/0.0.1")]

pub mod authorities;
pub mod command_palette;
pub mod frame_model;
pub mod host_intent;
pub mod nav;
pub mod omnibar;
pub mod routing;
pub mod suggest;
pub mod toolbar;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
