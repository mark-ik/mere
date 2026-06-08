/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Chrome color tokens — the browser-shell band a host paints around the graph
//! (toolbar / omnibar / suggestions / command palette / settings / context menu
//! / comms pane). These were hardcoded in meerkat's `CHROME_SHEET` CSS; lifting
//! them into a [`ChromeTheme`] makes the shell chrome theme-switchable the same
//! way the graph chrome already is, and keeps one source of truth per theme.
//!
//! The token set is deliberately small (a handful of surfaces + text weights)
//! and reused across the many CSS classes — the toolbar, dropdowns, palette, and
//! comms pane all draw from the same dozen-and-change colors, so a theme reads as
//! one coherent surface rather than a pile of one-off hex values.

pub use kernel::color::Color32;

/// Color tokens for the host's chrome band (the shell around the graph canvas).
///
/// Surfaces step up in three tiers — `toolbar_bg` (the band) < `panel_bg`
/// (dropdowns docked to it) < `surface_bg` (floated panels: palette / settings)
/// — with `control_bg` / `field_bg` for the interactive widgets on top. Text
/// comes in three weights (`body_text` / `strong_text` / `muted_text`) plus the
/// `disabled_*` and `error_*` pairs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChromeTheme {
    /// The toolbar band itself (the chrome's base surface).
    pub toolbar_bg: Color32,
    /// Button background (back / forward / palette buttons, settings + comms btns).
    pub control_bg: Color32,
    /// Button label text.
    pub control_text: Color32,
    /// Text-field (omnibar / palette query) background.
    pub field_bg: Color32,
    /// Text-field text.
    pub field_text: Color32,
    /// Dropdown / docked-panel surface (suggestions, comms pane) — a step over the
    /// toolbar band.
    pub panel_bg: Color32,
    /// Floated-panel surface (command palette, settings) — a step over `panel_bg`.
    pub surface_bg: Color32,
    /// Default row / body text on panels.
    pub body_text: Color32,
    /// Emphasized text (active rows, titles).
    pub strong_text: Color32,
    /// De-emphasized text (the sync chip, empty-state hints).
    pub muted_text: Color32,
    /// Selection / active-row fill (the highlighted suggestion or command).
    pub active_bg: Color32,
    /// Disabled control text.
    pub disabled_text: Color32,
    /// Disabled control background.
    pub disabled_bg: Color32,
    /// Small floating menus (the right-click context menu, incoming comms bubbles).
    pub menu_bg: Color32,
    /// Failure-banner text (a comms send/load failure).
    pub error_text: Color32,
    /// Failure-banner background.
    pub error_bg: Color32,
}

impl ChromeTheme {
    /// Mere's dark chrome — the exact palette `CHROME_SHEET` shipped, lifted into
    /// tokens unchanged. Used by both the default and dark themes (both sit on a
    /// near-black graph backdrop).
    pub fn mere_dark() -> Self {
        Self {
            toolbar_bg: Color32::from_rgb(28, 31, 38),
            control_bg: Color32::from_rgb(44, 48, 58),
            control_text: Color32::from_rgb(222, 226, 234),
            field_bg: Color32::from_rgb(36, 39, 48),
            field_text: Color32::from_rgb(232, 234, 240),
            panel_bg: Color32::from_rgb(30, 33, 41),
            surface_bg: Color32::from_rgb(34, 37, 46),
            body_text: Color32::from_rgb(206, 210, 220),
            strong_text: Color32::from_rgb(234, 238, 246),
            muted_text: Color32::from_rgb(150, 156, 168),
            active_bg: Color32::from_rgb(48, 58, 82),
            disabled_text: Color32::from_rgb(108, 114, 126),
            disabled_bg: Color32::from_rgb(34, 37, 45),
            menu_bg: Color32::from_rgb(38, 42, 52),
            error_text: Color32::from_rgb(240, 184, 184),
            error_bg: Color32::from_rgb(50, 32, 32),
        }
    }

    /// A light chrome — paper-toned surfaces with dark text, paired with the light
    /// theme's light graph backdrop.
    pub fn mere_light() -> Self {
        Self {
            toolbar_bg: Color32::from_rgb(238, 240, 244),
            control_bg: Color32::from_rgb(224, 228, 234),
            control_text: Color32::from_rgb(32, 38, 48),
            field_bg: Color32::from_rgb(250, 251, 253),
            field_text: Color32::from_rgb(24, 28, 36),
            panel_bg: Color32::from_rgb(244, 246, 250),
            surface_bg: Color32::from_rgb(248, 250, 253),
            body_text: Color32::from_rgb(60, 66, 78),
            strong_text: Color32::from_rgb(24, 28, 36),
            muted_text: Color32::from_rgb(118, 124, 136),
            active_bg: Color32::from_rgb(200, 216, 244),
            disabled_text: Color32::from_rgb(160, 166, 176),
            disabled_bg: Color32::from_rgb(230, 232, 236),
            menu_bg: Color32::from_rgb(236, 238, 243),
            error_text: Color32::from_rgb(140, 32, 32),
            error_bg: Color32::from_rgb(250, 228, 228),
        }
    }

    /// A maximum-contrast chrome — black surfaces, white text, a muted-gray active
    /// fill (a yellow fill would fail contrast against the white row text).
    pub fn high_contrast() -> Self {
        Self {
            toolbar_bg: Color32::from_rgb(0, 0, 0),
            control_bg: Color32::from_rgb(0, 0, 0),
            control_text: Color32::from_rgb(255, 255, 255),
            field_bg: Color32::from_rgb(0, 0, 0),
            field_text: Color32::from_rgb(255, 255, 255),
            panel_bg: Color32::from_rgb(0, 0, 0),
            surface_bg: Color32::from_rgb(0, 0, 0),
            body_text: Color32::from_rgb(255, 255, 255),
            strong_text: Color32::from_rgb(255, 255, 255),
            muted_text: Color32::from_rgb(255, 255, 255),
            active_bg: Color32::from_rgb(40, 40, 40),
            disabled_text: Color32::from_rgb(160, 160, 160),
            disabled_bg: Color32::from_rgb(0, 0, 0),
            menu_bg: Color32::from_rgb(0, 0, 0),
            error_text: Color32::from_rgb(255, 128, 128),
            error_bg: Color32::from_rgb(40, 0, 0),
        }
    }
}

impl Default for ChromeTheme {
    fn default() -> Self {
        Self::mere_dark()
    }
}
