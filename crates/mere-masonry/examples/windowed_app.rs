// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0
//
// Run with:
//   cargo run --release --example windowed_app

//! Full-stack windowed demo: substrate + runtime + masonry, driven
//! through `mere_host_substrate::MereHostApp`.
//!
//! Differences from the sibling `windowed_demo.rs`:
//!
//! - **MereHostApp** replaces the raw `SubstrateHost`: the host owns
//!   substrate + scene + compositor + `TileManager` + `ManifestStore`
//!   in one struct.
//! - Tiles are opened through `MereHostApp::tiles` with synthetic
//!   `NodeKey`s + dummy `EngineDocument`s; `sync_scene_from_tiles`
//!   projects them into a substrate scene each time the tile list
//!   changes.
//! - `MasonryEmbeddedRenderer` is registered for
//!   `NodeContentKind::Panel`. The masonry tiles render real (empty)
//!   widget content via the GPU `imaging_vello` path.
//! - Diagnostic callback streams `RendererRegistered` /
//!   `RouteDegraded` / etc. to stderr.
//! - Input callback streams `TileClicked { node_key, .. }` /
//!   `BackgroundClicked` / etc. to stderr — clicks resolve back to
//!   the runtime's `NodeKey` automatically.
//! - On `TileClicked`, the demo also wraps the event in a
//!   `BusAction::pane(PaneId(0), FocusTile { index })` and runs it
//!   through `MereHostApp::action_bus`. A registered listener logs
//!   each dispatched action so the click → bus → listener round-trip
//!   is visible on stderr.
//!
//! The windowing + wgpu + present pipeline is the same as
//! `windowed_demo.rs`; this file focuses on the host shape.

use std::sync::Arc;

use inker::{DocumentProvenance, DocumentTrustState, EngineDocument};
use mere_frame::PaneId;
use mere_host_runtime::{ActionKind, BusAction, BusDispatchOutcome};
use mere_host_substrate::{MereHostApp, SubstrateInputEvent};
use masonry_core::core::{DefaultProperties, NewWidget};
use masonry_testing::ModularWidget;
use mere_masonry::{MasonryEmbeddedRenderer, RootWidgetFactory};
use mere_renderer_registry::NodeContentKind;
use petgraph::graph::NodeIndex;
use vello::peniko::Color;
use vello::{AaConfig, AaSupport, RenderParams, RendererOptions};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const INITIAL_WIDTH: u32 = 960;
const INITIAL_HEIGHT: u32 = 720;

/// Frame id the demo persists view-intent state under. Real hosts
/// derive this from their FrameLayout; the demo uses one fixed
/// frame because there's no pane management.
const DEMO_FRAME_ID: &str = "demo-frame";
/// Pane id within `DEMO_FRAME_ID`. Demo has a single root pane.
const DEMO_PANE_ID: u64 = 1;

fn fake_document(url: &str) -> EngineDocument {
    EngineDocument {
        address: url.to_string(),
        title: None,
        content_type: "text/html".to_string(),
        lang: None,
        provenance: DocumentProvenance::default(),
        trust: DocumentTrustState::default(),
        diagnostics: Vec::new(),
        blocks: Vec::new(),
    }
}

fn build_masonry_factory() -> RootWidgetFactory {
    Arc::new(|| {
        let widget: ModularWidget<()> = ModularWidget::new(());
        NewWidget::new(widget).erased()
    })
}

#[derive(Default)]
struct App {
    state: Option<RuntimeState>,
}

