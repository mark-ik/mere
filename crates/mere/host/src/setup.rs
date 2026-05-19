// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Host bootstrap: builds the window, wgpu device, vello renderer,
//! `HostApp`, registers the renderer chain, seeds a graph + frame
//! layout, projects the first frame, and binds the session store.
//! Returns a fully-initialised [`RuntimeState`] for the event loop to
//! consume.

use std::sync::Arc;

use masonry_core::core::DefaultProperties;
use frame::GraphId;
use session_runtime::ActionKind;
use host_substrate::HostApp;
use mere_masonry::MasonryEmbeddedRenderer;
use register_renderer::NodeContentKind;
use petgraph::graph::NodeIndex;
use vello::{AaSupport, RendererOptions};
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use crate::cartography_projection::{pane_identity_closure, project_orreries};
use crate::graph_registry::GraphRegistry;
use crate::orrery_renderer::OrreryRenderer;
use crate::runtime::RuntimeState;
use crate::seed;
use crate::splitter_renderer::SplitterRenderer;
use crate::strategy_registry::StrategyRegistry;

pub const INITIAL_WIDTH: u32 = 960;
pub const INITIAL_HEIGHT: u32 = 720;

/// Frame id the host persists view-intent state under. v0 single-
/// frame; once frame-switching lands this comes from the active
/// frame's saved state.
pub const HOST_FRAME_ID: &str = "host-frame";
/// Pane id under [`HOST_FRAME_ID`] the host saves view-intent for.
/// v0 single-pane camera persistence; per-pane state arrives when
/// the workbench / orrery / gloss panes each carry their own camera.
pub const HOST_PANE_ID: u64 = 1;

pub fn viewport_size(width: u32, height: u32) -> kurbo::Size {
    kurbo::Size::new(width.max(1) as f64, height.max(1) as f64)
}

/// Build the host's runtime state. Called from
/// `App::resumed` exactly once.
pub fn build_runtime_state(event_loop: &ActiveEventLoop) -> RuntimeState {
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("mere host")
                    .with_inner_size(winit::dpi::PhysicalSize::new(INITIAL_WIDTH, INITIAL_HEIGHT)),
            )
            .expect("create window"),
    );

    let instance = wgpu::Instance::default();
    let surface = instance
        .create_surface(window.clone())
        .expect("create surface");

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: Some(&surface),
        ..Default::default()
    }))
    .expect("request adapter");

    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("request device");

    let size = window.inner_size();
    let surface_caps = surface.get_capabilities(&adapter);
    // Prefer Rgba8Unorm — vello renders into the intermediate via a
    // STORAGE_BINDING write, which most backends only allow on
    // Rgba8Unorm. Bgra8Unorm (typical Windows-surface default)
    // would panic on storage binding.
    let surface_format = surface_caps
        .formats
        .iter()
        .copied()
        .find(|f| *f == wgpu::TextureFormat::Rgba8Unorm)
        .or_else(|| surface_caps.formats.iter().copied().find(|f| !f.is_srgb()))
        .unwrap_or(surface_caps.formats[0]);
    let surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
        format: surface_format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: surface_caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &surface_config);

    let renderer = vello::Renderer::new(
        &device,
        RendererOptions {
            use_cpu: false,
            antialiasing_support: AaSupport::area_only(),
            num_init_threads: None,
            pipeline_cache: None,
        },
    )
    .expect("vello renderer init");

    // -- Host state -------------------------------------------------
    let mut host_app = HostApp::new();
    install_callbacks(&mut host_app);
    let orrery_snapshots = register_renderers(&mut host_app, &adapter, &device, &queue);

    // Seed: a graph in the registry + a frame layout bound to it.
    let mut graph_registry = GraphRegistry::new();
    let graph_id = GraphId::new();
    graph_registry.insert(graph_id, seed::build_seed_graph());
    let frame_layout = seed::build_seed_layout(graph_id);

    // Tile-manager state stays seeded for when the workbench
    // renderer wakes up; not consumed by the v0 projection.
    for (i, url) in [
        "https://a.example",
        "https://b.example",
        "https://c.example",
        "https://d.example",
    ]
    .iter()
    .enumerate()
    {
        host_app
            .tiles
            .open_or_focus(NodeIndex::new(i), url.to_string(), seed::fake_document(url));
    }

    // Project the frametree into the substrate scene, then run the
    // cartography projection for each orrery pane.
    let viewport = viewport_size(size.width, size.height);
    host_app.sync_scene_from_frame_layout(&frame_layout, viewport);
    let strategies = StrategyRegistry::with_defaults();
    let report = project_orreries(
        &frame_layout,
        viewport,
        pane_identity_closure(&host_app),
        &graph_registry,
        &strategies,
        &orrery_snapshots,
    );
    eprintln!(
        "[cartography] projected {} orrery pane(s), {} registered strategies",
        report.projected,
        strategies.len(),
    );

    // Session bind / create / restore (camera) — mirrors the prior
    // session lifecycle, narrowed to the canonical host path.
    let session_root = std::path::PathBuf::from("./mere-sessions");
    let load_report = host_app
        .bind_session_root(&session_root)
        .expect("bind session root");
    let session_id = match load_report.loaded.first().copied() {
        Some(id) => {
            host_app.activate_session(id);
            eprintln!(
                "[session] resumed {:?} ({} sessions on disk)",
                id,
                load_report.loaded.len()
            );
            id
        }
        None => {
            let id = host_app.create_session();
            eprintln!("[session] created {:?}", id);
            id
        }
    };
    if let Ok(Some(true)) = host_app.load_active_view_intent(HOST_FRAME_ID, HOST_PANE_ID) {
        eprintln!("[session] restored view intent");
    }
    let _ = session_id;

    eprintln!(
        "host up — {}×{} pixels, {} substrate nodes, surface format {:?}",
        size.width,
        size.height,
        host_app.scene.len(),
        surface_format
    );

    RuntimeState {
        window,
        device,
        queue,
        surface,
        surface_config,
        renderer,
        host_app,
        frame_layout,
        graph_registry,
        strategies,
        orrery_snapshots,
        dragging_splitter: None,
        target_texture: None,
        target_view: None,
        cursor: None,
    }
}

