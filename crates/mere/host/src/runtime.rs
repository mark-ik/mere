// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Host runtime — `App` (winit's `ApplicationHandler`) and
//! `RuntimeState` (the per-window state the event loop mutates).
//! Event dispatch lives here; setup, render, and orchestration are
//! sibling modules.

use std::sync::Arc;

use frame::{FrameLayout, PaneId, SplitAxis};
use kernel::geometry::PortablePoint;
use kernel::graph::NodeKey;
use session_runtime::{ActionKind, BusAction, BusDispatchOutcome};
use host_substrate::{HostApp, SplitterDrag, SubstrateInputEvent, compute_container_size, walk_leaves};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::cartography_projection::{pane_identity_closure, project_orreries};
use crate::graph_node_explode::{
    GraphNodeIdentities, NodeOverrides, explode_projection_into_scene,
};
use crate::graph_node_renderer::GraphNodeSelection;
use crate::graph_registry::GraphRegistry;
use crate::orrery_renderer::OrrerySnapshots;
use crate::render::render_frame;
use crate::setup::{HOST_FRAME_ID, HOST_PANE_ID, build_runtime_state, viewport_size};
use crate::strategy_registry::StrategyRegistry;

/// In-progress drag of a single exploded graph node. Captured on
/// mouse-down over a `GraphNode`; consumed by `CursorMoved` while held
/// to update the node's position override; cleared on mouse-up.
#[derive(Clone, Debug)]
pub struct NodeDrag {
    pub pane_id: PaneId,
    pub node_key: NodeKey,
    /// Window-space offset from the node's center to the grab point,
    /// so the node tracks the cursor without snapping its center to it.
    pub grab_offset: kurbo::Vec2,
    /// The pane's window-space origin, snapshotted at grab time (the
    /// layout is static during a node drag).
    pub pane_origin: kurbo::Point,
    /// The pane's size, snapshotted at grab time. The dragged node's
    /// pane-local position is clamped to `[0, w] × [0, h]` so it can't
    /// wander out of its origin orrery pane into neighbouring panes.
    pub pane_size: kurbo::Size,
}

/// The winit application handle — wraps an `Option<RuntimeState>`
/// (None until `resumed` mints the window).
#[derive(Default)]
pub struct App {
    state: Option<RuntimeState>,
}

/// winit + wgpu/vello plumbing for one window. The irreducible
/// platform layer the host genuinely needs the OS + GPU for.
pub struct WindowGpu {
    pub window: Arc<Window>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub renderer: vello::Renderer,
    /// Lazily-allocated intermediate vello renders into; replaced on
    /// resize. Blitted to the surface each present.
    pub target_texture: Option<wgpu::Texture>,
    pub target_view: Option<wgpu::TextureView>,
}

/// Graph → substrate projection state plus the shared renderer
/// handles the projection writes into. Everything the cartography
/// pass touches lives here so `RuntimeState` doesn't accumulate the
/// pieces individually.
pub struct CartographyState {
    /// App-scope graph registry; each pane in the frametree carries a
    /// `graph_id` resolved through this map at projection time.
    pub graph_registry: GraphRegistry,
    /// `projection_id` → boxed `LayoutStrategy`. Each [`ViewPreset`]
    /// resolves to a registered strategy here.
    pub strategies: StrategyRegistry,
    /// Stable `(PaneId, NodeKey) → NodeIdentity` for Path-B (exploded)
    /// nodes; survives scene rebuilds so a node keeps its identity.
    pub identities: GraphNodeIdentities,
    /// Pane-local manual position overrides; consulted by the Path-B
    /// explosion so a dragged node stays where it was dropped.
    pub overrides: NodeOverrides,
    /// Host-shared orrery projection map (Path A), cloned from the
    /// registered `OrreryRenderer`.
    pub orrery_snapshots: OrrerySnapshots,
    /// Host-shared selection set, cloned from the registered
    /// `GraphNodeRenderer`; the host writes it on click, the renderer
    /// reads it to highlight selected nodes.
    pub selection: GraphNodeSelection,
}

