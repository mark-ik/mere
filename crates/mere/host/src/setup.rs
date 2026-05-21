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
use vello::{AaSupport, RendererOptions};
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use crate::cartography_projection::{pane_identity_closure, project_orreries};
use crate::graph_node_explode::{GraphNodeIdentities, explode_projection_into_scene};
use crate::graph_node_renderer::GraphNodeRenderer;
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
    // Shared tokio runtime for xilem panels (ViewCtx requires one) and
    // the diagnostics snapshot the reactive panel reads each frame.
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime"),
    );
    let diagnostics: crate::diagnostics_panel::DiagnosticsHandle = Arc::new(std::sync::Mutex::new(
        crate::diagnostics_panel::DiagnosticsSnapshot::default(),
    ));

    let mut host_app = HostApp::new();
    install_callbacks(&mut host_app);
    let (orrery_snapshots, graph_node_selection) = register_renderers(
        &mut host_app,
        &adapter,
        &device,
        &queue,
        diagnostics.clone(),
        runtime.clone(),
    );
    let node_overrides = crate::graph_node_explode::NodeOverrides::new();

    // ── Session lifecycle: bind the store, resolve resumed vs fresh ──
    // Session-first: the session decides the workspace content, not the
    // other way round. This is the seed → restore boundary — a resumed
    // session restores its persisted graph; a fresh one seeds the
    // default workspace and persists it.
    let session_root = std::path::PathBuf::from("./mere-sessions");
    let load_report = host_app
        .bind_session_root(&session_root)
        .expect("bind session root");
    let session_id = match load_report.loaded.first().copied() {
        Some(id) => {
            host_app.activate_session(id);
            eprintln!(
                "[session] resumed {:?} ({} session(s) on disk)",
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
    // Persist the manifest immediately (not just on clean close) so the
    // session resumes next run even after a crash, and so its directory
    // exists for the graph save below. Flush writes only dirty
    // manifests — a resumed session is already on disk and untouched.
    match host_app.manifests.flush_dirty() {
        Ok(n) if n > 0 => eprintln!("[session] persisted {n} manifest(s)"),
        Ok(_) => {}
        Err(e) => eprintln!("[session] manifest flush failed: {e}"),
    }
    let session_dir = host_app
        .active_session_dir()
        .expect("active session has a dir");
    // Belt-and-suspenders: flush_dirty created the dir for a fresh
    // session; ensure it exists regardless before save_graph.
    std::fs::create_dir_all(&session_dir).ok();

    // ── Workspace content: restore the session's graph, or seed it ──
    // The graph_id comes from the session manifest so the host registry
    // key, the frame layout's panes, and the on-disk session all agree.
    let graph_id = host_app
        .manifests
        .get(session_id)
        .map(|m| m.root_graph_id)
        .unwrap_or_else(GraphId::new);
    let graph = match session_runtime::session_graph_store::load_graph(&session_dir) {
        Ok(Some(g)) => {
            eprintln!("[session] restored graph ({} node(s))", g.nodes().count());
            g
        }
        Ok(None) | Err(_) => {
            let g = seed::default_workspace_graph();
            match session_runtime::session_graph_store::save_graph(&session_dir, &g) {
                Ok(()) => eprintln!("[session] seeded default workspace graph"),
                Err(e) => eprintln!("[session] seed graph save failed: {e}"),
            }
            g
        }
    };
    let mut graph_registry = GraphRegistry::new();
    graph_registry.insert(graph_id, graph);

    // Frame layout + placeholder tiles seed each boot — layout/tile
    // persistence is Phase D of the host roadmap. Both reference the
    // session's `graph_id`.
    let frame_layout = seed::default_workspace_layout(graph_id);
    seed::open_placeholder_tiles(&mut host_app);

    // Project the frametree into the substrate scene, then run the
    // cartography projection for each orrery pane. Painted panes write
    // a snapshot; exploded panes are returned for insertion as per-node
    // substrate entities below.
    let viewport = viewport_size(size.width, size.height);
    host_app.sync_scene_from_frame_layout(&frame_layout, viewport);
    let strategies = StrategyRegistry::with_defaults();
    let mut graph_node_identities = GraphNodeIdentities::new();
    let outcome = project_orreries(
        &frame_layout,
        viewport,
        pane_identity_closure(&host_app),
        &graph_registry,
        &strategies,
        &orrery_snapshots,
    );
    let mut exploded_nodes = 0;
    for pane in &outcome.exploded {
        let report = explode_projection_into_scene(
            &mut host_app.scene,
            &mut graph_node_identities,
            &node_overrides,
            pane.pane_id,
            pane.pane_origin,
            &pane.projection,
        );
        exploded_nodes += report.nodes;
    }
    eprintln!(
        "[cartography] {} painted, {} exploded pane(s) → {} graph nodes, {} strategies",
        outcome.report.painted,
        outcome.report.exploded,
        exploded_nodes,
        strategies.len(),
    );

    // Restore the persisted camera (view intent) for the active
    // session, now that the scene is projected.
    if let Ok(Some(true)) = host_app.load_active_view_intent(HOST_FRAME_ID, HOST_PANE_ID) {
        eprintln!("[session] restored view intent");
    }

    eprintln!(
        "host up — {}×{} pixels, {} substrate nodes, surface format {:?}",
        size.width,
        size.height,
        host_app.scene.len(),
        surface_format
    );

    RuntimeState {
        gpu: crate::runtime::WindowGpu {
            window,
            device,
            queue,
            surface,
            surface_config,
            renderer,
            target_texture: None,
            target_view: None,
        },
        host_app,
        frame_layout,
        cartography: crate::runtime::CartographyState {
            graph_registry,
            strategies,
            identities: graph_node_identities,
            overrides: node_overrides,
            orrery_snapshots,
            selection: graph_node_selection,
        },
        interaction: crate::runtime::InteractionState::default(),
        diagnostics,
        frame_count: 0,
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

/// Register the v0 renderers (reactive panel via masonry+xilem,
/// orrery, graph-node, splitter) and return the orrery renderer's
/// snapshot handle. Cloning the `OrrerySnapshots` Arc before `register`
/// keeps the host-side write path alive after the registry takes
/// ownership of the boxed renderer.
///
/// `diagnostics` + `runtime` feed the reactive panel factory: every
/// `Panel` node renders a xilem-driven diagnostics view reading the
/// shared snapshot. (v0 routes all panels to diagnostics; per-node
/// routing — workbench vs apparatus vs gloss — is a follow-up.)
fn register_renderers(
    host_app: &mut HostApp,
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    diagnostics: crate::diagnostics_panel::DiagnosticsHandle,
    runtime: Arc<tokio::runtime::Runtime>,
) -> (
    crate::orrery_renderer::OrrerySnapshots,
    crate::graph_node_renderer::GraphNodeSelection,
) {
    let default_props = Arc::new(DefaultProperties::new());
    let panel_factory: mere_masonry::PanelFactory = Arc::new(move |size, scale| {
        crate::diagnostics_panel::build_diagnostics_panel(
            diagnostics.clone(),
            runtime.clone(),
            default_props.clone(),
            size,
            scale,
        )
    });
    let masonry = MasonryEmbeddedRenderer::new(
        "host.masonry.panel",
        NodeContentKind::Panel,
        adapter.clone(),
        device.clone(),
        queue.clone(),
        panel_factory,
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

    let graph_node = GraphNodeRenderer::default();
    let selection = graph_node.selection();
    host_app
        .substrate
        .registry_mut()
        .register(Box::new(graph_node))
        .expect("register graph-node renderer");

    host_app
        .substrate
        .registry_mut()
        .register(Box::new(SplitterRenderer::default()))
        .expect("register splitter renderer");

    (snapshots, selection)
}
