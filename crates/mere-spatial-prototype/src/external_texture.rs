// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `ExternalTextureCompositor` — host-side composite path for renderers
//! whose composition mode is `EmbeddedFrame`.
//!
//! Built atop vello 0.9's `Renderer::register_texture` + `Scene::draw_image`,
//! which lets a wgpu texture produced by another subsystem participate as
//! an image source in a vello scene without CPU upload. Same-device only —
//! cross-device producers (scrying's DX12-shared, Metal IOSurface, dmabuf)
//! must be imported into the host's device first (scrying's `native_frame`
//! module does this; this compositor is downstream of import).
//!
//! ## Composition with the renderer registry
//!
//! A renderer whose `composition_mode() == CompositionMode::EmbeddedFrame`
//! produces a `wgpu::Texture` each frame (or holds a long-lived one and
//! redraws into it). The substrate's per-frame loop:
//!
//! 1. Calls the renderer's `next_frame(handle)` → `Option<wgpu::Texture>`.
//! 2. Hands the texture to [`ExternalTextureCompositor::register`] keyed by
//!    the node's `NodeIdentity` (no-op if already registered).
//! 3. Calls [`ExternalTextureCompositor::compose`] to emit the draw_image
//!    op into the host scene at the node's placement transform.
//! 4. On node removal, calls [`ExternalTextureCompositor::unregister`].
//!
//! ## Path A vs path B
//!
//! This compositor implements *path A* from the renderer-registry contract
//! brief: vello samples the producer texture directly during its compute
//! pass; the draw_image op interleaves correctly with other scene ops
//! (clipping, blending, z-order all just work).
//!
//! *Path B* (netrender's separate-pass external_texture pipeline) is for
//! cases vello can't sample directly — different format, different device,
//! sync requirements vello doesn't model. Path B isn't built here; the
//! compositor exposes `wgpu::Texture` cleanly enough that adding it later
//! is mechanical.

use std::collections::HashMap;

use kurbo::{Affine, Size};
use mere_renderer_registry::NodeIdentity;
use vello::peniko;

/// Caches the `peniko::ImageData` handles vello returns from
/// `register_texture`, keyed by the node identity that owns the texture.
///
/// Lives for the host's vello::Renderer lifetime. Drops without an
/// explicit `unregister_all` simply leak the vello-side overrides until
/// the renderer is dropped; that's safe but wasteful for long-lived
/// hosts that churn nodes.
#[derive(Default)]
pub struct ExternalTextureCompositor {
    images: HashMap<NodeIdentity, peniko::ImageData>,
}

impl ExternalTextureCompositor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a node's external texture with the host's vello renderer.
    ///
    /// No-op if the node already has a registered texture (caller is
    /// expected to call [`Self::unregister`] before re-registering with
    /// a different texture; the producer typically keeps the same wgpu
    /// texture across frames and just redraws into it).
    ///
    /// Returns `true` if a new registration happened, `false` if the key
    /// was already present.
    pub fn register(
        &mut self,
        renderer: &mut vello::Renderer,
        key: NodeIdentity,
        texture: wgpu::Texture,
    ) -> bool {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.images.entry(key) {
            entry.insert(renderer.register_texture(texture));
            true
        } else {
            false
        }
    }

    /// Drop a node's registration. Returns `true` if the node was
    /// registered (and is now no longer), `false` if it wasn't.
    pub fn unregister(&mut self, renderer: &mut vello::Renderer, key: NodeIdentity) -> bool {
        if let Some(image) = self.images.remove(&key) {
            renderer.unregister_texture(image);
            true
        } else {
            false
        }
    }

    /// Drop every registration. Call at host shutdown for clean teardown.
    pub fn unregister_all(&mut self, renderer: &mut vello::Renderer) {
        for (_, image) in self.images.drain() {
            renderer.unregister_texture(image);
        }
    }

    /// True if `key` has a registered texture.
    pub fn is_registered(&self, key: NodeIdentity) -> bool {
        self.images.contains_key(&key)
    }

    /// Count of currently-registered textures.
    pub fn len(&self) -> usize {
        self.images.len()
    }

    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// Emit a draw_image op into `scene` for the node's registered texture,
    /// scaled to `size` and placed at `placement`.
    ///
    /// `placement` is the same affine the InScene path receives from
    /// `PaintCtx::node_transform`; `size` is the node's logical size from
    /// `SceneNodeRef::size`. The compositor composes `placement *
    /// scale_non_uniform(size.w / tex.w, size.h / tex.h)` so the texture
    /// fills the node regardless of its native resolution.
    pub fn compose(
        &self,
        scene: &mut vello::Scene,
        key: NodeIdentity,
        placement: Affine,
        size: Size,
    ) -> Result<(), CompositorError> {
        let image = self
            .images
            .get(&key)
            .ok_or(CompositorError::NotRegistered(key))?;
        if image.width == 0 || image.height == 0 {
            return Err(CompositorError::ZeroSizedTexture(key));
        }
        if size.width <= 0.0 || size.height <= 0.0 {
            return Err(CompositorError::ZeroSizedNode(key));
        }
        let scale = Affine::scale_non_uniform(
            size.width / image.width as f64,
            size.height / image.height as f64,
        );
        scene.draw_image(image, placement * scale);
        Ok(())
    }
}

