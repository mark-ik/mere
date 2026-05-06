/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pure AppState -> FrameViewModel shaping helpers.
//!
//! This is deliberately not Graph Cartography projection vocabulary. These
//! helpers shape already-selected shell/runtime state into host-facing frame
//! view-models; they do not derive graph-memory aggregates.

use std::collections::HashMap;
use std::time::Duration;

use graph_tree::{OwnedTreeRow, SplitBoundary, TabEntry};
use graphshell_core::content::ContentLoadState;
use graphshell_core::geometry::PortableRect;
use graphshell_core::graph::NodeKey;
use graphshell_core::overlay::OverlayStrokePass;
use graphshell_core::pane::{PaneId, TileRenderMode};
use graphshell_core::shell_state::frame_model::{
    AccessibilityViewModel, CommandPaletteScopeView, CommandPaletteViewModel, DegradedReceiptSpec,
    DialogsViewModel, FocusRingSettingsView, FocusRingSpec, FocusViewModel, GraphSearchViewModel,
    OmnibarProviderStatusView, OmnibarSessionKindView, OmnibarViewModel, SettingsViewModel,
    ThumbnailSettingsView, ToastSpec, ToolbarViewModel,
};
use graphshell_core::shell_state::toolbar::ToolbarDraft;
use graphshell_core::time::PortableInstant;

/// Portable inputs needed to shape the focus section of `FrameViewModel`.
pub struct FocusProjectionInput<'a> {
    pub graph_surface_focused: bool,
    pub focus_ring_node_key: Option<NodeKey>,
    pub focus_ring_started_at: Option<PortableInstant>,
    pub focus_ring_settings: FocusRingSettingsView,
    pub pane_activation: Option<PaneId>,
    pub pane_node_order: &'a [(PaneId, NodeKey)],
    pub now: PortableInstant,
}

/// Focus projection plus the active pane node consumed by the top-level frame.
#[derive(Debug, Clone, PartialEq)]
pub struct FocusProjectionOutput {
    pub active_pane: Option<NodeKey>,
    pub focus: FocusViewModel,
}

/// Shape focus runtime state into host-facing focus view-model fields.
pub fn project_focus_view_model(input: FocusProjectionInput<'_>) -> FocusProjectionOutput {
    let first_pane_node = input.pane_node_order.first().map(|(_, node_key)| *node_key);
    let active_pane = input
        .pane_activation
        .and_then(|active_id| {
            input
                .pane_node_order
                .iter()
                .find(|(pane_id, _)| *pane_id == active_id)
                .map(|(_, node_key)| *node_key)
        })
        .or(first_pane_node);

    let ring_spec_candidate = input.focus_ring_node_key.map(|node_key| FocusRingSpec {
        node_key,
        started_at: input.focus_ring_started_at.unwrap_or(input.now),
        duration: Duration::from_millis(u64::from(input.focus_ring_settings.duration_ms)),
    });

    let focus_ring_alpha = if input.focus_ring_settings.enabled {
        ring_spec_candidate
            .as_ref()
            .map(|spec| {
                spec.alpha_at_with_curve(active_pane, input.now, input.focus_ring_settings.curve)
            })
            .unwrap_or(0.0)
    } else {
        0.0
    };

    let focus_ring = ring_spec_candidate.filter(|_| focus_ring_alpha > 0.0);

    FocusProjectionOutput {
        active_pane,
        focus: FocusViewModel {
            focused_node: if input.graph_surface_focused {
                None
            } else {
                first_pane_node
            },
            graph_surface_focused: input.graph_surface_focused,
            focus_ring,
            focus_ring_alpha,
        },
    }
}

