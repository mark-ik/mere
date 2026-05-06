/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable host services consumed directly by `GraphshellRuntime::tick`.
//!
//! Originally scoped to finalize-action side effects (`ToastSpec`
//! delivery and clipboard writes). 2026-04-25 servo-into-verso S3a
//! expanded the crate to own the full host-port trait surface so the
//! iced launch path can compile without `servo-engine`:
//!
//! - [`HostInputPort`] — raw input ingress
//! - [`HostSurfacePort`] — content-surface mounting & presentation
//! - [`HostPaintPort`] — overlay painting
//! - [`HostTexturePort`] — favicon / image cache
//! - [`HostAccessibilityPort`] — accesskit bridging
//! - [`HostClipboardPort`] / [`HostToastPort`] — runtime-owned
//!   finalize-action services (kept here from M3.5)
//!
//! Per VERSO_AS_PEER's "graphshell is a chrome + spatial canvas; the
//! content engines are pluggable" framing, the trait surface above
//! is what the chrome-side runtime uses; concrete impls live in
//! whichever host adapter (egui or iced) is presenting.
//!
//! ## Identity vocabulary
//!
//! [`ViewerSurfaceId`] is the host-neutral identity for a content
//! surface (Servo webview, wry overlay, middlenet pane, etc.). Servo
//! had its own `WebViewId` type that bundled a pipeline namespace
//! and an index; for a chrome that doesn't depend on Servo we want
//! a stable opaque key whose only requirement is `Copy + Eq + Hash`.
//! `ViewerSurfaceId` is a pair of `u32`s so existing `WebViewId`
//! converters (graphshell-main side, behind `servo-engine`) can be
//! a one-line `From` impl.

use std::sync::Arc;
use std::time::Duration;

use graphshell_core::geometry::{PortablePoint, PortableRect};
use graphshell_core::graph::NodeKey;
use graphshell_core::host_event::{HostEvent, ModifiersState};
use graphshell_core::overlay::GlyphOverlay;
use graphshell_core::shell_state::frame_model::{ToastSeverity, ToastSpec};

// ---------------------------------------------------------------------------
// Identity vocabulary
// ---------------------------------------------------------------------------

/// Opaque viewer-surface identity used across host-port traits.
///
/// Two `u32` fields mirror Servo's `WebViewId` shape (namespace +
/// index) so the `servo-engine`-on path can convert from
/// `servo::WebViewId` with a one-line `From` impl living in
/// graphshell main. Without `servo-engine`, hosts mint these
/// directly (e.g., from a UUID hash, a counter, a wry handle index).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ViewerSurfaceId {
    /// Namespace component. Servo uses this to disambiguate webviews
    /// across pipeline boundaries; iced/wry hosts can use 0 or any
    /// stable group id.
    pub namespace: u32,
    /// Index within the namespace. Strictly monotonic per host
    /// instance is convenient but not required.
    pub index: u32,
}

impl ViewerSurfaceId {
    /// Construct from raw fields.
    pub const fn new(namespace: u32, index: u32) -> Self {
        Self { namespace, index }
    }

    /// Pack into a single `u64` for hashing/serialization paths that
    /// want a single integer key. Round-trips via `Self::from_u64`.
    pub const fn as_u64(self) -> u64 {
        ((self.namespace as u64) << 32) | (self.index as u64)
    }

    /// Inverse of [`Self::as_u64`].
    pub const fn from_u64(packed: u64) -> Self {
        Self {
            namespace: (packed >> 32) as u32,
            index: packed as u32,
        }
    }
}

// ---------------------------------------------------------------------------
// Backend-neutral surface viewport
// ---------------------------------------------------------------------------

/// Pixel-space viewport descriptor used by [`HostSurfacePort`] content
/// callbacks. Moved here from `shell::desktop::render_backend` so the
/// trait signature is host-neutral. The vertical origin is encoded as
/// `from_bottom_px` because the legacy callback path still uses bottom-origin
/// viewport math; hosts with top-origin painting can ignore or convert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackendViewportInPixels {
    pub left_px: i32,
    pub from_bottom_px: i32,
    pub width_px: i32,
    pub height_px: i32,
}

/// Clipboard get/set for runtime-owned finalize actions.
pub trait RuntimeClipboardPort {
    /// Read current clipboard text. Returns `None` if unavailable or empty.
    fn get_text(&mut self) -> Option<String>;

