/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The scrying pool: system-WebView tiles resident on the UI thread.
//!
//! The constellation's content actors render `netrender::Scene`s off-thread;
//! a scrying tile inverts that — the WebView renders itself, and the host
//! imports its GPU frames and composites them at the card rect (scrying tile
//! plan, X1). The producer is UI-thread-bound (WebView2's composition
//! controller is COM/HWND-tied and its callbacks ride the winit message
//! pump), so this pool lives beside the constellation, not inside it.
//!
//! X1 scope: Windows (WebView2 composition producer), the focused card only,
//! frames acquired per redraw via the non-blocking `try_acquire_frame`. The
//! WebView2 frame protocol is handle-handoff: a frame with
//! `resource_is_new == true` carries a fresh shared D3D11 texture the host
//! must import (and whose NT handle the host must close after import);
//! subsequent frames reuse the same allocation — the producer's
//! `CopyResource` overwrites the memory behind the already-imported
//! `wgpu::Texture`, so the host keeps sampling the same view. That protocol
//! is only reachable on the concrete producer (`acquire_full_frame` /
//! `try_acquire_frame`); the type-erased `inker::SurfaceProducer` lane drops
//! the metadata, which is why this pool holds `PlatformWebSurfaceProducer`
//! directly rather than going through the engine registry (recorded in the
//! plan's Findings).

use mere::forme::GraphMemberId;

/// A captured live-surface thumbnail ready to persist onto the node when a WebView goes away.
pub struct CapturedThumbnail {
    pub png_bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub url: Option<String>,
}

/// A mouse button, host-neutral (mapped to scrying's vocabulary inside the
/// Windows pool). (Scrying tile plan, X2.)
#[derive(Clone, Copy)]
pub enum MouseBtn {
    Left,
    Right,
    Middle,
}

/// A mouse action forwarded into a scrying tile: a move, a button press, or a
/// button release. Positions are **tile-local** (window point minus the card
/// rect origin); the caller offsets before forwarding. (X2.)
#[derive(Clone, Copy)]
pub enum MousePress {
    Move,
    Down(MouseBtn),
    Up(MouseBtn),
}

/// Keyboard modifier state forwarded alongside a key. (X2.)
#[derive(Clone, Copy, Default)]
pub struct KeyMods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

/// One window's live compatibility-view producer pool: the HWND-parented
/// WebViews that serve this window's surface-engine tiles. The *pins* (which
/// members route to `scrying.web`) are shared session state on
/// `SharedState.content.engine_pins`; this pool is per-`WindowView`, because each
/// producer is bound to one window's HWND.
/// (Scrying X1; the per-window/shared split is MW2 (b2).)
#[derive(Default)]
pub struct ScryingHost {
    #[cfg(all(target_os = "windows", feature = "engine-scry"))]
    pool: windows_pool::Pool,
}

impl ScryingHost {
    #[allow(dead_code)] // public API; the per-window carve constructs via `Default`
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop `member`'s WebView (if any). The shared pin is untouched.
    pub fn reap(&mut self, member: GraphMemberId) -> Option<CapturedThumbnail> {
        #[cfg(all(target_os = "windows", feature = "engine-scry"))]
        return self.pool.reap(member);
        #[cfg(not(all(target_os = "windows", feature = "engine-scry")))]
        {
            let _ = member;
            None
        }
    }

    /// Drop every WebView in this window's pool (multi-graph switch, mirrors
    /// `Constellation::clear`). The shared pins are cleared by the caller.
    pub fn clear(&mut self) -> Vec<(GraphMemberId, CapturedThumbnail)> {
        #[cfg(all(target_os = "windows", feature = "engine-scry"))]
        return self.pool.clear();
        #[cfg(not(all(target_os = "windows", feature = "engine-scry")))]
        {
            Vec::new()
        }
    }

    /// Reap every live tile whose member is not in `keep` (the surfaces shown this
    /// frame), tearing down its WebView. A compat node that is no longer shown is
    /// dropped immediately (reap-on-deselect), so its visual cannot freeze at its
    /// last position. Multiple panes share one per-HWND composition target (scry's
    /// `new_attached`), so any number of compat tiles can stay live at once; this is
    /// just the "drop what's gone" pass. Called each frame before the shown surfaces
    /// are driven. (X2/X3 lifecycle; multi-tile.)
    pub fn retain(
        &mut self,
        keep: &std::collections::HashSet<GraphMemberId>,
    ) -> Vec<(GraphMemberId, CapturedThumbnail)> {
        #[cfg(all(target_os = "windows", feature = "engine-scry"))]
        return self.pool.retain(keep);
        #[cfg(not(all(target_os = "windows", feature = "engine-scry")))]
        {
            let _ = keep;
            Vec::new()
        }
    }

    /// Spawn / resize / navigate `member`'s WebView as needed and pump one
    /// non-blocking frame acquisition. Call once per redraw while the tile is
    /// visible. No-op (with a one-time warning) on platforms X1 does not
    /// cover yet.
    #[allow(clippy::too_many_arguments)]
    pub fn drive(
        &mut self,
        member: GraphMemberId,
        url: &str,
        width: u32,
        height: u32,
        origin: (f32, f32),
        window: &std::sync::Arc<winit::window::Window>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        session_dir: &std::path::Path,
    ) {
        #[cfg(all(target_os = "windows", feature = "engine-scry"))]
        self.pool.drive(
            member,
            url,
            width,
            height,
            origin,
            window,
            device,
            queue,
            session_dir,
        );
        #[cfg(not(all(target_os = "windows", feature = "engine-scry")))]
        {
            let _ = (
                member,
                url,
                width,
                height,
                origin,
                window,
                device,
                queue,
                session_dir,
            );
            tracing::warn!("compatibility view: scrying X1 is Windows-only (plan X4)");
        }
    }