/// Portable inputs needed to shape the per-frame layout-cache block of
/// `FrameViewModel`. These come from the shell's `graph_runtime` cached
/// layout outputs (`active_pane_rects`, `pane_render_modes`,
/// `pane_viewer_ids`, `cached_tree_rows`, `cached_tab_order`,
/// `cached_split_boundaries`).
///
/// 2026-04-25: The shell-side `egui::Rect` → [`PortableRect`] conversion
/// stays at the call site so this helper has zero shell deps. The shell
/// adapts its egui rect roster, then hands the portable form to this
/// helper.
pub struct GraphRuntimeLayoutProjectionInput<'a> {
    pub active_pane_rects: &'a [(PaneId, NodeKey, PortableRect)],
    pub pane_render_modes: &'a HashMap<PaneId, TileRenderMode>,
    pub pane_viewer_ids: &'a HashMap<PaneId, String>,
    pub tree_rows: &'a [OwnedTreeRow<NodeKey>],
    pub tab_order: &'a [TabEntry<NodeKey>],
    pub split_boundaries: &'a [SplitBoundary<NodeKey>],
}

/// Output bundle for the layout-cache projection. Mirrors the
/// matching `FrameViewModel` field block plus the derived
/// `is_graph_view` boolean.
#[derive(Clone)]
pub struct GraphRuntimeLayoutProjection {
    pub active_pane_rects: Vec<(PaneId, NodeKey, PortableRect)>,
    pub pane_render_modes: HashMap<PaneId, TileRenderMode>,
    pub pane_viewer_ids: HashMap<PaneId, String>,
    pub tree_rows: Vec<OwnedTreeRow<NodeKey>>,
    pub tab_order: Vec<TabEntry<NodeKey>>,
    pub split_boundaries: Vec<SplitBoundary<NodeKey>>,
    /// `true` when no panes are visible (i.e. the graph canvas surface
    /// is the only thing on screen). Mirrors the predicate
    /// `EguiHost::is_graph_view` used directly by the egui-host's
    /// frame loop. Equivalent to
    /// `pane_queries::tree_has_active_node_pane(graph_app)` inverted.
    pub is_graph_view: bool,
}

/// Shape per-frame layout-cache outputs into the host-facing layout
/// block of `FrameViewModel`. The mapping is a passthrough today —
/// the helper exists to (1) pin the layout-cache → view-model contract
/// with focused unit tests, (2) name the runtime-boundary entry point
/// for layout outputs, and (3) provide a single place to add filtering
/// or validation when richer policies arrive.
pub fn project_graph_runtime_layout_view_model(
    input: GraphRuntimeLayoutProjectionInput<'_>,
) -> GraphRuntimeLayoutProjection {
    let is_graph_view = input.active_pane_rects.is_empty();
    GraphRuntimeLayoutProjection {
        active_pane_rects: input.active_pane_rects.to_vec(),
        pane_render_modes: input.pane_render_modes.clone(),
        pane_viewer_ids: input.pane_viewer_ids.clone(),
        tree_rows: input.tree_rows.to_vec(),
        tab_order: input.tab_order.to_vec(),
        split_boundaries: input.split_boundaries.to_vec(),
        is_graph_view,
    }
}

/// Portable inputs needed to shape the settings section of `FrameViewModel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsProjectionInput {
    pub focus_ring: FocusRingSettingsView,
    pub thumbnail: ThumbnailSettingsView,
}

/// Shape app settings mirrors into the host-facing settings view-model.
pub fn project_settings_view_model(input: SettingsProjectionInput) -> SettingsViewModel {
    SettingsViewModel {
        focus_ring: input.focus_ring,
        thumbnail: input.thumbnail,
    }
}

/// Portable inputs needed to shape the accessibility section of `FrameViewModel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessibilityProjectionInput {
    pub focused_node: Option<NodeKey>,
    pub snapshot_version: u32,
    pub snapshot_published: bool,
}

/// Shape shell accessibility snapshot metadata into a host-facing summary.
pub fn project_accessibility_view_model(
    input: AccessibilityProjectionInput,
) -> AccessibilityViewModel {
    AccessibilityViewModel {
        focused_node: input.focused_node,
        snapshot_version: input.snapshot_version,
        snapshot_published: input.snapshot_published,
    }
}

/// Portable inputs needed to shape the graph-search section of `FrameViewModel`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphSearchProjectionInput {
    pub open: bool,
    pub query: String,
    pub filter_mode: bool,
    pub match_count: usize,
    pub active_match_index: Option<usize>,
}

