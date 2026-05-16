// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Renderer identity / capability data types.

use std::borrow::Cow;

/// Stable identifier for a registered renderer.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct RendererId(Cow<'static, str>);

impl RendererId {
    pub const fn from_static(s: &'static str) -> Self {
        Self(Cow::Borrowed(s))
    }

    pub fn from_dynamic(s: String) -> Self {
        Self(Cow::Owned(s))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl std::fmt::Display for RendererId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What profile-binding scopes a renderer accepts.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ProfileBindingExpectation {
    /// Renderer doesn't need a profile binding.
    None,
    /// Renderer accepts persona-scoped UDF only.
    PersonaOnly,
    /// Renderer accepts persona-, session-, or graph-scoped UDF.
    Any,
}

/// Per-renderer capability declaration.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RendererCapabilities {
    pub accepts_input: bool,
    pub handles_ime: bool,
    pub handles_a11y: bool,
    pub scrollable: bool,
    pub hit_testable_subregions: bool,
    pub profile_binding: ProfileBindingExpectation,
    pub supports_capture: bool,
}

impl RendererCapabilities {
    /// All-`false` baseline.
    pub const NONE: Self = Self {
        accepts_input: false,
        handles_ime: false,
        handles_a11y: false,
        scrollable: false,
        hit_testable_subregions: false,
        profile_binding: ProfileBindingExpectation::None,
        supports_capture: false,
    };

    /// Sensible default for an interactive panel renderer.
    pub const INTERACTIVE_PANEL: Self = Self {
        accepts_input: true,
        handles_ime: true,
        handles_a11y: true,
        scrollable: true,
        hit_testable_subregions: true,
        profile_binding: ProfileBindingExpectation::None,
        supports_capture: true,
    };
}

/// Screen-space rectangle for overlay positioning.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
