// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0
//
// Run with:
//   cargo run --release --example windowed_demo

//! Windowed proof-of-life: opens a real OS window, renders a substrate
//! scene with mixed renderer tenants every frame, routes mouse clicks
//! through `SubstrateHost::deliver_input_at`.
//!
//! Scene layout:
//!
//! - Backdrop (`CustomCanvas`, dark navy) fills the viewport.
//! - Top-left + bottom-right tiles: `DocumentTile` (green
//!   `SolidRectRenderer`).
//! - Top-right + bottom-left tiles: `Panel` (red
//!   `SolidRectRenderer`). NB: not the `MasonryEmbeddedRenderer` —
//!   the demo deliberately keeps the renderer mix simple. Masonry's
//!   integration is verified by `tests/substrate_integration.rs`;
//!   adding a Masonry tile here would need real widget content to be
//!   visually distinguishable from the solid rects.
//!
//! Mouse clicks: `WindowEvent::MouseInput` → `deliver_input_at` with
//! the cursor position. The dispatch goes through the same
//! composition-mode-aware path the integration tests cover. Hit
//! results print to stderr for visibility.
//!
//! Rendering pipeline:
//!
//! 1. `SubstrateHost::render_scene` walks the scene, dispatches each
//!    node through its registered renderer.
//! 2. `vello::Renderer::render_to_texture` rasterizes the resulting
//!    `vello::Scene` into an intermediate `Rgba8Unorm` texture.
//! 3. `copy_texture_to_texture` blits the intermediate into the
//!    current surface texture (with format conversion handled by the
//!    matched intermediate format).
//! 4. `surface_texture.present()` displays.

use std::sync::Arc;

use kurbo::Size;
use mere_renderer_registry::{
    InputEvent, ModifiersState, NodeContentKind, Placement, PointerButton, PointerEventKind,
    RendererRegistry,
};
use mere_spatial_prototype::{
    ExternalTextureCompositor, SolidRectRenderer, SubstrateHost, SubstrateNode, SubstrateScene,
};
use vello::peniko::Color;
use vello::{AaConfig, AaSupport, RenderParams, RendererOptions};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const INITIAL_WIDTH: u32 = 800;
const INITIAL_HEIGHT: u32 = 600;

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
    compositor: ExternalTextureCompositor,
    host: SubstrateHost,
    scene: SubstrateScene,
    /// Intermediate STORAGE_BINDING-capable texture vello renders into;
    /// blitted to the surface texture each frame. Rebuilt on resize.
    target_texture: Option<wgpu::Texture>,
    target_view: Option<wgpu::TextureView>,
    cursor: Option<kurbo::Point>,
}

impl App {
    fn new() -> Self {
        Self::default()
    }
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
                        .with_title("mere spatial substrate — windowed demo")
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
        // Pick the first non-srgb format (vello renders linear; mismatch
        // produces visible washout on Bgra8UnormSrgb surfaces). If only
        // srgb formats are available, fall back to the first one.
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
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

        // Renderer registry: SolidRect for backdrop + DocumentTile + Panel.
        let mut registry = RendererRegistry::with_default_selector();
        registry
            .register(Box::new(SolidRectRenderer::for_kind(
                "demo.bg",
                NodeContentKind::CustomCanvas,
                Color::from_rgba8(20, 30, 60, 255),
            )))
            .unwrap();
        registry
            .register(Box::new(SolidRectRenderer::for_kind(
                "demo.doctile",
                NodeContentKind::DocumentTile,
                Color::from_rgba8(80, 200, 100, 255),
            )))
            .unwrap();
        registry
            .register(Box::new(SolidRectRenderer::for_kind(
                "demo.panel",
                NodeContentKind::Panel,
                Color::from_rgba8(220, 80, 80, 255),
            )))
            .unwrap();

        let scene = build_scene(size.width, size.height);
        let host = SubstrateHost::new(registry);
        let compositor = ExternalTextureCompositor::new();

        eprintln!(
            "demo window up — {}×{} pixels, surface format {:?}",
            size.width, size.height, surface_format
        );
        eprintln!("click in the window to hit-test against the scene");