/// Shape graph-search runtime state into the host-facing search view-model.
pub fn project_graph_search_view_model(input: GraphSearchProjectionInput) -> GraphSearchViewModel {
    GraphSearchViewModel {
        open: input.open,
        query: input.query,
        filter_mode: input.filter_mode,
        match_count: input.match_count,
        active_match_index: input.active_match_index,
    }
}

/// Portable inputs needed to shape the toolbar section of `FrameViewModel`.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolbarProjectionInput {
    pub location: String,
    pub location_dirty: bool,
    pub location_submitted: bool,
    pub load_status: Option<ContentLoadState>,
    pub status_text: Option<String>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub active_pane_draft: Option<(PaneId, ToolbarDraft)>,
}

/// Shape toolbar runtime state into the host-facing toolbar view-model.
pub fn project_toolbar_view_model(input: ToolbarProjectionInput) -> ToolbarViewModel {
    ToolbarViewModel {
        location: input.location,
        location_dirty: input.location_dirty,
        location_submitted: input.location_submitted,
        load_status: input.load_status,
        status_text: input.status_text,
        can_go_back: input.can_go_back,
        can_go_forward: input.can_go_forward,
        active_pane_draft: input.active_pane_draft,
    }
}

/// Portable inputs needed to shape the omnibar section of `FrameViewModel`.
#[derive(Debug, Clone, PartialEq)]
pub struct OmnibarProjectionInput {
    pub kind: OmnibarSessionKindView,
    pub query: String,
    pub match_count: usize,
    pub active_match_index: usize,
    pub selected_index_count: usize,
    pub provider_status: OmnibarProviderStatusView,
}

/// Shape an omnibar session mirror into the host-facing omnibar view-model.
pub fn project_omnibar_view_model(input: OmnibarProjectionInput) -> OmnibarViewModel {
    OmnibarViewModel {
        kind: input.kind,
        query: input.query,
        match_count: input.match_count,
        active_match_index: input.active_match_index,
        selected_index_count: input.selected_index_count,
        provider_status: input.provider_status,
    }
}

/// Portable inputs needed to shape the command-palette section of `FrameViewModel`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPaletteProjectionInput {
    pub open: bool,
    pub contextual_mode: bool,
    pub query: String,
    pub scope: CommandPaletteScopeView,
    pub selected_index: Option<usize>,
    pub toggle_requested: bool,
}

/// Shape command-palette runtime state into the host-facing command-palette view-model.
pub fn project_command_palette_view_model(
    input: CommandPaletteProjectionInput,
) -> CommandPaletteViewModel {
    CommandPaletteViewModel {
        open: input.open,
        contextual_mode: input.contextual_mode,
        query: input.query,
        scope: input.scope,
        selected_index: input.selected_index,
        toggle_requested: input.toggle_requested,
    }
}

/// Portable inputs needed to shape the dialogs section of `FrameViewModel`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DialogsProjectionInput {
    pub bookmark_import_open: bool,
    pub command_palette_toggle_requested: bool,
    pub show_command_palette: bool,
    pub show_context_palette: bool,
    pub show_help_panel: bool,
    pub show_radial_menu: bool,
    pub show_settings_overlay: bool,
    pub show_clip_inspector: bool,
    pub show_scene_overlay: bool,
    pub show_clear_data_confirm: bool,
    pub clear_data_confirm_deadline_secs: Option<f64>,
}

/// Shape dialog/open-state flags into the host-facing dialog view-model.
pub fn project_dialogs_view_model(input: DialogsProjectionInput) -> DialogsViewModel {
    DialogsViewModel {
        bookmark_import_open: input.bookmark_import_open,
        command_palette_toggle_requested: input.command_palette_toggle_requested,
        show_command_palette: input.show_command_palette,
        show_context_palette: input.show_context_palette,
        show_help_panel: input.show_help_panel,
        show_radial_menu: input.show_radial_menu,
        show_settings_overlay: input.show_settings_overlay,
        show_clip_inspector: input.show_clip_inspector,
        show_scene_overlay: input.show_scene_overlay,
        show_clear_data_confirm: input.show_clear_data_confirm,
        clear_data_confirm_deadline_secs: input.clear_data_confirm_deadline_secs,
    }
}

/// Portable inputs needed to shape transient per-frame host outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransientFrameOutputsProjectionInput {
    pub captures_in_flight: usize,
}

