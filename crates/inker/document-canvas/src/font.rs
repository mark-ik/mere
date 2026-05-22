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
//! - [`crate::paint_list::paint_list_from_packet`] — the resolver maps
//!   each glyph run's `(family, weight, style)` to the **font face bytes**
//!   ([`FontFaceData`]) so the produced `PaintList` can carry them in its
//!   `fonts()` side-table. The `paint_list_render` translator registers
//!   those bytes into the renderer's font palette and resolves each run's
//!   `FontInstanceKey` to a concrete font. Runs whose face the resolver
//!   can't supply fall back to a placeholder rect.
//!
//! ## Bytes, not pre-registered ids
//!
//! Earlier the resolver returned an opaque `netrender::FontId` (assuming
//! the host had pre-registered the face out-of-band — an in-process-only
//! model). The PaintList contract instead carries face bytes with the
//! paint output so the list is self-contained for IPC / capture-replay;
//! the renderer owns registration and dedups by blob identity across
//! resends. Hence [`FontResolver::resolve_font_data`].
//!
//! ## Re-resolve at render time
//!
//! v1 *re-resolves* the (family, weight, style) tuple at packet-emit time
//! rather than baking font identity into [`crate::types::GlyphRun`]. This
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

/// Font face bytes for a resolved request, destined for a `PaintList`'s
/// font side-table ([`paint_list_api::FontResource`]). The renderer
/// interns these into its font palette; producers ship them once per
/// unique face and reference by `FontInstanceKey`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFaceData {
    /// TTF / OTF / TTC font bytes.
    pub data: Vec<u8>,
    /// Index within a font collection (TTC); `0` for single-font files.
    pub index: u32,
}

/// Trait the host implements to provide fonts for both layout (parley)
/// and rendering (the PaintList side-table → renderer palette).
pub trait FontResolver: Send + Sync {
    /// Register all fonts this resolver provides with parley's font
    /// context. Called once when the [`crate::text::LayoutEnvironment`]
    /// is constructed via [`crate::text::LayoutEnvironment::with_resolver`].
    ///
    /// Default impl is a no-op so resolvers that only handle the render
    /// side (e.g. resolvers backed by parley's bundled fonts) can opt
    /// out without boilerplate.
    fn register_with_parley(&self, _font_cx: &mut parley::FontContext) {}

    /// Map a font request to its face bytes ([`FontFaceData`]) for the
    /// produced `PaintList`'s font side-table. Returns `None` if the
    /// resolver doesn't have a face for this request — the producer
    /// falls back to a placeholder rect for that run.
    ///
    /// The same `FontRequest` must map to the same face for a given
    /// resolver instance (producers dedup by request and ship each face
    /// once).
    fn resolve_font_data(&self, request: FontRequest<'_>) -> Option<FontFaceData>;
}

/// A no-op resolver. Registers nothing with parley (parley falls back to
/// its own bundled defaults), returns `None` for every render-side
/// lookup. Useful as a default when the host hasn't wired fonts yet.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoFontResolver;

impl FontResolver for NoFontResolver {
    fn resolve_font_data(&self, _request: FontRequest<'_>) -> Option<FontFaceData> {
        None
    }
}
