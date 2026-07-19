// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Toolbar session state.
//!
//! **Migrated to `chrome::toolbar` (2026-05-09).** This module
//! re-exports the canonical types from the new location so existing
//! `shell_state::toolbar::ToolbarState` / `ToolbarEditable`
//! / `ToolbarDraft` call sites resolve unchanged. New code should
//! import from `chrome::toolbar` directly.

pub use chrome::toolbar::*;
