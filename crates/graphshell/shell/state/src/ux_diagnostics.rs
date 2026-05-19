/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Bridge from UxEvent to a diagnostics-channel sink.
//!
//! **Migrated to `mere-ux-events::ux_diagnostics` (2026-05-10).**
//! Re-exports for backward compat. New code should import from
//! `ux_events::ux_diagnostics`.

pub use ux_events::ux_diagnostics::*;
