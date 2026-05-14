/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Surface-engine traits and registry — parallel dispatch path for
//! long-lived, frame-streaming engines alongside [`crate::engine`].
//!
//! Document engines are request/response: fetch → render → `EngineDocument`.
//! Surface engines are lifecycle-bound: spawn → long-lived session producing
//! a composited-frame stream + events until torn down. Both registries
//! coexist; the host dispatches through whichever holds the resolved engine ID
//! (document registry for `nematic.*` / `serval.web`; surface registry for
//! `scrying.web` / `wry.web`).

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::routing::EngineRouteDecision;

// ── Errors ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceError {
    EngineNotFound(String),
    SpawnFailed(String),
    NavigationFailed(String),
    InputFailed(String),
    FrameAcquisitionFailed(String),
    Unsupported(String),
}

impl fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EngineNotFound(id) => write!(f, "surface engine not registered: {id}"),
            Self::SpawnFailed(reason) => write!(f, "spawn failed: {reason}"),
            Self::NavigationFailed(reason) => write!(f, "navigation failed: {reason}"),
            Self::InputFailed(reason) => write!(f, "input failed: {reason}"),
            Self::FrameAcquisitionFailed(reason) => write!(f, "frame acquisition: {reason}"),
            Self::Unsupported(reason) => write!(f, "unsupported: {reason}"),
        }
    }
}

impl std::error::Error for SurfaceError {}

// ── Spawn request ──────────────────────────────────────────────────────────

/// Persona/session binding passed to the surface engine at spawn time.
///
/// The host resolves `user_data_dir` from persona + graph context before
/// constructing the request. The engine plumbs it to the producer's data-store
/// config (e.g. `WebView2CompositionConfig::user_data_dir` on Windows).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineProfileBinding {
    pub user_data_dir: String,
}

/// Input to [`SurfaceEngine::spawn`].
///
/// Bypasses the inker fetch path entirely — the underlying WebView manages
/// its own HTTP stack; there is no raw body to hand in.
#[derive(Clone, Debug)]
pub struct SurfaceSpawnRequest {
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub profile: EngineProfileBinding,
    /// Platform fence share-handle for explicit GPU sync. `None` falls back
    /// to the producer's barrier/cache path. Windows: D3D12 fence HANDLE cast
    /// to u64. Other platforms: reserved.
    pub fence_handle: Option<u64>,
}

// ── Frame vocabulary ───────────────────────────────────────────────────────

/// Platform-specific texture handle emitted by [`SurfaceProducer::acquire_frame`].
#[non_exhaustive]
#[derive(Debug)]
pub enum NativeTextureHandle {
    /// Windows: D3D12 shared texture HANDLE cast to u64.
    D3d12Shared(u64),
    /// macOS: IOSurface ref (opaque u64; downcast on the host side).
    IoSurface(u64),
    /// Linux: DMA-BUF fd (negative means absent/invalid).
    DmaBuf(i64),
}

/// Synchronization handle accompanying a [`SurfaceFrame`].
#[non_exhaustive]
#[derive(Debug)]
pub enum SurfaceSyncHandle {
    /// Windows: D3D12 fence + signal value.
    D3d12Fence { handle: u64, value: u64 },
    /// Synchronization already complete before the handle was emitted.
    None,
}

/// A composited frame from a surface producer.
#[derive(Debug)]
pub struct SurfaceFrame {
    pub texture: NativeTextureHandle,
    pub sync: SurfaceSyncHandle,
    pub width: u32,
    pub height: u32,
}

