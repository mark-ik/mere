// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::FocusRingCurve;
use kernel::graph::NodeKey;

// ---------------------------------------------------------------------------
// §12.14 — Settings view-model (host-neutral projection of user settings)
// ---------------------------------------------------------------------------

/// Host-neutral projection of user-configurable settings. Lives on
/// [`FrameViewModel::settings`](super::FrameViewModel::settings) and is
/// populated by the runtime each frame from
/// `chrome_ui.{focus_ring,thumbnail}_settings` (and future settings groups).
///
/// §12.14 (2026-04-24): exists so iced / egui hosts render settings UI from
/// the same projection rather than reading `chrome_ui.focus_ring_settings`
/// directly. The canonical settings types live in
/// `app/settings_persistence.rs` (graphshell crate); this module mirrors the
/// read-side shape with POD types so the kernel stays independent of the app
/// crate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SettingsViewModel {
    /// Focus-ring animation behavior (enabled toggle, fade duration,
    /// fade curve, optional color override).
    pub focus_ring: FocusRingSettingsView,

    /// Thumbnail capture behavior (enabled toggle, target dimensions,
    /// resampling filter, output format / aspect policy). §12.14
    /// (2026-04-24, second pass): POD mirror of
    /// `app::ThumbnailSettings` so the iced settings panel can render
    /// without depending on the app crate.
    pub thumbnail: ThumbnailSettingsView,
}

/// POD mirror of `app::FocusRingSettings` for host rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusRingSettingsView {
    /// Master enable for the focus-ring overlay. Reduced-motion
    /// accessibility preferences set this to `false`.
    pub enabled: bool,
    /// Fade-out duration in milliseconds.
    pub duration_ms: u32,
    /// Fade-out reshape curve.
    pub curve: FocusRingCurve,
    /// Optional user-chosen RGB color override; `None` means inherit
    /// the active presentation theme's `focus_ring` color.
    pub color_override: Option<[u8; 3]>,
}

impl Default for FocusRingSettingsView {
    fn default() -> Self {
        Self {
            enabled: true,
            duration_ms: 500,
            curve: FocusRingCurve::Linear,
            color_override: None,
        }
    }
}

/// POD mirror of `app::ThumbnailSettings` for host rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbnailSettingsView {
    /// Master enable for the thumbnail-capture pipeline.
    pub enabled: bool,
    /// Target thumbnail width in pixels.
    pub width: u32,
    /// Target thumbnail height in pixels.
    pub height: u32,
    /// Resampling filter applied during downscale.
    pub filter: ThumbnailFilterView,
    /// Encoded output format (PNG / JPEG / WebP).
    pub format: ThumbnailFormatView,
    /// JPEG encoder quality on a 1..=100 scale; ignored when
    /// `format != ThumbnailFormatView::Jpeg`.
    pub jpeg_quality: u8,
    /// Aspect-ratio policy for the downscale pass.
    pub aspect: ThumbnailAspectView,
}

impl Default for ThumbnailSettingsView {
    fn default() -> Self {
        Self {
            enabled: true,
            width: 256,
            height: 192,
            filter: ThumbnailFilterView::Triangle,
            format: ThumbnailFormatView::Png,
            jpeg_quality: 85,
            aspect: ThumbnailAspectView::Fixed,
        }
    }
}

/// POD mirror of `app::ThumbnailFilter`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThumbnailFilterView {
    Nearest,
    #[default]
    Triangle,
    CatmullRom,
    Gaussian,
    Lanczos3,
}

/// POD mirror of `app::ThumbnailFormat`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThumbnailFormatView {
    #[default]
    Png,
    Jpeg,
    WebP,
}

/// POD mirror of `app::ThumbnailAspect`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThumbnailAspectView {
    #[default]
    Fixed,
    MatchSource,
    Square,
}

// ---------------------------------------------------------------------------
// §12.15 — Accessibility view-model (host-neutral summary of AT semantics)
// ---------------------------------------------------------------------------

/// Host-neutral summary of accessibility (AT) state. Lives on
/// [`FrameViewModel::accessibility`](super::FrameViewModel::accessibility)
/// and is populated by the runtime each frame from focus state + the
/// published UxTreeSnapshot.
///
/// §12.15 (2026-04-24): the full UxTreeSnapshot (semantic / presentation /
/// trace nodes) lives shell-side in `shell/desktop/workbench/ux_tree`. This
/// summary carries only the fields hosts need to decide whether to refresh
/// their AccessKit-side AT tree:
///
/// - `focused_node`: which graph node currently owns AT focus.
/// - `snapshot_version`: monotonic counter bumped when semantic content
///   changes; hosts refresh when it advances.
/// - `snapshot_published`: whether the runtime has published a snapshot at
///   all this session. Pre-first-frame, hosts skip the AT pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AccessibilityViewModel {
    pub focused_node: Option<NodeKey>,
    pub snapshot_version: u32,
    pub snapshot_published: bool,
}
