/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable view-model + host-input types at the runtime ↔ host
//! boundary.
//!
//! Pre-M4 slice 8 (2026-04-22) these types lived in
//! `shell/desktop/ui/frame_model.rs` alongside `FrameViewModel`. Slice
//! 8 extracted the view-model *children* (focus / toolbar / omnibar
//! / graph-search / command-palette / dialogs / toasts /
//! degraded-receipts projections) and `FrameHostInput` to graphshell-
//! core; the shell re-exports from the original path.
//!
//! The top-level `FrameViewModel` aggregate remains shell-side for now
//! because one of its fields (`overlays: Vec<OverlayStrokePass>`)
//! depends on egui-coupled compositor descriptors that haven't been
//! extracted yet. That's a follow-on slice.
//!
//! Time representation: [`FocusRingSpec.started_at`](FocusRingSpec) is a
//! [`PortableInstant`] — the host supplies monotonic ms-from-origin
//! timestamps via [`FrameHostInput`]; the runtime never asks the
//! platform "what time is it now".

use std::collections::HashMap;
use std::time::Duration;

use forme::{OwnedTreeRow, SplitBoundary, TabEntry};

use crate::toolbar::ToolbarDraft;
use kernel::content::ContentLoadState;
use kernel::geometry::{PortablePoint, PortableRect, PortableSize};
use kernel::graph::NodeKey;
use kernel::host_event::{HostEvent, ModifiersState};
use kernel::overlay::OverlayStrokePass;
use kernel::pane::{PaneId, TileRenderMode};
use kernel::time::PortableInstant;

// ---------------------------------------------------------------------------
// FrameViewModel — aggregate per-frame host-painting snapshot
// ---------------------------------------------------------------------------

/// Per-frame snapshot produced by `GraphshellRuntime` for the host to
/// paint.
///
/// All fields are read-only from the host's perspective. The host may
/// rasterise, lay out, or cache derived quantities, but must not mutate
/// the model — any feedback flows back through host ports /
/// [`FrameHostInput`].
///
/// No `Debug` derive because `OverlayStrokePass` transitively contains
/// non-Debug fields. Can be revisited independently.
#[derive(Clone, Default)]
pub struct FrameViewModel {
    /// Visible panes with their screen rects (portable units), in
    /// stable iteration order.
    pub active_pane_rects: Vec<(PaneId, NodeKey, PortableRect)>,

    /// `PaneId` → `TileRenderMode` mapping, refreshed per frame
    /// alongside `active_pane_rects`. Mirrors
    /// `graph_runtime.pane_render_modes` for hosts that can't read
    /// the runtime state directly (iced).
    pub pane_render_modes: HashMap<PaneId, TileRenderMode>,

    /// `PaneId` → viewer-ID string mapping, refreshed per frame.
    /// Resolves to the string identifier of the viewer implementation
    /// a pane currently hosts (e.g., "servo", "wry:…"). Consumed by
    /// compositor semantic-input resolution.
    pub pane_viewer_ids: HashMap<PaneId, String>,

    /// GraphTree rows for sidebar / navigator rendering.
    pub tree_rows: Vec<OwnedTreeRow<NodeKey>>,

    /// Flat tab ordering for a tab-bar view.
    pub tab_order: Vec<TabEntry<NodeKey>>,

    /// Split boundaries (draggable gutter handles between panes).
    pub split_boundaries: Vec<SplitBoundary<NodeKey>>,

    /// Currently active member (the pane that owns keyboard focus).
    pub active_pane: Option<NodeKey>,

    /// Aggregate focus state (which surface is focused, focus ring
    /// animation).
    pub focus: FocusViewModel,

    /// Toolbar / location bar state.
    pub toolbar: ToolbarViewModel,

    /// Omnibar search session projection. `None` when no session is
    /// active.
    pub omnibar: Option<OmnibarViewModel>,

    /// Graph-search (Ctrl+G) panel state projection.
    pub graph_search: GraphSearchViewModel,

