/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Orrery pane input wiring.
//!
//! The orrery is a canvas-shaped pane: pointer events route through
//! `graph_canvas::engine::InteractionEngine`, which emits
//! `CanvasAction`s that this module turns into HostRoot state changes
//! (camera pan / zoom, hover updates, tile-opening on click).

use std::cell::Cell;
use std::rc::Rc;

use euclid::default::Point2D;
use gpui::{
    AnyElement, Bounds, Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, ScrollDelta, ScrollWheelEvent, Window, div, prelude::*,
};
use graph_canvas::camera::CanvasViewport;
use graph_canvas::input::{CanvasInputEvent, Modifiers as CanvasModifiers, PointerButton};
use graph_canvas::interaction::CanvasAction;
use mere_kernel::graph::NodeKey;

use mere_frame::{GraphId, PaneId};

use crate::host_helpers::error_document;
use crate::loader;
use crate::panes::PaneRenderCtx;
use crate::HostRoot;

fn orrery_state(
    this: &HostRoot,
    pane_id: PaneId,
) -> Option<&crate::pane_state::OrreryPaneState> {
    this.panes.get(&pane_id)?.as_orrery()
}

fn orrery_state_mut(
    this: &mut HostRoot,
    pane_id: PaneId,
) -> Option<&mut crate::pane_state::OrreryPaneState> {
    this.panes.get_mut(&pane_id)?.as_orrery_mut()
}

/// Resolve a pane's UUID-keyed hide set into the `NodeKey`-keyed set
/// the orrery renderer + scene derivation consume. Stale entries
/// (a node was removed) are silently dropped.
fn resolve_hidden_relations(
    graph: &mere_kernel::graph::Graph,
    hidden: &std::collections::HashSet<crate::pane_state::HiddenRelationKey>,
) -> mere_host_orrery::HiddenRelations {
    hidden
        .iter()
        .filter_map(|(from_id, to_id, kind)| {
            let from = graph.get_node_key_by_id(*from_id)?;
            let to = graph.get_node_key_by_id(*to_id)?;
            Some((from, to, *kind))
        })
        .collect()
}

/// Convert a gpui mouse-button into the graph-canvas vocabulary.
fn pointer_button(b: MouseButton) -> Option<PointerButton> {
    match b {
        MouseButton::Left => Some(PointerButton::Primary),
        MouseButton::Right => Some(PointerButton::Secondary),
        MouseButton::Middle => Some(PointerButton::Middle),
        _ => None,
    }
}

fn canvas_modifiers(m: gpui::Modifiers) -> CanvasModifiers {
    CanvasModifiers {
        shift: m.shift,
        ctrl: m.control || m.platform,
        alt: m.alt,
    }
}

