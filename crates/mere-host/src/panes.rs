/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pane-tree recursion: walk a `FrameLayout`, paint each split's
//! children with a draggable splitter between them, and dispatch each
//! leaf to its per-domain renderer crate.
//!
//! The orrery's interactivity lives in [`crate::orrery_input`]; this
//! module just wires the rendered orrery element into the right leaf
//! slot.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, Context, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point,
    Size, Window, canvas, div, prelude::*, px, relative, rgb,
};
use mere_frame::{
    FrameLayout, GraphId, PaneContent, PaneId, PaneNode, SplitAxis, SplitChoice, SplitPath,
};
use mere_host_apparatus::CapturedEvent;
use mere_kernel::graph::{Graph, NodeKey};
use uxtree::UxTree;

use crate::host_helpers::tile_label;
use crate::layout_config::EdgePosition;
use crate::orrery_input::render_orrery_with_interactivity;
use crate::pane_state::PaneState;
use crate::HostRoot;

mod header;
mod pinned_tile;

use header::{open_tile_row_context_menu, render_pane_header};
use pinned_tile::render_pinned_tile_pane;

/// Pixel thickness of the draggable splitter between two split
/// children.
pub(crate) const SPLITTER_THICKNESS: f32 = 4.0;

/// State for an in-progress splitter drag. The path identifies which
/// split in the layout tree the user grabbed; the cursor + ratio
/// snapshots let pixel deltas map to ratio deltas at any nesting
/// depth.
#[derive(Clone, Debug)]
pub(crate) struct SplitterDrag {
    pub path: SplitPath,
    pub axis: SplitAxis,
    pub start_cursor: Point<Pixels>,
    pub start_ratio: f32,
}

/// Bundle of read-only host state passed through pane-rendering
/// recursion. Each leaf walks the tree, picks its data out by
/// `pane_id` / `graph_id`, and renders. Per-pane bits (camera,
/// bounds sink, tile list) come from `panes`; per-graph bits (the
/// live graph itself) come from `graphs`, populated once at the
/// top of `HostRoot::render` from the registry.
pub(crate) struct PaneRenderCtx<'a> {
    /// Which edge of the workbench pane the tile strip lives on.
    pub workbench_strip_position: EdgePosition,
    pub app_tree: &'a UxTree,
    pub events: &'a [CapturedEvent],
    pub frame_layout: &'a FrameLayout,
    /// Per-pane state (orrery cameras, workbench tile lists, …)
    /// keyed by leaf `PaneId`. Looked up by each leaf renderer.
    pub panes: &'a HashMap<PaneId, PaneState>,
    /// Live graph references indexed by `GraphId`. Built per render
    /// from `Entity<GraphRegistry>`; each leaf looks up its
    /// `graph_id` here to get the graph to render.
    pub graphs: HashMap<GraphId, &'a Graph>,
    /// Active workbench in this window (most-recently-focused).
    /// Used by the orrery to highlight which node has its tile
    /// open, when the active workbench shares the orrery's graph.
    pub active_workbench: Option<PaneId>,
}

/// Walk down the layout tree from the window root, multiplying
/// dimensions by each split's ratio along the path. Returns the
/// pixel-space size of the container that holds the split AT `path`
/// (so that pixel deltas translate to correct ratio deltas during a
/// drag at any nesting depth).
pub(crate) fn compute_container_size(
    layout: &FrameLayout,
    path: &[SplitChoice],
    window_size: Size<Pixels>,
) -> Size<Pixels> {
    let mut size = window_size;
    let mut node = &layout.root;
    for step in path {
        let PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } = node
        else {
            return size;
        };
        let (next_size, next_node) = match step {
            SplitChoice::First => (
                match axis {
                    SplitAxis::Horizontal => Size::new(size.width * *ratio, size.height),
                    SplitAxis::Vertical => Size::new(size.width, size.height * *ratio),
                },
                first.as_ref(),
            ),
            SplitChoice::Second => (
                match axis {
                    SplitAxis::Horizontal => Size::new(size.width * (1.0 - *ratio), size.height),
                    SplitAxis::Vertical => Size::new(size.width, size.height * (1.0 - *ratio)),
                },
                second.as_ref(),
            ),
        };
        size = next_size;
        node = next_node;
    }
    size
}