    /// Write text to the clipboard.
    fn set_text(&mut self, text: &str) -> Result<(), String>;
}

/// Transient notification delivery for runtime-owned finalize actions.
pub trait RuntimeToastPort {
    /// Enqueue a toast for display.
    fn enqueue(&mut self, toast: ToastSpec);

    /// Convenience helper for constructing a `ToastSpec` inline.
    fn enqueue_message(
        &mut self,
        severity: ToastSeverity,
        message: impl Into<String>,
        duration: Option<Duration>,
    ) {
        self.enqueue(ToastSpec {
            severity,
            message: message.into(),
            duration,
        });
    }
}

/// Composite bound for the portable port subset `GraphshellRuntime::tick`
/// actually uses today.
pub trait RuntimeTickPorts: RuntimeClipboardPort + RuntimeToastPort {}

impl<T> RuntimeTickPorts for T where T: RuntimeClipboardPort + RuntimeToastPort {}

// ---------------------------------------------------------------------------
// HostInputPort — raw input ingress
// ---------------------------------------------------------------------------

/// The runtime queries this to read raw input each tick. The host
/// translates its native events (egui's `InputState`, iced's `Event`,
/// etc.) into the host-neutral [`HostEvent`] vocabulary.
pub trait HostInputPort {
    /// Drain input events accumulated since the last tick.
    fn poll_events(&mut self) -> Vec<HostEvent>;

    /// Current pointer hover position in screen coordinates, if any.
    fn pointer_hover_position(&self) -> Option<PortablePoint>;

    /// Does a host-owned widget (text input, dialog field) currently
    /// want keyboard input? When true, the runtime should not route
    /// keyboard events to content.
    fn wants_keyboard_input(&self) -> bool;

    /// Does a host-owned widget currently want pointer input?
    fn wants_pointer_input(&self) -> bool;

    /// Active keyboard modifier state this tick.
    fn modifiers(&self) -> ModifiersState;
}

// ---------------------------------------------------------------------------
// HostSurfacePort — content-surface mounting and presentation
// ---------------------------------------------------------------------------

/// The host uses this to register content-surface callbacks the
/// runtime invokes when a surface needs painting (a Servo webview
/// frame, a wry overlay reposition, a middlenet pane redraw, etc.).
///
/// The associated `BackendContext` lets each host carry whatever
/// context type its painter needs without forcing every host to ship
/// the same backend. The egui-on-Servo host uses
/// `glow::Context`; iced typically uses `()` because it paints
/// inline via `canvas::Program::draw` rather than receiving a
/// borrowed graphics context callback.
pub trait HostSurfacePort {
    /// Backend graphics context type passed into content callbacks.
    /// Hosts that don't need one (iced, headless tests) should set
    /// this to `()`.
    type BackendContext: ?Sized;

    /// Notify the host that a surface's content has changed and
    /// should be presented on the next paint. The host consults its
    /// surface registry to resolve the node key to a concrete handle.
    fn present_surface(&mut self, node_key: NodeKey);

    /// Retire a surface (node closed, tombstoned, or moved off
    /// screen).
    fn retire_surface(&mut self, node_key: NodeKey);

    /// Register a content callback invoked when a surface paints.
    fn register_content_callback(
        &mut self,
        node_key: NodeKey,
        callback: Arc<dyn Fn(&Self::BackendContext, BackendViewportInPixels) + Send + Sync>,
    );

    /// Unregister a previously-registered content callback.
    fn unregister_content_callback(&mut self, node_key: NodeKey);
}

// ---------------------------------------------------------------------------
// HostPaintPort — overlay painting
// ---------------------------------------------------------------------------

/// Overlay painting operations invoked by the runtime's compositor
/// pass. Overlays are described host-neutrally by `OverlayStrokePass`
/// descriptors; this port translates descriptor intent into concrete
/// draw calls against whatever painter the host owns.
pub trait HostPaintPort {
    /// Paint a rectangular stroke outline for an overlay affordance
    /// (focus ring, selection outline, ...).
    fn draw_overlay_stroke(
        &mut self,
        node_key: NodeKey,
        rect: PortableRect,
        stroke: graph_canvas::packet::Stroke,
        rounding: f32,
    );

