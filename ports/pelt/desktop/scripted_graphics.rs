/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pelt's same-device adapter from the script WebGL contract to webgl-wgpu.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use script_runtime_api::{WebGlFactory, WebGlHandler};
use webgl_wgpu::{
    BufferTarget, BufferUsage, PrimitiveMode, ShaderStage, WebGlBufferId, WebGlCanvasDescriptor,
    WebGlContext, WebGlError, WebGlProgramId, WebGlShaderId, WebGlTextureId, WebGlUniformLocation,
};

const COLOR_BUFFER_BIT: u32 = 0x4000;
const TRIANGLES: u32 = 0x0004;
const ARRAY_BUFFER: u32 = 0x8892;
const ELEMENT_ARRAY_BUFFER: u32 = 0x8893;
const VERTEX_SHADER: u32 = 0x8B31;
const FRAGMENT_SHADER: u32 = 0x8B30;
const DITHER: u32 = 0x0BD0;
const SCISSOR_TEST: u32 = 0x0C11;

#[derive(Clone, Default)]
pub(crate) struct WebGlTextureRegistry {
    textures: Arc<Mutex<HashMap<u64, wgpu::Texture>>>,
    next_key: Arc<AtomicU64>,
}

impl WebGlTextureRegistry {
    pub(crate) fn texture(&self, key: u64) -> Option<wgpu::Texture> {
        self.textures.lock().ok()?.get(&key).cloned()
    }
    pub(crate) fn factory(&self, device: wgpu::Device, queue: wgpu::Queue) -> WebGlFactory {
        let registry = self.clone();
        Box::new(move |width, height| {
            let texture_key = registry
                .next_key
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1);
            let context = WebGlContext::from_wgpu_handles(
                device.clone(),
                queue.clone(),
                WebGlCanvasDescriptor::new(width.max(1), height.max(1)),
            )
            .expect("Pelt WebGL context must use its already-booted wgpu device");
            registry
                .textures
                .lock()
                .expect("Pelt WebGL texture registry poisoned")
                .insert(texture_key, context.texture().texture.clone());
            Box::new(PeltWebGl::new(
                texture_key,
                context,
                registry.textures.clone(),
            ))
        })
    }
}

