/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-neutral viewer-surface state types.
//!
//! Step 1 of the viewer_surfaces extraction (per the 2026-04-25
//! servo-into-verso plan): the lifecycle state — `ContentSurfaceHandle`
//! (parameterized over the host's texture-token type), `ViewerSurfaceFramePath`,
//! and the `content_generation: u64` counter — lives here. Each host (egui /
//! iced) instantiates the handle with its own texture-token vocabulary, but the
//! enum shape and the frame-path taxonomy are the same.
//!
//! Step 2 (later) will introduce a portable `RenderingContextProducer` trait so
//! that `ViewerSurfaceBacking`'s host-neutral parts can join the runtime crate.
//! Today the backing remains shell-side because it references Servo's
//! `RenderingContextCore` and `OffscreenRenderingContext`.

/// What the compositor displays for a given node, parameterized over the
/// host's texture-token type.
///
/// - `ImportedWgpu(token)` — the content engine produced or imported a
///   wgpu-compatible host texture identified by `token`.
/// - `CallbackFallback` — a paint callback is registered for this pass (the
///   named legacy callback/GL-compat path).
/// - `Placeholder` — no usable surface yet (loading, runtime not ready, or
///   lifecycle Cold).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentSurfaceHandle<TextureToken> {
    ImportedWgpu(TextureToken),
    CallbackFallback,
    Placeholder,
}

impl<TextureToken> ContentSurfaceHandle<TextureToken> {
    pub const fn is_wgpu(&self) -> bool {
        matches!(self, Self::ImportedWgpu(_))
    }
}

/// The viewer-surface/content-bridge path the compositor actually exercised on
/// the current frame. Used for diagnostics and parity work to pin which path a
/// host took frame-to-frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewerSurfaceFramePath {
    SharedWgpuImported,
    CallbackFallback,
    MissingSurface,
}

#[cfg(test)]
mod tests {
    use super::{ContentSurfaceHandle, ViewerSurfaceFramePath};

    #[test]
    fn is_wgpu_only_true_for_imported_variant() {
        let imported: ContentSurfaceHandle<u32> = ContentSurfaceHandle::ImportedWgpu(7);
        let callback: ContentSurfaceHandle<u32> = ContentSurfaceHandle::CallbackFallback;
        let placeholder: ContentSurfaceHandle<u32> = ContentSurfaceHandle::Placeholder;

        assert!(imported.is_wgpu());
        assert!(!callback.is_wgpu());
        assert!(!placeholder.is_wgpu());
    }

    #[test]
    fn frame_path_variants_are_distinct() {
        assert_ne!(
            ViewerSurfaceFramePath::SharedWgpuImported,
            ViewerSurfaceFramePath::CallbackFallback,
        );
        assert_ne!(
            ViewerSurfaceFramePath::CallbackFallback,
            ViewerSurfaceFramePath::MissingSurface,
        );
    }
}