/// Transient per-frame outputs that are not yet sourced from runtime tick phases.
pub struct TransientFrameOutputsProjection {
    pub overlays: Vec<OverlayStrokePass>,
    pub toasts: Vec<ToastSpec>,
    pub surfaces_to_present: Vec<NodeKey>,
    pub degraded_receipts: Vec<DegradedReceiptSpec>,
    pub captures_in_flight: usize,
}

/// Shape currently tick-owned transient output placeholders for `FrameViewModel`.
pub fn project_transient_frame_outputs(
    input: TransientFrameOutputsProjectionInput,
) -> TransientFrameOutputsProjection {
    TransientFrameOutputsProjection {
        overlays: Vec::new(),
        toasts: Vec::new(),
        surfaces_to_present: Vec::new(),
        degraded_receipts: Vec::new(),
        captures_in_flight: input.captures_in_flight,
    }
}

#[cfg(test)]
mod tests {
    use graphshell_core::shell_state::frame_model::FocusRingCurve;

    use super::*;

    fn settings() -> FocusRingSettingsView {
        FocusRingSettingsView {
            enabled: true,
            duration_ms: 1_000,
            curve: FocusRingCurve::Linear,
            color_override: None,
        }
    }

    #[test]
    fn active_pane_uses_activation_then_falls_back_to_first_pane() {
        let pane_a = PaneId::new();
        let pane_b = PaneId::new();
        let node_a = NodeKey::new(1);
        let node_b = NodeKey::new(2);
        let panes = [(pane_a, node_a), (pane_b, node_b)];

        let projected = project_focus_view_model(FocusProjectionInput {
            graph_surface_focused: false,
            focus_ring_node_key: None,
            focus_ring_started_at: None,
            focus_ring_settings: settings(),
            pane_activation: Some(pane_b),
            pane_node_order: &panes,
            now: PortableInstant(1_000),
        });
        assert_eq!(projected.active_pane, Some(node_b));
        assert_eq!(projected.focus.focused_node, Some(node_a));

        let projected = project_focus_view_model(FocusProjectionInput {
            pane_activation: Some(PaneId::new()),
            pane_node_order: &panes,
            ..FocusProjectionInput {
                graph_surface_focused: false,
                focus_ring_node_key: None,
                focus_ring_started_at: None,
                focus_ring_settings: settings(),
                pane_activation: None,
                pane_node_order: &[],
                now: PortableInstant(1_000),
            }
        });
        assert_eq!(projected.active_pane, Some(node_a));
    }

    #[test]
    fn graph_surface_focus_hides_focused_node_but_keeps_active_pane() {
        let pane = PaneId::new();
        let node = NodeKey::new(7);
        let panes = [(pane, node)];

        let projected = project_focus_view_model(FocusProjectionInput {
            graph_surface_focused: true,
            focus_ring_node_key: None,
            focus_ring_started_at: None,
            focus_ring_settings: settings(),
            pane_activation: Some(pane),
            pane_node_order: &panes,
            now: PortableInstant(1_000),
        });

        assert_eq!(projected.active_pane, Some(node));
        assert_eq!(projected.focus.focused_node, None);
        assert!(projected.focus.graph_surface_focused);
    }

    #[test]
    fn focus_ring_is_published_only_while_alpha_is_positive() {
        let pane = PaneId::new();
        let node = NodeKey::new(3);
        let panes = [(pane, node)];

        let live = project_focus_view_model(FocusProjectionInput {
            graph_surface_focused: false,
            focus_ring_node_key: Some(node),
            focus_ring_started_at: Some(PortableInstant(1_000)),
            focus_ring_settings: settings(),
            pane_activation: Some(pane),
            pane_node_order: &panes,
            now: PortableInstant(1_500),
        });
        assert!(live.focus.focus_ring.is_some());
        assert!((live.focus.focus_ring_alpha - 0.5).abs() < 1e-6);

        let expired = project_focus_view_model(FocusProjectionInput {
            now: PortableInstant(2_000),
            ..FocusProjectionInput {
                graph_surface_focused: false,
                focus_ring_node_key: Some(node),
                focus_ring_started_at: Some(PortableInstant(1_000)),
                focus_ring_settings: settings(),
                pane_activation: Some(pane),
                pane_node_order: &panes,
                now: PortableInstant(0),
            }
        });
        assert!(expired.focus.focus_ring.is_none());
        assert_eq!(expired.focus.focus_ring_alpha, 0.0);
    }

