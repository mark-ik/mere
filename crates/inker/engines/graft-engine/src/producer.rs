/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! [`GraftSurface`] — the host seam (graft's composite producer, since graft has
//! no single library producer type) — and [`GraftProducer`], the adapter that
//! satisfies `inker::SurfaceProducer` over it.

use inker::{
    CursorShape, FocusReason, KeyboardEvent, MouseEvent, NativeTextureHandle, NavigationEvent,
    PointerEvent, SurfaceError, SurfaceFrame, SurfaceProducer, SurfaceSettings, SurfaceSyncHandle,
    WebMessage,
};

/// A frame produced by a [`GraftSurface`]: the shared GPU texture handle the
/// host imports, plus the import-once metadata that `inker::SurfaceFrame` omits.
pub struct GraftFrame {
    /// The platform shared-texture handle (Windows: a DX12 shared HANDLE from the
    /// adapter's `current_dx12_shared_texture()`; Linux: a DMA-BUF fd; macOS: an
    /// IOSurface ref). The host imports this on its own wgpu device.
    pub texture: NativeTextureHandle,
    pub sync: SurfaceSyncHandle,
    pub width: u32,
    pub height: u32,
    /// `true` when graft (re)allocated the shared texture (first frame / resize /
    /// context restart) and the host must (re)import it; `false` when it
    /// overwrote the same allocation and the host should keep sampling its
    /// existing import. This is the handle-handoff metadata `inker::SurfaceFrame`
    /// drops; it is preserved here for the host's surface pool (see the
    /// engine-picker plan's frame-transport option (a)). Reach it via
    /// [`GraftProducer::inner_mut`].
    pub is_new: bool,
}

/// The host-implemented graft composite: a `servo::Servo` instance + `WebView` +
/// `servo_wgpu_interop_adapter::ServoWgpuInteropAdapter`. This crate cannot
/// fabricate any of those (it does not depend on Servo), so it defines the seam
/// and the host wires it. The live grafting/Servo calls each method maps to,
/// when the Servo lane is built:
///
/// - [`resize`](GraftSurface::resize) → adapter + `WebView` resize.
/// - [`acquire_frame`](GraftSurface::acquire_frame) → `Servo::spin_event_loop()`,
///   then the adapter's `current_dx12_shared_texture()` (Windows shared-handle
///   path) / `import_current_frame_default()` / `read_full_frame()`.
/// - [`load_url`](GraftSurface::load_url) / [`load_html`](GraftSurface::load_html)
///   → `WebViewBuilder` / `WebView::load`.
/// - `go_back` / `go_forward` → `WebView::go_back` / `go_forward`.
/// - `notify_*` → `WebView::notify_input_event(servo::InputEvent::…)`.
/// - `poll_*` → drained from the `WebViewDelegate` callbacks the host registers.
///
/// Not `Send`: a graft surface owns Servo's non-`Send` GL context; the host
/// drives it from one thread per surface (the `inker::SurfaceProducer` contract).
pub trait GraftSurface {
    fn resize(&mut self, width: u32, height: u32) -> Result<(), SurfaceError>;

    /// Pump Servo and return the latest frame, if a new composited frame is
    /// ready. `Ok(None)` when nothing new this tick.
    fn acquire_frame(&mut self) -> Result<Option<GraftFrame>, SurfaceError>;

    fn load_url(&mut self, url: &str) -> Result<(), SurfaceError>;
    fn load_html(&mut self, html: &str) -> Result<(), SurfaceError>;
    fn reload(&mut self) -> Result<(), SurfaceError>;
    fn stop(&mut self) -> Result<(), SurfaceError>;
    fn go_back(&mut self) -> Result<(), SurfaceError>;
    fn go_forward(&mut self) -> Result<(), SurfaceError>;
    fn can_go_back(&self) -> bool;
    fn can_go_forward(&self) -> bool;

    fn notify_mouse(&mut self, ev: MouseEvent) -> Result<(), SurfaceError>;
    fn notify_pointer(&mut self, ev: PointerEvent) -> Result<(), SurfaceError>;
    fn notify_keyboard(&mut self, ev: KeyboardEvent) -> Result<(), SurfaceError>;
    fn focus(&mut self, reason: FocusReason) -> Result<(), SurfaceError>;

    fn poll_navigation_event(&mut self) -> Option<NavigationEvent>;
    fn poll_cursor_shape(&mut self) -> Option<CursorShape>;
    fn poll_web_message(&mut self) -> Option<WebMessage>;

    fn apply_settings(&mut self, settings: &SurfaceSettings) -> Result<(), SurfaceError>;
    fn capture_snapshot_png(&mut self) -> Result<Vec<u8>, SurfaceError>;
}

/// Adapts a `Box<dyn GraftSurface>` onto `inker::SurfaceProducer`.
pub struct GraftProducer {
    inner: Box<dyn GraftSurface>,
}

impl GraftProducer {
    pub fn new(inner: Box<dyn GraftSurface>) -> Self {
        Self { inner }
    }

    /// Borrow the underlying graft surface for the typed [`GraftFrame`] (with
    /// `is_new`) that `SurfaceProducer::acquire_frame` flattens to a bare
    /// `SurfaceFrame`. The host's surface pool uses this for the import-once /
    /// resample protocol. Mirrors `ScryingProducer::inner_mut`.
    pub fn inner_mut(&mut self) -> &mut dyn GraftSurface {
        self.inner.as_mut()
    }
}

impl SurfaceProducer for GraftProducer {
    fn resize(&mut self, width: u32, height: u32) -> Result<(), SurfaceError> {
        self.inner.resize(width, height)
    }

    /// No-op: a graft surface renders offscreen and the host composites the
    /// imported texture at the tile rect, so there is no producer-side on-host
    /// visual to offset (unlike a scrying WebView2 composition visual).
    fn set_offset(&mut self, _x: i32, _y: i32) -> Result<(), SurfaceError> {
        Ok(())
    }

    fn acquire_frame(&mut self) -> Result<Option<SurfaceFrame>, SurfaceError> {
        // Flattens GraftFrame -> SurfaceFrame, dropping `is_new` (the inker
        // contract has no slot for it); the host pool reads it via `inner_mut`.
        Ok(self.inner.acquire_frame()?.map(|f| SurfaceFrame {
            texture: f.texture,
            sync: f.sync,
            width: f.width,
            height: f.height,
        }))
    }

    fn navigate_to_url(&mut self, url: &str) -> Result<(), SurfaceError> {
        self.inner.load_url(url)
    }

    fn navigate_to_string(&mut self, html: &str) -> Result<(), SurfaceError> {
        self.inner.load_html(html)
    }

    fn reload(&mut self) -> Result<(), SurfaceError> {
        self.inner.reload()
    }

    fn stop(&mut self) -> Result<(), SurfaceError> {
        self.inner.stop()
    }

    fn go_back(&mut self) -> Result<(), SurfaceError> {
        self.inner.go_back()
    }

    fn go_forward(&mut self) -> Result<(), SurfaceError> {
        self.inner.go_forward()
    }

    fn can_go_back(&self) -> bool {
        self.inner.can_go_back()
    }

    fn can_go_forward(&self) -> bool {
        self.inner.can_go_forward()
    }

    fn send_mouse_input(&mut self, ev: MouseEvent) -> Result<(), SurfaceError> {
        self.inner.notify_mouse(ev)
    }

    fn send_pointer_input(&mut self, ev: PointerEvent) -> Result<(), SurfaceError> {
        self.inner.notify_pointer(ev)
    }

    fn send_keyboard_input(&mut self, ev: KeyboardEvent) -> Result<(), SurfaceError> {
        self.inner.notify_keyboard(ev)
    }

    fn move_focus(&mut self, reason: FocusReason) -> Result<(), SurfaceError> {
        self.inner.focus(reason)
    }

    fn poll_navigation_event(&mut self) -> Option<NavigationEvent> {
        self.inner.poll_navigation_event()
    }

    fn poll_cursor_shape(&mut self) -> Option<CursorShape> {
        self.inner.poll_cursor_shape()
    }

    fn poll_web_message(&mut self) -> Option<WebMessage> {
        self.inner.poll_web_message()
    }

    fn apply_settings(&mut self, settings: &SurfaceSettings) -> Result<(), SurfaceError> {
        self.inner.apply_settings(settings)
    }

    fn capture_snapshot_png(&mut self) -> Result<Vec<u8>, SurfaceError> {
        self.inner.capture_snapshot_png()
    }
}