/// Apply `CanvasAction`s emitted by the engine to HostRoot state.
/// Mutations are per-pane: each orrery leaf's camera + interaction
/// state lives in `host.panes.get(&pane_id)`. Hover state lives
/// inside that pane's engine (read at render time).
pub(crate) fn apply_orrery_actions(
    this: &mut HostRoot,
    pane_id: PaneId,
    graph_id: GraphId,
    actions: Vec<CanvasAction<NodeKey>>,
    cx: &mut Context<HostRoot>,
) {
    if actions.is_empty() {
        return;
    }
    for action in actions {
        match action {
            CanvasAction::PanCamera(delta) => {
                if let Some(o) = orrery_state_mut(this, pane_id) {
                    o.camera.pan += delta;
                }
            }
            CanvasAction::ZoomCamera { factor, focus: _ } => {
                let bounds_now = orrery_state(this, pane_id)
                    .map(|o| o.bounds_sink.get())
                    .unwrap_or(None);
                let (min_zoom, max_zoom) = orrery_zoom_bounds(bounds_now);
                if let Some(o) = orrery_state_mut(this, pane_id) {
                    o.zoom_target = (o.zoom_target * factor).clamp(min_zoom, max_zoom);
                }
            }
            CanvasAction::SetPanInertia(velocity) => {
                if let Some(o) = orrery_state_mut(this, pane_id) {
                    o.camera.pan_velocity = velocity;
                }
            }
            CanvasAction::HoverNode(_) | CanvasAction::HoverEdge(_) => {}
            CanvasAction::DragNode { node, delta } => {
                // Move the node within the orrery's graph (looked
                // up via registry by the orrery leaf's graph_id).
                // Both windows observing this graph entity see the
                // move on next render.
                if let Some(graph_entity) =
                    this.registry.read(cx).get(graph_id).cloned()
                {
                    graph_entity.update(cx, |g, gcx| {
                        if let Some(node_ref) = g.get_node(node) {
                            let p = node_ref.projected_position();
                            let next =
                                euclid::default::Point2D::new(p.x + delta.x, p.y + delta.y);
                            let _ = g.set_node_position(node, next);
                            gcx.notify();
                        }
                    });
                }
            }
            CanvasAction::SelectNode(node) => {
                // Clicking a node in the orrery opens (or focuses)
                // its tile in a workbench bound to the same
                // graph_id. v0 rule: pick the first workbench in
                // the frame whose graph_id matches; if none, the
                // click is ignored (Phase 1B Part 3 will summon a
                // workbench on demand).
                let target_workbench = this
                    .frame_layout
                    .iter_leaves()
                    .find(|(_, content, gid)| {
                        matches!(content, mere_frame::PaneContent::Workbench)
                            && *gid == graph_id
                    })
                    .map(|(pid, _, _)| pid);
                let Some(target_workbench) = target_workbench else {
                    tracing::debug!(
                        ?graph_id,
                        "orrery click: no workbench for this graph in window"
                    );
                    return;
                };
                let Some(graph_entity) =
                    this.registry.read(cx).get(graph_id).cloned()
                else {
                    return;
                };
                let address = graph_entity
                    .read(cx)
                    .get_node(node)
                    .map(|n| n.url().to_string());
                if let Some(address) = address {
                    // Look up the existing tile's document (if any)
                    // in the target workbench, else load.
                    let cached = this
                        .panes
                        .get(&target_workbench)
                        .and_then(|s| s.as_workbench())
                        .and_then(|w| w.tiles.document_for(node))
                        .cloned();
                    let document = cached.unwrap_or_else(|| {
                        match loader::load(
                            &this.engine_registry,
                            &this.engine_policy,
                            &address,
                        ) {
                            Ok(doc) => doc,
                            Err(e) => error_document(&address, &e),
                        }
                    });
                    if let Some(workbench) = this
                        .panes
                        .get_mut(&target_workbench)
                        .and_then(|s| s.as_workbench_mut())
                    {
                        workbench.tiles.open_or_focus(
                            node,
                            address.clone(),
                            document,
                        );
                    }
                    this.active_workbench = Some(target_workbench);
                    this.sync_omnibar_to_active_tile(cx);
                    this.rebuild_app_tree(cx);
                }
            }
            other => tracing::debug!(?other, "orrery canvas action (no host effect yet)"),
        }
    }
    cx.notify();
}

/// Zoom-target bounds for the orrery, derived from the current pane
/// size. **Upper bound** is "one node-diameter fits the pane's short
/// side" — zooming past that is staring at one node closeup with no
/// surrounding context, so it's the meaningful information ceiling.
/// **Lower bound** stays a fixed minimum for now; a future refinement
/// could derive it from the graph's bounding box (so you can't zoom
/// out so far that the graph becomes a single pixel).
///
/// Falls back to a (0.1, 10.0) safe range when bounds haven't been
/// captured yet (first frame before paint).
fn orrery_zoom_bounds(bounds: Option<Bounds<Pixels>>) -> (f32, f32) {
    /// World-space radius of the smallest currently-renderable graph
    /// element. Matches `platen::CanvasSceneOptions::default()`'s
    /// `default_node_radius`. When per-node radii vary, this should
    /// be the min across all visible nodes.
    const REFERENCE_RADIUS_WORLD: f32 = 18.0;
    /// Fraction of the pane's short side a node-diameter should occupy
    /// at maximum zoom. 1.0 = one node fills the pane edge-to-edge.
    /// Slightly under so a bit of breathing room remains.
    const MAX_ZOOM_FILL: f32 = 0.85;
    const MIN_ZOOM_FLOOR: f32 = 0.1;

    let Some(bounds) = bounds else {
        return (MIN_ZOOM_FLOOR, 10.0);
    };
    let min_dim = bounds.size.width.as_f32().min(bounds.size.height.as_f32());
    let max_zoom = (min_dim * MAX_ZOOM_FILL / (REFERENCE_RADIUS_WORLD * 2.0)).max(1.0);
    (MIN_ZOOM_FLOOR, max_zoom)
}

