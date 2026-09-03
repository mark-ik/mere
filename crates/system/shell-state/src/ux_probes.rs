// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! UX probes — assertion-shaped consumers of the UxEvent stream.
//!
//! **Migrated to `ux-events::ux_probes` (2026-05-10).**
//! Re-exports for backward compat. New code should import from
//! `ux_events::ux_probes`.

pub use ux_events::ux_probes::*;
