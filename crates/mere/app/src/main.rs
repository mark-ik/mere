// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Mere — idiomatic Xilem app (2026-05-21 re-scaffold).
//!
//! Per `design_docs/mere_docs/technical_architecture/2026-05-21_app_architecture_rescaffold.md`:
//! the chrome (frametree splits, workbench, panels) is a Xilem view
//! tree over **one** [`AppState`]; the graph canvas (orrery) will be a
//! custom Masonry widget. No substrate-as-host, no renderer registry,
//! no action bus — Xilem's `View<State>` + state mutation is the whole
//! app-coordination layer (Woodshed's proven shape).
//!
//! Stage 1 (this file): the chrome shape is touchable — a frametree of
//! `split` views with stock-widget panes, one of which (orrery) is a
//! placeholder until the `GraphCanvas` widget lands. The "+ node"
//! button proves the reactive loop end-to-end (mutate `AppState.graph`
//! → view rebuilds → node count updates).

use kernel::geometry::PortablePoint;
use kernel::graph::Graph;
use xilem::masonry::kurbo::Axis;
use xilem::view::{button, flex_col, label, prose, split};
use xilem::{EventLoop, WidgetView, WindowOptions, Xilem};

/// The single application state the Xilem driver owns. Widgets mutate
/// it in place through their callbacks; the view tree rebuilds on diff.
/// Per-pane UI sub-state gets its own struct here as panes grow
/// (Woodshed's pattern); for the skeleton it's just the graph + a
/// frame label.
struct AppState {
    /// Graph truth — the orrery projects this. Foundational `kernel`
    /// crate, framework-free.
    graph: Graph,
    /// Human label for the current frame/workspace (placeholder for the
    /// real `FrameLayout` once the canvas + multi-pane interaction land).
    frame_label: String,
}

impl AppState {
    fn new() -> Self {
        let mut graph = Graph::new();
        for i in 0..6 {
            graph.add_node(format!("seed://node/{i}"), PortablePoint::new(0.0, 0.0));
        }
        Self {
            graph,
            frame_label: "Mere".to_string(),
        }
    }

    fn node_count(&self) -> usize {
        self.graph.nodes().count()
    }
}

/// The whole app view: a frametree of splits. Left = workbench; right
/// column = orrery (top) over apparatus (bottom). This *is* the
/// frametree — splits are split views, panes are view functions.
fn app_logic(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    split(
        workbench_pane(state),
        split(orrery_pane(state), apparatus_pane(state)).split_axis(Axis::Vertical),
    )
}

/// Workbench pane — will host the tile strip + engine content; static
/// placeholder for now.
fn workbench_pane(_state: &AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        label("Workbench").text_size(18.0),
        prose("tile strip — engine content (nematic / serval / scrying) lands in a later phase"),
    ))
}

/// Orrery pane — placeholder for the custom `GraphCanvas` Masonry
/// widget (the spatial graph view). The "+ node" button proves the
/// reactive loop until the canvas widget paints the graph.
fn orrery_pane(state: &AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        label("Orrery").text_size(18.0),
        prose(format!("graph: {} nodes (GraphCanvas widget pending)", state.node_count())),
        button(label("+ node"), |state: &mut AppState| {
            let n = state.node_count();
            state
                .graph
                .add_node(format!("seed://node/{n}"), PortablePoint::new(0.0, 0.0));
        }),
    ))
}

/// Apparatus pane — diagnostics / inspector. Static placeholder reading
/// app state.
fn apparatus_pane(state: &AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        label("Apparatus").text_size(18.0),
        prose(format!("frame: {}", state.frame_label)),
        prose(format!("graph nodes: {}", state.node_count())),
    ))
}

fn main() {
    Xilem::new_simple(AppState::new(), app_logic, WindowOptions::new("Mere"))
        .run_in(EventLoop::with_user_event())
        .expect("run mere");
}
