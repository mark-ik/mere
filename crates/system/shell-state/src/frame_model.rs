// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Frame view-model + host-input types.
//!
//! **Migrated to `chrome::frame_model` (2026-05-10).**
//! Re-exports for backward compat. New code should import from
//! `chrome::frame_model`.
//!
//! Per-type future destinations (TODO):
//! - `FrameHostInput` belongs in `host-contract` (host→runtime
//!   input).
//! - `SettingsViewModel` + `FocusRingSettingsView` +
//!   `ThumbnailSettings*` belong in a future `mere-system` crate
//!   (settings module).
//! - `AccessibilityViewModel` may move to `apparatus` as a11y
//!   inspection grows.
//!
//! For now everything stays under `chrome::frame_model` for
//! coherence with the other chrome view-models (toolbar, omnibar,
//! command_palette).

pub use chrome::frame_model::*;
