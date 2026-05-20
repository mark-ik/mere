// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Reactive diagnostics panel — the first real xilem-driven panel.
//!
//! A `MasonryEmbeddedRenderer` panel whose content is a xilem
//! `View<DiagnosticsState, (), ViewCtx>`: a column of labels showing
//! live host metrics (frame count, substrate node count, last pointer
//! position). The host pushes a fresh [`DiagnosticsSnapshot`] each
//! frame through a shared handle; the panel's `tick` re-runs the view
//! logic and rebuilds, so the labels update live.
//!
//! Read-only for v0 — no buttons / action routing. The reactive loop
//! proven here is: host mutates shared state → panel re-runs logic →
//! masonry rebuilds the widget tree → texture repaints.

use std::sync::{Arc, Mutex};

use mere_masonry::xilem_masonry::view::{flex_col, label};
use mere_masonry::xilem_masonry::{AnyWidgetView, WidgetView};
use mere_masonry::{PanelLogic, PanelTile, TileSize, XilemPanel};
use masonry_core::core::DefaultProperties;

/// Live host metrics rendered by the diagnostics panel. Cheap to copy;
/// the host overwrites the shared snapshot each frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DiagnosticsSnapshot {
    /// Frames rendered since startup.
    pub frame_count: u64,
    /// Substrate scene node count (panes + splitters + exploded graph
    /// nodes) at last render.
    pub scene_nodes: usize,
    /// Last pointer position in window coordinates, if the cursor is
    /// over the window.
    pub cursor: Option<(f64, f64)>,
}

/// Shared handle the host writes and the panel reads. The panel's
/// `State` is a clone of the latest snapshot, refreshed each `tick`.
pub type DiagnosticsHandle = Arc<Mutex<DiagnosticsSnapshot>>;

/// The panel's own reactive state: the latest snapshot plus the shared
/// handle it pulls fresh values from each frame.
struct PanelState {
    handle: DiagnosticsHandle,
    latest: DiagnosticsSnapshot,
}

/// Build the view tree for the current snapshot — a titled column of
/// metric labels.
fn diagnostics_view(state: &mut PanelState) -> Box<AnyWidgetView<PanelState, ()>> {
    // Pull the freshest snapshot the host has published.
    state.latest = *state.handle.lock().expect("diagnostics handle poisoned");
    let snap = state.latest;
    let cursor = match snap.cursor {
        Some((x, y)) => format!("cursor: {x:.0}, {y:.0}"),
        None => "cursor: —".to_string(),
    };
    flex_col((
        label("Diagnostics").text_size(18.0),
        label(format!("frame: {}", snap.frame_count)),
        label(format!("scene nodes: {}", snap.scene_nodes)),
        label(cursor),
    ))
    .boxed()
}

/// Construct a reactive diagnostics [`XilemPanel`] reading from
/// `handle`. Boxed as a [`PanelTile`] for the embedded renderer's
/// producer map.
pub fn build_diagnostics_panel(
    handle: DiagnosticsHandle,
    runtime: Arc<tokio::runtime::Runtime>,
    default_properties: Arc<DefaultProperties>,
    size: TileSize,
    scale_factor: f64,
) -> Box<dyn PanelTile> {
    let state = PanelState {
        handle,
        latest: DiagnosticsSnapshot::default(),
    };
    let logic: PanelLogic<PanelState> = Box::new(diagnostics_view);
    Box::new(XilemPanel::new(
        state,
        logic,
        runtime,
        default_properties,
        size,
        scale_factor,
    ))
}
