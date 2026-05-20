// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `PanelTile` — the object-safe interface the embedded-frame renderer
//! drives per producer.
//!
//! Two implementors:
//!
//! - [`crate::MasonryTile`] — a plain masonry composition root built
//!   once from a `NewWidget`. `tick` is a no-op (no reactive layer).
//! - [`crate::XilemPanel`] — a masonry tile driven by a xilem
//!   `View<State, (), ViewCtx>`; `tick` re-runs the panel's logic
//!   against the latest state and rebuilds the widget tree in place.
//!
//! The renderer holds `Box<dyn PanelTile>` so it doesn't need to be
//! generic over panel state — reactive panels keep their `State` and
//! view machinery internal.

use accesskit::TreeUpdate;
use masonry_core::app::VisualLayerPlan;
use masonry_core::core::{PointerEvent, TextEvent};

use crate::signal::TileSignal;
use crate::tile::{MasonryTile, TileSize};

/// Per-producer panel interface: a masonry tile the renderer drives
/// each frame (optional reactive update → render → layer extraction →
/// input → a11y).
pub trait PanelTile {
    /// Per-frame reactive update. Plain tiles no-op; reactive tiles
    /// re-run their logic against current state and rebuild the widget
    /// tree. Called before [`render_layers`](Self::render_layers).
    fn tick(&mut self);

    /// Drive masonry's render pass and stash the visual-layer plan +
    /// AccessKit tree update.
    fn render_layers(&mut self);

    /// The most recent visual-layer plan (set by
    /// [`render_layers`](Self::render_layers)). The renderer extracts
    /// base + overlay scenes from this for the GPU texture path.
    fn pending_layers(&self) -> Option<&VisualLayerPlan>;

    /// Take the pending AccessKit tree update, if any.
    fn take_accesskit_update(&mut self) -> Option<TreeUpdate>;

    /// Forward a substrate-translated pointer event.
    fn handle_pointer(&mut self, event: PointerEvent);

    /// Forward a substrate-translated text / IME event.
    fn handle_text(&mut self, event: TextEvent);

    /// Resize the tile.
    fn resize(&mut self, size: TileSize);

    /// Drain substrate-shaped signals collected since the last call.
    fn drain_signals(&mut self) -> Vec<TileSignal>;
}

impl PanelTile for MasonryTile {
    fn tick(&mut self) {
        // Plain masonry tile: no reactive layer, nothing to recompute.
    }

    fn render_layers(&mut self) {
        self.redraw_layers();
    }

    fn pending_layers(&self) -> Option<&VisualLayerPlan> {
        MasonryTile::pending_layers(self)
    }

    fn take_accesskit_update(&mut self) -> Option<TreeUpdate> {
        MasonryTile::take_accesskit_update(self)
    }

    fn handle_pointer(&mut self, event: PointerEvent) {
        MasonryTile::handle_pointer(self, event);
    }

    fn handle_text(&mut self, event: TextEvent) {
        MasonryTile::handle_text(self, event);
    }

    fn resize(&mut self, size: TileSize) {
        MasonryTile::resize(self, size);
    }

    fn drain_signals(&mut self) -> Vec<TileSignal> {
        MasonryTile::drain_signals(self)
    }
}
