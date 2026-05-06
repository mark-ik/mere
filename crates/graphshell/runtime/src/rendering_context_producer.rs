/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-neutral viewer-surface rendering-context contract.
//!
//! Step 2 of the viewer_surfaces extraction (per the 2026-04-25 servo-into-verso
//! plan). The shell's compositor consumes a small slice of Servo's
//! `RenderingContextCore` from `ViewerSurfaceBacking::NativeRenderingContext`:
//! query/adjust the rendered area's pixel size and present finalized frames.
//! That is a wgpu-first operation set; nothing here requires a GL context.
//!
//! `RenderingContextProducer` exposes that surface using only primitive types
//! so it can ship from `graphshell-runtime` without dragging Servo-only
//! dependencies (`embedder_traits::RefreshDriver`, `webrender_api::units`) or
//! GL-ecosystem dependencies (`surfman`, `gleam`, `glow`) into the runtime
//! crate.
//!
//! ## Wgpu-first scoping
//!
//! Earlier drafts considered including GL `make_current` / `prepare_for_rendering`
//! in the trait. They were dropped: graphshell is wgpu-only (Servo lives at the
//! `servo-wgpu` fork; renderer is `webrender-wgpu`). A wgpu-first producer
//! (Servo's `SharedWgpuRenderingContext` today or iced-host's eventual native
//! producer) has no notion of "make current".

/// Host-neutral viewer-surface rendering-context contract.
///
/// Implementors describe a single viewer surface — the per-node texture/wgpu
/// surface the compositor renders into and presents. The trait deliberately
/// omits readback, GL/wgpu capability sub-traits, and window/refresh handles:
/// those stay on whichever concrete type the host-side adapter wraps.
pub trait RenderingContextProducer {
    /// Current size of the rendered area, in device pixels.
    fn size_in_pixels(&self) -> (u32, u32);

    /// Resize the rendered area to `width` × `height` device pixels. Idempotent
    /// when the new size matches the current size; implementations are expected
    /// to short-circuit in that case.
    fn resize(&self, width: u32, height: u32);

    /// Finalize the current frame (swapchain present, texture commit, etc.).
    fn present(&self);
}

#[cfg(test)]
mod tests {
    use super::RenderingContextProducer;
    use std::cell::Cell;

    /// Minimal mock used to verify the trait surface is honestly callable from
    /// portable code (no Servo / GL toolchain coupling).
    struct MockProducer {
        size: Cell<(u32, u32)>,
        present_count: Cell<u32>,
    }

    impl MockProducer {
        fn new(width: u32, height: u32) -> Self {
            Self {
                size: Cell::new((width, height)),
                present_count: Cell::new(0),
            }
        }
    }

    impl RenderingContextProducer for MockProducer {
        fn size_in_pixels(&self) -> (u32, u32) {
            self.size.get()
        }

        fn resize(&self, width: u32, height: u32) {
            self.size.set((width, height));
        }

        fn present(&self) {
            self.present_count.set(self.present_count.get() + 1);
        }
    }

    #[test]
    fn resize_is_observed_on_subsequent_size_query() {
        let producer = MockProducer::new(640, 480);
        assert_eq!(producer.size_in_pixels(), (640, 480));
        producer.resize(800, 600);
        assert_eq!(producer.size_in_pixels(), (800, 600));
    }

    #[test]
    fn present_records_frame_finalization() {
        let producer = MockProducer::new(1, 1);
        producer.present();
        producer.present();
        assert_eq!(producer.present_count.get(), 2);
    }

    #[test]
    fn trait_is_object_safe() {
        let producer: Box<dyn RenderingContextProducer> = Box::new(MockProducer::new(2, 2));
        assert_eq!(producer.size_in_pixels(), (2, 2));
        producer.resize(4, 4);
        assert_eq!(producer.size_in_pixels(), (4, 4));
    }
}
