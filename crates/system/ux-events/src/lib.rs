// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # ux-events
//!
//! Portable chrome-event taxonomy for the Mere browser — third pillar
//! between code-level tracing, registry-level register-diagnostics,
//! and host-emitted chrome events. See README for scope.

#![doc(html_root_url = "https://docs.rs/ux-events/0.0.1")]

pub mod command_surface_telemetry;
pub mod ux_diagnostics;
pub mod ux_observability;
pub mod ux_probes;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