fn viewport_for(bounds: Bounds<Pixels>) -> CanvasViewport {
    CanvasViewport::new(
        Point2D::new(0.0, 0.0),
        euclid::default::Size2D::new(bounds.size.width.as_f32(), bounds.size.height.as_f32()),
        1.0,
    )
}

/// Read the orrery's last-painted bounds and convert a window-coords
/// cursor to pane-local. Returns `None` when no bounds have been
/// captured yet (first frame) or the cursor is outside the pane.
fn local_position(
    sink: &Rc<Cell<Option<Bounds<Pixels>>>>,
    cursor: Point<Pixels>,
) -> Option<(Point2D<f32>, Bounds<Pixels>)> {
    let bounds = sink.get()?;
    if !bounds.contains(&cursor) {
        return None;
    }
    Some((
        Point2D::new(
            cursor.x.as_f32() - bounds.origin.x.as_f32(),
            cursor.y.as_f32() - bounds.origin.y.as_f32(),
        ),
        bounds,
    ))
}

/// Drive `InteractionEngine::process_event` for the orrery leaf at
/// `pane_id` (bound to `graph_id`) and apply emitted CanvasActions
/// against THIS pane's per-pane state.
fn dispatch_to_engine(
    this: &mut HostRoot,
    pane_id: PaneId,
    graph_id: GraphId,
    cx: &mut Context<HostRoot>,
    bounds: Bounds<Pixels>,
    event: CanvasInputEvent,
) {
    let Some(camera) = orrery_state(this, pane_id).map(|o| o.camera.clone()) else {
        return;
    };
    let Some(graph_entity) = this.registry.read(cx).get(graph_id).cloned() else {
        return;
    };
    let hidden = orrery_state(this, pane_id)
        .map(|o| resolve_hidden_relations(graph_entity.read(cx), &o.hidden_relations))
        .unwrap_or_default();
    let scene = mere_host_orrery::derive_scene_for_bounds(
        graph_entity.read(cx),
        &camera,
        bounds,
        &hidden,
    );
    let viewport = viewport_for(bounds);
    let Some(orrery) = orrery_state_mut(this, pane_id) else {
        return;
    };
    let actions = orrery
        .engine
        .process_event(&event, &scene.hit_proxies, &camera, &viewport);
    apply_orrery_actions(this, pane_id, graph_id, actions, cx);
}

