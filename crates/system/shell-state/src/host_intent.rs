// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Host→runtime intent surface.
//!
//! **Migrated to `chrome::host_intent` (2026-05-10)** as part
//! of the frame_model migration (frame_model references HostIntent).
//! Re-exports for backward compat. New code should import from
//! `chrome::host_intent`.
//!
//! Future move: this type ultimately belongs in `host-contract`
//! (the host-runtime boundary contract). Living in `chrome`
//! is a transitional home until the host-contract crate absorbs it.

pub use chrome::host_intent::*;