    /// Command-palette (F2 / Ctrl+K) session projection.
    pub command_palette: CommandPaletteViewModel,

    /// Overlay descriptors the host must paint this frame (focus
    /// rings, selection strokes, lens glyphs, etc.).
    pub overlays: Vec<OverlayStrokePass>,

    /// Which dialogs / overlays are open.
    pub dialogs: DialogsViewModel,

    /// Toasts queued this frame — the host drains and displays them.
    pub toasts: Vec<ToastSpec>,

    /// Content surfaces whose content changed this frame and should be
    /// presented. The host consults its surface registry to resolve
    /// each key to a concrete handle.
    pub surfaces_to_present: Vec<NodeKey>,

    /// UX-visible degraded-mode receipts the host should render as
    /// chrome (e.g., "content viewer is in degraded mode").
    pub degraded_receipts: Vec<DegradedReceiptSpec>,

    /// Number of viewer thumbnail captures currently pending async
    /// completion. Hosts can gate a "capture in progress" spinner on
    /// `captures_in_flight > 0`.
    pub captures_in_flight: usize,

    /// User-configurable settings projected into a host-neutral shape.
    /// §12.14 (2026-04-24): the canonical settings types live in
    /// `app/settings_persistence.rs` (graphshell crate), but the
    /// host-facing FrameViewModel must not depend on app types — POD
    /// mirror in [`SettingsViewModel`] keeps the data flow portable.
    /// Egui and iced both render settings UI from this projection
    /// instead of reading `chrome_ui.focus_ring_settings` /
    /// `chrome_ui.thumbnail_settings` directly.
    pub settings: SettingsViewModel,

    /// Accessibility (AT) semantics projected into a host-neutral
    /// summary. §12.15 (2026-04-24): the full UxTreeSnapshot lives
    /// shell-side at `shell/desktop/workbench/ux_tree::latest_snapshot()`
    /// — the view-model carries only the correlation seam (which node
    /// AT focus is on, whether the AT tree has been published this
    /// frame, snapshot version counters) so hosts can decide whether
    /// to refresh their AccessKit-side tree without the kernel
    /// depending on the shell-side UxTree types.
    pub accessibility: AccessibilityViewModel,

    /// Whether the workbench is currently displaying the
    /// graph-canvas-only view (no node panes mounted). §12.6
    /// (2026-04-24, second pass): EguiHost previously derived this on
    /// every read by walking the tile tree
    /// (`pane_queries::tree_has_active_node_pane`); projecting it once
    /// per frame lets hosts gate graph-vs-detail UI off the cached
    /// view-model rather than re-running the predicate ad hoc.
    pub is_graph_view: bool,
}

// ---------------------------------------------------------------------------
// Focus ring animation + curve
// ---------------------------------------------------------------------------

/// Shape of the focus-ring fade-out curve the runtime applies between
/// a freshly-latched focus transition and the ring's expiry.
///
/// `Linear` is the historical default (constant-rate fade). `EaseOut`
/// gives a slower fade at first that accelerates toward zero — makes
/// the ring feel like it's "settling in" before fading. `Step` skips
/// the animation entirely (ring is either fully lit or fully off),
/// which is the right choice for reduced-motion accessibility
/// profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum FocusRingCurve {
    /// alpha = 1 − t/d (default).
    #[default]
    Linear,
    /// alpha = 1 − (t/d)² — slow at start, fast at end.
    EaseOut,
    /// alpha = 1 while t < d, else 0 — instant cutoff, no fade.
    Step,
}

impl FocusRingCurve {
    /// Reshape a normalized fade progress in `[0.0, 1.0]` (0 = just
    /// latched, 1 = animation complete) into a visual alpha value in
    /// `[0.0, 1.0]`.
    pub fn alpha_from_progress(self, progress: f32) -> f32 {
        let p = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => 1.0 - p,
            Self::EaseOut => {
                let remaining = 1.0 - p;
                remaining * remaining
            }
            Self::Step => {
                if p >= 1.0 {
                    0.0
                } else {
                    1.0
                }
            }
        }
    }
}