/// Build the orrery pane body for the leaf at `pane_id` (bound to
/// `graph_id`), with mouse handlers that capture pane_id so the
/// closures route to THIS pane's state — not the window's
/// "primary." Two orrery leaves in one window each get their own
/// pan/zoom + hover, even when they show different graphs.
pub(crate) fn render_orrery_with_interactivity(
    pane_id: PaneId,
    graph_id: GraphId,
    pctx: &PaneRenderCtx,
    cx: &Context<HostRoot>,
) -> AnyElement {
    let Some(this_orrery) = pctx.panes.get(&pane_id).and_then(|s| s.as_orrery()) else {
        return div()
            .text_color(gpui::rgb(0x707070))
            .child("no orrery state")
            .into_any_element();
    };
    let Some(graph) = pctx.graphs.get(&graph_id).copied() else {
        return div()
            .text_color(gpui::rgb(0x707070))
            .child("graph not registered")
            .into_any_element();
    };
    // Build the display state for THIS orrery from THIS pane's
    // engine state + the active workbench's node (if the active
    // workbench is for the same graph).
    let display = mere_host_orrery::OrreryDisplayState {
        hover: this_orrery.engine.state.hovered_node,
        active: crate::panes::active_node_for_graph(pctx, graph_id),
        titles: graph
            .nodes()
            .map(|(key, node)| (key, node.url().to_string()))
            .collect(),
    };

    let bounds_sink = this_orrery.bounds_sink.clone();
    let camera = this_orrery.camera.clone();
    let hidden = resolve_hidden_relations(graph, &this_orrery.hidden_relations);

    let inner =
        mere_host_orrery::render(graph, camera, display, bounds_sink.clone(), &hidden);

    let bounds_sink_move = bounds_sink.clone();
    let on_mouse_move = cx.listener(
        move |this, ev: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<HostRoot>| {
            let Some((position, bounds)) = local_position(&bounds_sink_move, ev.position)
            else {
                return;
            };
            dispatch_to_engine(
                this,
                pane_id,
                graph_id,
                cx,
                bounds,
                CanvasInputEvent::PointerMoved { position },
            );
        },
    );

    let bounds_sink_scroll = bounds_sink.clone();
    let on_scroll = cx.listener(
        move |this, ev: &ScrollWheelEvent, _window: &mut Window, cx: &mut Context<HostRoot>| {
            let hovered = orrery_state(this, pane_id).and_then(|o| o.engine.state.hovered_node);
            let Some((position, bounds)) = local_position(
                &bounds_sink_scroll,
                hovered.map(|_| ev.position).unwrap_or(ev.position),
            ) else {
                return;
            };
            let dy = match ev.delta {
                ScrollDelta::Pixels(p) => p.y.as_f32(),
                ScrollDelta::Lines(p) => p.y * 32.0,
            };
            dispatch_to_engine(
                this,
                pane_id,
                graph_id,
                cx,
                bounds,
                CanvasInputEvent::Scroll {
                    position,
                    delta: dy,
                    modifiers: canvas_modifiers(ev.modifiers),
                },
            );
        },
    );

    let bounds_sink_down_left = bounds_sink.clone();
    let on_left_down = cx.listener(
        move |this, ev: &MouseDownEvent, _window: &mut Window, cx: &mut Context<HostRoot>| {
            let Some((position, bounds)) = local_position(&bounds_sink_down_left, ev.position)
            else {
                return;
            };
            let Some(button) = pointer_button(ev.button) else {
                return;
            };
            // Double-click on a node: open as a *new tile*
            // (NewTile mode). Single-click falls through to the
            // engine which emits SelectNode → "open or focus in
            // active workbench" (existing semantics). Per the
            // pane-UX brief §4.1 click hierarchy: left=manipulate,
            // double=strong-intent.
            if button == PointerButton::Primary && ev.click_count >= 2 {
                if let Some(node) =
                    orrery_state(this, pane_id).and_then(|o| o.engine.state.hovered_node)
                {
                    tracing::debug!(?node, ?pane_id, "orrery node double-clicked");
                    open_node_as_new_tile(this, graph_id, node, cx);
                    return;
                }
            }
            if button == PointerButton::Primary {
                if let Some(node) =
                    orrery_state(this, pane_id).and_then(|o| o.engine.state.hovered_node)
                {
                    tracing::debug!(?node, ?pane_id, "orrery node clicked");
                }
            }
            dispatch_to_engine(
                this,
                pane_id,
                graph_id,
                cx,
                bounds,
                CanvasInputEvent::PointerPressed {
                    position,
                    button,
                    modifiers: canvas_modifiers(ev.modifiers),
                },
            );
        },
    );

    let bounds_sink_down_mid = bounds_sink.clone();
    let on_middle_down = cx.listener(
        move |this, ev: &MouseDownEvent, _window: &mut Window, cx: &mut Context<HostRoot>| {
            let Some((position, bounds)) = local_position(&bounds_sink_down_mid, ev.position)
            else {
                return;
            };
            dispatch_to_engine(
                this,
                pane_id,
                graph_id,
                cx,
                bounds,
                CanvasInputEvent::PointerPressed {
                    position,
                    button: PointerButton::Middle,
                    modifiers: canvas_modifiers(ev.modifiers),
                },
            );
        },
    );

    let bounds_sink_up_left = bounds_sink.clone();
    let on_left_up = cx.listener(
        move |this, ev: &MouseUpEvent, _window: &mut Window, cx: &mut Context<HostRoot>| {
            let Some((position, bounds)) = local_position(&bounds_sink_up_left, ev.position)
            else {
                return;
            };
            dispatch_to_engine(
                this,
                pane_id,
                graph_id,
                cx,
                bounds,
                CanvasInputEvent::PointerReleased {
                    position,
                    button: PointerButton::Primary,
                    modifiers: canvas_modifiers(ev.modifiers),
                },
            );
        },
    );

    let bounds_sink_up_mid = bounds_sink;
    let on_middle_up = cx.listener(
        move |this, ev: &MouseUpEvent, _window: &mut Window, cx: &mut Context<HostRoot>| {
            let Some((position, bounds)) = local_position(&bounds_sink_up_mid, ev.position)
            else {
                return;
            };
            dispatch_to_engine(
                this,
                pane_id,
                graph_id,
                cx,
                bounds,
                CanvasInputEvent::PointerReleased {
                    position,
                    button: PointerButton::Middle,
                    modifiers: canvas_modifiers(ev.modifiers),
                },
            );
        },
    );

    // Right-click → context menu for the hovered node, else the
    // hovered edge, else the orrery background.
    let on_right_down = cx.listener(
        move |this, ev: &MouseDownEvent, _window: &mut Window, cx: &mut Context<HostRoot>| {
            let hovered_node =
                orrery_state(this, pane_id).and_then(|o| o.engine.state.hovered_node);
            let hovered_edge = orrery_state(this, pane_id)
                .and_then(|o| o.engine.state.hovered_edge.clone());
            if let Some(node) = hovered_node {
                open_node_context_menu(this, pane_id, graph_id, node, ev.position, cx);
            } else if let Some(edge) = hovered_edge {
                open_edge_context_menu(
                    this,
                    pane_id,
                    graph_id,
                    edge.source,
                    edge.target,
                    edge.tag,
                    ev.position,
                    cx,
                );
            } else {
                // No-node right-click: orrery-background menu —
                // reserved for future entries (Fit to graph, Reset
                // camera, Open empty tile, etc.). v0: do nothing.
                tracing::debug!(?pane_id, "orrery background right-click (no menu yet)");
            }
        },
    );

    div()
        .id(gpui::SharedString::from(format!("orrery-interactive-{}", pane_id.0)))
        .size_full()
        .on_mouse_move(on_mouse_move)
        .on_scroll_wheel(on_scroll)
        .on_mouse_down(MouseButton::Middle, on_middle_down)
        .on_mouse_down(MouseButton::Left, on_left_down)
        .on_mouse_down(MouseButton::Right, on_right_down)
        .on_mouse_up(MouseButton::Middle, on_middle_up)
        .on_mouse_up(MouseButton::Left, on_left_up)
        .child(inner)
        .into_any_element()
}

