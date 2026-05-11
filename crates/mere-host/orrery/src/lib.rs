/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # mere-host-orrery
//!
//! gpui renderer for the orrery pane: pulls a `mere_kernel::graph::Graph`
//! through `platen::build_canvas_scene_input` →
//! `graph_canvas::derive_scene` and paints each `SceneDrawItem` via
//! gpui's Canvas + paint primitives.
//!
//! Camera and viewport are computed inside the canvas paint closure so
//! the projection adapts to the pane's actual bounds. Camera state
//! itself stays static for now (centered on origin, 1.0 zoom);
//! pan/zoom interactivity comes when host-side input events get
//! routed through `graph_canvas::engine::process_event`.
//!
//! Vello / netrender path: deferred until `PlatformSurface` lands on
//! Windows + Linux. Native gpui paint is sufficient for v0.

#![doc(html_root_url = "https://docs.rs/mere-host-orrery/0.0.1")]

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use euclid::default::{Point2D, Size2D};
use gpui::{
    AnyElement, App, Background, BorderStyle, Bounds, ContentMask, Hsla, PathBuilder, Pixels,
    Point, Rgba, TextRun, Window, canvas, div, point, prelude::*, px, quad, rgb, size,
    transparent_black,
};
use graph_canvas::camera::{CanvasCamera, CanvasViewport};
use graph_canvas::derive::{DeriveConfig, NodeVisualOverride, derive_scene};
use graph_canvas::packet::{Color, HitProxy, ProjectedScene, SceneDrawItem, Stroke};
use mere_kernel::graph::{Graph, GraphViewId, NodeKey};
use platen::{CanvasSceneOptions, build_canvas_scene_input};

/// Per-node display state the host computes and the orrery reads.
///
/// Three states a node can be in:
///
/// - **Idle** — not active, not hovered. Plain circle. (A real
///   favicon-glyph treatment lives in a follow-up slice; for v0 the
///   circle is the visual.)
/// - **Hovered** — mouse is over the node. Title label appears
///   anchored to the node so the user can identify it before
///   clicking. Per Mark: "on the node, not over the node."
/// - **Active** — the node's tile is the active workbench tile.
///   Brighter fill, thicker stroke, title shown always.
#[derive(Clone, Default, Debug)]
pub struct OrreryDisplayState {
    pub hover: Option<NodeKey>,
    pub active: Option<NodeKey>,
    /// Display titles, sparse — `None` for any node not present
    /// falls back to "the node's URL" (or a generic placeholder if
    /// even that's missing).
    pub titles: HashMap<NodeKey, String>,
}

/// Render the orrery pane body. Stateless: the host owns the
/// `CanvasCamera` and the [`OrreryDisplayState`] (hover + active +
/// titles) and passes them in. The host also wraps the returned
/// element in its own mouse-event handlers (pan / zoom / hit-test)
/// since this crate stays gpui-render-only.
///
/// `bounds_sink` receives the canvas's window-relative bounds on
/// every paint so the host's mouse handlers can convert cursor
/// positions to pane-local coordinates for hit-testing.
pub fn render(
    graph: &Graph,
    camera: CanvasCamera,
    state: OrreryDisplayState,
    bounds_sink: Rc<Cell<Option<Bounds<Pixels>>>>,
) -> AnyElement {
    let scene_input = build_canvas_scene_input(
        graph,
        CanvasSceneOptions::new(GraphViewId::from_uuid(uuid::Uuid::nil())),
    );
    let node_count = scene_input.nodes.len();
    let edge_count = scene_input.edges.len();

    // Single visual region — the pane chrome (border + tag header)
    // supplies the surrounding frame; the orrery body is the canvas
    // itself, edge-to-edge, with the debug HUD floated on top.
    div()
        .relative()
        .size_full()
        .child(
            div()
                .absolute()
                .left(px(8.0))
                .top(px(8.0))
                .text_xs()
                .text_color(rgb(0x707070))
                .child(format!(
                    "orrery — {node_count} node{} · {edge_count} edge{} (camera pan {:.0},{:.0} zoom {:.2})",
                    if node_count == 1 { "" } else { "s" },
                    if edge_count == 1 { "" } else { "s" },
                    camera.pan.x,
                    camera.pan.y,
                    camera.zoom,
                )),
        )
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, cx| {
                    bounds_sink.set(Some(bounds));
                    let scene = derive_for_bounds(&scene_input, &camera, bounds, state.active);
                    // Clip canvas paint to the orrery body's bounds so
                    // panned/zoomed nodes don't bleed into neighbouring
                    // panes. paint_quad and paint_path write straight
                    // to window coords; the content mask intersects
                    // them with this rectangle.
                    window.with_content_mask(
                        Some(ContentMask { bounds }),
                        |window| {
                            paint_scene(&scene, bounds.origin, window, &state);
                            // Title labels overlay the scene — drawn
                            // after the world layer so they stay
                            // legible on top of edges + circles.
                            paint_node_labels(&scene, bounds.origin, &state, window, cx);
                        },
                    );
                },
            )
            .size_full(),
        )
        .into_any_element()
}