    #[test]
    fn disabled_focus_ring_projects_zero_alpha() {
        let pane = PaneId::new();
        let node = NodeKey::new(5);
        let panes = [(pane, node)];
        let mut disabled = settings();
        disabled.enabled = false;

        let projected = project_focus_view_model(FocusProjectionInput {
            graph_surface_focused: false,
            focus_ring_node_key: Some(node),
            focus_ring_started_at: Some(PortableInstant(1_000)),
            focus_ring_settings: disabled,
            pane_activation: Some(pane),
            pane_node_order: &panes,
            now: PortableInstant(1_100),
        });

        assert!(projected.focus.focus_ring.is_none());
        assert_eq!(projected.focus.focus_ring_alpha, 0.0);
    }

    #[test]
    fn graph_runtime_layout_projection_passthrough_for_empty_layout() {
        let input = GraphRuntimeLayoutProjectionInput {
            active_pane_rects: &[],
            pane_render_modes: &HashMap::new(),
            pane_viewer_ids: &HashMap::new(),
            tree_rows: &[],
            tab_order: &[],
            split_boundaries: &[],
        };

        let projected = project_graph_runtime_layout_view_model(input);

        assert!(projected.active_pane_rects.is_empty());
        assert!(projected.pane_render_modes.is_empty());
        assert!(projected.pane_viewer_ids.is_empty());
        assert!(projected.tree_rows.is_empty());
        assert!(projected.tab_order.is_empty());
        assert!(projected.split_boundaries.is_empty());
        // Empty layout means the graph canvas is the only visible
        // surface — `is_graph_view` is true.
        assert!(projected.is_graph_view);
    }

    #[test]
    fn graph_runtime_layout_projection_preserves_populated_inputs() {
        let pane_a = PaneId::new();
        let pane_b = PaneId::new();
        let node_a = NodeKey::new(1);
        let node_b = NodeKey::new(2);
        // PortableRect's concrete shape is `euclid::default::Rect<f32>`;
        // tests just need two distinct values for passthrough checks.
        let rect_a = PortableRect::default();
        let rect_b = PortableRect::default();
        let mut render_modes = HashMap::new();
        render_modes.insert(pane_a, TileRenderMode::CompositedTexture);
        render_modes.insert(pane_b, TileRenderMode::NativeOverlay);
        let mut viewer_ids = HashMap::new();
        viewer_ids.insert(pane_a, "viewer:webview".to_string());
        viewer_ids.insert(pane_b, "viewer:wry".to_string());
        let rects = vec![(pane_a, node_a, rect_a), (pane_b, node_b, rect_b)];

        let input = GraphRuntimeLayoutProjectionInput {
            active_pane_rects: &rects,
            pane_render_modes: &render_modes,
            pane_viewer_ids: &viewer_ids,
            tree_rows: &[],
            tab_order: &[],
            split_boundaries: &[],
        };

        let projected = project_graph_runtime_layout_view_model(input);

        assert_eq!(projected.active_pane_rects.len(), 2);
        assert_eq!(projected.active_pane_rects[0], (pane_a, node_a, rect_a));
        assert_eq!(projected.active_pane_rects[1], (pane_b, node_b, rect_b));
        assert_eq!(projected.pane_render_modes.len(), 2);
        assert_eq!(
            projected.pane_render_modes.get(&pane_a),
            Some(&TileRenderMode::CompositedTexture)
        );
        assert_eq!(
            projected.pane_viewer_ids.get(&pane_b).map(String::as_str),
            Some("viewer:wry")
        );
        // Populated layout means at least one pane is visible —
        // `is_graph_view` is false.
        assert!(!projected.is_graph_view);
    }

