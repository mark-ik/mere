// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! The orrery's `GraphCanvas` — a **bespoke Masonry `Widget`** painting the
//! kernel graph with real spatial interaction.
//!
//! Per the composition spine, the orrery is the Cartography projection of graph
//! truth. The paint-only first cut (Xilem's stock `canvas` view) couldn't take
//! input; this promotes it to a custom `Widget` so it can hit-test, **drag
//! nodes**, and **pan/zoom the camera** — the first real spatial interaction.
//!
//! Layering: the pure geometry (world↔screen [`Camera`], [`hit_test`]) lives in
//! [`crate::camera`] (window-free, unit-tested). This file is the masonry/xilem
//! glue: the `Widget` (paint + pointer) and the [`graph_canvas`] Xilem `View`
//! that feeds it a [`GraphScene`] from the graph and routes [`NodeMoved`] back
//! to app state. Camera state is **widget-internal** (pan/zoom don't touch app
//! state); only graph mutations (a dragged node's new world position) flow out.
//!
//! Still deferred: real cartography layout (force-directed / radial via
//! `graph-layout`/`cartography`), LOD, and edge styling by relation kind.

use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

use kernel::graph::Graph;
use uuid::Uuid;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::accesskit::{Node as AccessNode, Role};
use xilem::masonry::core::pointer::PointerButton;
use xilem::masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButtonEvent,
    PointerEvent, PointerScrollEvent, PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx,
    ScrollDelta, Widget, WidgetMut, render_text,
};
use xilem::masonry::imaging::Painter;
use xilem::masonry::kurbo::{Affine, Axis, Circle, Line, Point, Size, Stroke, Vec2};
use xilem::masonry::layout::{LenReq, Length};
use xilem::masonry::parley::{Alignment, AlignmentOptions, StyleProperty};
use xilem::masonry::peniko::Color;
use xilem::{Pod, ViewCtx};

use crate::camera::{Camera, GraphScene, SceneNode, hit_test};

const NODE_FILL: Color = Color::from_rgb8(70, 110, 200);
const NODE_RING: Color = Color::from_rgb8(220, 228, 245);
const EDGE_COLOR: Color = Color::from_rgb8(120, 130, 150);
const LABEL_COLOR: Color = Color::from_rgb8(228, 233, 242);
const LABEL_SIZE: f32 = 11.0;
const BASE_NODE_RADIUS: f64 = 15.0;
const DEFAULT_LENGTH: Length = Length::const_px(100.0);

/// Disc radius in screen pixels at the current zoom (clamped so nodes stay
/// usable when zoomed far in/out). Hit-testing uses the same radius.
fn screen_node_radius(zoom: f64) -> f64 {
    (BASE_NODE_RADIUS * zoom).clamp(7.0, 40.0)
}

/// Action emitted when the user drags a node: its stable id and new world
/// position. The view routes this to app state (write the kernel graph).
#[derive(Clone, Copy, Debug)]
pub struct NodeMoved {
    pub id: Uuid,
    pub world: Point,
}

/// An in-progress pointer gesture. `Drag` is `Copy` so `on_pointer_event` can
/// match it by value and freely mutate the rest of the widget.
#[derive(Clone, Copy, Debug)]
enum Drag {
    /// Moving a node: `grab` is the (world-space) offset from the cursor to the
    /// node center at grab time, so the node doesn't jump under the cursor.
    Node { index: usize, grab: Vec2 },
    /// Panning the empty canvas; `last` is the previous cursor screen position.
    Pan { last: Point },
}

