/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # mere-graphshell
//!
//! Graphshell domain module — root toolbar, app menu, address/omnibar,
//! window chrome view-models for the Mere browser.
//!
//! Narrower than the legacy `crates/graphshell/` crate; absorbs slices
//! of `graphshell-shell-state` as the migration progresses. See README
//! for migration progress.
//!
//! Each module is WASM-clean (no egui, no servo, no tokio, no platform
//! I/O) and testable without booting a host.

#![doc(html_root_url = "https://docs.rs/mere-graphshell/0.0.1")]

pub mod authorities;
pub mod command_palette;
pub mod frame_model;
pub mod host_intent;
pub mod omnibar;
pub mod routing;
pub mod toolbar;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