    /// The tile's imported WebView texture, ready to composite.
    pub fn texture_view(&self, member: GraphMemberId) -> Option<&wgpu::TextureView> {
        #[cfg(all(target_os = "windows", feature = "engine-scry"))]
        return self.pool.texture_view(member);
        #[cfg(not(all(target_os = "windows", feature = "engine-scry")))]
        {
            let _ = member;
            None
        }
    }

    /// Capture a thumbnail from a live WebView without reaping it. Used for navigation-away
    /// deposits where the tile stays open but its node retargets to a new URL.
    pub fn capture_thumbnail(&mut self, member: GraphMemberId) -> Option<CapturedThumbnail> {
        #[cfg(all(target_os = "windows", feature = "engine-scry"))]
        return self.pool.capture_thumbnail(member);
        #[cfg(not(all(target_os = "windows", feature = "engine-scry")))]
        {
            let _ = member;
            None
        }
    }

    /// The tile's last spawn/frame error, for the card placeholder (the
    /// placeholder consumer lands with X2's chrome integration).
    #[allow(dead_code)]
    pub fn last_error(&self, member: GraphMemberId) -> Option<&str> {
        #[cfg(all(target_os = "windows", feature = "engine-scry"))]
        return self.pool.last_error(member);
        #[cfg(not(all(target_os = "windows", feature = "engine-scry")))]
        {
            let _ = member;
            Some("compatibility view is Windows-only for now")
        }
    }

    /// Forward a mouse move / press / release into `member`'s WebView at
    /// **tile-local** `(x, y)`. No-op off Windows or with no live tile. (X2.)
    pub fn forward_mouse(&mut self, member: GraphMemberId, x: i32, y: i32, press: MousePress) {
        #[cfg(all(target_os = "windows", feature = "engine-scry"))]
        self.pool.forward_mouse(member, x, y, press);
        #[cfg(not(all(target_os = "windows", feature = "engine-scry")))]
        let _ = (member, x, y, press);
    }

    /// Forward a vertical wheel delta (Win32 convention: 120 per notch) into
    /// `member`'s WebView at tile-local `(x, y)`. (X2.)
    pub fn forward_wheel(&mut self, member: GraphMemberId, x: i32, y: i32, delta_y: i32) {
        #[cfg(all(target_os = "windows", feature = "engine-scry"))]
        self.pool.forward_wheel(member, x, y, delta_y);
        #[cfg(not(all(target_os = "windows", feature = "engine-scry")))]
        let _ = (member, x, y, delta_y);
    }

    /// Forward a key event into `member`'s WebView. `vk` is the platform virtual
    /// key code; `text` is the produced character(s), if any. (X2.)
    pub fn forward_key(
        &mut self,
        member: GraphMemberId,
        vk: u32,
        text: Option<&str>,
        pressed: bool,
        mods: KeyMods,
    ) {
        #[cfg(all(target_os = "windows", feature = "engine-scry"))]
        self.pool.forward_key(member, vk, text, pressed, mods);
        #[cfg(not(all(target_os = "windows", feature = "engine-scry")))]
        let _ = (member, vk, text, pressed, mods);
    }

    /// Hand keyboard focus to `member`'s WebView (a click / Tab into the tile). (X2.)
    pub fn focus_tile(&mut self, member: GraphMemberId) {
        #[cfg(all(target_os = "windows", feature = "engine-scry"))]
        self.pool.focus_tile(member);
        #[cfg(not(all(target_os = "windows", feature = "engine-scry")))]
        let _ = member;
    }

    /// Capture a semantic clip fragment from a scriptable live surface at tile-local
    /// coordinates. Non-surface/off-platform builds report a normal command error.
    pub fn capture_clip(
        &mut self,
        member: GraphMemberId,
        x: i32,
        y: i32,
        source_url: &str,
    ) -> Result<crate::web_clip::ClipFragment, String> {
        #[cfg(all(target_os = "windows", feature = "engine-scry"))]
        return self.pool.capture_clip(member, x, y, source_url);
        #[cfg(not(all(target_os = "windows", feature = "engine-scry")))]
        {
            let _ = (member, x, y, source_url);
            Err("no scriptable live surface on this platform".to_string())
        }
    }

    /// Stage a compatibility-view flip for `member`: the place/session captured from
    /// the serval side at the serval -> `scrying.web` pin. When this member's tile next
    /// spawns, the pool sets the carried cookies and navigates (instead of a blank
    /// load), then restores scroll / forms once the load completes (`verso-scry`'s
    /// forward-inject). A no-op off Windows (no producer to drive). (Verso flip.)
    pub fn begin_flip(&mut self, member: GraphMemberId, state: verso_tile::api::PortableViewState) {
        #[cfg(all(target_os = "windows", feature = "engine-scry"))]
        self.pool.begin_flip(member, state);
        #[cfg(not(all(target_os = "windows", feature = "engine-scry")))]
        let _ = (member, state);
    }
}

#[cfg(all(target_os = "windows", feature = "engine-scry"))]
mod factory;
#[cfg(all(target_os = "windows", feature = "engine-scry"))]
mod frame_import;
#[cfg(all(target_os = "windows", feature = "engine-scry"))]
mod scry_surface;
#[cfg(all(target_os = "windows", feature = "engine-scry"))]
mod windows_pool;