/// Build a [`GraphScene`] (world positions + edges as index pairs) from graph
/// truth. Positions come from each node's projected position; relations
/// collapse to undirected, de-duplicated index pairs.
pub fn scene_from_graph(graph: &Graph) -> GraphScene {
    let mut nodes = Vec::new();
    let mut index_of = HashMap::new();
    for (key, node) in graph.nodes() {
        let w = graph.node_projected_position(key).unwrap_or_default();
        index_of.insert(key, nodes.len());
        nodes.push(SceneNode {
            id: node.id,
            world: Point::new(w.x as f64, w.y as f64),
            label: if node.title.is_empty() {
                node.url().to_string()
            } else {
                node.title.clone()
            },
        });
    }

    let mut seen = HashSet::new();
    let mut edges = Vec::new();
    for rel in graph.relations() {
        if let (Some(&a), Some(&b)) = (index_of.get(&rel.from), index_of.get(&rel.to)) {
            let pair = if a <= b { (a, b) } else { (b, a) };
            if a != b && seen.insert(pair) {
                edges.push((a, b));
            }
        }
    }
    GraphScene { nodes, edges }
}

// =============================================================================
// Widget
// =============================================================================

/// The custom Masonry widget. Owns the render scene + camera + in-progress
/// drag; the camera is internal (pan/zoom never leave the widget).
pub struct GraphCanvasWidget {
    scene: GraphScene,
    camera: Camera,
    size: Size,
    drag: Option<Drag>,
}

impl GraphCanvasWidget {
    fn new(scene: GraphScene) -> Self {
        Self {
            scene,
            camera: Camera::default(),
            size: Size::ZERO,
            drag: None,
        }
    }

    /// Refresh the render scene from a rebuild (camera + drag are preserved).
    fn set_scene(this: &mut WidgetMut<'_, Self>, scene: GraphScene) {
        this.widget.scene = scene;
        this.ctx.request_render();
    }
}