    /// Paint a dashed rectangular stroke (drag previews, ephemeral
    /// affordances).
    fn draw_dashed_overlay_stroke(
        &mut self,
        node_key: NodeKey,
        rect: PortableRect,
        stroke: graph_canvas::packet::Stroke,
    );

    /// Paint lens glyph overlays positioned relative to a tile rect.
    fn draw_overlay_glyphs(
        &mut self,
        node_key: NodeKey,
        rect: PortableRect,
        glyphs: &[GlyphOverlay],
        color: graph_canvas::packet::Color,
    );

    /// Paint chrome markers (tick/indicator lines at tile edges).
    fn draw_overlay_chrome_markers(
        &mut self,
        node_key: NodeKey,
        rect: PortableRect,
        stroke: graph_canvas::packet::Stroke,
    );

    /// Paint a degraded-mode receipt (small in-tile text banner).
    fn draw_degraded_receipt(&mut self, rect: PortableRect, message: &str);
}

// ---------------------------------------------------------------------------
// HostTexturePort — favicon / image cache
// ---------------------------------------------------------------------------

/// Texture cache. The host owns the concrete handle type (egui's
/// `TextureHandle`, iced's `image::Handle`); the runtime only names
/// textures by a stable key string.
pub trait HostTexturePort {
    /// Opaque handle type — caller treats it as a black box.
    type TextureHandle: Clone;

    /// Load or reuse a texture for `key` from raw pixel data.
    fn load_texture(
        &mut self,
        key: &str,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Self::TextureHandle;

    /// Look up a previously-loaded texture by key.
    fn texture(&self, key: &str) -> Option<Self::TextureHandle>;

    /// Release a texture. Subsequent lookups return `None`.
    fn drop_texture(&mut self, key: &str);
}

// ---------------------------------------------------------------------------
// HostAccessibilityPort — accesskit bridging
// ---------------------------------------------------------------------------

/// Accessibility focus + tree integration that's host-neutral.
///
/// The trait is intentionally narrow today: just programmatic-focus
/// requests, which both egui and iced support uniformly. Tree-update
/// injection lives in a separate Servo-specific extension trait
/// (`shell::desktop::ui::host_ports::ServoAccessibilityInjectionPort`)
/// because Servo's accesskit stream is keyed on `servo::WebViewId`
/// and the egui-host's accesskit anchor derivation operates on the
/// same WebViewId-shaped key throughout. Decoupling that fully (e.g.,
/// switching the egui-host's HashMap to [`ViewerSurfaceId`] keys) is
/// future architectural work; the host-port trait surface in this
/// crate only expresses what's portable today.
pub trait HostAccessibilityPort {
    /// Request the host transfer programmatic focus to a particular
    /// node (e.g., when keyboard navigation lands somewhere
    /// chrome-owned).
    fn request_focus(&mut self, node_id: accesskit::NodeId);
}

// ---------------------------------------------------------------------------
// HostPorts composite — the bundle the runtime borrows per-tick
// ---------------------------------------------------------------------------

/// Composite bound: any type that implements the six non-texture
/// ports automatically qualifies as a `HostPorts`. Texture handling
/// has an associated type on its own trait
/// ([`HostTexturePort`]) and is not part of this composite; call
/// sites that need textures bind `T: HostPorts + HostTexturePort`
/// explicitly.
pub trait HostPorts:
    HostInputPort
    + HostSurfacePort
    + HostPaintPort
    + RuntimeClipboardPort
    + RuntimeToastPort
    + HostAccessibilityPort
{
}

impl<T> HostPorts for T where
    T: HostInputPort
        + HostSurfacePort
        + HostPaintPort
        + RuntimeClipboardPort
        + RuntimeToastPort
        + HostAccessibilityPort
{
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_surface_id_round_trips_through_u64() {
        for (ns, idx) in [(0, 0), (1, 0), (0, 1), (u32::MAX, u32::MAX), (7, 42)] {
            let id = ViewerSurfaceId::new(ns, idx);
            assert_eq!(ViewerSurfaceId::from_u64(id.as_u64()), id);
        }
    }

    #[test]
    fn viewer_surface_id_default_is_zero() {
        let zero = ViewerSurfaceId::default();
        assert_eq!(zero.namespace, 0);
        assert_eq!(zero.index, 0);
        assert_eq!(zero.as_u64(), 0);
    }
}
