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

use forme::GraphMemberId;

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
    #[cfg(target_os = "windows")]
    pool: windows_pool::Pool,
}

impl ScryingHost {
    #[allow(dead_code)] // public API; the per-window carve constructs via `Default`
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop `member`'s WebView (if any). The shared pin is untouched.
    pub fn reap(&mut self, member: GraphMemberId) {
        #[cfg(target_os = "windows")]
        self.pool.reap(member);
        #[cfg(not(target_os = "windows"))]
        let _ = member;
    }

    /// Drop every WebView in this window's pool (multi-graph switch, mirrors
    /// `Constellation::clear`). The shared pins are cleared by the caller.
    pub fn clear(&mut self) {
        #[cfg(target_os = "windows")]
        self.pool.clear();
    }

    /// Reap every live tile whose member is not in `keep` (the surfaces shown this
    /// frame), tearing down its WebView. A compat node that is no longer shown is
    /// dropped immediately (reap-on-deselect), so its visual cannot freeze at its
    /// last position. Multiple panes share one per-HWND composition target (scry's
    /// `new_attached`), so any number of compat tiles can stay live at once; this is
    /// just the "drop what's gone" pass. Called each frame before the shown surfaces
    /// are driven. (X2/X3 lifecycle; multi-tile.)
    pub fn retain(&mut self, keep: &std::collections::HashSet<GraphMemberId>) {
        #[cfg(target_os = "windows")]
        self.pool.retain(keep);
        #[cfg(not(target_os = "windows"))]
        let _ = keep;
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
        #[cfg(target_os = "windows")]
        self.pool
            .drive(member, url, width, height, origin, window, device, queue, session_dir);
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (member, url, width, height, origin, window, device, queue, session_dir);
            tracing::warn!("compatibility view: scrying X1 is Windows-only (plan X4)");
        }
    }

    /// The tile's imported WebView texture, ready to composite.
    pub fn texture_view(&self, member: GraphMemberId) -> Option<&wgpu::TextureView> {
        #[cfg(target_os = "windows")]
        return self.pool.texture_view(member);
        #[cfg(not(target_os = "windows"))]
        {
            let _ = member;
            None
        }
    }

    /// The tile's last spawn/frame error, for the card placeholder (the
    /// placeholder consumer lands with X2's chrome integration).
    #[allow(dead_code)]
    pub fn last_error(&self, member: GraphMemberId) -> Option<&str> {
        #[cfg(target_os = "windows")]
        return self.pool.last_error(member);
        #[cfg(not(target_os = "windows"))]
        {
            let _ = member;
            Some("compatibility view is Windows-only for now")
        }
    }

    /// Forward a mouse move / press / release into `member`'s WebView at
    /// **tile-local** `(x, y)`. No-op off Windows or with no live tile. (X2.)
    pub fn forward_mouse(&mut self, member: GraphMemberId, x: i32, y: i32, press: MousePress) {
        #[cfg(target_os = "windows")]
        self.pool.forward_mouse(member, x, y, press);
        #[cfg(not(target_os = "windows"))]
        let _ = (member, x, y, press);
    }

    /// Forward a vertical wheel delta (Win32 convention: 120 per notch) into
    /// `member`'s WebView at tile-local `(x, y)`. (X2.)
    pub fn forward_wheel(&mut self, member: GraphMemberId, x: i32, y: i32, delta_y: i32) {
        #[cfg(target_os = "windows")]
        self.pool.forward_wheel(member, x, y, delta_y);
        #[cfg(not(target_os = "windows"))]
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
        #[cfg(target_os = "windows")]
        self.pool.forward_key(member, vk, text, pressed, mods);
        #[cfg(not(target_os = "windows"))]
        let _ = (member, vk, text, pressed, mods);
    }

    /// Hand keyboard focus to `member`'s WebView (a click / Tab into the tile). (X2.)
    pub fn focus_tile(&mut self, member: GraphMemberId) {
        #[cfg(target_os = "windows")]
        self.pool.focus_tile(member);
        #[cfg(not(target_os = "windows"))]
        let _ = member;
    }
}

#[cfg(target_os = "windows")]
mod windows_pool {
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Arc;