/// Derive a `ProjectedScene` for given graph + camera + bounds. Exposed
/// so the host can do hit-testing on the same projected positions
/// without duplicating the projection.
pub fn derive_scene_for_bounds(
    graph: &Graph,
    camera: &CanvasCamera,
    bounds: Bounds<Pixels>,
) -> ProjectedScene<NodeKey> {
    let scene_input = build_canvas_scene_input(
        graph,
        CanvasSceneOptions::new(GraphViewId::from_uuid(uuid::Uuid::nil())),
    );
    derive_for_bounds(&scene_input, camera, bounds, None)
}

fn derive_for_bounds(
    scene_input: &graph_canvas::scene::CanvasSceneInput<NodeKey>,
    camera: &CanvasCamera,
    bounds: Bounds<Pixels>,
    active: Option<NodeKey>,
) -> ProjectedScene<NodeKey> {
    let width = bounds.size.width.as_f32();
    let height = bounds.size.height.as_f32();
    let viewport = CanvasViewport::new(Point2D::new(0.0, 0.0), Size2D::new(width, height), 1.0);

    let mut config = DeriveConfig::default();
    config.default_edge_width = 2.5;
    config.default_edge_color = Color::new(0.65, 0.7, 0.8, 0.85);

    // Active node — the one whose tile is currently in focus — gets
    // a brighter fill + thicker stroke so it's findable at a glance.
    // Others use the default idle treatment.
    let node_overrides = |_index: usize, key: &NodeKey| {
        if Some(*key) == active {
            NodeVisualOverride {
                fill: Some(Color::new(0.55, 0.8, 1.0, 1.0)),
                stroke: Some(Stroke {
                    color: Color::new(1.0, 1.0, 1.0, 1.0),
                    width: 3.0,
                }),
                label_color: None,
            }
        } else {
            NodeVisualOverride {
                fill: None,
                stroke: Some(Stroke {
                    color: Color::new(0.85, 0.9, 1.0, 0.9),
                    width: 1.5,
                }),
                label_color: None,
            }
        }
    };

    derive_scene(
        scene_input,
        camera,
        &viewport,
        &|_| 0.0,
        &node_overrides,
        &config,
    )
}

fn paint_scene(
    scene: &ProjectedScene<NodeKey>,
    pane_origin: Point<Pixels>,
    window: &mut gpui::Window,
    state: &OrreryDisplayState,
) {
    for item in &scene.background {
        paint_item(item, pane_origin, window);
    }
    for item in &scene.world {
        paint_item(item, pane_origin, window);
    }
    for item in &scene.overlays {
        paint_item(item, pane_origin, window);
    }
    if let Some(hover_key) = state.hover {
        if let Some((center, radius)) = node_hit_proxy(scene, hover_key) {
            paint_hover_ring(center, radius, pane_origin, window);
        }
    }
}

fn node_hit_proxy(
    scene: &ProjectedScene<NodeKey>,
    key: NodeKey,
) -> Option<(Point2D<f32>, f32)> {
    scene.hit_proxies.iter().find_map(|p| match p {
        HitProxy::Node {
            id,
            center,
            radius,
        } if *id == key => Some((*center, *radius)),
        _ => None,
    })
}

/// Paint the title labels for hovered + active nodes. Per Mark's
/// framing: the label sits **on** the node, not over it — anchored
/// just below the circle so the node icon remains visible.
fn paint_node_labels(
    scene: &ProjectedScene<NodeKey>,
    pane_origin: Point<Pixels>,
    state: &OrreryDisplayState,
    window: &mut Window,
    cx: &mut App,
) {
    // Active first so the hover label can paint over it (with a
    // brighter colour) when both apply.
    if let Some(active) = state.active {
        if let Some((center, radius)) = node_hit_proxy(scene, active) {
            if let Some(label) = lookup_label(state, active) {
                paint_node_label(
                    &label,
                    center,
                    radius,
                    rgb(0xffffff).into(),
                    pane_origin,
                    window,
                    cx,
                );
            }
        }
    }
    if let Some(hover) = state.hover {
        if Some(hover) != state.active {
            if let Some((center, radius)) = node_hit_proxy(scene, hover) {
                if let Some(label) = lookup_label(state, hover) {
                    paint_node_label(
                        &label,
                        center,
                        radius,
                        rgb(0xd0d0d0).into(),
                        pane_origin,
                        window,
                        cx,
                    );
                }
            }
        }
    }
}

fn lookup_label(state: &OrreryDisplayState, key: NodeKey) -> Option<String> {
    state.titles.get(&key).cloned()
}

