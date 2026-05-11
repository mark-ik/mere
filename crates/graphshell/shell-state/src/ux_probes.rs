/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! UX probes — assertion-shaped consumers of the UxEvent stream.
//!
//! **Migrated to `mere-ux-events::ux_probes` (2026-05-10).**
//! Re-exports for backward compat. New code should import from
//! `mere_ux_events::ux_probes`.

pub use mere_ux_events::ux_probes::*;