/// Pop a context menu for a right-clicked orrery node. Entries
/// per the [pane-UX brief](../../../design_docs/mere_docs/design/2026-05-11_pane_ux_design_pass_brief.md)
/// §4.3 node menu: open in tile, open as branch/fork; future
/// entries (pin, rename, delete) appear disabled.
fn open_node_context_menu(
    this: &mut HostRoot,
    pane_id: PaneId,
    graph_id: GraphId,
    node: NodeKey,
    position: Point<Pixels>,
    cx: &mut Context<HostRoot>,
) {
    use crate::context_menu::ContextMenuEntry;
    use mere_host_runtime::{ActionKind, BusAction, TearOutMode};

    // Resolve the URL for the right-clicked node so NavigateTo can
    // carry it as its address. Without a URL the open-in-tile
    // entries don't have meaningful targets.
    let url = this
        .registry
        .read(cx)
        .get(graph_id)
        .and_then(|g| g.read(cx).get_node(node).map(|n| n.url().to_string()));
    let Some(url) = url else {
        tracing::warn!(?node, ?graph_id, "node context menu: no URL for node");
        return;
    };

    let mut entries = vec![
        ContextMenuEntry::new(
            "Open in tile",
            BusAction::app(ActionKind::NavigateTo {
                address: url.clone(),
                new_tile: false,
            }),
        ),
        ContextMenuEntry::new(
            "Open in new tile",
            BusAction::app(ActionKind::NavigateTo {
                address: url.clone(),
                new_tile: true,
            }),
        )
        .with_shortcut("dbl-click")
        .with_separator(),
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
    ];
    // Pin node → open as a `PaneContent::Tile` leaf in the frame.
    entries.push(
        ContextMenuEntry::new(
            "Pin to frame",
            BusAction::pane(
                pane_id,
                ActionKind::PinTileToFrame {
                    node: mere_frame::LeafNodeRef(node.index() as u32),
                    graph_id,
                },
            ),
        )
        .with_separator(),
    );
    // Rename stays reserved — needs an in-menu text affordance that
    // doesn't exist yet.
    entries.push(
        ContextMenuEntry::new("Rename node", BusAction::app(ActionKind::Quit))
            .disabled("not yet implemented"),
    );
    // Delete removes the node + all its incident edges from graph
    // truth. Per the relation-taxonomy plan §6.1, node deletion is
    // a deliberate authorship gesture (unlike per-relation hide).
    entries.push(ContextMenuEntry::new(
        "Delete node",
        BusAction::app(ActionKind::RemoveNode { graph_id, node }),
    ));

    this.open_context_menu(position, entries, cx);
}