struct PeltWebGl {
    key: u64,
    registry: Arc<Mutex<HashMap<u64, wgpu::Texture>>>,
    context: WebGlContext,
    next: u64,
    buffers: HashMap<u64, WebGlBufferId>,
    shaders: HashMap<u64, WebGlShaderId>,
    programs: HashMap<u64, WebGlProgramId>,
    textures: HashMap<u64, WebGlTextureId>,
    uniforms: Vec<WebGlUniformLocation>,
    clear: [f32; 4],
    enabled: HashSet<u32>,
}
impl PeltWebGl {
    fn new(
        key: u64,
        context: WebGlContext,
        registry: Arc<Mutex<HashMap<u64, wgpu::Texture>>>,
    ) -> Self {
        let mut enabled = HashSet::new();
        enabled.insert(DITHER);
        Self {
            key,
            registry,
            context,
            next: 1,
            buffers: HashMap::new(),
            shaders: HashMap::new(),
            programs: HashMap::new(),
            textures: HashMap::new(),
            uniforms: vec![],
            clear: [0.0; 4],
            enabled,
        }
    }
    fn id(&mut self) -> u64 {
        let id = self.next;
        self.next += 1;
        id
    }
    fn target(v: u32) -> Option<BufferTarget> {
        match v {
            ARRAY_BUFFER => Some(BufferTarget::ArrayBuffer),
            ELEMENT_ARRAY_BUFFER => Some(BufferTarget::ElementArrayBuffer),
            _ => None,
        }
    }
}
impl Drop for PeltWebGl {
    fn drop(&mut self) {
        if let Ok(mut textures) = self.registry.lock() {
            textures.remove(&self.key);
        }
    }
}
impl WebGlHandler for PeltWebGl {
    fn external_texture_key(&self) -> Option<u64> {
        Some(self.key)
    }
    fn clear_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.clear = [r, g, b, a]
    }
    fn clear(&mut self, mask: u32) {
        if mask & COLOR_BUFFER_BIT != 0 {
            let c = self.clear;
            self.context.clear(wgpu::Color {
                r: c[0] as f64,
                g: c[1] as f64,
                b: c[2] as f64,
                a: c[3] as f64,
            })
        }
    }
    fn viewport(&mut self, x: i32, y: i32, width: u32, height: u32) {
        if x >= 0 && y >= 0 {
            self.context.viewport(x as u32, y as u32, width, height);
        }
    }
    fn enable(&mut self, cap: u32) {
        self.enabled.insert(cap);
        if cap == SCISSOR_TEST {
            self.context.set_scissor_test_enabled(true)
        }
    }
    fn disable(&mut self, cap: u32) {
        self.enabled.remove(&cap);
        if cap == SCISSOR_TEST {
            self.context.set_scissor_test_enabled(false)
        }
    }
    fn is_enabled(&mut self, cap: u32) -> bool {
        self.enabled.contains(&cap)
    }
    fn color_mask(&mut self, r: bool, g: bool, b: bool, a: bool) {
        self.context.set_color_mask(r, g, b, a)
    }
    fn create_buffer(&mut self) -> u64 {
        let id = self.id();
        let v = self.context.create_buffer();
        self.buffers.insert(id, v);
        id
    }
    fn bind_buffer(&mut self, target: u32, buffer: Option<u64>) {
        if let Some(t) = Self::target(target) {
            let v = buffer.and_then(|x| self.buffers.get(&x).copied());
            self.context.bind_buffer(t, v)
        }
    }
    fn buffer_data_f32(&mut self, target: u32, data: &[f32], _usage: u32) {
        if let Some(t) = Self::target(target) {
            self.context
                .buffer_data_f32(t, data, BufferUsage::StaticDraw)
        }
    }
    fn create_shader(&mut self, stage: u32) -> u64 {
        let s = match stage {
            VERTEX_SHADER => ShaderStage::Vertex,
            FRAGMENT_SHADER => ShaderStage::Fragment,
            _ => return 0,
        };
        let id = self.id();
        let v = self.context.create_shader(s);
        self.shaders.insert(id, v);
        id
    }
    fn shader_source(&mut self, id: u64, source: &str) {
        if let Some(&v) = self.shaders.get(&id) {
            self.context.shader_source(v, source)
        }
    }
    fn compile_shader(&mut self, id: u64) {
        if let Some(&v) = self.shaders.get(&id) {
            self.context.compile_shader(v)
        }
    }
    fn get_shader_compile_status(&mut self, id: u64) -> bool {
        self.shaders
            .get(&id)
            .is_some_and(|&v| self.context.get_shader_compile_status(v))
    }
    fn get_shader_info_log(&mut self, id: u64) -> String {
        self.shaders
            .get(&id)
            .and_then(|&v| self.context.get_shader_info_log(v))
            .unwrap_or_default()
    }
    fn create_program(&mut self) -> u64 {
        let id = self.id();
        let v = self.context.create_program();
        self.programs.insert(id, v);
        id
    }
    fn attach_shader(&mut self, p: u64, s: u64) {
        if let (Some(&p), Some(&s)) = (self.programs.get(&p), self.shaders.get(&s)) {
            self.context.attach_shader(p, s)
        }
    }
    fn link_program(&mut self, id: u64) {
        if let Some(&v) = self.programs.get(&id) {
            self.context.link_program(v)
        }
    }
    fn get_program_link_status(&mut self, id: u64) -> bool {
        self.programs
            .get(&id)
            .is_some_and(|&v| self.context.get_program_link_status(v))
    }
    fn get_program_info_log(&mut self, id: u64) -> String {
        self.programs
            .get(&id)
            .and_then(|&v| self.context.get_program_info_log(v))
            .unwrap_or_default()
    }
    fn use_program(&mut self, id: Option<u64>) {
        self.context
            .use_program(id.and_then(|x| self.programs.get(&x).copied()))
    }
    fn get_attrib_location(&mut self, p: u64, name: &str) -> i32 {
        self.programs
            .get(&p)
            .map_or(-1, |&v| self.context.get_attrib_location(v, name))
    }
    fn get_uniform_location(&mut self, p: u64, name: &str) -> i32 {
        let Some(v) = self
            .programs
            .get(&p)
            .and_then(|&v| self.context.get_uniform_location(v, name))
        else {
            return -1;
        };
        let id = self.uniforms.len() as i32;
        self.uniforms.push(v);
        id
    }
    fn enable_vertex_attrib_array(&mut self, i: u32) {
        self.context.enable_vertex_attrib_array(i)
    }
    fn vertex_attrib_pointer_f32(&mut self, i: u32, size: u32, n: bool, stride: u32, offset: u32) {
        self.context
            .vertex_attrib_pointer_f32(i, size, n, stride as u64, offset as u64)
    }
    fn uniform4f(&mut self, l: i32, x: f32, y: f32, z: f32, w: f32) {
        if let Some(&v) = usize::try_from(l).ok().and_then(|i| self.uniforms.get(i)) {
            self.context.uniform4f(v, x, y, z, w)
        }
    }
    fn uniform_matrix4fv(&mut self, l: i32, _t: bool, value: &[f32]) {
        if value.len() >= 16
            && let Some(&v) = usize::try_from(l).ok().and_then(|i| self.uniforms.get(i))
        {
            let mut m = [0.0; 16];
            m.copy_from_slice(&value[..16]);
            self.context.uniform_matrix4fv(v, &m)
        }
    }
    fn uniform1i(&mut self, l: i32, value: i32) {
        if let Some(&v) = usize::try_from(l).ok().and_then(|i| self.uniforms.get(i)) {
            self.context.uniform1i(v, value)
        }
    }
    fn create_texture(&mut self) -> u64 {
        let id = self.id();
        let v = self.context.create_texture();
        self.textures.insert(id, v);
        id
    }
    fn bind_texture_2d(&mut self, id: Option<u64>) {
        self.context
            .bind_texture_2d(id.and_then(|x| self.textures.get(&x).copied()))
    }
    fn active_texture(&mut self, u: u32) {
        self.context.active_texture(u)
    }
    fn tex_image_2d_rgba8(&mut self, w: u32, h: u32, p: &[u8]) {
        self.context.tex_image_2d_rgba8(w, h, p)
    }
    fn draw_arrays(&mut self, mode: u32, first: i32, count: i32) {
        if mode == TRIANGLES && first >= 0 && count >= 0 {
            self.context
                .draw_arrays(PrimitiveMode::Triangles, first as u32, count as u32)
        }
    }
    fn get_error(&mut self) -> u32 {
        match self.context.get_error() {
            WebGlError::NoError => 0,
            WebGlError::InvalidEnum => 0x0500,
            WebGlError::InvalidValue => 0x0501,
            WebGlError::InvalidOperation => 0x0502,
            WebGlError::InvalidFramebufferOperation => 0x0506,
            WebGlError::ContextLostWebgl => 0x9242,
        }
    }
    fn read_pixels_rgba8(&mut self, x: i32, y: i32, w: u32, h: u32) -> Vec<u8> {
        if x < 0 || y < 0 {
            return vec![0; (w * h * 4) as usize];
        }
        self.context
            .read_pixels(x as u32, y as u32, w, h)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::WebGlTextureRegistry;
    use genet_scripted::{LiveryScriptedDocument, ResourceFetcher, ScriptedDocumentOptions};
    use netrender::{ColorLoad, NetrenderOptions};

    #[derive(Clone, Copy)]
    struct EmptyResources;
    impl ResourceFetcher for EmptyResources {
        fn fetch(&self, _url: &str) -> Option<Vec<u8>> {
            None
        }
    }

    fn pixel(bytes: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * width + x) * 4) as usize;
        bytes[offset..offset + 4].try_into().unwrap()
    }

    #[test]
    fn ordinary_scripted_document_presents_and_retires_webgl_canvas() {
        const WIDTH: u32 = 48;
        const HEIGHT: u32 = 48;
        let core = genet_render_host::RenderCore::boot(NetrenderOptions {
            tile_cache_size: Some(16),
            enable_vello: true,
            ..Default::default()
        })
        .expect("shared render host boots");
        let registry = WebGlTextureRegistry::default();
        let html = r#"
            <style>
              html, body { margin: 0; padding: 0; background: white; }
              canvas { display: block; width: 32px; height: 32px; }
              #later { position: absolute; left: 12px; top: 12px;
                       width: 8px; height: 8px; background: blue; }
            </style>
            <canvas id="canvas" width="32" height="32"></canvas>
            <div id="later"></div>
            <script>
              const gl = document.getElementById('canvas').getContext('webgl');
              gl.clearColor(1, 0, 0, 1);
              gl.clear(gl.COLOR_BUFFER_BIT);
            </script>
        "#;
        let mut doc =
            LiveryScriptedDocument::<script_engine_boa::BoaEngine>::from_body_with_options(
                html,
                EmptyResources,
                "https://pelt.test/webgl-receipt.html",
                ScriptedDocumentOptions {
                    webgl: Some(registry.factory(core.device().clone(), core.queue().clone())),
                    ..Default::default()
                },
            )
            .expect("ordinary scripted document loads");

        let frame = doc.frame_with_external_textures(WIDTH, HEIGHT);
        assert_eq!(frame.external_textures.len(), 1);
        let draw = &frame.external_textures[0];
        assert_eq!(draw.dest_rect, [0.0, 0.0, 32.0, 32.0]);
        assert!(draw.scene_op_boundary < frame.scene.ops.len());
        let key = draw.texture_key;
        let textures: Vec<_> = frame
            .external_textures
            .iter()
            .map(|draw| {
                (
                    registry
                        .texture(draw.texture_key)
                        .expect("registered canvas texture"),
                    netrender::ExternalTexturePlacement::new(draw.dest_rect)
                        .with_opacity(draw.opacity),
                    draw.scene_op_boundary,
                )
            })
            .collect();
        let views: Vec<_> = textures
            .iter()
            .map(|(texture, placement, boundary)| {
                (
                    texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    *placement,
                    *boundary,
                )
            })
            .collect();
        let composites: Vec<_> = views
            .iter()
            .map(|(view, placement, boundary)| {
                netrender::ExternalTextureComposite::new(view, *placement)
                    .with_scene_op_boundary(*boundary)
            })
            .collect();
        let (texture, _) = core.rasterize_scaled_with_external_textures(
            &frame.scene,
            WIDTH,
            HEIGHT,
            ColorLoad::Clear(wgpu::Color::WHITE),
            1.0,
            &composites,
        );
        let rgba = core
            .read_rgba8_texture(&texture, WIDTH, HEIGHT)
            .expect("receipt texture reads back");
        assert_eq!(pixel(&rgba.rgba, WIDTH, 4, 4), [255, 0, 0, 255]);
        assert_eq!(pixel(&rgba.rgba, WIDTH, 16, 16), [0, 0, 255, 255]);
        assert_eq!(pixel(&rgba.rgba, WIDTH, 40, 40), [255, 255, 255, 255]);

        drop(doc);
        assert!(
            registry.texture(key).is_none(),
            "document drop retires its GPU texture"
        );
    }
}