/// Walk the frame layout, dispatching each leaf to its renderer crate
/// and inserting a draggable splitter between each split's children.
///
/// `path` accumulates `SplitChoice` steps from the root so each split
/// has a stable identifier that the splitter handler uses to target
/// the right node in `FrameLayout::set_split_ratio`.
pub(crate) fn render_pane_node(
    node: &PaneNode,
    path: SplitPath,
    pctx: &PaneRenderCtx,
    cx: &Context<HostRoot>,
) -> AnyElement {
    match node {
        PaneNode::Leaf {
            pane_id,
            content,
            graph_id,
        } => render_leaf(*pane_id, content, *graph_id, pctx, cx),
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            let mut first_path = path.clone();
            first_path.push(SplitChoice::First);
            let mut second_path = path.clone();
            second_path.push(SplitChoice::Second);

            let first_child = render_pane_node(first, first_path, pctx, cx);
            let second_child = render_pane_node(second, second_path, pctx, cx);
            let splitter = render_splitter(*axis, path, cx);

            // Splitter is the only fixed-pixel element; the second
            // child takes the remainder via flex_1 so first's exact
            // ratio and the splitter's exact thickness both honor.
            match axis {
                SplitAxis::Horizontal => div()
                    .flex()
                    .flex_row()
                    .size_full()
                    .overflow_hidden()
                    .child(
                        div()
                            .w(relative(*ratio))
                            .h_full()
                            .overflow_hidden()
                            .child(first_child),
                    )
                    .child(splitter)
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .overflow_hidden()
                            .child(second_child),
                    )
                    .into_any_element(),
                SplitAxis::Vertical => div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .overflow_hidden()
                    .child(
                        div()
                            .h(relative(*ratio))
                            .w_full()
                            .overflow_hidden()
                            .child(first_child),
                    )
                    .child(splitter)
                    .child(
                        div()
                            .flex_1()
                            .w_full()
                            .overflow_hidden()
                            .child(second_child),
                    )
                    .into_any_element(),
            }
        }
    }
}

fn render_splitter(
    axis: SplitAxis,
    path: SplitPath,
    cx: &Context<HostRoot>,
) -> AnyElement {
    let id_label = path
        .iter()
        .map(|c| match c {
            SplitChoice::First => "F",
            SplitChoice::Second => "S",
        })
        .collect::<String>();
    let splitter_id: gpui::ElementId =
        gpui::SharedString::from(format!("splitter-{:?}-{id_label}", axis)).into();

    let captured_path = path.clone();
    let on_down = cx.listener(
        move |this, ev: &MouseDownEvent, _window: &mut Window, cx: &mut Context<HostRoot>| {
            let Some((axis, start_ratio)) = this.frame_layout.split_at(&captured_path) else {
                return;
            };
            this.dragging_splitter = Some(SplitterDrag {
                path: captured_path.clone(),
                axis,
                start_cursor: ev.position,
                start_ratio,
            });
            cx.notify();
        },
    );

    let base = div()
        .id(splitter_id)
        .bg(rgb(0x2a2a2a))
        .hover(|s| s.bg(rgb(0x4a90ff)))
        .on_mouse_down(MouseButton::Left, on_down);
    match axis {
        SplitAxis::Horizontal => base
            .w(px(SPLITTER_THICKNESS))
            .h_full()
            .cursor_col_resize()
            .into_any_element(),
        SplitAxis::Vertical => base
            .h(px(SPLITTER_THICKNESS))
            .w_full()
            .cursor_row_resize()
            .into_any_element(),
    }
}