/// Pointer + drag state for one window.
#[derive(Default)]
pub struct InteractionState {
    pub cursor: Option<kurbo::Point>,
    /// Active splitter drag — set on `SplitterClicked`, consumed by
    /// `CursorMoved` while held, cleared on mouse-up.
    pub dragging_splitter: Option<SplitterDrag>,
    /// In-progress graph-node drag, if the user grabbed a node.
    pub dragging_node: Option<NodeDrag>,
}

/// Per-window host state. Owned by `App`; constructed in `resumed`
/// via [`crate::setup::build_runtime_state`]. Deliberately thin: the
/// host is an orchestrator, so subsystem state groups into the
/// sub-structs above rather than landing here as loose fields.
pub struct RuntimeState {
    pub gpu: WindowGpu,
    /// The substrate ↔ runtime bridge.
    pub host_app: HostApp,
    /// Current frametree — projected into the substrate scene via
    /// `HostApp::sync_scene_from_frame_layout`.
    pub frame_layout: FrameLayout,
    pub cartography: CartographyState,
    pub interaction: InteractionState,
    /// Shared diagnostics snapshot the reactive panel reads. The host
    /// overwrites it each frame in `render_frame`.
    pub diagnostics: crate::diagnostics_panel::DiagnosticsHandle,
    /// Shared `NodeIdentity → PaneRole` map the panel factory routes on.
    /// Refreshed each sync so a pane's panel matches its content kind.
    pub pane_roles: crate::panels::PaneRoles,
    /// Frames rendered since startup — published into `diagnostics`.
    pub frame_count: u64,
}

