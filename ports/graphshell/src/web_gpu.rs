//! WebGPU surface and NetRender composition for the browser presenter.

use netrender::{ColorLoad, ExternalTexturePlacement, NetrenderOptions, Scene};
use web_sys::HtmlCanvasElement;

pub(crate) struct GpuPresenter {
    renderer: netrender::Renderer,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    format: wgpu::TextureFormat,
    config: wgpu::SurfaceConfiguration,
    blitter: wgpu::util::TextureBlitter,
}

impl GpuPresenter {
    pub(crate) async fn boot(
        canvas: HtmlCanvasElement,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        let handles = netrender_device::boot_async()
            .await
            .map_err(|error| format!("WebGPU boot failed: {error:?}"))?;
        let surface = handles
            .instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|error| format!("canvas surface failed: {error:?}"))?;
        let caps = surface.get_capabilities(&handles.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .ok_or("browser supplied no surface format")?;
        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .ok_or("browser supplied no surface alpha mode")?;
        let device = handles.device.clone();
        let queue = handles.queue.clone();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        let renderer = netrender::create_netrender_instance(
            handles,
            NetrenderOptions {
                tile_cache_size: Some(1024),
                enable_vello: true,
                ..Default::default()
            },
        )
        .map_err(|error| format!("NetRender initialization failed: {error:?}"))?;
        let blitter = wgpu::util::TextureBlitter::new(&device, format);
        Ok(Self {
            renderer,
            device,
            queue,
            surface,
            format,
            config,
            blitter,
        })
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    fn target(
        &self,
        label: &'static str,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    pub(crate) fn present(
        &self,
        content: &Scene,
        chrome: &Scene,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let (_content_texture, content_view) = self.target("graphshell content", width, height);
        self.renderer.render_vello_scaled_for(
            1,
            content,
            &content_view,
            ColorLoad::Clear(wgpu::Color {
                r: 0.027,
                g: 0.047,
                b: 0.059,
                a: 1.0,
            }),
            1.0,
        );
        let (_chrome_texture, chrome_view) = self.target("graphshell chrome", width, height);
        self.renderer.render_vello_scaled_for(
            2,
            chrome,
            &chrome_view,
            ColorLoad::Clear(wgpu::Color::TRANSPARENT),
            1.0,
        );
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            other => return Err(format!("surface acquisition failed: {other:?}")),
        };
        let frame_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("graphshell content blit"),
            });
        self.blitter
            .copy(&self.device, &mut encoder, &content_view, &frame_view);
        self.queue.submit([encoder.finish()]);
        self.renderer.compose_external_texture(
            &chrome_view,
            &frame_view,
            self.format,
            width,
            height,
            ExternalTexturePlacement::new([0.0, 0.0, width as f32, height as f32]),
        );
        frame.present();
        Ok(())
    }
}
