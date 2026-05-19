// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `NodeRenderer` + `InScenePaintRenderer` impl for [`MasonryTile`].
//!
//! This is the seam that registers `mere-masonry` against the
//! [`register_renderer`] dispatch model. The substrate selects this
//! renderer for any node whose `content_kind == NodeContentKind::Panel`,
//! resolves to a per-tile [`MasonryTile`] (typically held inside a wrapper
//! that owns the `(NodeIdentity → MasonryTile)` map), and dispatches paint /
//! input through the trait methods below.
//!
//! ## Note on the impl shape
//!
//! [`MasonryTile`] is one tile's composition root — it has no awareness of
//! "which node am I." The substrate's per-frame dispatch pairs the
//! [`SceneNodeRef`] with the right tile (via `node.identity` lookup); by the
//! time `paint(...)` / `input(...)` is called, the `&mut self` IS the tile
//! for that node.
//!
//! For v0 this means `mere-masonry` consumers will register *one*
//! `MasonryTile` per node (the host owns the lifecycle), and the
//! `InScenePaintRenderer` impl is implemented directly on `MasonryTile`. A
//! later refactor may introduce a `MasonryFleet` type that holds many tiles
//! and implements `InScenePaintRenderer` once for all of them — that's a
//! v1 ergonomic, not a v0 requirement.

use register_renderer::{
    CompositionMode, InScenePaintRenderer, InputDisposition, InputEvent, NodeContentKind,
    NodeContentKindSet, NodeRenderer, PaintCtx, PaintResult, RendererCapabilities, RendererId,
    SceneNodeRef,
};

use crate::input::{translate_to_masonry_pointer, translate_to_masonry_text};
use crate::tile::MasonryTile;

/// Stable renderer ID for `mere-masonry`.
pub const RENDERER_ID: &str = "mere-masonry";

impl NodeRenderer for MasonryTile {
    fn renderer_id(&self) -> RendererId {
        RendererId::from_static(RENDERER_ID)
    }

    fn handles(&self) -> NodeContentKindSet {
        NodeContentKindSet::from_one(NodeContentKind::Panel)
    }

    fn composition_mode(&self) -> CompositionMode {
        CompositionMode::InScenePaint
    }

    fn capabilities(&self) -> RendererCapabilities {
        // Interactive panel: input + IME + a11y + scroll + sub-region hit-test
        // + capture. No profile binding (panels have no persistent engine
        // state).
        RendererCapabilities::INTERACTIVE_PANEL
    }
}

impl InScenePaintRenderer for MasonryTile {
    fn paint(&mut self, _node: &SceneNodeRef, ctx: &mut PaintCtx<'_>) -> PaintResult {
        // Drive masonry's render passes; capture visual layers + AccessKit
        // tree update internally. The final scene-merge step (visual layers
        // → ctx.scene at ctx.node_transform) is documented in
        // `MasonryTile::render` as a deferred decision (custom PaintSink vs
        // texture round-trip). Until decided, `paint` runs masonry's pass
        // chain and returns Ok — the host will see no pixels for masonry
        // tiles, but accessibility + signals + layer inspection all work.
        self.render(ctx.scene, ctx.node_transform);
        Ok(())
    }

    fn input(&mut self, _node: &SceneNodeRef, event: &InputEvent) -> InputDisposition {
        // Three text-shaped event classes (Key, Text, Ime, Focus) all collapse
        // into masonry's TextEvent vocabulary; pointer events are their own
        // class. Dispatch accordingly.
        if let Some(text_event) = translate_to_masonry_text(event) {
            self.handle_text(text_event);
            return InputDisposition::Consumed;
        }
        if let Some(pointer_event) = translate_to_masonry_pointer(event) {
            self.handle_pointer(pointer_event);
            return InputDisposition::Consumed;
        }
        // Event class wasn't a tile-routable text or pointer event.
        InputDisposition::Passthrough
    }
}
