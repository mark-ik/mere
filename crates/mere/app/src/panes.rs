// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! The app's view tree — toolbar + panes over one [`AppState`]. Each pane is a
//! plain view function; this is the whole chrome layer (Woodshed's shape). The
//! orrery's spatial widget lives in [`crate::graph_canvas`]; engine rendering
//! in [`crate::engine_tile`].

use std::collections::HashMap;

use forme::ArrangementNodeKind;
use kernel::geometry::PortablePoint;
use platen::{PlanSlot, TilePlan, project_tree};
use xilem::masonry::kurbo::Axis;
use xilem::view::{
    CrossAxisAlignment, FlexExt as _, button, flex_col, flex_row, label, portal, prose, split,
    text_input,
};
use xilem::{AnyWidgetView, WidgetView};

use crate::engine_tile::{self, RenderedTile};
use crate::graph_canvas::{GraphAction, graph_canvas, scene_from_graph};
use crate::{AppState, ring_world};

/// The whole app view: a toolbar over a frametree of splits. Left = workbench;
/// right column = orrery (top) over apparatus (bottom).
pub fn app_logic(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        toolbar(state),
        split(
            workbench_pane(state),
            split(orrery_pane(state), apparatus_pane(state)).split_axis(Axis::Vertical),
        )
        .flex(1.0),
    ))
}

/// The toolbar: back / forward, the omnibar (type an address + Enter), and Go.
/// Navigation routes through [`crate::navigation`] → `inker` and updates the
/// live tile.
fn toolbar(state: &AppState) -> impl WidgetView<AppState> + use<> {
    flex_row((
        button(label("◀"), |state: &mut AppState| state.back()),
        button(label("▶"), |state: &mut AppState| state.forward()),
        text_input(state.omnibar.clone(), |state: &mut AppState, text| {
            state.omnibar = text;
        })
        .on_enter(|state: &mut AppState, text| state.navigate(&text))
        .flex(1.0),
        button(label("Go"), |state: &mut AppState| {
            let address = state.omnibar.clone();
            state.navigate(&address);
        }),
    ))
}

