/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Platform a11y adapter.
//!
//! Walks the bridge from a portable `accesskit::TreeUpdate` (which
//! `uxtree::UxTree::to_tree_update` already produces) to the
//! per-platform OS-level adapter:
//!
//! - **Windows**: `accesskit_windows::Adapter` subclasses the HWND
//!   to intercept `WM_GETOBJECT` and expose the tree to UI Automation
//!   / screen readers (Narrator, NVDA, JAWS, …).
//! - **Linux**: `accesskit_unix::Adapter` registers an AT-SPI peer.
//! - **macOS**: `accesskit_macos::Adapter` retrofits onto the NSView.
//!
//! All three accept a raw window/view handle, which the Glass-HQ
//! `gpui::Window` exposes through `HasWindowHandle`. Per-platform
//! variants live in this single file so the host wires it once and
//! the platform conditional stays small.

use accesskit::TreeUpdate;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Cross-platform wrapper. `new` may fail to construct an actual
/// adapter (handle extraction failures, unsupported platforms,
/// missing AT-SPI bus, etc.) — when it does, calls become no-ops and
/// the application still runs.
///
/// **Re-entry warning**: every accesskit call here can synchronously
/// re-enter the platform's window message dispatch (UIA's
/// `UiaLookupId` etc.). Callers must therefore only invoke these
/// methods from **outside** any borrow of gpui's `App` — typically
/// from inside a spawn-task body, never from inside `Render::render`
/// or an `on_next_frame` callback.
pub struct A11yAdapter {
    inner: Option<PlatformAdapter>,
}

impl A11yAdapter {
    /// Snapshot the raw window handle from a gpui `Window`. Cheap
    /// and side-effect-free — safe to call from `Render`. The
    /// returned value can later be passed to `A11yAdapter::from_raw`
    /// from a spawn-task body without re-entering gpui.
    pub fn capture_handle(window: &impl HasWindowHandle) -> Option<RawHandle> {
        let raw = window.window_handle().ok()?.as_raw();
        match raw {
            RawWindowHandle::Win32(w32) => Some(RawHandle::Win32 {
                hwnd: w32.hwnd.get(),
            }),
            RawWindowHandle::AppKit(_) | RawWindowHandle::Xlib(_) | RawWindowHandle::Wayland(_) => {
                // TODO: capture per-platform handle bits when we
                // wire the corresponding accesskit adapters.
                None
            }
            _ => None,
        }
    }

    /// Build the platform adapter from a previously-captured handle.
    /// Must be called outside any gpui `App` borrow (see the
    /// re-entry warning on the type).
    pub fn from_raw(handle: RawHandle, initial_tree: TreeUpdate) -> Self {
        let inner = PlatformAdapter::from_raw(handle, initial_tree);
        if inner.is_some() {
            tracing::info!("a11y adapter active for this window");
        } else {
            tracing::debug!("a11y adapter unavailable (handle / platform / runtime)");
        }
        Self { inner }
    }

    /// Push a new `TreeUpdate` to the OS. No-op when the adapter
    /// failed to construct. Must be called outside any `App` borrow.
    pub fn update(&mut self, tree: TreeUpdate) {
        if let Some(inner) = self.inner.as_mut() {
            inner.update(tree);
        }
    }

    /// Notify the OS that the window's focus state changed. Some
    /// screen readers care about this (e.g. to choose which window's
    /// tree to announce on Tab cycling).
    #[allow(dead_code)] // wired in when gpui surfaces window-focus events
    pub fn set_window_focused(&mut self, focused: bool) {
        if let Some(inner) = self.inner.as_mut() {
            inner.set_window_focused(focused);
        }
    }
}

/// Platform-specific raw window handle, captured up front so it can
/// be passed across an async boundary into a spawn-task body without
/// needing a `Window` reference.
#[derive(Clone, Copy, Debug)]
pub enum RawHandle {
    Win32 { hwnd: isize },
    // Future platform variants land alongside their adapters.
}

// ---------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------

#[cfg(windows)]
mod platform {
    use super::{RawHandle, TreeUpdate};
    use accesskit::{ActionHandler, ActionRequest};
    use accesskit_windows::{Adapter, HWND};

    pub(super) struct PlatformAdapter {
        adapter: Adapter,
    }

    /// Stub action handler — v0 doesn't yet route accesskit actions
    /// back into gpui's action dispatcher. When that lands, this
    /// becomes the bridge: each `ActionRequest` translates to an
    /// `App::dispatch_action` call against the matching gpui action.
    struct StubActionHandler;
    impl ActionHandler for StubActionHandler {
        fn do_action(&mut self, request: ActionRequest) {
            tracing::debug!(
                action = ?request.action,
                target_node = ?request.target_node,
                "accesskit action request (not yet routed to gpui)"
            );
        }
    }

    impl PlatformAdapter {
        pub fn from_raw(handle: RawHandle, initial_tree: TreeUpdate) -> Option<Self> {
            let RawHandle::Win32 { hwnd } = handle;
            let hwnd = HWND(hwnd as *mut _);
            // `Adapter::new` calls `init_uia()` internally, which
            // synchronously pumps platform messages. Doing this
            // from inside gpui's render path re-enters the wndproc
            // while gpui's `App` RefCell is borrowed and panics.
            // Callers therefore drive `from_raw` from a spawn-task
            // body, where no `App` borrow is held.
            let mut adapter = Adapter::new(hwnd, true, StubActionHandler);
            if let Some(events) = adapter.update_if_active(|| initial_tree) {
                events.raise();
            }
            Some(Self { adapter })
        }

        pub fn update(&mut self, tree: TreeUpdate) {
            if let Some(events) = self.adapter.update_if_active(|| tree) {
                events.raise();
            }
        }

        #[allow(dead_code)]
        pub fn set_window_focused(&mut self, focused: bool) {
            if let Some(events) = self.adapter.update_window_focus_state(focused) {
                events.raise();
            }
        }
    }
}

// ---------------------------------------------------------------------
// Linux (AT-SPI via D-Bus)
// ---------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod platform {
    use super::{RawHandle, TreeUpdate};

    pub(super) struct PlatformAdapter;

    impl PlatformAdapter {
        pub fn from_raw(_handle: RawHandle, _initial_tree: TreeUpdate) -> Option<Self> {
            // TODO: wire accesskit_unix::Adapter once we have a Linux
            // dev environment to validate against.
            None
        }
        pub fn update(&mut self, _tree: TreeUpdate) {}
        pub fn set_window_focused(&mut self, _focused: bool) {}
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{RawHandle, TreeUpdate};

    pub(super) struct PlatformAdapter;

    impl PlatformAdapter {
        pub fn from_raw(_handle: RawHandle, _initial_tree: TreeUpdate) -> Option<Self> {
            // TODO: wire accesskit_macos::Adapter — needs the NSView
            // pointer captured via a future `RawHandle::AppKit`
            // variant.
            None
        }
        pub fn update(&mut self, _tree: TreeUpdate) {}
        pub fn set_window_focused(&mut self, _focused: bool) {}
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod platform {
    use super::{RawHandle, TreeUpdate};

    pub(super) struct PlatformAdapter;

    impl PlatformAdapter {
        pub fn from_raw(_handle: RawHandle, _initial_tree: TreeUpdate) -> Option<Self> {
            None
        }
        pub fn update(&mut self, _tree: TreeUpdate) {}
        pub fn set_window_focused(&mut self, _focused: bool) {}
    }
}

use platform::PlatformAdapter;