impl std::fmt::Display for FocusRingCurve {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Linear => "linear",
            Self::EaseOut => "ease_out",
            Self::Step => "step",
        };
        f.write_str(s)
    }
}

impl std::str::FromStr for FocusRingCurve {
    type Err = ();

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim() {
            "linear" => Ok(Self::Linear),
            "ease_out" => Ok(Self::EaseOut),
            "step" => Ok(Self::Step),
            _ => Err(()),
        }
    }
}

/// Focus-ring animation state the host renders over a node pane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FocusRingSpec {
    pub node_key: NodeKey,
    pub started_at: PortableInstant,
    pub duration: Duration,
}

impl FocusRingSpec {
    /// Paint alpha at `now` for a given currently-focused node using
    /// the default linear curve. Returns 0.0 when the ring does not
    /// apply (different node, or animation elapsed); otherwise a
    /// linear fade-out from 1.0 to 0.0 across `duration`.
    pub fn alpha_at(&self, focused_node_key: Option<NodeKey>, now: PortableInstant) -> f32 {
        self.alpha_at_with_curve(focused_node_key, now, FocusRingCurve::Linear)
    }

    /// Paint alpha at `now` with the supplied fade reshape. Same
    /// gating semantics as [`Self::alpha_at`] — returns 0.0 when the
    /// ring doesn't apply to `focused_node_key` or when the animation
    /// has elapsed — but the in-window alpha is piped through
    /// [`FocusRingCurve::alpha_from_progress`] so callers can honor
    /// user preference (linear, ease-out, step).
    pub fn alpha_at_with_curve(
        &self,
        focused_node_key: Option<NodeKey>,
        now: PortableInstant,
        curve: FocusRingCurve,
    ) -> f32 {
        if Some(self.node_key) != focused_node_key {
            return 0.0;
        }
        let duration_ms = u64::try_from(self.duration.as_millis()).unwrap_or(u64::MAX);
        if duration_ms == 0 {
            // Avoid a division-by-zero when the user has configured an
            // instant-off ring (`duration_ms = 0`). Step-like behavior.
            return 0.0;
        }
        let elapsed_ms = now.saturating_elapsed_since(self.started_at);
        if elapsed_ms >= duration_ms {
            return 0.0;
        }
        let progress = (elapsed_ms as f32) / (duration_ms as f32);
        curve.alpha_from_progress(progress)
    }
}

// ---------------------------------------------------------------------------
// View-model projections — read-only shapes the host paints each frame.
// ---------------------------------------------------------------------------

/// Aggregate focus state exposed to the host.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FocusViewModel {
    /// Currently focused node (for node-pane focus; None for
    /// graph-surface focus or no focus).
    pub focused_node: Option<NodeKey>,

    /// Whether the graph canvas has focus (as opposed to a node pane
    /// or chrome).
    pub graph_surface_focused: bool,

    /// Active focus-ring animation, if any.
    pub focus_ring: Option<FocusRingSpec>,

    /// Focus-ring paint alpha for the current focused node at
    /// projection time (0.0 when no ring applies; 1.0→0.0 linear
    /// fade-out while the ring animation is live). Hosts paint the
    /// ring proportional to this value without having to read
    /// `started_at`/`duration` and re-derive the math.
    pub focus_ring_alpha: f32,
}

/// Toolbar / location-bar projection for the host.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolbarViewModel {
    pub location: String,
    pub location_dirty: bool,
    pub location_submitted: bool,
    pub load_status: Option<ContentLoadState>,
    pub status_text: Option<String>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    /// Draft snapshot for the currently active pane, if one has been
    /// captured. Hosts rarely consume the draft directly — it is
    /// exposed so iced can render per-pane indicators (e.g., a
    /// "draft pending" dot on tab chrome) without reaching into the
    /// runtime's `toolbar_drafts` map.
    pub active_pane_draft: Option<(PaneId, ToolbarDraft)>,
}

