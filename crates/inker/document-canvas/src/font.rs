/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Font-resolution seam between the host and document-canvas's renderers.
//!
//! parley needs fonts during *layout* (to shape text into glyph IDs +
//! positions). Downstream renderers (netrender / gpui-native /
//! AccessKit-only) need to know *which concrete font* to render those
//! glyphs against. Both lookups should go through the host's font
//! collection so layout and render stay consistent.
//!
//! The host implements [`FontResolver`] once and hands it to:
//!
//! - [`crate::text::LayoutEnvironment::with_resolver`] — the resolver
//!   registers its fonts with parley's `FontContext` so layout sees them.
//! - [`crate::netrender_backend::scene_from_packet`] (when the
//!   `netrender` feature is on) — the resolver maps each glyph run's
//!   `(family, weight, style)` to a `netrender::FontId` so real glyph
//!   runs get emitted instead of placeholder rects.
//!
//! ## Re-resolve at render time
//!
//! v1 *re-resolves* the (family, weight, style) tuple at render time
//! rather than baking a `FontId` into [`crate::types::GlyphRun`]. This
//! keeps the packet shape pure (no rendering metadata in the layout
//! result) and lets the same packet feed multiple backends. The cost: if
//! parley fell back to a different concrete font than the resolver
//! advertises (rare but possible), the rendered glyphs may not match the
//! laid-out ones. v2 can plumb parley's actual font identity through.

use crate::types::TextStyle;

/// A request to resolve a font face. Carries everything a resolver needs
/// to map back to a concrete font in its registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontRequest<'a> {
    pub family: &'a str,
    /// CSS-style weight: 100..=900 (400 = regular, 700 = bold).
    pub weight: u16,
    pub style: TextStyle,
}

/// Trait the host implements to provide fonts for both layout (parley)
/// and rendering (netrender / future backends).
pub trait FontResolver: Send + Sync {
    /// Register all fonts this resolver provides with parley's font
    /// context. Called once when the [`crate::text::LayoutEnvironment`]
    /// is constructed via [`crate::text::LayoutEnvironment::with_resolver`].
    ///
    /// Default impl is a no-op so resolvers that only handle the render
    /// side (e.g. resolvers backed by parley's bundled fonts) can opt
    /// out without boilerplate.
    fn register_with_parley(&self, _font_cx: &mut parley::FontContext) {}

    /// Map a font request to an opaque `netrender::FontId`. Returns
    /// `None` if the resolver doesn't have a font for this request — the
    /// renderer falls back to a placeholder rect in that case.
    ///
    /// The same `FontRequest` must always map to the same ID for a given
    /// resolver instance. Renderers may cache by ID.
    fn resolve_font_id(&self, request: FontRequest<'_>) -> Option<u32>;
}

/// A no-op resolver. Registers nothing with parley (parley falls back to
/// its own bundled defaults), returns `None` for every render-side
/// lookup. Useful as a default when the host hasn't wired fonts yet.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoFontResolver;

impl FontResolver for NoFontResolver {
    fn resolve_font_id(&self, _request: FontRequest<'_>) -> Option<u32> {
        None
    }
}