// ── Input vocabulary ───────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicalPosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Back,
    Forward,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum MouseEventKind {
    Moved,
    Pressed,
    Released,
    ScrollPixels { delta_x: f32, delta_y: f32 },
    ScrollLines { delta_x: f32, delta_y: f32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MouseEvent {
    pub position: PhysicalPosition,
    pub button: Option<MouseButton>,
    pub kind: MouseEventKind,
}

/// Pointer event for stylus / touch input (adds pressure and tilt).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PointerEvent {
    pub position: PhysicalPosition,
    pub button: Option<MouseButton>,
    pub kind: MouseEventKind,
    /// Normalized pressure [0.0, 1.0]; `None` when absent.
    pub pressure: Option<f32>,
    /// Tilt from vertical in degrees, X axis; `None` when absent.
    pub tilt_x: Option<f32>,
    /// Tilt from vertical in degrees, Y axis; `None` when absent.
    pub tilt_y: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyboardEvent {
    /// Host-framework virtual key code (gpui key code on the main path).
    pub key_code: u32,
    /// Hardware scan code; zero when absent.
    pub scan_code: u32,
    pub modifiers: KeyboardModifiers,
    pub pressed: bool,
    /// Composed text for printable keys; `None` for non-printable and key-up.
    pub text: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FocusReason {
    Mouse,
    Tab,
    ShiftTab,
    Programmatic,
}

// ── Producer event vocabulary ──────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavigationEvent {
    Started { url: String },
    Committed { url: String },
    Finished { url: String, title: Option<String> },
    Failed { url: String, reason: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CursorShape {
    Default,
    Text,
    Pointer,
    Grab,
    Grabbing,
    Crosshair,
    Move,
    ResizeNs,
    ResizeEw,
    ResizeNesw,
    ResizeNwse,
    NotAllowed,
    Hidden,
}

/// A message posted from the page via the JS bridge (postMessage-style).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebMessage {
    pub tag: String,
    pub payload: String,
}

// ── Settings ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurfaceSettings {
    /// Background fill color (RGBA). Informs pre-composited transparency.
    pub background_color: [u8; 4],
    /// Zoom factor (1.0 = 100 %).
    pub zoom_factor: f64,
    pub dev_tools: bool,
}

impl Default for SurfaceSettings {
    fn default() -> Self {
        Self {
            background_color: [255, 255, 255, 255],
            zoom_factor: 1.0,
            dev_tools: false,
        }
    }
}

// ── Traits ─────────────────────────────────────────────────────────────────

/// Factory for [`SurfaceProducer`] instances.
///
/// Parallel to [`crate::Engine`] for surface-producing engines. A single
/// `SurfaceEngine` may spawn many producers (one per tile).
pub trait SurfaceEngine: Send + Sync {
    /// Stable engine identifier. Must match the `engine_id` of the
    /// [`EngineRouteDecision`] that selected this engine.
    fn engine_id(&self) -> &str;

    /// Spawn a new producer for the given request.
    fn spawn(&self, request: &SurfaceSpawnRequest) -> Result<Box<dyn SurfaceProducer>, SurfaceError>;
}

/// Long-lived surface producer. Owns a WebView control until dropped.
///
/// All methods take `&mut self`: the producer is single-owner, driven
/// sequentially by the host's render loop. Input flows in through `send_*`
/// and `move_focus`; output flows out through `acquire_frame` and `poll_*`.
///
/// Not `Send`: producers may be STA-bound (Windows WebView2 COM) or
/// main-thread-only (macOS WKWebView, gpui main thread). The host drives them
/// from a single thread per producer.
pub trait SurfaceProducer {
    // ── Layout ──────────────────────────────────────────────────────────────
    fn resize(&mut self, width: u32, height: u32) -> Result<(), SurfaceError>;
    fn set_offset(&mut self, x: i32, y: i32) -> Result<(), SurfaceError>;

    // ── Frame acquisition ────────────────────────────────────────────────────
    fn acquire_frame(&mut self) -> Result<Option<SurfaceFrame>, SurfaceError>;

    // ── Navigation ───────────────────────────────────────────────────────────
    fn navigate_to_url(&mut self, url: &str) -> Result<(), SurfaceError>;
    fn navigate_to_string(&mut self, html: &str) -> Result<(), SurfaceError>;
    fn reload(&mut self) -> Result<(), SurfaceError>;
    fn stop(&mut self) -> Result<(), SurfaceError>;
    fn go_back(&mut self) -> Result<(), SurfaceError>;
    fn go_forward(&mut self) -> Result<(), SurfaceError>;
    fn can_go_back(&self) -> bool;
    fn can_go_forward(&self) -> bool;

    // ── Input ────────────────────────────────────────────────────────────────
    fn send_mouse_input(&mut self, ev: MouseEvent) -> Result<(), SurfaceError>;
    fn send_pointer_input(&mut self, ev: PointerEvent) -> Result<(), SurfaceError>;
    fn send_keyboard_input(&mut self, ev: KeyboardEvent) -> Result<(), SurfaceError>;
    fn move_focus(&mut self, reason: FocusReason) -> Result<(), SurfaceError>;

    // ── Events ───────────────────────────────────────────────────────────────
    fn poll_navigation_event(&mut self) -> Option<NavigationEvent>;
    fn poll_cursor_shape(&mut self) -> Option<CursorShape>;
    fn poll_web_message(&mut self) -> Option<WebMessage>;

    // ── Settings ─────────────────────────────────────────────────────────────
    fn apply_settings(&mut self, settings: &SurfaceSettings) -> Result<(), SurfaceError>;

    // ── Snapshot ─────────────────────────────────────────────────────────────
    fn capture_snapshot_png(&mut self) -> Result<Vec<u8>, SurfaceError>;
}

// ── Registry ───────────────────────────────────────────────────────────────

/// Engine ID → `SurfaceEngine` instance dispatch. Parallel to
/// [`crate::EngineRegistry`] for the surface dispatch path.
#[derive(Default)]
pub struct SurfaceEngineRegistry {
    engines: HashMap<String, Box<dyn SurfaceEngine>>,
}

impl SurfaceEngineRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, engine: Box<dyn SurfaceEngine>) {
        let id = engine.engine_id().to_string();
        self.engines.insert(id, engine);
    }

    pub fn engine(&self, id: &str) -> Option<&dyn SurfaceEngine> {
        self.engines.get(id).map(|e| e.as_ref())
    }

    pub fn contains(&self, id: &str) -> bool {
        self.engines.contains_key(id)
    }

    pub fn engine_ids(&self) -> impl Iterator<Item = &str> {
        self.engines.keys().map(String::as_str)
    }

    /// Spawn a producer using the engine selected by `decision.engine_id`.
    #[tracing::instrument(
        level = "debug",
        skip(self, decision, request),
        fields(engine_id = %decision.engine_id, url = %request.url),
    )]
    pub fn spawn(
        &self,
        decision: &EngineRouteDecision,
        request: &SurfaceSpawnRequest,
    ) -> Result<Box<dyn SurfaceProducer>, SurfaceError> {
        let engine = self.engine(&decision.engine_id).ok_or_else(|| {
            tracing::warn!(
                engine_id = %decision.engine_id,
                "surface engine not registered"
            );
            SurfaceError::EngineNotFound(decision.engine_id.clone())
        })?;
        engine.spawn(request)
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::{SurfaceContract, SurfaceContractMode, SurfaceTargetId};

    struct StubProducer;

    impl SurfaceProducer for StubProducer {
        fn resize(&mut self, _: u32, _: u32) -> Result<(), SurfaceError> { Ok(()) }
        fn set_offset(&mut self, _: i32, _: i32) -> Result<(), SurfaceError> { Ok(()) }
        fn acquire_frame(&mut self) -> Result<Option<SurfaceFrame>, SurfaceError> { Ok(None) }
        fn navigate_to_url(&mut self, _: &str) -> Result<(), SurfaceError> { Ok(()) }
        fn navigate_to_string(&mut self, _: &str) -> Result<(), SurfaceError> { Ok(()) }
        fn reload(&mut self) -> Result<(), SurfaceError> { Ok(()) }
        fn stop(&mut self) -> Result<(), SurfaceError> { Ok(()) }
        fn go_back(&mut self) -> Result<(), SurfaceError> { Ok(()) }
        fn go_forward(&mut self) -> Result<(), SurfaceError> { Ok(()) }
        fn can_go_back(&self) -> bool { false }
        fn can_go_forward(&self) -> bool { false }
        fn send_mouse_input(&mut self, _: MouseEvent) -> Result<(), SurfaceError> { Ok(()) }
        fn send_pointer_input(&mut self, _: PointerEvent) -> Result<(), SurfaceError> { Ok(()) }
        fn send_keyboard_input(&mut self, _: KeyboardEvent) -> Result<(), SurfaceError> { Ok(()) }
        fn move_focus(&mut self, _: FocusReason) -> Result<(), SurfaceError> { Ok(()) }
        fn poll_navigation_event(&mut self) -> Option<NavigationEvent> { None }
        fn poll_cursor_shape(&mut self) -> Option<CursorShape> { None }
        fn poll_web_message(&mut self) -> Option<WebMessage> { None }
        fn apply_settings(&mut self, _: &SurfaceSettings) -> Result<(), SurfaceError> { Ok(()) }
        fn capture_snapshot_png(&mut self) -> Result<Vec<u8>, SurfaceError> {
            Err(SurfaceError::Unsupported("stub".into()))
        }
    }

    struct StubSurfaceEngine;

    impl SurfaceEngine for StubSurfaceEngine {
        fn engine_id(&self) -> &str { "test.surface" }
        fn spawn(&self, _: &SurfaceSpawnRequest) -> Result<Box<dyn SurfaceProducer>, SurfaceError> {
            Ok(Box::new(StubProducer))
        }
    }

    fn decision(id: &str) -> EngineRouteDecision {
        EngineRouteDecision {
            engine_id: id.to_string(),
            surface_contract: SurfaceContract {
                target: SurfaceTargetId::new("test:1"),
                mode: SurfaceContractMode::CompositedTexture,
            },
        }
    }

    fn stub_request() -> SurfaceSpawnRequest {
        SurfaceSpawnRequest {
            url: "https://example.com".into(),
            width: 800,
            height: 600,
            profile: EngineProfileBinding {
                user_data_dir: "/tmp/test-profile".into(),
            },
            fence_handle: None,
        }
    }

    #[test]
    fn registry_contains_registered_engine() {
        let mut reg = SurfaceEngineRegistry::new();
        reg.register(Box::new(StubSurfaceEngine));
        assert!(reg.contains("test.surface"));
        assert!(!reg.contains("absent.engine"));
    }

    #[test]
    fn registry_spawns_registered_engine() {
        let mut reg = SurfaceEngineRegistry::new();
        reg.register(Box::new(StubSurfaceEngine));
        // `Box<dyn SurfaceProducer>` doesn't implement Debug, so avoid .expect()
        assert!(reg.spawn(&decision("test.surface"), &stub_request()).is_ok());
    }

    #[test]
    fn registry_reports_missing_engine() {
        let reg = SurfaceEngineRegistry::new();
        let result = reg.spawn(&decision("absent.engine"), &stub_request());
        assert!(matches!(result, Err(SurfaceError::EngineNotFound(_))));
    }
}
