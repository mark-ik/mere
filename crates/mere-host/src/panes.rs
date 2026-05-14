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

use std::collections::HashMap;

use gpui::{
    AnyElement, App, Bounds, Context, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point,
    Size, Window, div, prelude::*, px, relative, rgb,
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
    // Pane wrapper accepts `PaneDragPayload` drops — the source
    // pane gets reparented as a right-sibling of THIS pane. v0:
    // hardcoded `InsertSide::Right`; quadrant-based side inference
    // (top/bottom/left/right zones) is a follow-up.
    let target_pane = pane_id;
    let on_pane_drop = cx.listener(
        move |this,
              payload: &mere_host_runtime::PaneDragPayload,
              _window: &mut Window,
              cx: &mut Context<HostRoot>| {
            if payload.pane_id == target_pane {
                return; // self-drop: no-op
            }
            crate::host_action_bus::dispatch(
                this,
                mere_host_runtime::BusAction::pane(
                    target_pane,
                    mere_host_runtime::ActionKind::ReparentPane {
                        source: payload.pane_id,
                        side: mere_frame::InsertSide::Right,
                    },
                ),
                cx,
            );
        },
    );
    let wrapper_id: gpui::ElementId =
        gpui::SharedString::from(format!("pane-wrapper-{}", pane_id.0)).into();
    div()
        .id(wrapper_id)
        .flex()
        .flex_col()
        .size_full()
        .overflow_hidden()
        .border_1()
        .border_color(rgb(0x2a2a2a))
        .on_drop::<mere_host_runtime::PaneDragPayload>(on_pane_drop)
        .child(render_pane_header(pane_id, content, cx))
        .child(body_container)
        .into_any_element()
}