/// Build the workbench leaf body for the leaf at `pane_id` (bound
/// to `graph_id`). Each tile entry's click handlers capture
/// `pane_id` so they target THIS workbench's tile list — multiple
/// workbenches in one window stay independent.
fn render_workbench_pane(
    pane_id: PaneId,
    graph_id: GraphId,
    pctx: &PaneRenderCtx,
    cx: &Context<HostRoot>,
) -> AnyElement {
    let Some(workbench_state) = pctx.panes.get(&pane_id).and_then(|s| s.as_workbench()) else {
        return placeholder("no workbench state");
    };
    let Some(graph) = pctx.graphs.get(&graph_id).copied() else {
        return placeholder("graph not registered");
    };
    let workbench_tiles = &workbench_state.tiles;
    let tiles: Vec<mere_host_workbench::WorkbenchTile> = workbench_tiles
        .open_tiles()
        .iter()
        .enumerate()
        .map(|(i, &node)| {
            let label = tile_label(node, graph, workbench_tiles.document_for(node));
            // Capture pane_id so the click routes to THIS
            // workbench, not whichever happens to be "active."
            // Bus-routed: target = Pane(pane_id), kind carries the
            // tile index.
            let on_select = Box::new(cx.listener(
                move |this, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<HostRoot>| {
                    this.active_workbench = Some(pane_id);
                    crate::host_action_bus::dispatch(
                        this,
                        mere_host_runtime::BusAction::pane(
                            pane_id,
                            mere_host_runtime::ActionKind::FocusTile { index: i },
                        ),
                        cx,
                    );
                },
            ))
                as Box<dyn Fn(&MouseUpEvent, &mut Window, &mut App)>;
            let on_close = Box::new(cx.listener(
                move |this, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<HostRoot>| {
                    crate::host_action_bus::dispatch(
                        this,
                        mere_host_runtime::BusAction::pane(
                            pane_id,
                            mere_host_runtime::ActionKind::CloseTile { index: i },
                        ),
                        cx,
                    );
                },
            ))
                as Box<dyn Fn(&MouseUpEvent, &mut Window, &mut App)>;
            // Tear-out drag: starting a drag on this tile-row
            // packages up `(pane_id, tile_index)` as a
            // `TileDragPayload`. The drag visual is a small chip
            // showing the tile's label. The host root's
            // `on_drop<TileDragPayload>` handler picks the
            // gesture's tear-out semantics (Phase 2 Part 2 v0:
            // sticky-note / leaf).
            let drag_label = label.clone();
            let drag_apply: mere_host_workbench::DragApply = Box::new(move |row| {
                row.on_drag(
                    crate::tearout::TileDragPayload {
                        pane_id,
                        tile_index: i,
                    },
                    move |_payload, _offset, _window, cx| {
                        cx.new(|_| crate::tearout::DraggedTileLabel {
                            text: drag_label.clone(),
                        })
                    },
                )
            });
            // Right-click → tile-strip-row context menu. Per the
            // pane-UX brief §4.3: Focus / Close / Tear out (Leaf/
            // Branch/Fork) / Pin to frame (reserved).
            let on_right_click: Box<
                dyn Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static,
            > = Box::new(cx.listener(
                move |this,
                      ev: &gpui::MouseDownEvent,
                      _window: &mut Window,
                      cx: &mut Context<HostRoot>| {
                    open_tile_row_context_menu(this, pane_id, i, ev.position, cx);
                },
            ));
            mere_host_workbench::WorkbenchTile {
                label,
                on_select,
                on_close,
                on_right_click: Some(on_right_click),
                drag_apply: Some(drag_apply),
            }
        })
        .collect();
    mere_host_workbench::render(
        tiles,
        workbench_tiles.active_index(),
        workbench_tiles.active_document(),
        strip_position_for_workbench(pctx.workbench_strip_position),
    )
}

fn placeholder(label: &'static str) -> AnyElement {
    gpui::div()
        .text_color(gpui::rgb(0x707070))
        .child(label)
        .into_any_element()
}

fn strip_position_for_workbench(edge: EdgePosition) -> mere_host_workbench::StripPosition {
    match edge {
        EdgePosition::Top => mere_host_workbench::StripPosition::Top,
        EdgePosition::Bottom => mere_host_workbench::StripPosition::Bottom,
        EdgePosition::Left => mere_host_workbench::StripPosition::Left,
        EdgePosition::Right => mere_host_workbench::StripPosition::Right,
    }
}

/// Resolve a graph's "active document" for the gloss pane that
/// shares its `graph_id`. v0 rule: if the active workbench is bound
/// to the same `graph_id`, show its active tile's doc; otherwise
/// show nothing.
pub(crate) fn active_doc_for_graph<'a>(
    pctx: &'a PaneRenderCtx<'a>,
    graph_id: GraphId,
) -> Option<&'a inker::EngineDocument> {
    let active_pane = pctx.active_workbench?;
    let active_graph_id = pctx
        .frame_layout
        .iter_leaves()
        .find(|(pid, _, _)| *pid == active_pane)
        .map(|(_, _, gid)| gid)?;
    if active_graph_id != graph_id {
        return None;
    }
    pctx.panes
        .get(&active_pane)?
        .as_workbench()?
        .tiles
        .active_document()
}

/// "Active tile node" — the node whose tile is currently selected
/// in the workbench bound to `graph_id` (if any). Used by orrery
/// rendering to highlight which node has an open tile.
pub(crate) fn active_node_for_graph(
    pctx: &PaneRenderCtx,
    graph_id: GraphId,
) -> Option<NodeKey> {
    let active_pane = pctx.active_workbench?;
    let active_graph_id = pctx
        .frame_layout
        .iter_leaves()
        .find(|(pid, _, _)| *pid == active_pane)
        .map(|(_, _, gid)| gid)?;
    if active_graph_id != graph_id {
        return None;
    }
    pctx.panes
        .get(&active_pane)?
        .as_workbench()?
        .tiles
        .active_node()
}

