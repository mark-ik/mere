/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Windows scrying producer pool (WebView2-backed external surfaces).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use mere::forme::GraphMemberId;
use grafting::{Dx12FenceSynchronizer, EpochCachedImporter, HostWgpuContext};
use inker::{
    FocusReason, KeyboardEvent, KeyboardModifiers, MouseButton, MouseEvent, MouseEventKind,
    SurfaceEngineRegistry, SurfaceProducer, WebFeatureStatus, WebSurfaceEvent,
};
use scrying::PlatformCompositionRoot;
use verso_scry::ScryForward;

use super::CapturedThumbnail;
use super::factory::{build_composition_root, registry_for_root, spawn};
use super::frame_import::drive_frame;
use super::scry_surface::drive_navigation;
use super::{KeyMods, MouseBtn, MousePress};

#[derive(Default)]
pub(super) struct Pool {
    tiles: HashMap<GraphMemberId, Tile>,
    /// Spawn failures, kept so a failed tile reports once instead of
    /// re-spawning every redraw. Cleared by reap (so a re-pin retries).
    failed: HashMap<GraphMemberId, String>,
    importer: Option<EpochCachedImporter>,
    /// The one off-screen composition root for this window's scrying pool.
    composition_root: Option<Arc<PlatformCompositionRoot>>,
    /// Registry used to spawn type-erased surface producers.
    registry: Option<SurfaceEngineRegistry>,
    /// D3D12 fence NT handle cast to u64 and handed to each producer spawn.
    fence_handle: Option<u64>,
    /// Compatibility-view flips staged by `begin_flip`, keyed by member,
    /// awaiting their tile's spawn.
    pending_flips: HashMap<GraphMemberId, ScryForward>,
}

pub(super) struct Tile {
    pub(super) producer: Box<dyn SurfaceProducer>,
    pub(super) texture_view: Option<wgpu::TextureView>,
    pub(super) resource_epoch: Option<u64>,
    pub(super) shown_url: Option<String>,
    pub(super) size: (u32, u32),
    pub(super) last_error: Option<String>,
    pub(super) last_title: Option<String>,
    pub(super) last_url: Option<String>,
    pub(super) capabilities_logged: bool,
    pub(super) last_browser_event: Option<String>,
    /// A live compatibility-view flip driving this tile.
    pub(super) flip: Option<ScryForward>,
}

impl Pool {
    pub(super) fn capture_thumbnail(&mut self, member: GraphMemberId) -> Option<CapturedThumbnail> {
        self.tiles.get_mut(&member).and_then(capture_thumbnail)
    }

    pub(super) fn reap(&mut self, member: GraphMemberId) -> Option<CapturedThumbnail> {
        let captured = self.tiles.get_mut(&member).and_then(capture_thumbnail);
        self.tiles.remove(&member);
        self.failed.remove(&member);
        self.pending_flips.remove(&member);
        captured
    }

    /// Stage a forward flip for `member`. Its tile may not exist yet, so the
    /// carry waits here and `drive` attaches it on spawn.
    pub(super) fn begin_flip(
        &mut self,
        member: GraphMemberId,
        state: verso_api::PortableViewState,
    ) {
        let flip = ScryForward::new(state);
        if flip.has_target() {
            self.pending_flips.insert(member, flip);
        } else {
            self.pending_flips.remove(&member);
        }
    }

    pub(super) fn clear(&mut self) -> Vec<(GraphMemberId, CapturedThumbnail)> {
        let captured = self
            .tiles
            .iter_mut()
            .filter_map(|(member, tile)| capture_thumbnail(tile).map(|png| (*member, png)))
            .collect();
        self.tiles.clear();
        self.failed.clear();
        self.pending_flips.clear();
        captured
    }

    /// Reap every tile whose member is not in `keep`.
    pub(super) fn retain(
        &mut self,
        keep: &HashSet<GraphMemberId>,
    ) -> Vec<(GraphMemberId, CapturedThumbnail)> {
        let stale: Vec<_> = self
            .tiles
            .keys()
            .copied()
            .filter(|member| !keep.contains(member))
            .collect();
        let captured = stale
            .iter()
            .filter_map(|member| {
                self.tiles
                    .get_mut(member)
                    .and_then(capture_thumbnail)
                    .map(|png| (*member, png))
            })
            .collect();
        self.tiles.retain(|member, _| keep.contains(member));
        self.failed.retain(|member, _| keep.contains(member));
        self.pending_flips.retain(|member, _| keep.contains(member));
        captured
    }

    pub(super) fn texture_view(&self, member: GraphMemberId) -> Option<&wgpu::TextureView> {
        self.tiles
            .get(&member)
            .and_then(|t| t.texture_view.as_ref())
    }

    #[allow(dead_code)]
    pub(super) fn last_error(&self, member: GraphMemberId) -> Option<&str> {
        self.failed.get(&member).map(String::as_str).or_else(|| {
            self.tiles
                .get(&member)
                .and_then(|t| t.last_error.as_deref())
        })
    }