    use forme::GraphMemberId;
    use scrying::native_frame::{
        Dx12FenceSynchronizer, HostWgpuContext, ImportOptions, ImportedTexture, TextureImporter,
        WgpuTextureImporter,
    };
    use scrying::{
        FocusReason, KeyEventKind, KeyModifierFlags, KeyboardInput, MouseEventKind, MouseInput,
        MouseVirtualKeys, PlatformCompositionRoot, PlatformWebSurfaceConfig,
        PlatformWebSurfaceProducer, WebSurfaceFrame,
    };

    use super::{KeyMods, MouseBtn, MousePress};

    #[derive(Default)]
    pub(super) struct Pool {
        tiles: HashMap<GraphMemberId, Tile>,
        /// Spawn failures, kept so a failed tile reports once instead of
        /// re-spawning every redraw. Cleared by reap (so a re-pin retries).
        failed: HashMap<GraphMemberId, String>,
        importer: Option<WgpuTextureImporter>,
        /// The one `DesktopWindowTarget` for this window's HWND, shared by every
        /// pane. Created once on the first spawn and never dropped while the pool
        /// lives — a second `CreateDesktopWindowTarget` on the same HWND fails with
        /// `WINDOW_ALREADY_COMPOSED`, so producers attach to this via
        /// `new_attached` rather than each making its own. (Multi-tile scry.)
        composition_root: Option<Arc<PlatformCompositionRoot>>,
        /// The shared D3D12 fence's NT handle, taken from the importer's
        /// `Dx12FenceSynchronizer` and handed to each producer (`with_fence_shared_handle`)
        /// so it signals the fence after every `CopyResource` and `import_frame` waits
        /// on it — explicit GPU sync in place of the default implicit ordering. `None`
        /// when the host device is not D3D12 (the importer then keeps the implicit
        /// synchronizer). Valid for the pool's life: the synchronizer (in `importer`)
        /// owns the handle and closes it on drop, after every tile producer is gone.
        fence_handle: Option<*mut core::ffi::c_void>,
        /// A 1x1 throwaway buffer for the per-frame cache-flush copy (see `drive`).
        /// Created once on the first import. The producer overwrites each tile's shared
        /// texture in place (`resource_is_new == false` after the first frame), so the
        /// host must force a D3D12 state-transition barrier on it every frame or D3D12
        /// keeps sampling the cached first frame forever. (Mirrors demo-win's renderer.)
        cache_flush_buffer: Option<wgpu::Buffer>,
    }

    struct Tile {
        producer: PlatformWebSurfaceProducer,
        /// The imported shared texture + its view. The producer copies every
        /// new frame into the same allocation, so this stays valid until a
        /// frame arrives with `resource_is_new` (resize / capture restart).
        imported: Option<(ImportedTexture, wgpu::TextureView)>,
        shown_url: Option<String>,
        size: (u32, u32),
        last_error: Option<String>,
        /// Consecutive redraws where `try_acquire_frame` returned no frame. WGC
        /// capture from an off-window host can stall; after a run of empties the
        /// capture session is torn down + restarted (`force_restart_capture`), the
        /// demo's recovery for a stalled session. Reset on any acquired frame.
        empty_polls: u32,
        /// Total `try_acquire_frame` calls, for the stall-restart log context.
        total_polls: u64,
    }

    impl Pool {
        pub(super) fn reap(&mut self, member: GraphMemberId) {
            self.tiles.remove(&member);
            self.failed.remove(&member);
        }

        pub(super) fn clear(&mut self) {
            self.tiles.clear();
            self.failed.clear();
        }

        /// Reap (drop the WebView for) every tile whose member is not in `keep`.
        /// Dropping a `Tile` tears down its producer and removes its pane visual from
        /// the shared composition root; the root itself stays (it's per-HWND).
        /// `failed` is pruned the same way so a re-pin retries. (Multi-tile scry.)
        pub(super) fn retain(&mut self, keep: &std::collections::HashSet<GraphMemberId>) {
            self.tiles.retain(|member, _| keep.contains(member));
            self.failed.retain(|member, _| keep.contains(member));
        }

        pub(super) fn texture_view(&self, member: GraphMemberId) -> Option<&wgpu::TextureView> {
            self.tiles
                .get(&member)
                .and_then(|t| t.imported.as_ref())
                .map(|(_, view)| view)
        }

        #[allow(dead_code)]
        pub(super) fn last_error(&self, member: GraphMemberId) -> Option<&str> {
            self.failed.get(&member).map(String::as_str).or_else(|| {
                self.tiles
                    .get(&member)
                    .and_then(|t| t.last_error.as_deref())
            })
        }

