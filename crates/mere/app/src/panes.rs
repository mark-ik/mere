// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! The app's view tree — toolbar + panes over one [`AppState`]. Each pane is a
//! plain view function; this is the whole chrome layer (Woodshed's shape). The
//! orrery's spatial widget lives in [`crate::graph_canvas`]; engine rendering
//! in [`crate::engine_tile`].

use std::collections::HashMap;

use forme::ArrangementNodeKind;
use kernel::geometry::{PortablePoint, PortableRect, PortableSize};
use platen::{LaidOutSlot, LayoutConfig, TilePlan, layout_plan, project_tree};
use xilem::masonry::kurbo::Vec2;
use xilem::masonry::layout::{Length, UnitPoint};
use xilem::masonry::properties::Dimensions;
use xilem::view::{
    CrossAxisAlignment, FlexExt as _, button, flex_col, flex_row, label, portal, prose,
    resize_observer, sized_box, text_input, transformed, zstack,
};
use xilem::{AnyWidgetView, WidgetView};

use crate::engine_tile::{self, RenderedTile};
use crate::graph_canvas::{GraphAction, graph_canvas};
use crate::surface_tile::surface_tile;
use crate::{AppState, MainView, ring_world};

/// The whole app view: a toolbar over a single main pane. The app opens on the
/// orrery (the graph is home); navigating shows a document; the toolbar switches
/// the pane to the workbench / apparatus. Side-by-side splitting is a separate
/// user gesture (a later slice), not the default layout.
pub fn app_logic(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((toolbar(state), content_pane(state).flex(1.0)))
}