/// Pop a context menu for a right-clicked orrery **edge**. When the
/// hit-test produced a `tag` ([`mere_kernel::graph::RelationKind`]
/// ordinal via [`mere_kernel::graph::RelationKind::tag`]), the menu
/// shows entries for *just that one relation* — multi-relation pairs
/// disambiguate via their per-lane hit proxy. When no tag was
/// produced (defensive fallback for legacy / untagged hit-tests), the
/// menu lists every relation on `(from, to)`.
///
/// Each relation gets a "Hide …" entry (view-local, always offered);
/// user-authored relations also get a "Delete …" entry that retracts
/// from graph truth. Per the 2026-05-11 relation-taxonomy plan §6.1
/// authorship table.
fn open_edge_context_menu(
    this: &mut HostRoot,
    pane_id: PaneId,
    graph_id: GraphId,
    from: NodeKey,
    to: NodeKey,
    hit_tag: Option<u32>,
    position: Point<Pixels>,
    cx: &mut Context<HostRoot>,
) {
    use crate::context_menu::ContextMenuEntry;
    use mere_host_runtime::{ActionKind, BusAction};
    use mere_kernel::graph::RelationKind;

    let relations: Vec<RelationKind> = match hit_tag.and_then(RelationKind::from_tag) {
        Some(kind) => vec![kind],
        None => {
            // Tag absent or unrecognised — defensive fallback to
            // listing every relation on the pair.
            let Some(graph_entity) = this.registry.read(cx).get(graph_id).cloned() else {
                return;
            };
            graph_entity
                .read(cx)
                .relations()
                .filter(|view| view.from == from && view.to == to)
                .map(|view| view.kind)
                .collect()
        }
    };
    if relations.is_empty() {
        tracing::debug!(?from, ?to, "edge context menu: no relations on pair");
        return;
    }

    let mut entries = Vec::new();
    for kind in relations {
        let label = relation_kind_label(kind);
        entries.push(ContextMenuEntry::new(
            format!("Hide {label}"),
            BusAction::pane(
                pane_id,
                ActionKind::HideRelation {
                    graph_id,
                    from,
                    to,
                    kind,
                },
            ),
        ));
        if let Some(selector) = retractable_selector(kind) {
            entries.push(ContextMenuEntry::new(
                format!("Delete {label}"),
                BusAction::app(ActionKind::RetractRelation {
                    graph_id,
                    from,
                    to,
                    selector,
                }),
            ));
        }
    }
    this.open_context_menu(position, entries, cx);
}