        pub(super) fn forward_mouse(&mut self, member: GraphMemberId, x: i32, y: i32, press: MousePress) {
            let Some(tile) = self.tiles.get_mut(&member) else {
                return;
            };
            let (kind, virtual_keys) = match press {
                MousePress::Move => (MouseEventKind::Move, MouseVirtualKeys::default()),
                MousePress::Down(b) => (down_kind(b), btn_keys(b)),
                MousePress::Up(b) => (up_kind(b), btn_keys(b)),
            };
            // Input failures are non-fatal and high-frequency; don't record them.
            let _ = tile.producer.send_mouse_input(MouseInput {
                kind,
                virtual_keys,
                mouse_data: 0,
                point: (x, y),
            });
        }

        pub(super) fn forward_wheel(&mut self, member: GraphMemberId, x: i32, y: i32, delta_y: i32) {
            let Some(tile) = self.tiles.get_mut(&member) else {
                return;
            };
            let _ = tile.producer.send_mouse_input(MouseInput {
                kind: MouseEventKind::Wheel,
                virtual_keys: MouseVirtualKeys::default(),
                mouse_data: delta_y,
                point: (x, y),
            });
        }

        pub(super) fn forward_key(
            &mut self,
            member: GraphMemberId,
            vk: u32,
            text: Option<&str>,
            pressed: bool,
            mods: KeyMods,
        ) {
            let Some(tile) = self.tiles.get_mut(&member) else {
                return;
            };
            let characters = text.unwrap_or_default().to_string();
            let _ = tile.producer.send_keyboard_input(KeyboardInput {
                kind: if pressed {
                    KeyEventKind::Down
                } else {
                    KeyEventKind::Up
                },
                virtual_key_code: vk,
                characters: characters.clone(),
                characters_ignoring_modifiers: characters,
                modifiers: KeyModifierFlags {
                    shift: mods.shift,
                    control: mods.ctrl,
                    alt: mods.alt,
                    meta: mods.meta,
                    caps_lock: false,
                },
                is_repeat: false,
            });
        }

        pub(super) fn focus_tile(&mut self, member: GraphMemberId) {
            if let Some(tile) = self.tiles.get_mut(&member) {
                let _ = tile.producer.move_focus(FocusReason::Programmatic);
            }
        }