    pub(super) fn forward_mouse(
        &mut self,
        member: GraphMemberId,
        x: i32,
        y: i32,
        press: MousePress,
    ) {
        let Some(tile) = self.tiles.get_mut(&member) else {
            return;
        };
        let (kind, button) = match press {
            MousePress::Move => (MouseEventKind::Moved, None),
            MousePress::Down(button) => (MouseEventKind::Pressed, Some(map_mouse_button(button))),
            MousePress::Up(button) => (MouseEventKind::Released, Some(map_mouse_button(button))),
        };
        let _ = tile.producer.send_mouse_input(MouseEvent {
            position: inker::PhysicalPosition {
                x: x as f32,
                y: y as f32,
            },
            button,
            kind,
        });
    }

    pub(super) fn forward_wheel(&mut self, member: GraphMemberId, x: i32, y: i32, delta_y: i32) {
        let Some(tile) = self.tiles.get_mut(&member) else {
            return;
        };
        let _ = tile.producer.send_mouse_input(MouseEvent {
            position: inker::PhysicalPosition {
                x: x as f32,
                y: y as f32,
            },
            button: None,
            kind: MouseEventKind::ScrollPixels {
                delta_x: 0.0,
                delta_y: delta_y as f32,
            },
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
        let _ = tile.producer.send_keyboard_input(KeyboardEvent {
            key_code: vk,
            scan_code: 0,
            modifiers: KeyboardModifiers {
                shift: mods.shift,
                ctrl: mods.ctrl,
                alt: mods.alt,
                meta: mods.meta,
            },
            pressed,
            text: text.map(str::to_string),
        });
    }

    pub(super) fn focus_tile(&mut self, member: GraphMemberId) {
        if let Some(tile) = self.tiles.get_mut(&member) {
            let _ = tile.producer.move_focus(FocusReason::Programmatic);
        }
    }

    pub(super) fn capture_clip(
        &mut self,
        member: GraphMemberId,
        x: i32,
        y: i32,
        source_url: &str,
    ) -> Result<crate::web_clip::ClipFragment, String> {
        let tile = self
            .tiles
            .get_mut(&member)
            .ok_or_else(|| "no live surface for focused node".to_string())?;
        let web = tile
            .producer
            .as_web_surface()
            .ok_or_else(|| "focused surface is not scriptable".to_string())?;
        let script = crate::web_clip::web_clip_script(x, y);
        let raw = web
            .execute_script_with_result(&script)
            .map_err(|err| format!("script failed: {err}"))?;
        let mut fragment = crate::web_clip::parse_web_clip(&raw, source_url)?;
        if let Ok(snapshot) = tile.producer.capture_snapshot_png() {
            crate::web_clip::attach_cropped_visual(&mut fragment, &snapshot, tile.size);
        }
        Ok(fragment)
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
        self.ensure_importer(device, queue);
        if self.ensure_registry(window).is_err() {
            self.failed
                .entry(member)
                .or_insert_with(|| "scrying registry setup failed".into());
            return;
        }

        let has_pending_flip = self.pending_flips.contains_key(&member);
        if !self.tiles.contains_key(&member) {
            let registry = self.registry.as_ref().expect("registry set above");
            match spawn(
                registry,
                member,
                url,
                width,
                height,
                session_dir,
                self.fence_handle,
                !has_pending_flip,
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

        let pending_flip = self.pending_flips.remove(&member);
        {
            let tile = self.tiles.get_mut(&member).expect("inserted above");
            drive_navigation(member, tile, url, width, height, pending_flip);
            drain_web_events(member, tile);
        }

        let tile = self.tiles.get_mut(&member).expect("inserted above");
        let importer = self.importer.as_mut().expect("importer set above");
        drive_frame(member, tile, importer);
    }

    fn ensure_importer(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.importer.is_some() {
            return;
        }
        let host = HostWgpuContext::new(device.clone(), queue.clone());
        match Dx12FenceSynchronizer::new(&host) {
            Ok(sync) => {
                self.fence_handle = Some(sync.shared_handle().0 as u64);
                self.importer = Some(EpochCachedImporter::with_synchronizer(host, Box::new(sync)));
            }
            Err(err) => {
                tracing::debug!(?err, "explicit D3D12 fence unavailable; implicit sync");
                self.importer = Some(EpochCachedImporter::new(host));
            }
        }
    }

    fn ensure_registry(&mut self, window: &Arc<winit::window::Window>) -> Result<(), String> {
        if self.registry.is_some() {
            return Ok(());
        }
        let root = match self.composition_root.as_ref() {
            Some(root) => root.clone(),
            None => {
                let root = build_composition_root(window)?;
                self.composition_root = Some(root.clone());
                root
            }
        };
        self.registry = Some(registry_for_root(root));
        Ok(())
    }
}

fn drain_web_events(member: GraphMemberId, tile: &mut Tile) {
    let Some(web) = tile.producer.as_web_surface() else {
        return;
    };
    if !tile.capabilities_logged {
        let caps = web.capabilities();
        tracing::info!(
            target: "meerkat.surface.capabilities",
            %member,
            backend = %caps.backend_name,
            transport = ?caps.frame_transport,
            script = %feature_label(&caps.script.result),
            cookies = %feature_label(&caps.cookie.write),
            snapshot = %feature_label(&caps.snapshot),
            degradations = %caps.degradation_reasons.join(" | "),
            "web surface capabilities"
        );
        tile.capabilities_logged = true;
    }

    let mut events = Vec::new();
    while let Some(event) = web.poll_web_event() {
        events.push(event);
    }
    let _ = web;
    for event in events {
        record_web_event(member, tile, event);
    }
}

fn capture_thumbnail(tile: &mut Tile) -> Option<CapturedThumbnail> {
    let (width, height) = tile.size;
    if width == 0 || height == 0 {
        return None;
    }
    let png_bytes = tile.producer.capture_snapshot_png().ok()?;
    Some(CapturedThumbnail {
        png_bytes,
        width,
        height,
        url: tile.last_url.clone().or_else(|| tile.shown_url.clone()),
    })
}

fn record_web_event(member: GraphMemberId, tile: &mut Tile, event: WebSurfaceEvent) {
    match event {
        WebSurfaceEvent::Navigation(event) => {
            let detail = format!("{event:?}");
            let url = match &event {
                inker::NavigationEvent::Started { url }
                | inker::NavigationEvent::Committed { url }
                | inker::NavigationEvent::Finished { url, .. }
                | inker::NavigationEvent::Failed { url, .. } => url,
            };
            if !url.is_empty() {
                tile.last_url = Some(url.clone());
            }
            tile.last_browser_event = Some(detail.clone());
            tracing::info!(
                target: "meerkat.surface.event",
                %member,
                event = %detail,
                "web navigation event"
            );
        }
        WebSurfaceEvent::TitleChanged { title } => {
            tile.last_title = Some(title.clone());
            tile.last_browser_event = Some(format!("title={title}"));
            tracing::info!(
                target: "meerkat.surface.event",
                %member,
                title = %title,
                "web title changed"
            );
        }
        WebSurfaceEvent::AddressChanged { url } => {
            tile.last_url = Some(url.clone());
            tile.last_browser_event = Some(format!("url={url}"));
            tracing::info!(
                target: "meerkat.surface.event",
                %member,
                url = %url,
                "web address changed"
            );
        }
        WebSurfaceEvent::ConsoleMessage {
            level,
            text,
            source,
            line,
        } => {
            let detail = format!("console {level}: {text}");
            tile.last_browser_event = Some(detail.clone());
            tracing::warn!(
                target: "meerkat.surface.event",
                %member,
                level = %level,
                text = %text,
                source = ?source,
                line = ?line,
                "web console message"
            );
        }
        WebSurfaceEvent::ScriptException { text, source, line } => {
            tile.last_browser_event = Some(format!("script exception: {text}"));
            tracing::warn!(
                target: "meerkat.surface.event",
                %member,
                text = %text,
                source = ?source,
                line = ?line,
                "web script exception"
            );
        }
        WebSurfaceEvent::ProcessCrashed { reason } => {
            tile.last_error = Some(format!("web process crashed: {reason}"));
            tile.last_browser_event = Some(format!("process crashed: {reason}"));
            tracing::error!(
                target: "meerkat.surface.event",
                %member,
                reason = %reason,
                "web process crashed"
            );
        }
        WebSurfaceEvent::BackendDiagnostic { severity, message } => {
            tile.last_browser_event = Some(message.clone());
            match severity.as_str() {
                "error" => tracing::error!(
                    target: "meerkat.surface.event",
                    %member,
                    message = %message,
                    "web backend diagnostic"
                ),
                "warn" => tracing::warn!(
                    target: "meerkat.surface.event",
                    %member,
                    message = %message,
                    "web backend diagnostic"
                ),
                _ => tracing::info!(
                    target: "meerkat.surface.event",
                    %member,
                    message = %message,
                    "web backend diagnostic"
                ),
            }
        }
        other => {
            let detail = format!("{other:?}");
            tile.last_browser_event = Some(detail.clone());
            tracing::info!(
                target: "meerkat.surface.event",
                %member,
                event = %detail,
                "web browser event"
            );
        }
    }
}

fn feature_label(status: &WebFeatureStatus) -> String {
    match status {
        WebFeatureStatus::Supported => "supported".into(),
        WebFeatureStatus::Unsupported { reason } => format!("unsupported: {reason}"),
        WebFeatureStatus::Partial { detail } => format!("partial: {detail}"),
    }
}

fn map_mouse_button(button: MouseBtn) -> MouseButton {
    match button {
        MouseBtn::Left => MouseButton::Left,
        MouseBtn::Right => MouseButton::Right,
        MouseBtn::Middle => MouseButton::Middle,
    }
}