/// Omnibar search session projection.
///
/// Captures the state the host must paint this frame when the omnibar
/// is active: query text, current match slate, active-match cursor,
/// and provider-suggestion status.
#[derive(Debug, Clone, PartialEq)]
pub struct OmnibarViewModel {
    pub kind: OmnibarSessionKindView,
    pub query: String,
    pub match_count: usize,
    pub active_match_index: usize,
    pub selected_index_count: usize,
    pub provider_status: OmnibarProviderStatusView,
}

/// Host-neutral classification of an omnibar session's origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmnibarSessionKindView {
    /// Graph-scoped navigation session (node/tab/edge match modes).
    Graph,
    /// External search-provider session (DuckDuckGo, Bing, Google).
    SearchProvider,
}

/// Host-neutral projection of the provider suggestion mailbox status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmnibarProviderStatusView {
    Idle,
    Loading,
    Ready,
    FailedNetwork,
    FailedHttp(u16),
    FailedParse,
}

/// Graph-search panel (Ctrl+G) projection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphSearchViewModel {
    pub open: bool,
    pub query: String,
    pub filter_mode: bool,
    pub match_count: usize,
    pub active_match_index: Option<usize>,
}

/// Command-palette projection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommandPaletteViewModel {
    pub open: bool,
    pub contextual_mode: bool,
    pub query: String,
    pub scope: CommandPaletteScopeView,
    pub selected_index: Option<usize>,
    pub toggle_requested: bool,
}

/// Host-neutral projection of `SearchPaletteScope`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CommandPaletteScopeView {
    CurrentTarget,
    ActivePane,
    ActiveGraph,
    #[default]
    Workbench,
}

/// Which dialogs / overlays are open. Flags for booleans; detailed
/// state for dialogs with content.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DialogsViewModel {
    pub bookmark_import_open: bool,
    pub command_palette_toggle_requested: bool,
    pub show_command_palette: bool,
    pub show_context_palette: bool,
    pub show_help_panel: bool,
    pub show_radial_menu: bool,
    pub show_settings_overlay: bool,
    pub show_clip_inspector: bool,
    pub show_scene_overlay: bool,
    /// "Clear graph and saved data" two-step confirmation is primed.
    pub show_clear_data_confirm: bool,
    /// Unix-seconds deadline for the clear-data confirm two-step
    /// prompt. `None` when not armed.
    pub clear_data_confirm_deadline_secs: Option<f64>,
}

/// Host-neutral toast spec. The host maps this onto its notification
/// system (egui_notify::Toasts, iced's toast widget, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct ToastSpec {
    pub severity: ToastSeverity,
    pub message: String,
    pub duration: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastSeverity {
    Info,
    Success,
    Warning,
    Error,
}

/// Host-neutral degraded-receipt spec. Mirrors the private
/// `DegradedReceipt` inside `tile_compositor`; the view-model exposes
/// it so any host can render the receipts without reaching into
/// compositor internals.
#[derive(Debug, Clone, PartialEq)]
pub struct DegradedReceiptSpec {
    pub tile_rect: PortableRect,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Host input: FrameHostInput
// ---------------------------------------------------------------------------

/// Per-frame input bundle flowing from the host into the runtime.
///
/// The runtime consumes this, advances state, and produces a view
/// model. Any host-specific side effects (webview creation requests,
/// clipboard writes, focus handoffs) travel back through host ports
/// rather than through the view model.
#[derive(Debug, Clone, Default)]
pub struct FrameHostInput {
    /// Host-neutral events translated from native input (keyboard,
    /// pointer, scroll, focus, resize, synthesized command-surface
    /// actions).
    pub events: Vec<HostEvent>,

    /// Current pointer hover position in screen coordinates (if any).
    pub pointer_hover: Option<PortablePoint>,

    /// Current viewport size.
    pub viewport_size: PortableSize,