/// Tag header for a pane: the content's tag on the left, a `×`
/// close affordance on the right. Clicking × routes to
/// `HostRoot::close_pane` — for orreries that cascades to every
/// panel bound to the same `graph_id`.
fn render_pane_header(
    pane_id: PaneId,
    content: &PaneContent,
    cx: &Context<HostRoot>,
) -> AnyElement {
    // Bus-routed close-pane: target = Pane(pane_id); execute path
    // calls HostRoot::close_pane.
    let close = cx.listener(
        move |this, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<HostRoot>| {
            crate::host_action_bus::dispatch(
                this,
                mere_host_runtime::BusAction::pane(
                    pane_id,
                    mere_host_runtime::ActionKind::ClosePane,
                ),
                cx,
            );
        },
    );
    // Right-click on the pane header opens the pane context menu.
    // Per the pane-UX brief §4.3: close pane + tear-out modes +
    // reparent (reserved until drag-rearrange lands).
    let content_is_workbench = matches!(content, PaneContent::Workbench);
    let on_right_click = cx.listener(
        move |this, ev: &gpui::MouseDownEvent, _window: &mut Window, cx: &mut Context<HostRoot>| {
            use crate::context_menu::ContextMenuEntry;
            use mere_host_runtime::{ActionKind, BusAction, TearOutMode};
            let mut entries = vec![
                ContextMenuEntry::new(
                    "Close pane",
                    BusAction::pane(pane_id, ActionKind::ClosePane),
                )
                .with_separator(),
            ];
            // Tear-out only makes sense for workbenches (the source
            // of tile state). Other pane kinds get a simpler menu.
            if content_is_workbench {
                entries.push(
                    ContextMenuEntry::new(
                        "Tear out as leaf",
                        BusAction::pane(
                            pane_id,
                            ActionKind::TearOutTile {
                                mode: TearOutMode::Leaf,
                                tile_index: None,
                            },
                        ),
                    ),
                );
                entries.push(
                    ContextMenuEntry::new(
                        "Tear out as branch",
                        BusAction::pane(
                            pane_id,
                            ActionKind::TearOutTile {
                                mode: TearOutMode::Branch,
                                tile_index: None,
                            },
                        ),
                    ),
                );
                entries.push(
                    ContextMenuEntry::new(
                        "Tear out as fork",
                        BusAction::pane(
                            pane_id,
                            ActionKind::TearOutTile {
                                mode: TearOutMode::Fork,
                                tile_index: None,
                            },
                        ),
                    )
                    .with_separator(),
                );
            }
            // Drag-to-reparent is wired on the pane header itself
            // (see `render_pane_header`'s `on_drag`). The context
            // menu surfaces the affordance as a hint — disabled
            // because clicking does nothing; the user drags the
            // header to invoke it.
            entries.push(
                ContextMenuEntry::new(
                    "Move pane \u{2014} drag the header",
                    BusAction::app(ActionKind::Quit),
                )
                .disabled("drag the pane header to relocate"),
            );
            this.open_context_menu(ev.position, entries, cx);
        },
    );
    // Make the header a drag source: dragging it carries a
    // `PaneDragPayload { pane_id }`; the drop handler on any pane
    // wrapper resolves the target and fires `ReparentPane`.
    let drag_label = content.tag().to_string();
    let header_id: gpui::ElementId =
        gpui::SharedString::from(format!("pane-header-{}", pane_id.0)).into();
    div()
        .id(header_id)
        .flex()
        .flex_row()
        .items_center()
        .px_3()
        .py_1()
        .text_xs()
        .text_color(rgb(0x808080))
        .bg(rgb(0x141414))
        .border_b_1()
        .border_color(rgb(0x2a2a2a))
        .cursor(gpui::CursorStyle::OpenHand)
        .on_drag(
            mere_host_runtime::PaneDragPayload { pane_id },
            move |_payload, _offset, _window, cx| {
                cx.new(|_| crate::tearout::DraggedPaneLabel {
                    text: drag_label.clone(),
                })
            },
        )
        .on_mouse_down(MouseButton::Right, on_right_click)
        .child(div().flex_1().child(content.tag().to_string()))
        .child(
            div()
                .id(gpui::SharedString::from(format!("pane-close-{}", pane_id.0)))
                .w(px(16.0))
                .h(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(rgb(0x707070))
                .rounded_sm()
                .hover(|s| s.bg(rgb(0x3a1a1a)).text_color(rgb(0xff8080)))
                .on_mouse_up(MouseButton::Left, close)
                .child("×"),
        )
        .into_any_element()
}

/// Render a **pinned tile** leaf — a single tile's document
/// surface without the workbench strip. Per the pane-UX brief §3
/// frametree side-by-side rendering: lets users keep a reference
/// tile visible while navigating in a sibling workbench.
///
/// Document lookup v0: walks every workbench pane in this window;
/// uses the first one whose TileState has the node cached. When
/// no workbench has it (closed all relevant tiles), renders a
/// placeholder. A later slice could re-fetch via `inker::loader`
/// to make pinned tiles self-sufficient.
fn render_pinned_tile_pane(
    leaf_node_ref: mere_frame::LeafNodeRef,
    graph_id: GraphId,
    pctx: &PaneRenderCtx,
) -> AnyElement {
    let node = mere_kernel::graph::NodeKey::new(leaf_node_ref.0 as usize);
    // Walk every workbench in the window with matching graph_id;
    // first hit's TileState.documents wins.
    let mut found_doc = None;
    for (other_pid, other_content, other_gid) in pctx.frame_layout.iter_leaves() {
        if !matches!(other_content, PaneContent::Workbench) || other_gid != graph_id {
            continue;
        }
        if let Some(workbench) = pctx.panes.get(&other_pid).and_then(|s| s.as_workbench()) {
            if let Some(doc) = workbench.tiles.document_for(node) {
                found_doc = Some(doc);
                break;
            }
        }
    }
    match found_doc {
        Some(doc) => {
            // Render just the document body — no strip. Reuse the
            // gloss renderer which already takes &EngineDocument
            // and produces a clean body view.
            mere_host_gloss::render(doc)
        }
        None => div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .size_full()
            .text_color(rgb(0x707070))
            .text_sm()
            .child("Pinned tile not loaded")
            .child(
                div()
                    .text_xs()
                    .mt_2()
                    .child("open the tile in a workbench to populate"),
            )
            .into_any_element(),
    }
}

/// Context menu builder for a tile-strip row. Per the
/// [pane-UX brief](../../../design_docs/mere_docs/design/2026-05-11_pane_ux_design_pass_brief.md)
/// §4.3 tile-strip-tile entries: Focus / Close / Tear-out
/// Leaf/Branch/Fork / Pin to frame (reserved).
fn open_tile_row_context_menu(
    this: &mut HostRoot,
    pane_id: PaneId,
    tile_index: usize,
    position: gpui::Point<gpui::Pixels>,
    cx: &mut Context<HostRoot>,
) {
    use crate::context_menu::ContextMenuEntry;
    use mere_host_runtime::{ActionKind, BusAction, TearOutMode};

    // Resolve (node, graph_id) for the "Pin to frame" entry.
    let tile_anchor = this
        .frame_layout
        .iter_leaves()
        .find(|(p, _, _)| *p == pane_id)
        .map(|(_, _, gid)| gid)
        .and_then(|gid| {
            this.panes
                .get(&pane_id)
                .and_then(|s| s.as_workbench())
                .and_then(|w| w.tiles.open_tiles().get(tile_index).copied())
                .map(|node| (node, gid))
        });

    let mut entries = vec![
        ContextMenuEntry::new(
            "Focus tile",
            BusAction::pane(pane_id, ActionKind::FocusTile { index: tile_index }),
        ),
        ContextMenuEntry::new(
            "Close tile",
            BusAction::pane(pane_id, ActionKind::CloseTile { index: tile_index }),
        )
        .with_separator(),
        ContextMenuEntry::new(
            "Tear out as leaf",
            BusAction::pane(
                pane_id,
                ActionKind::TearOutTile {
                    mode: TearOutMode::Leaf,
                    tile_index: Some(tile_index),
                },
            ),
        ),
        ContextMenuEntry::new(
            "Tear out as branch",
            BusAction::pane(
                pane_id,
                ActionKind::TearOutTile {
                    mode: TearOutMode::Branch,
                    tile_index: Some(tile_index),
                },
            ),
        ),
        ContextMenuEntry::new(
            "Tear out as fork",
            BusAction::pane(
                pane_id,
                ActionKind::TearOutTile {
                    mode: TearOutMode::Fork,
                    tile_index: Some(tile_index),
                },
            ),
        )
        .with_separator(),
    ];
    if let Some((node, graph_id)) = tile_anchor {
        entries.push(ContextMenuEntry::new(
            "Pin to frame",
            BusAction::pane(
                pane_id,
                ActionKind::PinTileToFrame {
                    node: mere_frame::LeafNodeRef(node.index() as u32),
                    graph_id,
                },
            ),
        ));
    } else {
        entries.push(
            ContextMenuEntry::new("Pin to frame", BusAction::app(ActionKind::Quit))
                .disabled("could not resolve tile node"),
        );
    }
    this.open_context_menu(position, entries, cx);
}