    #[test]
    fn settings_projection_preserves_focus_and_thumbnail_settings() {
        use graphshell_core::shell_state::frame_model::{
            ThumbnailAspectView, ThumbnailFilterView, ThumbnailFormatView,
        };

        let focus_ring = FocusRingSettingsView {
            enabled: false,
            duration_ms: 250,
            curve: FocusRingCurve::EaseOut,
            color_override: Some([10, 20, 30]),
        };
        let thumbnail = ThumbnailSettingsView {
            enabled: true,
            width: 320,
            height: 180,
            filter: ThumbnailFilterView::Lanczos3,
            format: ThumbnailFormatView::WebP,
            jpeg_quality: 77,
            aspect: ThumbnailAspectView::MatchSource,
        };

        let projected = project_settings_view_model(SettingsProjectionInput {
            focus_ring,
            thumbnail,
        });

        assert_eq!(projected.focus_ring, focus_ring);
        assert_eq!(projected.thumbnail, thumbnail);
    }

    #[test]
    fn accessibility_projection_preserves_snapshot_summary() {
        let focused_node = Some(NodeKey::new(17));

        let projected = project_accessibility_view_model(AccessibilityProjectionInput {
            focused_node,
            snapshot_version: 42,
            snapshot_published: true,
        });

        assert_eq!(projected.focused_node, focused_node);
        assert_eq!(projected.snapshot_version, 42);
        assert!(projected.snapshot_published);
    }

    #[test]
    fn graph_search_projection_preserves_query_and_match_state() {
        let projected = project_graph_search_view_model(GraphSearchProjectionInput {
            open: true,
            query: "needle".to_string(),
            filter_mode: true,
            match_count: 12,
            active_match_index: Some(3),
        });

        assert!(projected.open);
        assert_eq!(projected.query, "needle");
        assert!(projected.filter_mode);
        assert_eq!(projected.match_count, 12);
        assert_eq!(projected.active_match_index, Some(3));
    }

    #[test]
    fn toolbar_projection_preserves_navigation_and_draft_state() {
        let pane = PaneId::new();
        let draft = ToolbarDraft {
            location: "https://draft.test/".to_string(),
            location_dirty: true,
            location_submitted: false,
        };

        let projected = project_toolbar_view_model(ToolbarProjectionInput {
            location: "https://current.test/".to_string(),
            location_dirty: false,
            location_submitted: true,
            load_status: Some(ContentLoadState::HeadParsed),
            status_text: Some("Loading".to_string()),
            can_go_back: true,
            can_go_forward: false,
            active_pane_draft: Some((pane, draft.clone())),
        });

        assert_eq!(projected.location, "https://current.test/");
        assert!(!projected.location_dirty);
        assert!(projected.location_submitted);
        assert_eq!(projected.load_status, Some(ContentLoadState::HeadParsed));
        assert_eq!(projected.status_text.as_deref(), Some("Loading"));
        assert!(projected.can_go_back);
        assert!(!projected.can_go_forward);
        assert_eq!(projected.active_pane_draft, Some((pane, draft)));
    }

    #[test]
    fn omnibar_projection_preserves_provider_status_and_counts() {
        let projected = project_omnibar_view_model(OmnibarProjectionInput {
            kind: OmnibarSessionKindView::SearchProvider,
            query: "search term".to_string(),
            match_count: 5,
            active_match_index: 2,
            selected_index_count: 3,
            provider_status: OmnibarProviderStatusView::FailedHttp(429),
        });

        assert_eq!(projected.kind, OmnibarSessionKindView::SearchProvider);
        assert_eq!(projected.query, "search term");
        assert_eq!(projected.match_count, 5);
        assert_eq!(projected.active_match_index, 2);
        assert_eq!(projected.selected_index_count, 3);
        assert_eq!(
            projected.provider_status,
            OmnibarProviderStatusView::FailedHttp(429)
        );
    }

    #[test]
    fn command_palette_projection_preserves_session_state() {
        let projected = project_command_palette_view_model(CommandPaletteProjectionInput {
            open: true,
            contextual_mode: true,
            query: "open".to_string(),
            scope: CommandPaletteScopeView::ActivePane,
            selected_index: Some(4),
            toggle_requested: true,
        });

        assert!(projected.open);
        assert!(projected.contextual_mode);
        assert_eq!(projected.query, "open");
        assert_eq!(projected.scope, CommandPaletteScopeView::ActivePane);
        assert_eq!(projected.selected_index, Some(4));
        assert!(projected.toggle_requested);
    }