    /// Whether a host-owned widget currently wants keyboard input
    /// (affects whether the runtime routes keyboard events to
    /// content).
    pub wants_keyboard: bool,

    /// Whether a host-owned widget currently wants pointer input.
    pub wants_pointer: bool,

    /// Active keyboard modifier state this frame.
    pub modifiers: ModifiersState,

    /// True when the host observed at least one native input event
    /// this frame. Used by the runtime to mark a user gesture for
    /// idle-watchdog timing. Populated even when `events` is still
    /// empty during the partial event-translation migration.
    pub had_input_events: bool,

    /// Portable host-originated intents the runtime applies during
    /// this tick. Populated when chrome surfaces (toolbar submit,
    /// command palette action, omnibar selection) express a user
    /// decision that needs to reach the reducer without the host
    /// directly calling `apply_graph_delta_and_sync` — per §12.17.
    ///
    /// Runtime drain order: `apply_host_intents` runs immediately
    /// after `ingest_frame_input` so any view-model the tick
    /// projects reflects the applied intents.
    pub host_intents: Vec<crate::host_intent::HostIntent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(node: NodeKey, started: PortableInstant, duration_ms: u64) -> FocusRingSpec {
        FocusRingSpec {
            node_key: node,
            started_at: started,
            duration: Duration::from_millis(duration_ms),
        }
    }

    #[test]
    fn alpha_is_zero_when_focused_node_differs() {
        let now = PortableInstant(0);
        let s = spec(NodeKey::new(1), now, 500);
        assert_eq!(s.alpha_at(Some(NodeKey::new(2)), now), 0.0);
        assert_eq!(s.alpha_at(None, now), 0.0);
    }

    #[test]
    fn alpha_is_full_at_start_and_zero_past_duration() {
        let start = PortableInstant(1_000);
        let s = spec(NodeKey::new(7), start, 500);
        // Exactly at start -> full intensity.
        assert!((s.alpha_at(Some(NodeKey::new(7)), start) - 1.0).abs() < 1e-6);
        // After duration -> clamped to zero.
        let past = start.saturating_add_ms(600);
        assert_eq!(s.alpha_at(Some(NodeKey::new(7)), past), 0.0);
    }

    #[test]
    fn alpha_fades_linearly_through_duration() {
        let start = PortableInstant(1_000);
        let s = spec(NodeKey::new(3), start, 1_000);
        let half = start.saturating_add_ms(500);
        let alpha = s.alpha_at(Some(NodeKey::new(3)), half);
        // At t = duration/2: linear alpha = 0.5.
        assert!((alpha - 0.5).abs() < 1e-6);
    }

    #[test]
    fn alpha_at_with_curve_applies_ease_out() {
        let start = PortableInstant(0);
        let s = spec(NodeKey::new(5), start, 1_000);
        let half = PortableInstant(500);
        let linear = s.alpha_at_with_curve(Some(NodeKey::new(5)), half, FocusRingCurve::Linear);
        let ease_out = s.alpha_at_with_curve(Some(NodeKey::new(5)), half, FocusRingCurve::EaseOut);
        // EaseOut at half-progress: (1 - 0.5)² = 0.25, vs linear 0.5.
        assert!((linear - 0.5).abs() < 1e-6);
        assert!((ease_out - 0.25).abs() < 1e-6);
    }

    #[test]
    fn alpha_at_with_curve_applies_step() {
        let start = PortableInstant(0);
        let s = spec(NodeKey::new(5), start, 1_000);
        // Step: 1.0 while progress < 1.0, else 0.0.
        let half = PortableInstant(500);
        let almost = PortableInstant(999);
        let past = PortableInstant(1_000);
        assert_eq!(
            s.alpha_at_with_curve(Some(NodeKey::new(5)), half, FocusRingCurve::Step),
            1.0
        );
        assert_eq!(
            s.alpha_at_with_curve(Some(NodeKey::new(5)), almost, FocusRingCurve::Step),
            1.0
        );
        // At duration: clamped to 0.0 (elapsed >= duration gate).
        assert_eq!(
            s.alpha_at_with_curve(Some(NodeKey::new(5)), past, FocusRingCurve::Step),
            0.0
        );
    }