        #[allow(clippy::too_many_arguments)]
        pub(super) fn drive(
            &mut self,
            member: GraphMemberId,
            url: &str,
            width: u32,
            height: u32,
            _origin: (f32, f32),
            window: &Arc<winit::window::Window>,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            session_dir: &Path,
        ) {
            let (width, height) = (width.max(1), height.max(1));
            if self.failed.contains_key(&member) {
                return;
            }
            if self.importer.is_none() {
                let host = HostWgpuContext::new(device.clone(), queue.clone());
                // Prefer an explicit shared D3D12 fence: the synchronizer owns the
                // fence, each producer signals it after `CopyResource` (via the
                // `with_fence_shared_handle` wired in `build_producer`), and
                // `import_frame` waits on it. Falls back to the default implicit
                // synchronizer when the host wgpu device is not D3D12.
                match Dx12FenceSynchronizer::new(&host) {
                    Ok(sync) => {
                        self.fence_handle = Some(sync.shared_handle().0);
                        self.importer =
                            Some(WgpuTextureImporter::with_synchronizer(host, Box::new(sync)));
                    }
                    Err(err) => {
                        tracing::debug!(?err, "explicit D3D12 fence unavailable; implicit sync");
                        self.importer = Some(WgpuTextureImporter::new(host));
                    }
                }
            }
            let importer = self.importer.as_ref().expect("importer set above");

            if !self.tiles.contains_key(&member) {
                // Create the one per-HWND composition target on first spawn, then
                // attach every pane to it so any number of compat tiles coexist.
                let root = match self.composition_root.as_ref() {
                    Some(root) => root.clone(),
                    None => match build_composition_root(window) {
                        Ok(root) => self.composition_root.insert(root).clone(),
                        Err(err) => {
                            tracing::warn!(%member, %err, "scrying composition root failed");
                            self.failed.insert(member, err);
                            return;
                        }
                    },
                };
                match spawn(
                    &root,
                    member,
                    url,
                    width,
                    height,
                    session_dir,
                    self.fence_handle,
                ) {
                    Ok(tile) => {
                        self.tiles.insert(member, tile);
                    }
                    Err(err) => {
                        tracing::warn!(%member, %err, "scrying spawn failed");
                        self.failed.insert(member, err);
                        return;
                    }
                }
            }
            let tile = self.tiles.get_mut(&member).expect("inserted above");

            if tile.size != (width, height) {
                match tile.producer.resize(dpi::PhysicalSize::new(width, height)) {
                    Ok(()) => tile.size = (width, height),
                    Err(err) => tile.last_error = Some(format!("resize: {err}")),
                }
            }
            // No visual hosting on meerkat's window: the composition lives on
            // scrying's off-screen host (capture-only). meerkat composites the
            // captured texture under the chrome (render.rs:1398) and forwards input
            // by API, so the visual is never parked on-screen and never occludes the
            // chrome. (P2 off-window host; the per-frame `set_offset` chase is gone.)
            if tile.shown_url.as_deref() != Some(url) {
                // Non-blocking navigation: the blocking trait method pumps a
                // wait loop on the UI thread (plan X2 owns the full nav story).
                match tile.producer.load_url(url) {
                    Ok(()) => tile.shown_url = Some(url.to_string()),
                    Err(err) => tile.last_error = Some(format!("navigate: {err}")),
                }
            }

            tile.total_polls = tile.total_polls.saturating_add(1);
            match tile.producer.try_acquire_frame() {
                Ok(Some(full)) => {
                    tile.empty_polls = 0;
                    if full.resource_is_new {
                        if let WebSurfaceFrame::Native(native) = &full.frame {
                            match importer.import_frame(native, &ImportOptions::default()) {
                                Ok(imported) => {
                                    let view = imported
                                        .texture
                                        .create_view(&wgpu::TextureViewDescriptor::default());
                                    tile.imported = Some((imported, view));
                                    tile.last_error = None;
                                }
                                Err(err) => {
                                    tracing::warn!(%member, %err, "scry import FAILED");
                                    tile.last_error = Some(format!("frame import: {err}"));
                                }
                            }
                        }
                        // The handle was handed off to this frame exactly once;
                        // the importer opened its own reference, so close ours
                        // whether or not the import succeeded.
                        if !full.shared_handle.is_null() {
                            // The NT-handle close is FFI; the handle is owned by
                            // this frame per the producer's handoff contract.
                            #[allow(unsafe_code)]
                            if let Err(err) = unsafe {
                                scrying::windows_capture::close_shared_handle(full.shared_handle)
                            } {
                                tracing::warn!(%member, %err, "close_shared_handle failed");
                            }
                        }
                    }
                    // resource_is_new == false: the producer copied the new frame
                    // into the allocation already behind `tile.imported`.
                }
                Ok(None) => {
                    // WGC from an off-window host can stall; after ~1s of empty redraws
                    // tear down + restart the capture session (the demo's recovery) so a
                    // fresh session re-delivers frames.
                    tile.empty_polls = tile.empty_polls.saturating_add(1);
                    if tile.empty_polls >= 120 {
                        tracing::warn!(%member, polls = tile.total_polls, "scrying capture stalled; restarting session");
                        tile.producer.force_restart_capture();
                        tile.empty_polls = 0;
                    }
                }
                Err(err) => tile.last_error = Some(format!("acquire: {err}")),
            }

            // Cache-flush. The producer overwrites the tile's shared texture in place
            // every frame (`resource_is_new == false` after the first import), so with no
            // state transition on it each frame D3D12 keeps sampling the cached *first*
            // frame forever — captured before the off-window page had painted, hence a
            // blank tile. A throwaway 1x1 `copy_texture_to_buffer` forces a
            // SHADER_RESOURCE -> COPY_SRC -> SHADER_RESOURCE barrier that flushes the
            // cache, so the composite samples the live frame. (Mirrors demo-win's
            // renderer.rs; the off-window capture composites blank without it.)
            let flush_buf = self.cache_flush_buffer.get_or_insert_with(|| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("scrying cache-flush"),
                    size: wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as u64,
                    usage: wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            });
            if let Some((imported, _)) = self.tiles.get(&member).and_then(|t| t.imported.as_ref()) {
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("scrying cache-flush"),
                });
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: &imported.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: flush_buf,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT),
                            rows_per_image: Some(1),
                        },
                    },
                    wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                );
                queue.submit(std::iter::once(encoder.finish()));
            }
        }
    }

    fn down_kind(b: MouseBtn) -> MouseEventKind {
        match b {
            MouseBtn::Left => MouseEventKind::LeftButtonDown,
            MouseBtn::Right => MouseEventKind::RightButtonDown,
            MouseBtn::Middle => MouseEventKind::MiddleButtonDown,
        }
    }

    fn up_kind(b: MouseBtn) -> MouseEventKind {
        match b {
            MouseBtn::Left => MouseEventKind::LeftButtonUp,
            MouseBtn::Right => MouseEventKind::RightButtonUp,
            MouseBtn::Middle => MouseEventKind::MiddleButtonUp,
        }
    }

    fn btn_keys(b: MouseBtn) -> MouseVirtualKeys {
        let mut vk = MouseVirtualKeys::default();
        match b {
            MouseBtn::Left => vk.left_button = true,
            MouseBtn::Right => vk.right_button = true,
            MouseBtn::Middle => vk.middle_button = true,
        }
        vk
    }

    /// Create the capture-only composition root on scrying's private off-screen
    /// host window, NOT meerkat's visible window. The WebView visual then lives
    /// off-screen and is never composited over meerkat's swapchain, so the chrome
    /// and its overlays (menu, palette) always win. Both orrery cards and pelt
    /// tiles consume only the WGC-captured texture (render.rs:1398); input is
    /// forwarded by API (CDP keyboard / IME, `SendMouseInput`), so no on-window
    /// visual is needed. Called once per pool (cached). `size` only anchors the
    /// host (capture is per-visual, size-independent), so the window's current
    /// inner size is a fine bound. (P2 off-window host.)
    fn build_composition_root(
        window: &Arc<winit::window::Window>,
    ) -> Result<Arc<PlatformCompositionRoot>, String> {
        let inner = window.inner_size();
        PlatformCompositionRoot::new_offscreen(dpi::PhysicalSize::new(
            inner.width.max(1),
            inner.height.max(1),
        ))
        .map_err(|err| format!("composition root: {err}"))
    }

    fn spawn(
        root: &Arc<PlatformCompositionRoot>,
        member: GraphMemberId,
        url: &str,
        width: u32,
        height: u32,
        session_dir: &Path,
        fence_handle: Option<*mut core::ffi::c_void>,
    ) -> Result<Tile, String> {
        let producer = build_producer(root, member, width, height, session_dir, fence_handle)?;
        let mut tile = Tile {
            producer,
            imported: None,
            shown_url: None,
            size: (width, height),
            last_error: None,
            empty_polls: 0,
            total_polls: 0,
        };
        match tile.producer.load_url(url) {
            Ok(()) => tile.shown_url = Some(url.to_string()),
            Err(err) => tile.last_error = Some(format!("navigate: {err}")),
        }
        Ok(tile)
    }

    fn build_producer(
        root: &Arc<PlatformCompositionRoot>,
        member: GraphMemberId,
        width: u32,
        height: u32,
        session_dir: &Path,
        fence_handle: Option<*mut core::ffi::c_void>,
    ) -> Result<PlatformWebSurfaceProducer, String> {
        // Attach this pane to the shared off-screen host target at offset (0,0)
        // (see `with_offset` below). A
        // per-pane profile folder (keyed by member) keeps each tile's WebView2
        // environment on its own user-data dir — the proven multi-pane shape, one
        // environment per folder. A shared cookie jar across tiles is a later
        // refinement. (Multi-tile scry; the per-persona engine-profile binding
        // lands in X3.)
        let profile = session_dir
            .join("scrying")
            .join(format!("pane-{:032x}", member.as_u128()));
        let mut config =
            PlatformWebSurfaceConfig::new(dpi::PhysicalSize::new(width, height), profile)
                // Pane offset (0,0) on the off-screen host: capture is per-visual
                // and the texture is composited at the tile rect by render.rs, so
                // the on-host position only needs to keep the visual rendered.
                .with_offset(0.0, 0.0);
        // Explicit-fence sync: the producer opens this shared fence and signals it
        // after each `CopyResource`; the importer's `Dx12FenceSynchronizer` waits on
        // the per-frame value. `None` (non-D3D12 host) leaves the implicit path.
        if let Some(handle) = fence_handle {
            config = config.with_fence_shared_handle(handle);
        }
        #[allow(unsafe_code)]
        unsafe { PlatformWebSurfaceProducer::new_attached(root, config) }
            .map_err(|err| format!("WebView2 attach: {err}"))
    }
}