        self.state = Some(RuntimeState {
            window: window.clone(),
            device,
            queue,
            surface,
            surface_config,
            renderer,
            compositor,
            host,
            scene,
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
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    state.surface_config.width = size.width;
                    state.surface_config.height = size.height;
                    state.surface.configure(&state.device, &state.surface_config);
                    state.target_texture = None;
                    state.target_view = None;
                    state.scene = build_scene(size.width, size.height);
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
                    let event = InputEvent::Pointer {
                        position: kurbo::Point::ZERO,
                        kind: PointerEventKind::Down(PointerButton::Primary),
                        modifiers: ModifiersState::default(),
                    };
                    let dispatch = state.host.deliver_input_at(&state.scene, cursor, &event);
                    match (dispatch, state.scene.hit_test(cursor)) {
                        (Ok(Some(d)), Some(id)) => eprintln!(
                            "click @ ({:.1}, {:.1}) → node {} ({:?})",
                            cursor.x,
                            cursor.y,
                            id.as_u64(),
                            d
                        ),
                        (Ok(None), _) => {
                            eprintln!("click @ ({:.1}, {:.1}) → no hit", cursor.x, cursor.y)
                        }
                        (Err(e), _) => eprintln!("click dispatch error: {e}"),
                        _ => {}
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

fn build_scene(width: u32, height: u32) -> SubstrateScene {
    let mut scene = SubstrateScene::new();
    // Backdrop.
    scene.insert(SubstrateNode::new(
        NodeContentKind::CustomCanvas,
        Placement::IDENTITY,
        Size::new(width as f64, height as f64),
    ));

    // Two rows of two tiles, alternating Panel + DocumentTile.
    let w = (width as f64) * 0.4;
    let h = (height as f64) * 0.35;
    let gap_x = (width as f64) * 0.05;
    let gap_y = (height as f64) * 0.1;
    let col1_x = gap_x;
    let col2_x = gap_x * 2.0 + w;
    let row1_y = gap_y;
    let row2_y = gap_y * 2.0 + h;

    // Top-left: green DocumentTile
    scene.insert(SubstrateNode::new(
        NodeContentKind::DocumentTile,
        Placement::translate(col1_x, row1_y),
        Size::new(w, h),
    ));
    // Top-right: red Panel
    scene.insert(SubstrateNode::new(
        NodeContentKind::Panel,
        Placement::translate(col2_x, row1_y),
        Size::new(w, h),
    ));
    // Bottom-left: red Panel
    scene.insert(SubstrateNode::new(
        NodeContentKind::Panel,
        Placement::translate(col1_x, row2_y),
        Size::new(w, h),
    ));
    // Bottom-right: green DocumentTile
    scene.insert(SubstrateNode::new(
        NodeContentKind::DocumentTile,
        Placement::translate(col2_x, row2_y),
        Size::new(w, h),
    ));

    scene
}

fn ensure_target_texture(state: &mut RuntimeState) {
    if state.target_texture.is_some() {
        return;
    }
    let width = state.surface_config.width;
    let height = state.surface_config.height;
    let texture = state.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("substrate-demo intermediate"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        // Use the surface format so copy_texture_to_texture into the
        // surface texture is a straight bit copy (no format conversion
        // needed). Add STORAGE_BINDING so vello can compute into it.
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
            // Reconfigure and skip this frame.
            state.surface.configure(&state.device, &state.surface_config);
            state.target_texture = None;
            return;
        }
        CurrentSurfaceTexture::Timeout
        | CurrentSurfaceTexture::Occluded
        | CurrentSurfaceTexture::Validation => return,
    };

    // 1. Substrate dispatch → vello scene.
    let mut vello_scene = vello::Scene::new();
    let _report = state.host.render_scene(
        &state.scene,
        &mut vello_scene,
        &mut state.compositor,
        &mut state.renderer,
        state.window.scale_factor(),
    );

    // 2. Vello rasterize into intermediate texture.
    let target_view = state
        .target_view
        .as_ref()
        .expect("target view ensured above");
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
        .expect("vello render_to_texture");

    // 3. Blit intermediate → surface texture.
    let target_texture = state
        .target_texture
        .as_ref()
        .expect("target texture ensured above");
    let mut encoder = state
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("substrate-demo blit"),
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

    // 4. Present.
    state.window.pre_present_notify();
    surface_texture.present();
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("run_app");
}