    #[test]
    fn alpha_is_zero_when_duration_is_zero() {
        // Defensive: `duration_ms = 0` is a valid config (user picks
        // "instant-off ring" via reduced-motion preferences). Pin
        // that division-by-zero doesn't happen.
        let start = PortableInstant(0);
        let s = spec(NodeKey::new(2), start, 0);
        assert_eq!(s.alpha_at(Some(NodeKey::new(2)), start), 0.0);
    }

    #[test]
    fn focus_ring_curve_from_str_and_display_round_trip() {
        for curve in [
            FocusRingCurve::Linear,
            FocusRingCurve::EaseOut,
            FocusRingCurve::Step,
        ] {
            let s = curve.to_string();
            let back: FocusRingCurve = s.parse().expect("round trip");
            assert_eq!(back, curve);
        }
    }

    #[test]
    fn focus_ring_curve_default_is_linear() {
        assert_eq!(FocusRingCurve::default(), FocusRingCurve::Linear);
    }

    #[test]
    fn focus_ring_curve_alpha_from_progress_clamps() {
        // Progress outside [0,1] clamps — pin it so malformed inputs
        // don't produce out-of-range alphas.
        assert_eq!(FocusRingCurve::Linear.alpha_from_progress(-0.5), 1.0 - 0.0);
        assert_eq!(FocusRingCurve::Linear.alpha_from_progress(1.5), 0.0);
    }

    #[test]
    fn frame_host_input_default_is_empty() {
        let input = FrameHostInput::default();
        assert!(input.events.is_empty());
        assert!(input.pointer_hover.is_none());
        assert!(!input.wants_keyboard);
        assert!(!input.wants_pointer);
        assert!(!input.had_input_events);
    }
}

// ---------------------------------------------------------------------------
// §12.14 — Settings view-model (host-neutral projection of user settings)
// ---------------------------------------------------------------------------

/// Host-neutral projection of user-configurable settings. Lives on
/// [`FrameViewModel::settings`] and is populated by the runtime each
/// frame from `chrome_ui.{focus_ring,thumbnail}_settings` (and
/// future settings groups).
///
/// §12.14 (2026-04-24): exists so iced / egui hosts render settings
/// UI from the same projection rather than reading
/// `chrome_ui.focus_ring_settings` directly. The canonical settings
/// types live in `app/settings_persistence.rs` (graphshell crate);
/// this module mirrors the read-side shape with POD types so the
/// mere-kernel kernel stays independent of the app crate.
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
/// [`FrameViewModel::accessibility`] and is populated by the runtime
/// each frame from focus state + the published UxTreeSnapshot.
///
/// §12.15 (2026-04-24): the full UxTreeSnapshot (semantic / presentation
/// / trace nodes) lives shell-side in
/// `shell/desktop/workbench/ux_tree`. This summary carries only the
/// fields hosts need to decide whether to refresh their AccessKit-side
/// AT tree:
///
/// - `focused_node`: which graph node currently owns AT focus (mirrors
///   `FocusViewModel::focused_node` with explicit AT semantics).
/// - `snapshot_version`: monotonic counter the runtime bumps when the
///   UxTreeSnapshot semantic content changes; hosts cache the last
///   version they synced with AccessKit and refresh when it advances.
/// - `snapshot_published`: whether the runtime has published a snapshot
///   at all this session. Pre-first-frame, hosts skip the AT pass.
///
/// Hosts that need the full snapshot fetch it from
/// `ux_tree::latest_snapshot()` independently — the view-model is the
/// "do I need to look?" signal, not the data carrier.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AccessibilityViewModel {
    pub focused_node: Option<NodeKey>,
    pub snapshot_version: u32,
    pub snapshot_published: bool,
}
