/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Command-surface telemetry sink.
//!
//! **Migrated to `mere-ux-events::command_surface_telemetry`
//! (2026-05-10).** Re-exports for backward compat. New code should
//! import from `mere_ux_events::command_surface_telemetry`.

pub use mere_ux_events::command_surface_telemetry::*;