/// Human-readable label for a [`RelationKind`] menu row.
fn relation_kind_label(kind: mere_kernel::graph::RelationKind) -> String {
    use mere_kernel::graph::RelationKind;
    match kind {
        RelationKind::Semantic(sub_kind) => format!("{sub_kind:?} link"),
        RelationKind::Traversal => "traversal".to_string(),
        RelationKind::Containment(sub_kind) => format!("{sub_kind:?} containment"),
        RelationKind::Arrangement(sub_kind) => format!("{sub_kind:?} arrangement"),
        RelationKind::Imported(sub_kind) => format!("{sub_kind:?} import"),
        RelationKind::Provenance(sub_kind) => format!("{sub_kind:?} provenance"),
    }
}

/// Map a [`RelationKind`] to the [`RelationSelector`] that retracts
/// it — but only when the relation is **user-authored** and therefore
/// retractable. Returns `None` for document / derived / imported /
/// provenance / traversal relations, which are hide-only. Per the
/// 2026-05-11 relation-taxonomy plan §6.1.
fn retractable_selector(
    kind: mere_kernel::graph::RelationKind,
) -> Option<mere_kernel::graph::RelationSelector> {
    use mere_kernel::graph::{ContainmentSubKind, RelationKind, RelationSelector, SemanticSubKind};
    match kind {
        // Document hyperlinks are document-authored — hide-only.
        RelationKind::Semantic(SemanticSubKind::Hyperlink) => None,
        // Every other semantic sub-kind is user- or agent-authored.
        RelationKind::Semantic(sub_kind) => Some(RelationSelector::Semantic(sub_kind)),
        // Stored containment (folders, notebooks, collections) is
        // user-authored; URL/domain containment is derived → hide-only.
        RelationKind::Containment(
            sub_kind @ (ContainmentSubKind::UserFolder
            | ContainmentSubKind::NotebookSection
            | ContainmentSubKind::CollectionMember),
        ) => Some(RelationSelector::Containment(sub_kind)),
        // Derived containment, arrangement, imported, provenance,
        // traversal — hide-only.
        _ => None,
    }
}

/// Double-click on an orrery node → open the node as a **new tile**.
///
/// Per the [pane-UX brief](../../../design_docs/mere_docs/design/2026-05-11_pane_ux_design_pass_brief.md)
/// §4.1: single-click stays "open or focus in active workbench"
/// (current SelectNode handler); double-click commits to a fresh
/// tile entry under `NavigateMode::NewTile`. The node already
/// exists in the graph, so `ensure_node_for_address_near` is a
/// find-or-create that returns the existing node — no duplicate
/// minting. The lineage facet records a Traversal edge from the
/// previously-active anchor.
fn open_node_as_new_tile(
    this: &mut HostRoot,
    graph_id: GraphId,
    node: NodeKey,
    cx: &mut Context<HostRoot>,
) {
    // Pick a workbench in the same graph (mirrors SelectNode's
    // target-finding); set it active so navigate_to lands here.
    let target_workbench = this
        .frame_layout
        .iter_leaves()
        .find(|(_, content, gid)| {
            matches!(content, mere_frame::PaneContent::Workbench) && *gid == graph_id
        })
        .map(|(pid, _, _)| pid);
    let Some(target_workbench) = target_workbench else {
        tracing::debug!(?graph_id, "double-click: no workbench for this graph");
        return;
    };
    let Some(graph_entity) = this.registry.read(cx).get(graph_id).cloned() else {
        return;
    };
    let Some(url) = graph_entity
        .read(cx)
        .get_node(node)
        .map(|n| n.url().to_string())
    else {
        return;
    };
    this.active_workbench = Some(target_workbench);
    this.navigate_to(url, mere_host_runtime::NavigateMode::NewTile, cx);
}
