// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

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