    #[test]
    fn lightweight_projection_parity_target_exercises_extracted_helpers() {
        let pane = PaneId::new();
        let node = NodeKey::new(29);
        let panes = [(pane, node)];

        let focus = project_focus_view_model(FocusProjectionInput {
            graph_surface_focused: false,
            focus_ring_node_key: Some(node),
            focus_ring_started_at: Some(PortableInstant(10_000)),
            focus_ring_settings: settings(),
            pane_activation: Some(pane),
            pane_node_order: &panes,
            now: PortableInstant(10_250),
        });
        let toolbar = project_toolbar_view_model(ToolbarProjectionInput {
            location: "https://parity.test/".to_string(),
            location_dirty: true,
            location_submitted: false,
            load_status: Some(ContentLoadState::Complete),
            status_text: None,
            can_go_back: false,
            can_go_forward: true,
            active_pane_draft: None,
        });
        let search = project_graph_search_view_model(GraphSearchProjectionInput {
            open: true,
            query: "parity".to_string(),
            filter_mode: false,
            match_count: 2,
            active_match_index: Some(1),
        });
        let command_palette = project_command_palette_view_model(CommandPaletteProjectionInput {
            open: false,
            contextual_mode: false,
            query: String::new(),
            scope: CommandPaletteScopeView::Workbench,
            selected_index: None,
            toggle_requested: false,
        });
        let dialogs = project_dialogs_view_model(DialogsProjectionInput {
            bookmark_import_open: false,
            command_palette_toggle_requested: false,
            show_command_palette: false,
            show_context_palette: false,
            show_help_panel: false,
            show_radial_menu: false,
            show_settings_overlay: false,
            show_clip_inspector: false,
            show_scene_overlay: false,
            show_clear_data_confirm: false,
            clear_data_confirm_deadline_secs: None,
        });

        assert_eq!(focus.active_pane, Some(node));
        assert_eq!(focus.focus.focused_node, Some(node));
        assert!(focus.focus.focus_ring_alpha > 0.0);
        assert_eq!(toolbar.location, "https://parity.test/");
        assert!(toolbar.can_go_forward);
        assert_eq!(search.match_count, 2);
        assert_eq!(search.active_match_index, Some(1));
        assert_eq!(command_palette.scope, CommandPaletteScopeView::Workbench);
        assert!(!dialogs.show_clear_data_confirm);
    }

    #[test]
    fn dialogs_projection_preserves_open_flags_and_deadline() {
        let projected = project_dialogs_view_model(DialogsProjectionInput {
            bookmark_import_open: true,
            command_palette_toggle_requested: true,
            show_command_palette: true,
            show_context_palette: false,
            show_help_panel: true,
            show_radial_menu: false,
            show_settings_overlay: true,
            show_clip_inspector: false,
            show_scene_overlay: true,
            show_clear_data_confirm: true,
            clear_data_confirm_deadline_secs: Some(123.5),
        });

        assert!(projected.bookmark_import_open);
        assert!(projected.command_palette_toggle_requested);
        assert!(projected.show_command_palette);
        assert!(!projected.show_context_palette);
        assert!(projected.show_help_panel);
        assert!(!projected.show_radial_menu);
        assert!(projected.show_settings_overlay);
        assert!(!projected.show_clip_inspector);
        assert!(projected.show_scene_overlay);
        assert!(projected.show_clear_data_confirm);
        assert_eq!(projected.clear_data_confirm_deadline_secs, Some(123.5));
    }

    #[test]
    fn transient_frame_outputs_projection_preserves_capture_count() {
        let projected = project_transient_frame_outputs(TransientFrameOutputsProjectionInput {
            captures_in_flight: 3,
        });

        assert!(projected.overlays.is_empty());
        assert!(projected.toasts.is_empty());
        assert!(projected.surfaces_to_present.is_empty());
        assert!(projected.degraded_receipts.is_empty());
        assert_eq!(projected.captures_in_flight, 3);
    }
}
