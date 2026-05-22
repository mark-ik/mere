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

mod engine_tile;

use engine_tile::RenderedTile;
use forme::{Arrangement, ArrangementNodeKind};
use kernel::geometry::PortablePoint;
use kernel::graph::Graph;
use platen::{PlanSlot, TilePlan, project_tree};
use xilem::masonry::kurbo::Axis;
use xilem::view::{button, flex_col, flex_row, label, prose, split};
use xilem::{AnyWidgetView, EventLoop, WidgetView, WindowOptions, Xilem};

/// Seeded markdown for the live engine tile. Routed through `inker` and
/// rendered by the `nematic` markdown engine at startup — proof the engine
/// seam produces real content, not a placeholder. Edit here to change the
/// welcome page.
const WELCOME_MD: &str = "\
# Welcome to Mere

A **spatial browser** on the *composition spine*: graph truth → forme →
platen → verso → inker.

This tile is **live**. Its content was routed through `inker`'s engine
policy and parsed by the `nematic` markdown engine — nothing below the rule
is placeholder text.

## What you're looking at

- The **workbench** projects a forme arrangement into tiles
- The **orrery** is the spatial graph view (canvas widget pending)
- This document proves the engine-routing seam end to end

---

    route(address, content_type) -> Engine -> EngineDocument

Edit `WELCOME_MD` in `main.rs` to change this page.
";

/// The single application state the Xilem driver owns. Widgets mutate
/// it in place through their callbacks; the view tree rebuilds on diff.
/// Per-pane UI sub-state gets its own struct here as panes grow
/// (Woodshed's pattern); for the skeleton it's just the graph + a
/// frame label.
struct AppState {
    /// Graph truth — the orrery projects this. Foundational `kernel`
    /// crate, framework-free.
    graph: Graph,
    /// The Workbench pane's forme arrangement (curated tiles). Rendered
    /// via `platen::project_tree` → a tree-projected `WorkbenchPlan`.
    workbench: Arrangement,
    /// The live engine-backed tile: a document routed through `inker` and
    /// rendered by a `nematic` engine at startup. The v1 "one live tile."
    welcome: RenderedTile,
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

        // Seed a small workbench arrangement: one solo tile + two stacked
        // (tabs) — proving the projection produces a split slot + a tab slot.
        let mut workbench = Arrangement::new();
        let root = workbench.root();
        let solo = workbench.insert(
            ArrangementNodeKind::TileIntent { member: None },
            Some("a.example".into()),
        );
        workbench.attach(solo, root);
        let t2 = workbench.insert(
            ArrangementNodeKind::TileIntent { member: None },
            Some("b.example".into()),
        );
        workbench.attach(t2, root);
        let t3 = workbench.insert(
            ArrangementNodeKind::TileIntent { member: None },
            Some("c.example".into()),
        );
        workbench.attach(t3, root);
        workbench.stack(t2, t3);

        // Route + render the welcome document at startup through the engine
        // seam (inker policy → nematic markdown engine).
        let welcome = engine_tile::render_address("mere://welcome", WELCOME_MD, Some("text/markdown"));

        Self {
            graph,
            workbench,
            welcome,
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

/// Workbench pane — renders the forme arrangement through platen's tree
/// projection: slots laid side-by-side, tab-stacks shown grouped. The
/// "+ tile" button mutates the arrangement and the projection re-renders,
/// proving the forme → platen → view loop end-to-end. Tile *content*
/// (real engines) is a later slice; tiles show their label for now.
fn workbench_pane(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let plan = project_tree(&state.workbench);
    let slots: Vec<Box<AnyWidgetView<AppState>>> = plan.slots.iter().map(slot_view).collect();
    flex_col((
        label("Workbench").text_size(18.0),
        prose(format!(
            "forme → tree projection: {} slot(s), {} tile(s)",
            plan.slots.len(),
            plan.tile_count()
        )),
        button(label("+ tile"), |state: &mut AppState| {
            let root = state.workbench.root();
            let n = state.workbench.len();
            let id = state.workbench.insert(
                ArrangementNodeKind::TileIntent { member: None },
                Some(format!("tile {n}")),
            );
            state.workbench.attach(id, root);
        }),
        flex_row(slots),
        live_tile(state),
    ))
}

/// The live engine-backed tile: a header naming the engine that handled the
/// address, then the rendered document. This realizes the v1 "one live
/// engine-backed tile" — the projected slots above are still placeholders
/// (label-only), but *this* tile is real engine output.
fn live_tile(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let tile = &state.welcome;
    let mut children: Vec<Box<AnyWidgetView<AppState>>> = vec![
        label("▣ live tile").text_size(13.0).boxed(),
        prose(format!(
            "engine: {} · {}",
            tile.engine_id,
            tile.document.title.as_deref().unwrap_or("(untitled)"),
        ))
        .boxed(),
    ];
    children.extend(engine_tile::document_views(&tile.document));
    flex_col(children)
}

/// Render one workbench slot: a single tile, or a tab-stack.
fn slot_view(slot: &PlanSlot) -> Box<AnyWidgetView<AppState>> {
    match slot {
        PlanSlot::Tile(t) => {
            flex_col((label("▭ tile").text_size(13.0), prose(tile_label(t)))).boxed()
        }
        PlanSlot::Tabs(tabs) => {
            let rows: Vec<_> = tabs
                .iter()
                .map(|t| prose(format!("• {}", tile_label(t))))
                .collect();
            flex_col((label(format!("▭ tabs ({})", tabs.len())).text_size(13.0), rows)).boxed()
        }
    }
}

/// Display name for a tile: its label, else its bound member, else unbound.
fn tile_label(t: &TilePlan) -> String {
    t.label.clone().unwrap_or_else(|| match t.member {
        Some(m) => format!("member {}", &m.to_string()[..8]),
        None => "(unbound)".to_string(),
    })
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
