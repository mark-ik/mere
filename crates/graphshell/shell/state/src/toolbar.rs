/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Toolbar session state.
//!
//! **Migrated to `mere-graphshell::toolbar` (2026-05-09).** This module
//! re-exports the canonical types from the new location so existing
//! `shell_state::toolbar::ToolbarState` / `ToolbarEditable`
//! / `ToolbarDraft` call sites resolve unchanged. New code should
//! import from `chrome::toolbar` directly.

pub use chrome::toolbar::*;
