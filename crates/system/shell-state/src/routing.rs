// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Return-target classification used by focus-restore and command-
//! surface routing.
//!
//! **Migrated to `chrome::routing` (2026-05-09).** This module
//! re-exports the canonical types from the new location for backward
//! compat. New code should import from `chrome::routing`.

pub use chrome::routing::*;
