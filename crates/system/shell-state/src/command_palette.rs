// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Command palette session state.
//!
//! **Migrated to `chrome::command_palette` (2026-05-09).**
//! This module re-exports the canonical types from the new location
//! for backward compat. New code should import from
//! `chrome::command_palette`.

pub use chrome::command_palette::*;
