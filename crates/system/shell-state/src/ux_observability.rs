/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! UX observability layer.
//!
//! **Migrated to `ux-events::ux_observability` (2026-05-10).**
//! Re-exports for backward compat. New code should import from
//! `ux_events::ux_observability`.

pub use ux_events::ux_observability::*;