impl Widget for GraphCanvasWidget {
    type Action = NodeMoved;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary) | None,
                state,
                ..
            }) => {
                let local = ctx.local_position(state.position);
                ctx.capture_pointer();
                let r = screen_node_radius(self.camera.zoom);
                self.drag = match hit_test(&self.scene, &self.camera, self.size, local, r) {
                    Some(i) => {
                        let cursor_world = self.camera.screen_to_world(local, self.size);
                        Some(Drag::Node {
                            index: i,
                            grab: self.scene.nodes[i].world - cursor_world,
                        })
                    }
                    None => Some(Drag::Pan { last: local }),
                };
                ctx.request_render();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) if ctx.is_active() => {
                let local = ctx.local_position(current.position);
                match self.drag {
                    Some(Drag::Node { index, grab }) => {
                        let world = self.camera.screen_to_world(local, self.size) + grab;
                        if let Some(node) = self.scene.nodes.get_mut(index) {
                            node.world = world;
                            let id = node.id;
                            ctx.submit_action::<NodeMoved>(NodeMoved { id, world });
                        }
                        ctx.request_render();
                    }
                    Some(Drag::Pan { last }) => {
                        self.camera.pan_by_screen(local - last);
                        self.drag = Some(Drag::Pan { last: local });
                        ctx.request_render();
                    }
                    None => {}
                }
            }
            PointerEvent::Up(_) => {
                if self.drag.take().is_some() {
                    ctx.request_render();
                }
            }
            PointerEvent::Scroll(PointerScrollEvent { delta, state, .. }) => {
                let dy = match delta {
                    ScrollDelta::LineDelta(_, y) | ScrollDelta::PageDelta(_, y) => *y as f64,
                    ScrollDelta::PixelDelta(p) => p.y / 40.0,
                };
                if dy != 0.0 {
                    let local = ctx.local_position(state.position);
                    // Each wheel notch scales ~12% toward the cursor.
                    let factor = (1.0 + dy * 0.12).clamp(0.2, 5.0);
                    self.camera.zoom_around(local, self.size, factor);
                    ctx.request_render();
                }
            }
            _ => {}
        }
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        match len_req {
            LenReq::FitContent(space) => space,
            _ => DEFAULT_LENGTH,
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.size = size;
        ctx.set_clip_path(size.to_rect());
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let size = self.size;
        let r = screen_node_radius(self.camera.zoom);

        // Edges first, beneath the discs.
        let edge_stroke = Stroke::new(1.5);
        for &(a, b) in &self.scene.edges {
            if let (Some(na), Some(nb)) = (self.scene.nodes.get(a), self.scene.nodes.get(b)) {
                let pa = self.camera.world_to_screen(na.world, size);
                let pb = self.camera.world_to_screen(nb.world, size);
                painter.stroke(Line::new(pa, pb), &edge_stroke, EDGE_COLOR).draw();
            }
        }

        // Node discs.
        let ring_stroke = Stroke::new(2.0);
        for node in &self.scene.nodes {
            let p = self.camera.world_to_screen(node.world, size);
            let disc = Circle::new(p, r);
            painter.fill(disc, NODE_FILL).draw();
            painter.stroke(disc, &ring_stroke, NODE_RING).draw();
        }

        // Labels above the discs.
        for node in &self.scene.nodes {
            let p = self.camera.world_to_screen(node.world, size);
            draw_label(painter, ctx, &node.label, p, r);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Canvas
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut AccessNode,
    ) {
        node.set_description(format!("Spatial graph view: {} nodes", self.scene.nodes.len()));
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

/// Draw a centered text label just below the disc at screen point `center`.
fn draw_label(painter: &mut Painter<'_>, ctx: &mut PaintCtx<'_>, text: &str, center: Point, radius: f64) {
    let (fcx, lcx) = ctx.text_contexts();
    let mut builder = lcx.ranged_builder(fcx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(LABEL_SIZE));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout.align(Alignment::Start, AlignmentOptions::default());

    let x = center.x - layout.width() as f64 / 2.0;
    let y = center.y + radius + 3.0;
    render_text(painter, Affine::translate((x, y)), &layout, &[LABEL_COLOR.into()], true);
}

// =============================================================================
// Xilem view
// =============================================================================

/// A reactive orrery view over a [`GraphScene`]. `on_node_moved` writes a
/// dragged node's new world position back to app state (the kernel graph).
pub fn graph_canvas<State, Action, F>(scene: GraphScene, on_node_moved: F) -> GraphCanvas<State, F>
where
    State: 'static,
    F: Fn(&mut State, NodeMoved) -> Action + Send + Sync + 'static,
{
    GraphCanvas {
        scene,
        on_node_moved,
        phantom: PhantomData,
    }
}

/// The [`View`] created by [`graph_canvas`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct GraphCanvas<State, F> {
    scene: GraphScene,
    on_node_moved: F,
    phantom: PhantomData<fn() -> State>,
}

impl<State, F> ViewMarker for GraphCanvas<State, F> {}

impl<State, Action, F> View<State, Action, ViewCtx> for GraphCanvas<State, F>
where
    State: 'static,
    F: Fn(&mut State, NodeMoved) -> Action + Send + Sync + 'static,
{
    type Element = Pod<GraphCanvasWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        (
            ctx.with_action_widget(|ctx| ctx.create_pod(GraphCanvasWidget::new(self.scene.clone()))),
            (),
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        GraphCanvasWidget::set_scene(&mut element, self.scene.clone());
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<NodeMoved>() {
            Some(moved) => MessageResult::Action((self.on_node_moved)(app_state, *moved)),
            None => MessageResult::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::geometry::PortablePoint;
    use kernel::graph::{NavigationTrigger, Traversal};

    #[test]
    fn scene_from_graph_carries_positions_and_dedupes_edges() {
        let mut g = Graph::new();
        let a = g.add_node("seed://a".into(), PortablePoint::new(0.0, 0.0));
        let b = g.add_node("seed://b".into(), PortablePoint::new(0.0, 0.0));
        g.set_node_projected_position(a, PortablePoint::new(10.0, 20.0));
        g.set_node_projected_position(b, PortablePoint::new(-5.0, 7.0));
        // Two traversals between the same pair → one undirected edge.
        g.push_traversal(a, b, Traversal::now(NavigationTrigger::LinkClick));
        g.push_traversal(b, a, Traversal::now(NavigationTrigger::LinkClick));

        let scene = scene_from_graph(&g);
        assert_eq!(scene.nodes.len(), 2);
        assert_eq!(scene.edges.len(), 1);
        let pa = scene.nodes.iter().find(|n| n.label == "seed://a").unwrap();
        assert_eq!(pa.world, Point::new(10.0, 20.0));
    }
}
