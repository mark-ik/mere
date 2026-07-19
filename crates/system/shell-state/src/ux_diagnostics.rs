// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bridge from UxEvent to a diagnostics-channel sink.
//!
//! **Migrated to `ux-events::ux_diagnostics` (2026-05-10).**
//! Re-exports for backward compat. New code should import from
//! `ux_events::ux_diagnostics`.

pub use ux_events::ux_diagnostics::*;