struct RuntimeState {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    renderer: vello::Renderer,
    host_app: MereHostApp,
    target_texture: Option<wgpu::Texture>,
    target_view: Option<wgpu::TextureView>,
    cursor: Option<kurbo::Point>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("mere host (substrate + runtime + masonry)")
                        .with_inner_size(winit::dpi::PhysicalSize::new(
                            INITIAL_WIDTH,
                            INITIAL_HEIGHT,
                        )),
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
        // Prefer Rgba8Unorm — vello renders into the intermediate via
        // a STORAGE_BINDING write, which most backends only allow on
        // Rgba8Unorm. Bgra8Unorm (the typical Windows-surface default)
        // panics with "Texture usages TextureUsages(STORAGE_BINDING)
        // are not allowed on a texture of type Bgra8Unorm". Falling
        // back to first non-srgb format if Rgba8Unorm isn't offered.
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| *f == wgpu::TextureFormat::Rgba8Unorm)
            .or_else(|| {
                surface_caps
                    .formats
                    .iter()
                    .copied()
                    .find(|f| !f.is_srgb())
            })
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

        // -- MereHostApp setup ------------------------------------
        let mut host_app = MereHostApp::new();

        // Install diagnostic callback — streams registry events to stderr.
        host_app.set_diagnostic_callback(|event| {
            eprintln!("[diag] {event:?}");
        });

        // Install input callback — streams click resolutions to stderr.
        host_app.set_input_callback(|event| match event {
            SubstrateInputEvent::TileClicked {
                node_key,
                host_pos,
                scene_pos,
            } => eprintln!(
                "[click] tile {:?} @ host ({:.1}, {:.1}) / scene ({:.1}, {:.1})",
                node_key, host_pos.x, host_pos.y, scene_pos.x, scene_pos.y
            ),
            SubstrateInputEvent::EdgeClicked { edge, .. } => {
                eprintln!("[click] edge {:?}", edge.as_u64())
            }
            SubstrateInputEvent::BackgroundClicked { host_pos, .. } => eprintln!(
                "[click] background @ ({:.1}, {:.1})",
                host_pos.x, host_pos.y
            ),
            SubstrateInputEvent::UnknownTileHit { identity, .. } => {
                eprintln!("[click] unknown tile id {:?}", identity.as_u64())
            }
        });

        // Install a bus listener — every `BusAction` that the gate
        // allows fans out through here. Demo uses the default
        // `PermitEverythingGate`; a real host would swap in a policy
        // gate via `host_app.action_bus.set_gate(...)`.
        host_app.action_bus.add_listener(|action| {
            eprintln!("[bus] dispatched {:?} → {:?}", action.target, action.kind);
        });

        // Register the masonry renderer for Panel content.
        let masonry_renderer = MasonryEmbeddedRenderer::new(
            "demo.masonry.panel",
            NodeContentKind::Panel,
            adapter.clone(),
            device.clone(),
            queue.clone(),
            Arc::new(DefaultProperties::new()),
            build_masonry_factory(),
        );
        host_app
            .substrate
            .registry_mut()
            .register(Box::new(masonry_renderer))
            .expect("register masonry");

        // Seed a handful of tiles so the scene has content.
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
                .open_or_focus(NodeIndex::new(i), url.to_string(), fake_document(url));
        }
        host_app.sync_scene_from_tiles();

        // Bind session root + activate (or create) a session, then
        // try to restore the saved camera. Demo persists state under
        // a directory next to the binary so successive runs see the
        // same sessions; a real host would use a config-local dir.
        let session_root = std::path::PathBuf::from("./mere-demo-sessions");
        let report = host_app
            .bind_session_root(&session_root)
            .expect("bind session root");
        let session_id = match report.loaded.first().copied() {
            Some(id) => {
                host_app.activate_session(id);
                eprintln!(
                    "[session] resumed {:?} ({} sessions on disk)",
                    id,
                    report.loaded.len()
                );
                id
            }
            None => {
                let id = host_app.create_session();
                eprintln!("[session] created new session {:?}", id);
                id
            }
        };
        if let Ok(Some(true)) = host_app.load_active_view_intent(DEMO_FRAME_ID, DEMO_PANE_ID) {
            eprintln!(
                "[session] restored view intent for {:?}/{}",
                DEMO_FRAME_ID, DEMO_PANE_ID
            );
        }
        let _ = session_id; // For potential future logging.

        eprintln!(
            "host-app demo up — {}×{} pixels, {} tiles open, surface format {:?}",
            size.width,
            size.height,
            host_app.tracked_tile_count(),
            surface_format
        );
        eprintln!("click anywhere to see substrate event resolution");
        eprintln!("close window to save view intent + flush manifest");

        self.state = Some(RuntimeState {
            window: window.clone(),
            device,
            queue,
            surface,
            surface_config,
            renderer,
            host_app,
            target_texture: None,
            target_view: None,
            cursor: None,
        });
        window.request_redraw();
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
            WindowEvent::CloseRequested => {
                // Save view intent + flush manifest before exit so
                // the next launch resumes the same camera.
                match state
                    .host_app
                    .save_active_view_intent(DEMO_FRAME_ID, DEMO_PANE_ID)
                {
                    Ok(Some(true)) => eprintln!("[session] saved view intent on close"),
                    Ok(Some(false)) => {
                        eprintln!("[session] view intent skipped (identity camera)")
                    }
                    Ok(None) => eprintln!("[session] no active session — skipping save"),
                    Err(err) => eprintln!("[session] view-intent save failed: {err}"),
                }
                match state.host_app.manifests.flush_dirty() {
                    Ok(n) => eprintln!("[session] flushed {n} dirty manifest(s)"),
                    Err(err) => eprintln!("[session] manifest flush failed: {err}"),
                }
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    state.surface_config.width = size.width;
                    state.surface_config.height = size.height;
                    state.surface.configure(&state.device, &state.surface_config);
                    state.target_texture = None;
                    state.target_view = None;
                    // Re-sync scene; future cartography would consult
                    // the new viewport size at layout time.
                    state.host_app.sync_scene_from_tiles();
                    state.window.request_redraw();
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                state.cursor = Some(kurbo::Point::new(position.x, position.y));
            }

            WindowEvent::CursorLeft { .. } => {
                state.cursor = None;
            }

            WindowEvent::MouseInput {
                state: btn_state,
                button: MouseButton::Left,
                ..
            } if btn_state == ElementState::Pressed => {
                if let Some(cursor) = state.cursor {
                    let resolved = state.host_app.handle_pointer_press(cursor);
                    // Translate tile-hit events into a bus action. The
                    // demo uses a stub `PaneId(0)` because it doesn't
                    // own a frame layout yet — a real host would look
                    // up the active workbench pane id.
                    if let SubstrateInputEvent::TileClicked { node_key, .. } = resolved {
                        if let Some(index) = state.host_app.tile_index_for(node_key) {
                            let action = BusAction::pane(
                                PaneId(0),
                                ActionKind::FocusTile { index },
                            );
                            match state.host_app.action_bus.dispatch(&action) {
                                BusDispatchOutcome::Allowed => {}
                                outcome => {
                                    eprintln!("[bus] dispatch outcome: {:?}", outcome);
                                }
                            }
                        }
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                render_frame(state);
                state.window.request_redraw();
            }

            _ => {}
        }
    }
}

