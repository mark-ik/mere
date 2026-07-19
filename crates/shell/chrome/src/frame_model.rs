// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

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

mod settings;
pub use settings::*;
#[cfg(test)]
mod tests;
