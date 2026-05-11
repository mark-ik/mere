/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # mere-host-graphshell
//!
//! gpui renderer for the **graphshell domain** — the application's
//! root chrome row that lives above the frame layout. Composes a
//! toolbar with nav buttons + omnibar (location/search/command-arbitration
//! field) + status indicators.
//!
//! Companion to portable [`mere-graphshell`], which owns the toolbar /
//! omnibar / focus / palette view-models. This crate adds gpui
//! rendering without coupling the portable layer to gpui.
//!
//! ## Modular by aspect, not by file
//!
//! Per the audit principle "each domain, each aspect — testable in
//! isolation":
//!
//! - [`nav`] — back / forward / refresh button row
//! - [`omnibar`] — the omnibar / location field
//! - [`toolbar`] — the chrome row that composes nav + omnibar +
//!   status; the public [`render`] function lives here
//!
//! Each module is independently testable. Larger overlay surfaces
//! (popup menus, toasts, dialogs, floating windows) belong in their
//! own future crates (`mere-host-popups`, `mere-host-toast`,
//! `mere-host-dialog`) rather than under this chrome-row crate.

#![doc(html_root_url = "https://docs.rs/mere-host-graphshell/0.0.1")]

pub mod nav;
pub mod omnibar;
pub mod omnibar_input;
pub mod omnibar_text_element;
pub mod palette;
pub mod panel_buttons;
pub mod toolbar;

// Convenience re-exports so callers don't need to reach into
// individual aspect modules.
pub use omnibar_input::{
    KEY_CONTEXT as OMNIBAR_KEY_CONTEXT, OmnibarCancelled, OmnibarInput, OmnibarSubmitted,
    bindings as omnibar_bindings,
};
pub use palette::{
    KEY_CONTEXT as PALETTE_KEY_CONTEXT, Palette, PaletteClosed, PaletteEntry, PaletteInvoke,
    bindings as palette_bindings,
};
pub use toolbar::{ShellbarOrientation, render};
