// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Host runtime — `App` (winit's `ApplicationHandler`) and
//! `RuntimeState` (the per-window state the event loop mutates).
//! Event dispatch lives here; setup, render, and orchestration are
//! sibling modules.

use std::sync::Arc;

use cartography::LayoutStrategy;
use graph_layout::adapters::GridAdapter;
use frame::{FrameLayout, SplitAxis};
use session_runtime::{ActionKind, BusAction, BusDispatchOutcome};
use host_substrate::{HostApp, SplitterDrag, SubstrateInputEvent, compute_container_size};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::cartography_projection::{pane_identity_closure, project_orreries};
use crate::graph_registry::GraphRegistry;
use crate::orrery_renderer::OrrerySnapshots;
use crate::render::render_frame;
use crate::setup::{HOST_FRAME_ID, HOST_PANE_ID, build_runtime_state, viewport_size};

/// The winit application handle — wraps an `Option<RuntimeState>`
/// (None until `resumed` mints the window).
#[derive(Default)]
pub struct App {
    state: Option<RuntimeState>,
}

/// Per-window host state. Owned by `App`; constructed in `resumed`
/// via [`crate::setup::build_runtime_state`].
pub struct RuntimeState {
    pub window: Arc<Window>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub renderer: vello::Renderer,
    pub host_app: HostApp,
    /// Current frametree — owned by the host, projected into the
    /// substrate scene through `HostApp::sync_scene_from_frame_layout`.
    pub frame_layout: FrameLayout,
    /// App-scope graph registry; each pane in `frame_layout` carries
    /// a `graph_id` resolved through this map at projection time.
    pub graph_registry: GraphRegistry,
    /// Host-shared orrery projection map cloned from the registered
    /// `OrreryRenderer` at construction.
    pub orrery_snapshots: OrrerySnapshots,
    /// Active splitter drag — set on `SplitterClicked`, consumed by
    /// `CursorMoved` while held, cleared on mouse-up.
    pub dragging_splitter: Option<SplitterDrag>,
    pub target_texture: Option<wgpu::Texture>,
    pub target_view: Option<wgpu::TextureView>,
    pub cursor: Option<kurbo::Point>,
}

impl RuntimeState {
    /// Re-sync the substrate scene from the frametree, then re-run
    /// the cartography projection for each orrery. Called after any
    /// frame-layout mutation (resize, splitter drag) and on initial
    /// setup.
    fn resync(&mut self, viewport: kurbo::Size) {
        self.host_app
            .sync_scene_from_frame_layout(&self.frame_layout, viewport);
        let strategy = GridAdapter::default();
        let _report = project_orreries(
            &self.frame_layout,
            viewport,
            pane_identity_closure(&self.host_app),
            &self.graph_registry,
            &strategy as &dyn LayoutStrategy,
            &self.orrery_snapshots,
        );
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let state = build_runtime_state(event_loop);
        state.window.request_redraw();
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
        match event {
            WindowEvent::CloseRequested => handle_close(state, event_loop),
            WindowEvent::Resized(size) => handle_resize(state, size),
            WindowEvent::CursorMoved { position, .. } => handle_cursor_moved(state, position),
            WindowEvent::CursorLeft { .. } => state.cursor = None,
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
                state.window.request_redraw();
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
    state.surface_config.width = size.width;
    state.surface_config.height = size.height;
    state
        .surface
        .configure(&state.device, &state.surface_config);
    state.target_texture = None;
    state.target_view = None;
    state.resync(viewport_size(size.width, size.height));
    state.window.request_redraw();
}

fn handle_cursor_moved(state: &mut RuntimeState, position: winit::dpi::PhysicalPosition<f64>) {
    let cursor = kurbo::Point::new(position.x, position.y);
    state.cursor = Some(cursor);
    let Some(drag) = state.dragging_splitter.clone() else {
        return;
    };
    let viewport = viewport_size(state.surface_config.width, state.surface_config.height);
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
        state.window.request_redraw();
    }
}

fn handle_mouse_release(state: &mut RuntimeState) {
    if state.dragging_splitter.take().is_some() {
        eprintln!("[splitter] drag released");
    }
}

fn handle_mouse_press(state: &mut RuntimeState) {
    let Some(cursor) = state.cursor else {
        return;
    };
    let resolved = state.host_app.handle_pointer_press(cursor);
    match resolved {
        SubstrateInputEvent::SplitterClicked { path, .. } => {
            if let Some((axis, start_ratio)) = state.frame_layout.split_at(&path) {
                state.dragging_splitter = Some(SplitterDrag {
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
        _ => {}
    }
}