impl RuntimeState {
    /// Re-sync the substrate scene from the frametree, then re-run
    /// the cartography projection for each orrery. Called after any
    /// frame-layout mutation (resize, splitter drag) and on initial
    /// setup.
    fn resync(&mut self, viewport: kurbo::Size) {
        self.host_app
            .sync_scene_from_frame_layout(&self.frame_layout, viewport);
        // Keep the panel factory's identity→role map current — pane
        // identities are reassigned by the sync above.
        crate::panels::update_pane_roles(
            &self.host_app,
            &self.frame_layout,
            viewport,
            &self.pane_roles,
        );
        let outcome = project_orreries(
            &self.frame_layout,
            viewport,
            pane_identity_closure(&self.host_app),
            &self.cartography.graph_registry,
            &self.cartography.strategies,
            &self.cartography.orrery_snapshots,
        );
        // Path B: append per-node substrate entities for each exploded
        // pane. `sync_scene_from_frame_layout` rebuilt the scene above
        // (panes + splitters), so this re-explodes onto the fresh
        // scene every resync; the identity map keeps node ids stable
        // and `overrides` keeps dragged nodes where they were dropped.
        for pane in &outcome.exploded {
            explode_projection_into_scene(
                &mut self.host_app.scene,
                &mut self.cartography.identities,
                &self.cartography.overrides,
                pane.pane_id,
                pane.pane_origin,
                &pane.projection,
            );
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let state = build_runtime_state(event_loop);
        state.gpu.window.request_redraw();
        self.state = Some(state);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        // Event-driven render (ControlFlow::Wait): each handler that
        // changes visible state requests a redraw. An idle host issues
        // no redraws and parks — no core-peg. When an animation lane
        // (e.g. streaming layout) lands, it re-arms the redraw each
        // active frame.
        match event {
            WindowEvent::CloseRequested => handle_close(state, event_loop),
            WindowEvent::Resized(size) => handle_resize(state, size),
            WindowEvent::CursorMoved { position, .. } => handle_cursor_moved(state, position),
            WindowEvent::CursorLeft { .. } => {
                state.interaction.cursor = None;
                state.gpu.window.request_redraw();
            }
            WindowEvent::MouseInput {
                state: btn_state,
                button: MouseButton::Left,
                ..
            } if btn_state == ElementState::Released => handle_mouse_release(state),
            WindowEvent::MouseInput {
                state: btn_state,
                button: MouseButton::Left,
                ..
            } if btn_state == ElementState::Pressed => handle_mouse_press(state),
            WindowEvent::RedrawRequested => {
                render_frame(state);
            }
            _ => {}
        }
    }
}

fn handle_close(state: &mut RuntimeState, event_loop: &ActiveEventLoop) {
    match state
        .host_app
        .save_active_view_intent(HOST_FRAME_ID, HOST_PANE_ID)
    {
        Ok(Some(true)) => eprintln!("[session] saved view intent on close"),
        Ok(Some(false)) => eprintln!("[session] view intent skipped (identity camera)"),
        Ok(None) => eprintln!("[session] no active session — skipping save"),
        Err(err) => eprintln!("[session] view-intent save failed: {err}"),
    }
    match state.host_app.manifests.flush_dirty() {
        Ok(n) => eprintln!("[session] flushed {n} dirty manifest(s)"),
        Err(err) => eprintln!("[session] manifest flush failed: {err}"),
    }
    event_loop.exit();
}

fn handle_resize(state: &mut RuntimeState, size: winit::dpi::PhysicalSize<u32>) {
    if size.width == 0 || size.height == 0 {
        return;
    }
    state.gpu.surface_config.width = size.width;
    state.gpu.surface_config.height = size.height;
    state
        .gpu
        .surface
        .configure(&state.gpu.device, &state.gpu.surface_config);
    state.gpu.target_texture = None;
    state.gpu.target_view = None;
    state.resync(viewport_size(size.width, size.height));
    state.gpu.window.request_redraw();
}

fn handle_cursor_moved(state: &mut RuntimeState, position: winit::dpi::PhysicalPosition<f64>) {
    let cursor = kurbo::Point::new(position.x, position.y);
    state.interaction.cursor = Some(cursor);

    // Graph-node drag wins over splitter drag (a node grab is the more
    // specific gesture). Move the node's pane-local override so it
    // tracks the cursor, then re-explode through resync.
    if let Some(drag) = state.interaction.dragging_node.clone() {
        let node_center = cursor - drag.grab_offset;
        let local = node_center - drag.pane_origin;
        // Clamp to the origin pane's bounds — nodes stay inside their
        // orrery pane rather than wandering across the window.
        let clamped_x = local.x.clamp(0.0, drag.pane_size.width);
        let clamped_y = local.y.clamp(0.0, drag.pane_size.height);
        state.cartography.overrides.set(
            drag.pane_id,
            drag.node_key,
            PortablePoint::new(clamped_x as f32, clamped_y as f32),
        );
        let viewport =
            viewport_size(state.gpu.surface_config.width, state.gpu.surface_config.height);
        state.resync(viewport);
        state.gpu.window.request_redraw();
        return;
    }

    let Some(drag) = state.interaction.dragging_splitter.clone() else {
        // No drag in progress — still re-render so the live cursor
        // readout in the diagnostics panel tracks the pointer.
        state.gpu.window.request_redraw();
        return;
    };
    let viewport = viewport_size(state.gpu.surface_config.width, state.gpu.surface_config.height);
    let container = compute_container_size(&state.frame_layout, &drag.path, viewport);
    let delta = match drag.axis {
        SplitAxis::Horizontal => {
            (cursor.x - drag.start_cursor.x) as f32 / container.width.max(1.0) as f32
        }
        SplitAxis::Vertical => {
            (cursor.y - drag.start_cursor.y) as f32 / container.height.max(1.0) as f32
        }
    };
    let new_ratio = (drag.start_ratio + delta).clamp(0.05, 0.95);
    if state.frame_layout.set_split_ratio(&drag.path, new_ratio) {
        state.resync(viewport);
    }
    state.gpu.window.request_redraw();
}

fn handle_mouse_release(state: &mut RuntimeState) {
    if state.interaction.dragging_splitter.take().is_some() {
        eprintln!("[splitter] drag released");
    }
    if let Some(drag) = state.interaction.dragging_node.take() {
        eprintln!(
            "[graph-node] drag released pane={:?} node={:?}",
            drag.pane_id, drag.node_key
        );
    }
    state.gpu.window.request_redraw();
}

fn handle_mouse_press(state: &mut RuntimeState) {
    let Some(cursor) = state.interaction.cursor else {
        return;
    };
    let resolved = state.host_app.handle_pointer_press(cursor);
    match resolved {
        SubstrateInputEvent::SplitterClicked { path, .. } => {
            if let Some((axis, start_ratio)) = state.frame_layout.split_at(&path) {
                state.interaction.dragging_splitter = Some(SplitterDrag {
                    path: path.clone(),
                    axis,
                    start_cursor: cursor,
                    start_ratio,
                });
                eprintln!(
                    "[splitter] drag started axis={:?} ratio={:.2}",
                    axis, start_ratio
                );
            }
        }
        SubstrateInputEvent::PaneClicked { pane_id, .. } => {
            // v0 dispatch: a pane click toggles the workbench-shaped
            // action so the bus listener fires visibly. A proper
            // `FocusPane` action lands when the new host grows pane
            // focus semantics.
            let action = BusAction::pane(pane_id, ActionKind::ToggleWorkbench);
            match state.host_app.action_bus.dispatch(&action) {
                BusDispatchOutcome::Allowed => {}
                outcome => eprintln!("[bus] dispatch outcome: {:?}", outcome),
            }
        }
        SubstrateInputEvent::UnknownTileHit { identity, .. } => {
            // host-substrate doesn't know about graph nodes (a mere/host
            // concept) — they fall through to UnknownTileHit. If the hit
            // identity is one of our exploded nodes, select + start a drag.
            if let Some((pane_id, node_key)) =
                state.cartography.identities.node_for_identity(identity)
            {
                select_only(state, identity);
                start_node_drag(state, pane_id, node_key, identity, cursor);
            }
        }
        SubstrateInputEvent::BackgroundClicked { .. } => {
            clear_node_selection(state);
        }
        _ => {}
    }
    state.gpu.window.request_redraw();
}

/// Replace the graph-node selection with a single identity.
fn select_only(state: &RuntimeState, identity: register_renderer::NodeIdentity) {
    if let Ok(mut sel) = state.cartography.selection.write() {
        sel.clear();
        sel.insert(identity);
    }
}

/// Clear the graph-node selection (e.g. on background click).
fn clear_node_selection(state: &RuntimeState) {
    if let Ok(mut sel) = state.cartography.selection.write() {
        sel.clear();
    }
}

/// Begin dragging the exploded graph node `identity`. Snapshots the
/// grab offset (cursor → node center) and the pane's window-space
/// origin so `handle_cursor_moved` can map cursor motion to a pane-
/// local position override.
fn start_node_drag(
    state: &mut RuntimeState,
    pane_id: PaneId,
    node_key: NodeKey,
    identity: register_renderer::NodeIdentity,
    cursor: kurbo::Point,
) {
    let Some(node) = state.host_app.scene.get(identity) else {
        return;
    };
    let t = node.placement.transform.translation();
    let center = kurbo::Point::new(t.x + node.size.width / 2.0, t.y + node.size.height / 2.0);

    let viewport = viewport_size(state.gpu.surface_config.width, state.gpu.surface_config.height);
    let Some(leaf) = walk_leaves(&state.frame_layout, viewport)
        .into_iter()
        .find(|leaf| leaf.pane_id == pane_id)
    else {
        return;
    };
    let pane_origin = leaf.placement.transform.translation().to_point();

    state.interaction.dragging_node = Some(NodeDrag {
        pane_id,
        node_key,
        grab_offset: cursor - center,
        pane_origin,
        pane_size: leaf.size,
    });
    eprintln!("[graph-node] drag started pane={pane_id:?} node={node_key:?}");
}