/// Wrap a leaf body in pane chrome (border + tag header + scrollable
/// container) and dispatch the body to the matching renderer crate.
fn render_leaf(
    pane_id: PaneId,
    content: &PaneContent,
    graph_id: GraphId,
    pctx: &PaneRenderCtx,
    cx: &Context<HostRoot>,
) -> AnyElement {
    let body = match content {
        PaneContent::Workbench => render_workbench_pane(pane_id, graph_id, pctx, cx),
        PaneContent::Orrery => {
            render_orrery_with_interactivity(pane_id, graph_id, pctx, cx)
        }
        PaneContent::Gloss => match active_doc_for_graph(pctx, graph_id) {
            Some(doc) => mere_host_gloss::render(doc),
            None => div()
                .text_color(rgb(0x707070))
                .child("no active tile")
                .into_any_element(),
        },
        PaneContent::Apparatus => mere_host_apparatus::render(pctx.app_tree, pctx.events),
        PaneContent::Tile(leaf_node_ref) => {
            render_pinned_tile_pane(*leaf_node_ref, graph_id, pctx)
        }
        _ => div()
            .text_color(rgb(0x707070))
            .child("(empty pane)")
            .into_any_element(),
    };
    // Canvas-shaped panes (orrery, eventually graph views) want a
    // single edge-to-edge surface beneath the tag header: no padding,
    // no scroll, dark canvas bg. The workbench owns its own scroll
    // + padding inside the body container so the strip can pin and
    // the body alone scrolls. Other document panes get the default
    // padded scroll container.
    let body_container: AnyElement = match content {
        PaneContent::Orrery => div()
            .flex_1()
            .w_full()
            .overflow_hidden()
            .bg(rgb(0x141414))
            .child(body)
            .into_any_element(),
        PaneContent::Workbench => div()
            .flex_1()
            .w_full()
            .overflow_hidden()
            .child(body)
            .into_any_element(),
        _ => {
            let scroll_id: gpui::ElementId =
                gpui::SharedString::from(format!("pane-scroll-{}", content.tag())).into();
            div()
                .id(scroll_id)
                .flex_1()
                .w_full()
                .overflow_x_hidden()
                .overflow_y_scroll()
                .p_3()
                .child(body)
                .into_any_element()
        }
    };
    // Pane wrapper accepts `PaneDragPayload` drops. The cursor's
    // position within the target pane infers which edge the source
    // attaches to: left / right / above / below. Drops landing in
    // the central zone default to Right (the v0 fallback) since the
    // gesture is ambiguous.
    let target_pane = pane_id;
    let wrapper_bounds: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::new(Cell::new(None));
    let bounds_for_drop = wrapper_bounds.clone();
    let on_pane_drop = cx.listener(
        move |this,
              payload: &mere_host_runtime::PaneDragPayload,
              window: &mut Window,
              cx: &mut Context<HostRoot>| {
            if payload.pane_id == target_pane {
                return; // self-drop: no-op
            }
            let side = bounds_for_drop
                .get()
                .map(|bounds| {
                    crate::drop_zone::infer_drop_side(
                        window.mouse_position_in_window(),
                        bounds,
                    )
                })
                .unwrap_or(mere_frame::InsertSide::Right);
            crate::host_action_bus::dispatch(
                this,
                mere_host_runtime::BusAction::pane(
                    target_pane,
                    mere_host_runtime::ActionKind::ReparentPane {
                        source: payload.pane_id,
                        side,
                    },
                ),
                cx,
            );
        },
    );
    // Invisible canvas paints over the wrapper to capture its
    // window-local bounds on every paint — the drop handler reads
    // these to convert the cursor position into a quadrant.
    let bounds_for_canvas = wrapper_bounds;
    let bounds_capture = canvas(
        |_, _, _| {},
        move |bounds, _, _, _| bounds_for_canvas.set(Some(bounds)),
    )
    .absolute()
    .size_full();
    let wrapper_id: gpui::ElementId =
        gpui::SharedString::from(format!("pane-wrapper-{}", pane_id.0)).into();
    div()
        .id(wrapper_id)
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .overflow_hidden()
        .border_1()
        .border_color(rgb(0x2a2a2a))
        .on_drop::<mere_host_runtime::PaneDragPayload>(on_pane_drop)
        .child(bounds_capture)
        .child(render_pane_header(pane_id, content, cx))
        .child(body_container)
        .into_any_element()
}