/// Truncate to a sensible display length so wide labels don't drown
/// the canvas. Operates on grapheme-naive char boundaries — fine for
/// the v0 use case, where labels are URLs and document titles in
/// roman scripts. Real grapheme handling will get pulled in when
/// non-Latin titles become a real test case.
fn truncate_label(label: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in label.chars().enumerate() {
        if i >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

/// Shape and paint a single label, anchored just below the node
/// circle. Caps text length so very long titles don't paint past
/// the pane bounds.
fn paint_node_label(
    text: &str,
    center: Point2D<f32>,
    radius: f32,
    color: gpui::Hsla,
    pane_origin: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    const MAX_LABEL_CHARS: usize = 32;
    let display = truncate_label(text, MAX_LABEL_CHARS);
    let style = window.text_style();
    let run = TextRun {
        len: display.len(),
        font: style.font(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let font_size = style.font_size.to_pixels(window.rem_size());
    let line = window
        .text_system()
        .shape_line(display.into(), font_size, &[run], None);
    // Anchor below the node, centered horizontally on the circle.
    let text_width = line.width.as_f32();
    let label_left = pane_origin.x.as_f32() + center.x - text_width * 0.5;
    let label_top = pane_origin.y.as_f32() + center.y + radius + 4.0;
    let origin = point(px(label_left), px(label_top));
    let _ = line.paint(
        origin,
        window.line_height(),
        gpui::TextAlign::Left,
        None,
        window,
        cx,
    );
}

/// A bright outline around the hovered node, slightly larger than its
/// circle. Drawn on top of everything else.
fn paint_hover_ring(
    center: Point2D<f32>,
    radius: f32,
    pane_origin: Point<Pixels>,
    window: &mut gpui::Window,
) {
    let outer = radius + 4.0;
    let cx = pane_origin.x.as_f32() + center.x;
    let cy = pane_origin.y.as_f32() + center.y;
    let bounds = Bounds::new(
        point(px(cx - outer), px(cy - outer)),
        size(px(outer * 2.0), px(outer * 2.0)),
    );
    window.paint_quad(quad(
        bounds,
        px(outer),
        Background::from(transparent_black()),
        px(2.0),
        gpui_hsla(Color::new(1.0, 0.85, 0.4, 1.0)),
        BorderStyle::default(),
    ));
}

fn paint_item(item: &SceneDrawItem, pane_origin: Point<Pixels>, window: &mut gpui::Window) {
    match item {
        SceneDrawItem::Circle {
            center,
            radius,
            fill,
            stroke,
        } => {
            paint_circle(*center, *radius, *fill, stroke.as_ref(), pane_origin, window);
        }
        SceneDrawItem::Line { from, to, stroke } => {
            paint_line(*from, *to, stroke.color, stroke.width, pane_origin, window);
        }
        // RoundedRect / Label / ImageRef: deferred. Labels need text
        // system integration; ImageRef needs the host's image cache.
        _ => {}
    }
}

fn paint_circle(
    center: Point2D<f32>,
    radius: f32,
    fill: Color,
    stroke: Option<&graph_canvas::packet::Stroke>,
    pane_origin: Point<Pixels>,
    window: &mut gpui::Window,
) {
    // True circle via paint_quad with corner_radius = radius. Smoother
    // than 4-segment cubic-bezier circles which gpui's path tessellator
    // flattens into visible polygon corners at small radii.
    let cx = pane_origin.x.as_f32() + center.x;
    let cy = pane_origin.y.as_f32() + center.y;
    let bounds = Bounds::new(
        point(px(cx - radius), px(cy - radius)),
        size(px(radius * 2.0), px(radius * 2.0)),
    );
    let (border_width, border_color) = match stroke {
        Some(s) if s.width > 0.0 => (px(s.width), gpui_hsla(s.color)),
        _ => (px(0.0), transparent_black()),
    };
    window.paint_quad(quad(
        bounds,
        px(radius),
        gpui_color(fill),
        border_width,
        border_color,
        BorderStyle::default(),
    ));
}

fn paint_line(
    from: Point2D<f32>,
    to: Point2D<f32>,
    color: Color,
    width: f32,
    pane_origin: Point<Pixels>,
    window: &mut gpui::Window,
) {
    let mut builder = PathBuilder::stroke(px(width.max(1.0)));
    builder.move_to(point(
        px(pane_origin.x.as_f32() + from.x),
        px(pane_origin.y.as_f32() + from.y),
    ));
    builder.line_to(point(
        px(pane_origin.x.as_f32() + to.x),
        px(pane_origin.y.as_f32() + to.y),
    ));
    if let Ok(path) = builder.build() {
        window.paint_path(path, gpui_color(color));
    }
}

fn gpui_rgba(c: Color) -> Rgba {
    let pack = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u32;
    let mut col = rgb((pack(c.r) << 16) | (pack(c.g) << 8) | pack(c.b));
    col.a = c.a.clamp(0.0, 1.0);
    col
}

fn gpui_color(c: Color) -> Background {
    gpui_rgba(c).into()
}

fn gpui_hsla(c: Color) -> Hsla {
    gpui_rgba(c).into()
}