/// Workbench pane — renders the forme arrangement through platen's tree
/// projection: slots laid side-by-side, tab-stacks shown grouped. The "+ tile"
/// button mutates the arrangement and the projection re-renders. Tiles bound to
/// a graph member render that member's engine content (compact); unbound tiles
/// show a label placeholder.
fn workbench_pane(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let plan = project_tree(&state.workbench.arrangement);
    let slots: Vec<Box<AnyWidgetView<AppState>>> = plan
        .slots
        .iter()
        .map(|s| slot_view(s, &state.tile_docs))
        .collect();
    portal(
        flex_col((
            label("Workbench").text_size(18.0),
            prose(format!(
                "forme \"{}\" → tree projection: {} slot(s), {} tile(s) · persisted",
                state.workbench.label.as_deref().unwrap_or("workbench"),
                plan.slots.len(),
                plan.tile_count()
            )),
            // Wrapped in a row so Stretch doesn't widen the button full-width.
            flex_row((button(label("+ tile"), |state: &mut AppState| {
                let arr = &mut state.workbench.arrangement;
                let root = arr.root();
                let n = arr.len();
                let id = arr.insert(
                    ArrangementNodeKind::TileIntent { member: None },
                    Some(format!("tile {n}")),
                );
                arr.attach(id, root);
                state.persist_workbench();
            }),)),
            flex_row(slots),
            live_tile(state),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch),
    )
    .constrain_horizontal(true)
}

/// The focus tile: the omnibar/orrery-navigated document (`state.current`),
/// shown below the workbench's bound tiles. Header names the engine + address.
fn live_tile(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let tile = &state.current;
    let mut children: Vec<Box<AnyWidgetView<AppState>>> = vec![
        label("▣ live tile").text_size(13.0).boxed(),
        prose(format!(
            "{} · engine: {} · {}",
            tile.document.address,
            tile.engine_id,
            tile.document.title.as_deref().unwrap_or("(untitled)"),
        ))
        .boxed(),
    ];
    children.extend(engine_tile::document_views(&tile.document));
    flex_col(children).cross_axis_alignment(CrossAxisAlignment::Stretch)
}

/// Render one workbench slot: a single tile, or a tab-stack (active tab's
/// content + a strip of the rest).
fn slot_view(
    slot: &PlanSlot,
    docs: &HashMap<uuid::Uuid, RenderedTile>,
) -> Box<AnyWidgetView<AppState>> {
    match slot {
        PlanSlot::Tile(t) => tile_view(t, docs),
        PlanSlot::Tabs(tabs) => {
            let mut children: Vec<Box<AnyWidgetView<AppState>>> =
                vec![label(format!("▭ tabs ({})", tabs.len())).text_size(13.0).boxed()];
            for t in tabs.iter().skip(1) {
                children.push(prose(format!("• {}", tile_label(t))).boxed());
            }
            if let Some(active) = tabs.first() {
                children.push(tile_view(active, docs));
            }
            flex_col(children)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .boxed()
        }
    }
}

/// A single workbench tile: its bound member's rendered content (compact), or a
/// label placeholder when unbound / unresolved.
fn tile_view(t: &TilePlan, docs: &HashMap<uuid::Uuid, RenderedTile>) -> Box<AnyWidgetView<AppState>> {
    match t.member.and_then(|m| docs.get(&m)) {
        Some(tile) => {
            let mut children: Vec<Box<AnyWidgetView<AppState>>> = vec![
                label(format!(
                    "▭ {}",
                    tile.document.title.as_deref().unwrap_or("(untitled)")
                ))
                .text_size(13.0)
                .boxed(),
                prose(tile.document.address.clone()).boxed(),
            ];
            children.extend(
                engine_tile::document_views(&tile.document)
                    .into_iter()
                    .take(3),
            );
            flex_col(children)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .boxed()
        }
        None => flex_col((label("▭ tile").text_size(13.0), prose(tile_label(t))))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .boxed(),
    }
}

/// Display name for a tile: its label, else its bound member, else unbound.
fn tile_label(t: &TilePlan) -> String {
    t.label.clone().unwrap_or_else(|| match t.member {
        Some(m) => format!("member {}", &m.to_string()[..8]),
        None => "(unbound)".to_string(),
    })
}

/// Orrery pane — the spatial graph view, a bespoke Masonry widget (see
/// [`crate::graph_canvas`]). Drag a node to move it (written back to graph
/// truth), drag empty space to pan, wheel to zoom. The "+ node" button mutates
/// graph truth and the view re-renders.
fn orrery_pane(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let relations = state.graph.relations().count();
    flex_col((
        label("Orrery").text_size(18.0),
        prose(format!("graph: {} nodes · {relations} relations", state.node_count())),
        button(label("+ node"), |state: &mut AppState| {
            let n = state.node_count();
            let key = state
                .graph
                .add_node(format!("mere://node/{n}"), PortablePoint::new(0.0, 0.0));
            // Place it on the ring (committed position, so it persists).
            state.graph.set_node_position(key, ring_world(n, 8));
            state.persist_graph();
        }),
        graph_canvas(
            scene_from_graph(&state.graph),
            state.selected_node,
            |state: &mut AppState, action: GraphAction| match action {
                GraphAction::NodeMoved { id, world } => {
                    if let Some(key) = state.graph.get_node_key_by_id(id) {
                        // Per-move: update committed position in memory only
                        // (persist happens on NodeDropped, not every tick).
                        state.graph.set_node_position(
                            key,
                            PortablePoint::new(world.x as f32, world.y as f32),
                        );
                    }
                }
                GraphAction::NodeDropped => state.persist_graph(),
                GraphAction::NodeActivated { id } => {
                    state.selected_node = Some(id);
                    let url = state
                        .graph
                        .get_node_by_id(id)
                        .map(|(_, node)| node.url().to_string());
                    if let Some(url) = url {
                        state.navigate(&url);
                    }
                }
            },
        )
        .flex(1.0),
    ))
}

/// Apparatus pane — diagnostics / inspector reading app state.
fn apparatus_pane(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let forme_id = state.workbench.id.as_uuid().to_string();
    portal(
        flex_col((
            label("Apparatus").text_size(18.0),
            prose(format!("frame: {}", state.frame_label)),
            prose(format!("graph nodes: {}", state.node_count())),
            prose(format!("session: {}", state.session_dir.display())),
            prose(format!("workbench forme: {}", &forme_id[..8])),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch),
    )
    .constrain_horizontal(true)
}
