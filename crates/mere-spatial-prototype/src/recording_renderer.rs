// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `RecordingRenderer` — built-in in-scene-paint renderer that records
//! what it was asked to paint / handle input for.
//!
//! Used by this crate's own tests and reusable as a stub in downstream
//! tests. Paints nothing into the target scene — recording the call is
//! the point.

use std::cell::RefCell;
use std::rc::Rc;

use kurbo::Affine;
use mere_renderer_registry::{
    CompositionMode, InScenePaintRenderer, InputDisposition, InputEvent, NodeContentKind,
    NodeContentKindSet, NodeIdentity, NodeRenderer, PaintCtx, PaintResult,
    RendererCapabilities, RendererId, SceneNodeRef,
};

/// One paint() call's metadata, recorded.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintRecord {
    pub identity: NodeIdentity,
    pub node_transform: Affine,
    pub scale_factor: f64,
}

/// A renderer that records every paint and input call it receives.
pub struct RecordingRenderer {
    id: RendererId,
    handles: NodeContentKindSet,
    paint_log: Rc<RefCell<Vec<PaintRecord>>>,
    input_log: Rc<RefCell<Vec<(NodeIdentity, InputEvent)>>>,
}

impl RecordingRenderer {
    /// Construct a recorder that handles exactly one content kind.
    pub fn for_kind(id: &'static str, kind: NodeContentKind) -> Self {
        Self {
            id: RendererId::from_static(id),
            handles: NodeContentKindSet::from_one(kind),
            paint_log: Rc::new(RefCell::new(Vec::new())),
            input_log: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Construct a recorder that handles a set of content kinds.
    pub fn for_kinds(id: &'static str, kinds: impl IntoIterator<Item = NodeContentKind>) -> Self {
        Self {
            id: RendererId::from_static(id),
            handles: NodeContentKindSet::from_iter(kinds),
            paint_log: Rc::new(RefCell::new(Vec::new())),
            input_log: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Renderer's ID — useful when constructing tests.
    pub fn id(&self) -> RendererId {
        self.id.clone()
    }

    /// Shared handle to the paint log, so test code can inspect what
    /// dispatch recorded even after the renderer has been boxed into
    /// the registry.
    pub fn shared_paint_log(&self) -> Rc<RefCell<Vec<PaintRecord>>> {
        self.paint_log.clone()
    }

    /// Shared handle to the input log.
    pub fn shared_input_log(&self) -> Rc<RefCell<Vec<(NodeIdentity, InputEvent)>>> {
        self.input_log.clone()
    }
}

impl NodeRenderer for RecordingRenderer {
    fn renderer_id(&self) -> RendererId {
        self.id.clone()
    }

    fn handles(&self) -> NodeContentKindSet {
        self.handles.clone()
    }

    fn composition_mode(&self) -> CompositionMode {
        CompositionMode::InScenePaint
    }

    fn capabilities(&self) -> RendererCapabilities {
        RendererCapabilities::INTERACTIVE_PANEL
    }

    fn as_in_scene_paint(&mut self) -> Option<&mut dyn InScenePaintRenderer> {
        Some(self)
    }
}

impl InScenePaintRenderer for RecordingRenderer {
    fn paint(&mut self, node: &SceneNodeRef, ctx: &mut PaintCtx<'_>) -> PaintResult {
        self.paint_log.borrow_mut().push(PaintRecord {
            identity: node.identity,
            node_transform: ctx.node_transform,
            scale_factor: ctx.scale_factor,
        });
        Ok(())
    }

    fn input(&mut self, node: &SceneNodeRef, event: &InputEvent) -> InputDisposition {
        self.input_log
            .borrow_mut()
            .push((node.identity, event.clone()));
        InputDisposition::Consumed
    }
}

#[cfg(test)]
mod tests {
    use kurbo::Size;
    use mere_renderer_registry::{Placement, RendererRegistry};

    use super::*;
    use crate::host::SubstrateHost;
    use crate::scene::{SubstrateNode, SubstrateScene};

    #[test]
    fn handles_set_reflects_constructor() {
        let r = RecordingRenderer::for_kinds(
            "multi",
            [NodeContentKind::DocumentTile, NodeContentKind::Panel],
        );
        let h = r.handles();
        assert!(h.contains(NodeContentKind::DocumentTile));
        assert!(h.contains(NodeContentKind::Panel));
        assert!(!h.contains(NodeContentKind::WebPage));
    }

    #[test]
    fn input_dispatch_records_event() {
        use mere_renderer_registry::{ModifiersState, PointerButton, PointerEventKind};

        let renderer = RecordingRenderer::for_kind("recording", NodeContentKind::DocumentTile);
        let log = renderer.shared_input_log();
        let mut registry = RendererRegistry::with_default_selector();
        registry.register(Box::new(renderer)).unwrap();
        let mut host = SubstrateHost::new(registry);

        let mut scene = SubstrateScene::new();
        let id = scene.insert(SubstrateNode::new(
            NodeContentKind::DocumentTile,
            Placement::IDENTITY,
            Size::new(50.0, 50.0),
        ));

        let event = InputEvent::Pointer {
            position: kurbo::Point::new(10.0, 10.0),
            kind: PointerEventKind::Down(PointerButton::Primary),
            modifiers: ModifiersState::default(),
        };
        let disposition = host
            .deliver_input(&scene, id, &event)
            .expect("dispatch error");
        assert_eq!(disposition, Some(InputDisposition::Consumed));
        assert_eq!(log.borrow().len(), 1);
        assert_eq!(log.borrow()[0].0, id);
    }
}
