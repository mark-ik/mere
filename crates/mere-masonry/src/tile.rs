// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `MasonryTile` — one masonry composition root, scoped to a single substrate tile.
//!
//! Owns a [`masonry_core::app::RenderRoot`]; receives substrate-driven
//! events; emits substrate-shaped signals via [`drain_signals`](MasonryTile::drain_signals).
//! No winit, no event loop, no wgpu device — those are the substrate's
//! concerns.

use std::cell::RefCell;
use std::rc::Rc;

use accesskit::TreeUpdate;
use dpi::PhysicalSize;
use masonry_core::app::{
    RenderRoot, RenderRootOptions, RenderRootSignal, VisualLayerPlan, WindowSizePolicy,
};
use masonry_core::core::{
    DefaultProperties, NewWidget, PointerEvent, TextEvent, Widget, WindowEvent,
};
use kurbo::Affine;
use std::sync::Arc;

use crate::signal::TileSignal;

/// Newtype for tile-local physical size, in physical pixels.
///
/// The substrate hands this in per frame (or whenever the tile resizes); the
/// rect is in *tile-local* coordinates. The substrate's [`crate::MasonryTile::render`]
/// takes a separate `Affine` that places the tile within the host scene.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct TileSize(pub PhysicalSize<u32>);

/// One masonry-rendered tile in Mere's substrate.
///
/// Lifecycle:
///
/// 1. Construct with [`MasonryTile::new`], passing the initial size and the
///    panel's root widget.
/// 2. Each frame: feed substrate events via [`handle_pointer`](Self::handle_pointer)
///    / [`handle_text`](Self::handle_text) / etc; call
///    [`render`](Self::render) to paint into the host scene; drain
///    [`signals`](Self::drain_signals) and route each one substrate-side.
/// 3. On resize: call [`resize`](Self::resize).
/// 4. On removal: drop the `MasonryTile`. masonry's `RenderRoot::Drop` cleans
///    up the widget tree; cached scenes are released by the imaging layer.
pub struct MasonryTile {
    render_root: RenderRoot,

    /// Pending signals collected through `RenderRoot`'s signal sink.
    ///
    /// `RefCell` because the sink callback (created at `RenderRoot::new` time)
    /// captures this; we drain via `drain_signals()`.
    pending: Rc<RefCell<Vec<TileSignal>>>,

    /// AccessKit tree update produced by the most recent `render()` call.
    /// Drained by `take_accesskit_update()`.
    pending_tree_update: Option<TreeUpdate>,

    /// Visual layer plan from the most recent `render()` call. Held for
    /// inspection / scene-merge work; see `render()` for the deferred
    /// integration question.
    pending_layers: Option<VisualLayerPlan>,
}

impl MasonryTile {
    /// Construct a new tile.
    ///
    /// `default_properties` is the masonry property defaults (font, text colour,
    /// etc.) — typically host-wide and shared across tiles.
    /// `root_widget` is the masonry widget that takes up the tile's full area.
    /// `initial_size` is the tile's physical size; resize via [`resize`](Self::resize) later.
    /// `scale_factor` is the host's HiDPI scale (typically `1.0`, `1.5`, `2.0`).
    pub fn new(
        default_properties: Arc<DefaultProperties>,
        root_widget: NewWidget<dyn Widget>,
        initial_size: TileSize,
        scale_factor: f64,
    ) -> Self {
        let pending: Rc<RefCell<Vec<TileSignal>>> = Rc::new(RefCell::new(Vec::new()));

        // Sink callback collects signals; substrate drains via `drain_signals`.
        // Translate per-signal into the substrate vocabulary at sink time so
        // the host doesn't need to import masonry types.
        let sink_pending = pending.clone();
        let sink = move |signal: RenderRootSignal| {
            sink_pending.borrow_mut().push(TileSignal::from_render_root_signal(signal));
        };

        let options = RenderRootOptions {
            default_properties,
            use_system_fonts: true,
            size_policy: WindowSizePolicy::User, // substrate sizes us
            size: initial_size.0,
            scale_factor,
            test_font: None,
        };

        let render_root = RenderRoot::new(root_widget, sink, options);

        Self {
            render_root,
            pending,
            pending_tree_update: None,
            pending_layers: None,
        }
    }

    /// Resize the tile.
    ///
    /// Called by the substrate when the tile's bounding rect changes (window
    /// resize, layout reflow, etc.). Triggers masonry to re-run layout +
    /// paint on the next frame.
    pub fn resize(&mut self, size: TileSize) {
        self.render_root
            .handle_window_event(WindowEvent::Resize(size.0));
    }

    /// Forward a substrate-translated pointer event into masonry.
    ///
    /// The substrate's input router does the spatial hit-test (substrate
    /// scene-graph → tile id) and the coordinate mapping (host coords →
    /// tile-local coords) before calling here. By the time this is called,
    /// the [`PointerEvent`] is in masonry's vocabulary.
    pub fn handle_pointer(&mut self, event: PointerEvent) {
        let _handled = self.render_root.handle_pointer_event(event);
        // `_handled` will become a `bool` exposed from `handle_pointer` once
        // the substrate input router cares about it for shadowing decisions.
    }

    /// Forward a text event (typed key, IME composition, paste).
    pub fn handle_text(&mut self, event: TextEvent) {
        let _handled = self.render_root.handle_text_event(event);
    }