/// The single main pane, dispatched on the current [`MainView`].
fn content_pane(state: &AppState) -> Box<AnyWidgetView<AppState>> {
    match state.main_view {
        MainView::Orrery => orrery_pane(state).boxed(),
        MainView::Document => document_pane(state).boxed(),
        MainView::Workbench => workbench_pane(state).boxed(),
        MainView::Apparatus => apparatus_pane(state).boxed(),
        MainView::Surface => surface_pane(state).boxed(),
    }
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
        // View switches: the single pane's content kind (Document appears by
        // navigating). Side-by-side splitting is a later gesture.
        button(label("graph"), |state: &mut AppState| {
            state.main_view = MainView::Orrery;
        }),
        button(label("bench"), |state: &mut AppState| {
            state.main_view = MainView::Workbench;
        }),
        button(label("info"), |state: &mut AppState| {
            state.main_view = MainView::Apparatus;
        }),
        button(label("surf"), |state: &mut AppState| {
            state.main_view = MainView::Surface;
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
    let viewport = PortableSize::new(
        state.content_size.width as f32,
        state.content_size.height as f32,
    );
    let laid = layout_plan(&plan, viewport, &LayoutConfig::default());

    // Place each slot at its platen-computed rect. Masonry is not re-deriving
    // the between-tiles layout here — platen owns it (see the between-tiles
    // layout seam); each tile is an absolutely-positioned fixed-size box, and
    // Masonry lays out only the content *within* it.
    let mut placed: Vec<Box<AnyWidgetView<AppState>>> = Vec::new();
    for slot in &laid.slots {
        match slot {
            LaidOutSlot::Tile { tile, rect } => {
                placed.push(place(rect, tile_view(tile, &state.tile_docs)));
            }
            LaidOutSlot::Tabs { tabs, strip, content, .. } => {
                placed.push(place(strip, tabs_strip(tabs)));
                if let Some(active) = tabs.first() {
                    placed.push(place(content, tile_view(active, &state.tile_docs)));
                }
            }
        }
    }

    flex_col((
        // Chrome strip (natural height): header + the "+ tile" affordance.
        flex_row((
            label(format!(
                "Workbench \"{}\" · {} slot(s), {} tile(s) · platen-placed",
                state.workbench.label.as_deref().unwrap_or("workbench"),
                laid.slots.len(),
                plan.tile_count()
            )),
            button(label("+ tile"), |state: &mut AppState| {
                let arr = &mut state.workbench.arrangement;
                let root = arr.root();
                let n = arr.len();
                let id = arr.insert(
                    ArrangementNodeKind::TileIntent { member: None },
                    Some(format!("tile {n}")),
                );
                arr.attach(id, root);
                state.persist_workbench();
            }),
        )),
        // Tiling region: the platen-placed tiles, stretched to fill. Its
        // measured size round-trips into `content_size` for the next layout
        // pass (one-frame lag on resize, then stable).
        resize_observer(
            |state: &mut AppState, size| {
                state.content_size = size;
            },
            zstack(placed)
                .alignment(UnitPoint::TOP_LEFT)
                .prop(Dimensions::STRETCH),
        )
        .flex(1.0),
    ))
}

/// Document pane — the currently-navigated document (`state.current`), rendered
/// full-pane. This is what the main pane shows after navigating (omnibar or an
/// orrery node click).
fn document_pane(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let tile = &state.current;
    let title = tile
        .document
        .title
        .as_deref()
        .unwrap_or(tile.document.address.as_str());
    let mut children: Vec<Box<AnyWidgetView<AppState>>> = vec![
        label(title.to_string()).text_size(18.0).boxed(),
        prose(format!("{} · engine: {}", tile.document.address, tile.engine_id)).boxed(),
    ];
    children.extend(engine_tile::document_views(&tile.document));
    portal(flex_col(children).cross_axis_alignment(CrossAxisAlignment::Stretch))
        .constrain_horizontal(true)
}

/// Place a view at a platen-computed rect: a fixed-size box translated to the
/// rect's origin, inside the workbench's top-left-anchored `zstack`. This is
/// the route-1 absolute-placement seam — the host positions tiles at platen's
/// geometry rather than letting a flex container re-derive it.
fn place(rect: &PortableRect, view: Box<AnyWidgetView<AppState>>) -> Box<AnyWidgetView<AppState>> {
    transformed(
        sized_box(view)
            .fixed_width(Length::px(rect.size.width.max(0.0) as f64))
            .fixed_height(Length::px(rect.size.height.max(0.0) as f64)),
    )
    .translate(Vec2::new(rect.origin.x as f64, rect.origin.y as f64))
    .boxed()
}

/// The tab strip for a tab-stack slot: the active tab marked, the rest listed.
fn tabs_strip(tabs: &[TilePlan]) -> Box<AnyWidgetView<AppState>> {
    let labels: Vec<Box<AnyWidgetView<AppState>>> = tabs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mark = if i == 0 { "▣" } else { "▢" };
            prose(format!("{mark} {}", tile_label(t))).boxed()
        })
        .collect();
    flex_row(labels).boxed()
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
            state.rebuild_scene();
            state.persist_graph();
        }),
        graph_canvas(
            state.scene.clone(),
            state.selected_node,
            |state: &mut AppState, action: GraphAction| match action {
                GraphAction::NodeMoved { id, world } => {
                    if let Some(key) = state.graph.get_node_key_by_id(id) {
                        // Per-move: update committed position + cached scene in
                        // memory (persist happens on NodeDropped, not per tick).
                        state.graph.set_node_position(
                            key,
                            PortablePoint::new(world.x as f32, world.y as f32),
                        );
                        state.rebuild_scene();
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

/// Surface pane — a single external-surface tile (verso P3 stub). The
/// [`surface_tile`] widget reserves an external GPU layer for its rect; the
/// app's `with_external_compositor` hook fills it (stub: a solid color). Swap
/// the registered color for a `scrying` WebView texture to close the loop.
fn surface_pane(state: &AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        label("External Surface").text_size(18.0),
        prose(
            "A SurfaceTile reserves an external GPU layer; the compositor hook \
             fills it on the shared device. The blue region below is composited \
             outside Masonry's paint — proof of the external-surface pipe.",
        ),
        surface_tile(state.surface_registry.clone(), [60, 90, 160, 255]).flex(1.0),
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
