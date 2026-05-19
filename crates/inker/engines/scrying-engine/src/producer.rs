/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `ScryingProducer` — adapter wrapping a `Box<dyn scrying::WebSurfaceProducer>`
//! to satisfy `inker::SurfaceProducer`. Delegates each method via the
//! translation helpers in [`crate::translation`].
//!
//! Blocking caveat: scrying's `WebSurfaceProducer::navigate_to_url` /
//! `navigate_to_string` are blocking-with-timeout (and panic if called from
//! a host event-loop callback on macOS). v1 of this adapter uses them with
//! a short default timeout; once the host wires up event-loop-safe
//! non-blocking nav (concrete-producer-specific `load_url` on Windows/macOS),
//! this should switch to those.

use std::time::Duration;

use inker::{
    CursorShape, FocusReason, KeyboardEvent, MouseEvent, NavigationEvent, PointerEvent,
    SurfaceError, SurfaceFrame, SurfaceProducer, SurfaceSettings, WebMessage,
};
use scrying::WebSurfaceProducer;

use crate::translation::{
    map_cursor_shape, map_error, map_focus_reason, map_frame, map_keyboard, map_mouse,
    map_navigation_event, map_pointer, map_settings, wrap_web_message,
};

/// Default navigation timeout. Generous enough for typical HTTPS loads;
/// callers needing more control should drive navigation through the
/// concrete scrying producer directly.
const NAV_TIMEOUT: Duration = Duration::from_secs(15);

pub struct ScryingProducer {
    inner: Box<dyn WebSurfaceProducer>,
}

impl ScryingProducer {
    pub fn new(inner: Box<dyn WebSurfaceProducer>) -> Self {
        Self { inner }
    }

    /// Borrow the underlying scrying producer for platform-specific calls
    /// the inker trait doesn't expose. Hosts use this for non-blocking
    /// `load_url` / cookie store / download handler / etc.
    pub fn inner_mut(&mut self) -> &mut dyn WebSurfaceProducer {
        self.inner.as_mut()
    }
}

impl SurfaceProducer for ScryingProducer {
    fn resize(&mut self, width: u32, height: u32) -> Result<(), SurfaceError> {
        self.inner
            .resize(dpi::PhysicalSize::new(width, height))
            .map_err(map_error)
    }

    fn set_offset(&mut self, x: i32, y: i32) -> Result<(), SurfaceError> {
        self.inner.set_offset(x as f32, y as f32).map_err(map_error)
    }

    fn acquire_frame(&mut self) -> Result<Option<SurfaceFrame>, SurfaceError> {
        let frame = self.inner.acquire_frame().map_err(map_error)?;
        Ok(map_frame(frame))
    }

    fn navigate_to_url(&mut self, url: &str) -> Result<(), SurfaceError> {
        self.inner
            .navigate_to_url(url, NAV_TIMEOUT)
            .map_err(map_error)
    }

    fn navigate_to_string(&mut self, html: &str) -> Result<(), SurfaceError> {
        self.inner
            .navigate_to_string(html, NAV_TIMEOUT)
            .map_err(map_error)
    }

    fn reload(&mut self) -> Result<(), SurfaceError> {
        self.inner.reload().map_err(map_error)
    }

    fn stop(&mut self) -> Result<(), SurfaceError> {
        self.inner.stop().map_err(map_error)
    }

    fn go_back(&mut self) -> Result<(), SurfaceError> {
        self.inner.go_back().map(|_| ()).map_err(map_error)
    }

    fn go_forward(&mut self) -> Result<(), SurfaceError> {
        self.inner.go_forward().map(|_| ()).map_err(map_error)
    }

    fn can_go_back(&self) -> bool {
        self.inner.can_go_back()
    }

    fn can_go_forward(&self) -> bool {
        self.inner.can_go_forward()
    }

    fn send_mouse_input(&mut self, ev: MouseEvent) -> Result<(), SurfaceError> {
        self.inner
            .send_mouse_input(map_mouse(ev))
            .map_err(map_error)
    }

    fn send_pointer_input(&mut self, ev: PointerEvent) -> Result<(), SurfaceError> {
        self.inner
            .send_pointer_input(map_pointer(ev))
            .map_err(map_error)
    }

    fn send_keyboard_input(&mut self, ev: KeyboardEvent) -> Result<(), SurfaceError> {
        self.inner
            .send_keyboard_input(map_keyboard(ev))
            .map_err(map_error)
    }

    fn move_focus(&mut self, reason: FocusReason) -> Result<(), SurfaceError> {
        self.inner
            .move_focus(map_focus_reason(reason))
            .map_err(map_error)
    }

    fn poll_navigation_event(&mut self) -> Option<NavigationEvent> {
        self.inner
            .poll_navigation_event()
            .and_then(map_navigation_event)
    }

    fn poll_cursor_shape(&mut self) -> Option<CursorShape> {
        self.inner.poll_cursor_shape().map(map_cursor_shape)
    }

    fn poll_web_message(&mut self) -> Option<WebMessage> {
        self.inner.poll_web_message().map(wrap_web_message)
    }

    fn apply_settings(&mut self, settings: &SurfaceSettings) -> Result<(), SurfaceError> {
        self.inner
            .apply_settings(&map_settings(settings))
            .map_err(map_error)
    }

    fn capture_snapshot_png(&mut self) -> Result<Vec<u8>, SurfaceError> {
        self.inner.capture_snapshot_png().map_err(map_error)
    }
}