    /// Forward a window-level event (focus changes, theme changes).
    ///
    /// Most window events are substrate concerns and don't reach the tile;
    /// the ones that do are e.g. `WindowEvent::FocusChanged(false)` when the
    /// OS window loses focus, or `WindowEvent::ThemeChanged` when the user
    /// flips dark/light mode.
    pub fn handle_window_event(&mut self, event: WindowEvent) {
        let _handled = self.render_root.handle_window_event(event);
    }

    /// Forward an AccessKit action (assistive-tech driven).
    pub fn handle_access_event(&mut self, event: accesskit::ActionRequest) {
        self.render_root.handle_access_event(event);
    }

    /// Run masonry's render pass; capture the visual layers and AccessKit
    /// tree update for downstream consumers.
    ///
    /// **What's wired (this turn)**: `RenderRoot::redraw()` runs the rewrite
    /// passes (event / update / layout / paint) and produces a
    /// `(VisualLayerPlan, Option<TreeUpdate>)`. Both are stashed: the tree
    /// update for [`take_accesskit_update`](Self::take_accesskit_update),
    /// the layers for inspection.
    ///
    /// **What's not wired (next decision)**: turning the `VisualLayerPlan`'s
    /// `imaging::record::Scene` content into vello scene operations
    /// composited into `target_scene` at `tile_transform`. Two paths under
    /// consideration:
    ///
    /// - **(a) Custom `PaintSink`**: implement `masonry_core::imaging::PaintSink`
    ///   over a `vello::Scene` and call
    ///   [`VisualLayerPlan::replay_into`](masonry_core::app::VisualLayerPlan::replay_into)
    ///   into it. Zero texture round-trip; true in-scene-paint composition;
    ///   matches the contract brief's Option A. Bigger surface to write but
    ///   correct shape.
    /// - **(b) Texture round-trip**: use `masonry_imaging::vello::Renderer`
    ///   to render the layers into a wgpu texture (substrate provides the
    ///   `wgpu::Device` + `Queue`), then composite the texture into
    ///   `target_scene` as a textured quad. Reuses Linebender's well-tested
    ///   render path; effectively makes mere-masonry behave like an
    ///   `EmbeddedFrame` renderer at the implementation layer (even though
    ///   it's declared `InScenePaint`).
    ///
    /// (a) is architecturally cleaner; (b) is faster to write and gets us to
    /// pixels on screen sooner. Decision lives outside this `todo!()` —
    /// pick when the substrate's wgpu device handle is real.
    ///
    /// **⚠ Trap before naive-implementing path (a)**: this crate's dependency
    /// tree contains **two distinct vello versions** — `vello 0.9` (direct
    /// dep, what `target_scene` here is) and `vello 0.8.x` (transitive via
    /// `masonry_imaging::imaging_vello`, what masonry's `PaintSink` expects).
    /// They are different crates from Rust's point of view; their `Scene`
    /// types are not interchangeable. See the crate-level docs (`lib.rs`
    /// "Two-vello version skew" section) before writing path (a) — the
    /// preferred resolution is to wait for xilem's vello 0.9 bump, which
    /// dissolves the skew. Path (b) sidesteps it because the wgpu texture
    /// handle is the version-neutral bridge.
    pub fn render(&mut self, target_scene: &mut vello::Scene, tile_transform: Affine) {
        let (visual_layers, tree_update) = self.render_root.redraw();
        if let Some(update) = tree_update {
            self.pending_tree_update = Some(update);
        }
        self.pending_layers = Some(visual_layers);
        // TODO(scene-merge): see doc comment above. Until decided, the
        // host won't see masonry pixels in the substrate scene; the layers
        // and tree update ARE captured for tests / accessibility tree
        // routing, which is the next-most-important consumer.
        let _ = (target_scene, tile_transform);
    }

    /// Drain pending signals collected since the last call.
    ///
    /// The host calls this once per frame after render; routes each signal
    /// to the appropriate substrate sink (action bus, IME bridge, cursor
    /// manager, etc.).
    pub fn drain_signals(&mut self) -> Vec<TileSignal> {
        std::mem::take(&mut *self.pending.borrow_mut())
    }

    /// Take the pending AccessKit `TreeUpdate`, if any.
    ///
    /// Populated by [`render`](Self::render) (`RenderRoot::redraw()` returns
    /// a `(VisualLayerPlan, Option<TreeUpdate>)` pair; we store the second
    /// half for the host to drain). The host hands this to `uxtree`'s
    /// AccessKit consumer to merge into Mere's accessibility tree.
    pub fn take_accesskit_update(&mut self) -> Option<TreeUpdate> {
        self.pending_tree_update.take()
    }

    /// Inspect (without taking) the most recent visual layer plan.
    ///
    /// Useful for tests verifying that masonry produced the expected layer
    /// structure. Production code should generally let `render` consume the
    /// layers internally.
    pub fn pending_layers(&self) -> Option<&VisualLayerPlan> {
        self.pending_layers.as_ref()
    }
}

// MasonryTile is intentionally !Send and !Sync because RenderRoot uses
// Rc-typed internal state and isn't thread-safe. The substrate's renderer
// dispatch must happen on the host's UI thread; this matches gpui's existing
// constraint.