fn ensure_target_texture(state: &mut RuntimeState) {
    if state.target_texture.is_some() {
        return;
    }
    let width = state.surface_config.width;
    let height = state.surface_config.height;
    let texture = state.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("host-app intermediate"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: state.surface_config.format,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    state.target_view = Some(view);
    state.target_texture = Some(texture);
}

fn render_frame(state: &mut RuntimeState) {
    ensure_target_texture(state);

    use wgpu::CurrentSurfaceTexture;
    let surface_texture = match state.surface.get_current_texture() {
        CurrentSurfaceTexture::Success(t) | CurrentSurfaceTexture::Suboptimal(t) => t,
        CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
            state.surface.configure(&state.device, &state.surface_config);
            state.target_texture = None;
            return;
        }
        CurrentSurfaceTexture::Timeout
        | CurrentSurfaceTexture::Occluded
        | CurrentSurfaceTexture::Validation => return,
    };

    // Substrate dispatch into a vello scene.
    let mut vello_scene = vello::Scene::new();
    let _report = state.host_app.substrate.render_scene(
        &state.host_app.scene,
        &mut vello_scene,
        &mut state.host_app.compositor,
        &mut state.renderer,
        state.window.scale_factor(),
    );

    // Vello rasterise into intermediate texture.
    let target_view = state.target_view.as_ref().expect("ensured");
    let width = state.surface_config.width;
    let height = state.surface_config.height;
    state
        .renderer
        .render_to_texture(
            &state.device,
            &state.queue,
            &vello_scene,
            target_view,
            &RenderParams {
                base_color: Color::from_rgba8(15, 15, 25, 255),
                width,
                height,
                antialiasing_method: AaConfig::Area,
            },
        )
        .expect("vello render");

    // Blit → present.
    let target_texture = state.target_texture.as_ref().expect("ensured");
    let mut encoder = state
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("host-app blit"),
        });
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: target_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: &surface_texture.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    state.queue.submit(Some(encoder.finish()));
    state.window.pre_present_notify();
    surface_texture.present();
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("run_app");
}