/// Why a compose call failed.
#[derive(Debug)]
pub enum CompositorError {
    /// No texture is registered for this node — host called `compose`
    /// before `register` (or after `unregister`).
    NotRegistered(NodeIdentity),
    /// The registered texture reports zero width or height. Producer
    /// bug — vello's `register_texture` reads dimensions from the
    /// wgpu::Texture directly, so this only fires if the producer
    /// created a degenerate texture.
    ZeroSizedTexture(NodeIdentity),
    /// The node's scene-local size is zero or negative. Caller bug.
    ZeroSizedNode(NodeIdentity),
}

impl std::fmt::Display for CompositorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRegistered(id) => write!(f, "node {} has no registered external texture", id.as_u64()),
            Self::ZeroSizedTexture(id) => write!(f, "node {} texture has zero dimension", id.as_u64()),
            Self::ZeroSizedNode(id) => write!(f, "node {} has non-positive size", id.as_u64()),
        }
    }
}

impl std::error::Error for CompositorError {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vello::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

    use super::*;

    /// Build a fake ImageData for testing the compose path without a real
    /// vello::Renderer. Matches the shape vello's register_texture would
    /// return for a `width × height` Rgba8 texture (empty Blob; vello's
    /// override map resolves it back to the registered wgpu::Texture at
    /// render time, so the Blob being empty is fine in production too).
    fn fake_image(width: u32, height: u32) -> ImageData {
        let blob = Blob::new(Arc::new(&[][..]));
        ImageData {
            data: blob,
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width,
            height,
        }
    }

    fn inject(compositor: &mut ExternalTextureCompositor, key: NodeIdentity, image: ImageData) {
        compositor.images.insert(key, image);
    }

    #[test]
    fn new_compositor_is_empty() {
        let c = ExternalTextureCompositor::new();
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn is_registered_after_inject() {
        let mut c = ExternalTextureCompositor::new();
        let key = NodeIdentity::next();
        inject(&mut c, key, fake_image(128, 64));
        assert!(c.is_registered(key));
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn compose_unregistered_returns_not_registered() {
        let c = ExternalTextureCompositor::new();
        let key = NodeIdentity::next();
        let mut scene = vello::Scene::new();
        let err = c
            .compose(&mut scene, key, Affine::IDENTITY, Size::new(100.0, 100.0))
            .unwrap_err();
        assert!(matches!(err, CompositorError::NotRegistered(k) if k == key));
    }

    #[test]
    fn compose_zero_sized_node_returns_error() {
        let mut c = ExternalTextureCompositor::new();
        let key = NodeIdentity::next();
        inject(&mut c, key, fake_image(64, 64));
        let mut scene = vello::Scene::new();
        let err = c
            .compose(&mut scene, key, Affine::IDENTITY, Size::new(0.0, 100.0))
            .unwrap_err();
        assert!(matches!(err, CompositorError::ZeroSizedNode(k) if k == key));
    }

    #[test]
    fn compose_zero_sized_texture_returns_error() {
        let mut c = ExternalTextureCompositor::new();
        let key = NodeIdentity::next();
        inject(&mut c, key, fake_image(0, 64));
        let mut scene = vello::Scene::new();
        let err = c
            .compose(&mut scene, key, Affine::IDENTITY, Size::new(100.0, 100.0))
            .unwrap_err();
        assert!(matches!(err, CompositorError::ZeroSizedTexture(k) if k == key));
    }

    #[test]
    fn compose_registered_succeeds() {
        let mut c = ExternalTextureCompositor::new();
        let key = NodeIdentity::next();
        inject(&mut c, key, fake_image(64, 64));
        let mut scene = vello::Scene::new();
        // We can't easily introspect what scene.draw_image recorded, but
        // we can verify compose() returns Ok without panicking; downstream
        // GPU tests verify the visual outcome.
        let result = c.compose(
            &mut scene,
            key,
            Affine::translate((10.0, 20.0)),
            Size::new(128.0, 128.0),
        );
        assert!(result.is_ok());
    }
}