fn install_callbacks(host_app: &mut HostApp) {
    host_app.set_diagnostic_callback(|event| {
        eprintln!("[diag] {event:?}");
    });
    host_app.set_input_callback(|event| {
        use host_substrate::SubstrateInputEvent::*;
        match event {
            PaneClicked {
                pane_id, host_pos, ..
            } => eprintln!(
                "[click] pane {:?} @ ({:.1}, {:.1})",
                pane_id, host_pos.x, host_pos.y
            ),
            SplitterClicked { path, host_pos, .. } => eprintln!(
                "[click] splitter path={:?} @ ({:.1}, {:.1})",
                path, host_pos.x, host_pos.y
            ),
            BackgroundClicked { host_pos, .. } => {
                eprintln!(
                    "[click] background @ ({:.1}, {:.1})",
                    host_pos.x, host_pos.y
                )
            }
            EdgeClicked { edge, .. } => eprintln!("[click] edge {:?}", edge.as_u64()),
            TileClicked { node_key, .. } => eprintln!("[click] tile {:?}", node_key),
            UnknownTileHit { identity, .. } => {
                eprintln!("[click] unknown {:?}", identity.as_u64())
            }
        }
    });
    host_app.action_bus.add_listener(|action| {
        eprintln!("[bus] {:?} → {:?}", action.target, action.kind);
    });
    let _ = ActionKind::ToggleWorkbench;
}

/// Register the three v0 renderers (panel via masonry, orrery,
/// splitter) and return the orrery renderer's snapshot handle.
/// Cloning the `OrrerySnapshots` Arc before `register` keeps the
/// host-side write path alive after the registry takes ownership
/// of the boxed renderer.
fn register_renderers(
    host_app: &mut HostApp,
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> crate::orrery_renderer::OrrerySnapshots {
    let masonry = MasonryEmbeddedRenderer::new(
        "host.masonry.panel",
        NodeContentKind::Panel,
        adapter.clone(),
        device.clone(),
        queue.clone(),
        Arc::new(DefaultProperties::new()),
        seed::build_masonry_factory(),
    );
    host_app
        .substrate
        .registry_mut()
        .register(Box::new(masonry))
        .expect("register masonry panel renderer");

    let orrery = OrreryRenderer::default();
    let snapshots = orrery.snapshots();
    host_app
        .substrate
        .registry_mut()
        .register(Box::new(orrery))
        .expect("register orrery renderer");

    host_app
        .substrate
        .registry_mut()
        .register(Box::new(SplitterRenderer::default()))
        .expect("register splitter renderer");

    snapshots
}
