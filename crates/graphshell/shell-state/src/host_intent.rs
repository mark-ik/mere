/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host→runtime intent surface.
//!
//! **Migrated to `mere-graphshell::host_intent` (2026-05-10)** as part
//! of the frame_model migration (frame_model references HostIntent).
//! Re-exports for backward compat. New code should import from
//! `mere_graphshell::host_intent`.
//!
//! Future move: this type ultimately belongs in `mere-host-contract`
//! (the host-runtime boundary contract). Living in `mere-graphshell`
//! is a transitional home until the host-contract crate absorbs it.

pub use mere_graphshell::host_intent::*;
