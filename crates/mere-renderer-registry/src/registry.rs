// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `RendererRegistry` + `RendererSelector`.
//!
//! Substrate-owned dispatcher. Hosts register renderers at startup; the
//! substrate's per-frame dispatch walks scene nodes and resolves which
//! renderer paints / handles input via `select(node)` and then `get(id)`.
//!
//! v0 is intentionally minimal:
//!
//! - Single-threaded; no `Send + Sync` on `NodeRenderer`.
//! - First-candidate selector by default; the brief's full chain
//!   (per-node pin → profile-binding constraint → host capability filter →
//!   default policy → last-resort) lands when those input types are real.
//! - No diagnostics emission yet.
//! - No capability-gate hook yet.

use std::collections::HashMap;

use mere_renderer_registry_types::{
    DiagnosticEvent, DiagnosticSink, InputDisposition, InputEvent, NodeContentKind, NoopSink,
    RendererId, RouteDegradedReason, SceneNodeRef,
};

use crate::paint::{PaintCtx, PaintResult};
use crate::renderer::NodeRenderer;

/// Substrate-owned registry of renderers.
///
/// Lifecycle:
///
/// 1. Host constructs at startup ([`Self::with_default_selector`] or
///    [`Self::new`] with a custom [`RendererSelector`]).
/// 2. Host registers each renderer crate's renderer impl
///    ([`Self::register`]).
/// 3. Per-frame substrate dispatch:
///    - For each scene node, [`Self::select`] resolves the renderer.
///    - Host fetches the renderer ([`Self::get_mut`]) and dispatches paint
///      / input through the appropriate composition-mode trait.
///    - Renderer state mutates between calls (Rc<RefCell<...>>-friendly
///      since the registry is not `Send + Sync`).
pub struct RendererRegistry {
    renderers: HashMap<RendererId, Box<dyn NodeRenderer>>,
    by_kind: HashMap<NodeContentKind, Vec<RendererId>>,
    selector: Box<dyn RendererSelector>,
    /// Diagnostic sink. Defaults to a NoopSink; hosts wanting
    /// telemetry / action-bus emission install a real sink via
    /// [`Self::set_sink`] or [`Self::with_sink`].
    sink: Box<dyn DiagnosticSink>,
}

impl RendererRegistry {
    /// Construct an empty registry with a custom selector and the
    /// default no-op diagnostic sink.
    pub fn new(selector: Box<dyn RendererSelector>) -> Self {
        Self {
            renderers: HashMap::new(),
            by_kind: HashMap::new(),
            selector,
            sink: Box::new(NoopSink),
        }
    }

    /// Construct an empty registry with the default first-candidate
    /// selector and the default no-op sink.
    pub fn with_default_selector() -> Self {
        Self::new(Box::new(DefaultSelector))
    }

    /// Construct with a custom diagnostic sink. Selector defaults to
    /// `DefaultSelector`; pass `new(selector).with_sink(sink)` for
    /// both.
    pub fn with_sink(mut self, sink: Box<dyn DiagnosticSink>) -> Self {
        self.sink = sink;
        self
    }

    /// Replace the diagnostic sink. Pass `Box::new(NoopSink)` to
    /// silence emissions.
    pub fn set_sink(&mut self, sink: Box<dyn DiagnosticSink>) {
        self.sink = sink;
    }

    /// Emit a diagnostic event through the configured sink. Public
    /// because the substrate host's dispatch path (outside this
    /// crate) also emits events using the same sink for consistency.
    pub fn emit(&self, event: DiagnosticEvent) {
        self.sink.record(event);
    }

    /// Register a renderer. Emits
    /// [`DiagnosticEvent::RendererRegistered`] on success.
    ///
    /// Returns [`RegistryError::DuplicateId`] if a renderer with the same
    /// `RendererId` is already registered (no diagnostic emitted in
    /// that case — caller handles the error).
    pub fn register(
        &mut self,
        renderer: Box<dyn NodeRenderer>,
    ) -> Result<(), RegistryError> {
        let id = renderer.renderer_id();
        if self.renderers.contains_key(&id) {
            return Err(RegistryError::DuplicateId(id));
        }
        let kinds_set = renderer.handles();
        let kinds: Vec<NodeContentKind> = kinds_set.iter().copied().collect();
        for kind in &kinds {
            self.by_kind.entry(*kind).or_default().push(id.clone());
        }
        self.renderers.insert(id.clone(), renderer);
        self.sink.record(DiagnosticEvent::RendererRegistered { id, kinds });
        Ok(())
    }

    /// Unregister a renderer; returns the boxed renderer if it was
    /// registered. Emits [`DiagnosticEvent::RendererUnregistered`]
    /// when the id was actually present (silent for unknown ids).
    pub fn unregister(&mut self, id: &RendererId) -> Option<Box<dyn NodeRenderer>> {
        let removed = self.renderers.remove(id)?;
        for ids in self.by_kind.values_mut() {
            ids.retain(|i| i != id);
        }
        self.by_kind.retain(|_, ids| !ids.is_empty());
        self.sink.record(DiagnosticEvent::RendererUnregistered { id: id.clone() });
        Some(removed)
    }

    /// Resolve which renderer should handle this node.
    ///
    /// Returns `None` if no registered renderer handles the node's content
    /// kind. Indicates either a registration bug or genuinely-unsupported
    /// content; host paints a placeholder.
    pub fn select(&self, node: &SceneNodeRef) -> Option<RendererId> {
        let candidates = self.by_kind.get(&node.content_kind)?;
        self.selector.select(node, candidates)
    }

    /// Borrow a renderer immutably (for `renderer_id` / `handles` / etc.).
    pub fn get(&self, id: &RendererId) -> Option<&dyn NodeRenderer> {
        Some(&**self.renderers.get(id)?)
    }

    /// Borrow a renderer mutably (for paint / input dispatch).
    pub fn get_mut(&mut self, id: &RendererId) -> Option<&mut dyn NodeRenderer> {
        Some(&mut **self.renderers.get_mut(id)?)
    }

    /// Iterate all registered renderer IDs (for diagnostics, listings).
    pub fn iter_ids(&self) -> impl Iterator<Item = &RendererId> {
        self.renderers.keys()
    }

    /// Dispatch an in-scene paint for `node` through whichever renderer
    /// the selector chooses.
    ///
    /// Returns:
    /// - `Ok(None)` if no renderer handles `node.content_kind` — host
    ///   paints a placeholder.
    /// - `Ok(Some(Err(PaintError::NotReady)))` if the resolved renderer
    ///   declined to paint this frame.
    /// - `Ok(Some(Ok(())))` on success.
    /// - `Err(DispatchError::WrongCompositionMode)` if the resolved
    ///   renderer's `composition_mode()` is not `InScenePaint` — caller
    ///   asked an embedded-frame or overlay renderer to in-scene-paint.
    pub fn paint_node(
        &mut self,
        node: &SceneNodeRef,
        ctx: &mut PaintCtx<'_>,
    ) -> Result<Option<PaintResult>, DispatchError> {
        let Some(id) = self.select(node) else {
            self.sink.record(DiagnosticEvent::RouteDegraded {
                node: node.identity,
                reason: RouteDegradedReason::NoCandidates,
            });
            return Ok(None);
        };
        let renderer = self
            .renderers
            .get_mut(&id)
            .expect("selector returned id not in registry");
        let Some(painter) = renderer.as_in_scene_paint() else {
            self.sink.record(DiagnosticEvent::RouteDegraded {
                node: node.identity,
                reason: RouteDegradedReason::WrongCompositionMode { renderer: id.clone() },
            });
            return Err(DispatchError::WrongCompositionMode { id });
        };
        Ok(Some(painter.paint(node, ctx)))
    }

    /// Dispatch an in-scene input event for `node`.
    ///
    /// Mirrors [`Self::paint_node`]: `Ok(None)` when no renderer claims
    /// the content kind, `Err(WrongCompositionMode)` when the renderer
    /// doesn't implement `InScenePaintRenderer`.
    pub fn deliver_in_scene_input(
        &mut self,
        node: &SceneNodeRef,
        event: &InputEvent,
    ) -> Result<Option<InputDisposition>, DispatchError> {
        let Some(id) = self.select(node) else {
            self.sink.record(DiagnosticEvent::RouteDegraded {
                node: node.identity,
                reason: RouteDegradedReason::NoCandidates,
            });
            return Ok(None);
        };
        let renderer = self
            .renderers
            .get_mut(&id)
            .expect("selector returned id not in registry");
        let Some(painter) = renderer.as_in_scene_paint() else {
            self.sink.record(DiagnosticEvent::RouteDegraded {
                node: node.identity,
                reason: RouteDegradedReason::WrongCompositionMode { renderer: id.clone() },
            });
            return Err(DispatchError::WrongCompositionMode { id });
        };
        Ok(Some(painter.input(node, event)))
    }
}

/// Why a registry dispatch failed structurally (before reaching the renderer).
#[derive(Debug)]
pub enum DispatchError {
    /// The resolved renderer's composition mode doesn't match the
    /// dispatch the host attempted (e.g. host called `paint_node` on a
    /// renderer whose `composition_mode()` is `EmbeddedFrame`). Either
    /// the selector picked the wrong renderer or the host dispatched
    /// against the wrong mode for this node.
    WrongCompositionMode { id: RendererId },
}

impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongCompositionMode { id } => write!(
                f,
                "renderer {id} resolved for node but does not implement the expected composition-mode trait"
            ),
        }
    }
}

impl std::error::Error for DispatchError {}

/// Errors from registry mutation.
#[derive(Debug)]
pub enum RegistryError {
    /// A renderer with this ID is already registered.
    DuplicateId(RendererId),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateId(id) => write!(f, "renderer ID already registered: {id}"),
        }
    }
}

impl std::error::Error for RegistryError {}

/// Resolves which renderer to use when multiple renderers handle the same
/// content kind.
///
/// v0 ships [`DefaultSelector`] (first-candidate). The full chain from
/// [contract brief §5] (per-node pin → profile-binding constraint → host
/// capability filter → default policy → last-resort) is the v1 selector;
/// implements when the inputs to those filters are real types
/// (`EngineProfileBinding`, host capabilities, per-node pin).
///
/// [contract brief §5]: ../../../../design_docs/mere_docs/research/2026-05-15_renderer_registry_contract_brief.md
pub trait RendererSelector {
    fn select(&self, node: &SceneNodeRef, candidates: &[RendererId]) -> Option<RendererId>;
}

/// v0 selector implementing step 1 (per-node pin) and step 5
/// (last-resort: first candidate) of the [contract brief §5]
/// five-step chain. Steps 2–4 (profile-binding constraint, host
/// capability filter, named default policy) wait on the host-side
/// surfaces those steps consult.
///
/// Behavior:
/// - If `node.renderer_pin == Some(id)` and `id` is in `candidates`,
///   return `Some(id)`. The pin only applies when it survives
///   content-kind filtering — pinning to a renderer that doesn't
///   handle the kind falls through (a misconfiguration the host can
///   later flag via a diagnostic).
/// - Otherwise return the first candidate.
///
/// [contract brief §5]: ../../../../design_docs/mere_docs/research/2026-05-15_renderer_registry_contract_brief.md
pub struct DefaultSelector;

impl RendererSelector for DefaultSelector {
    fn select(&self, node: &SceneNodeRef, candidates: &[RendererId]) -> Option<RendererId> {
        if let Some(pin) = &node.renderer_pin
            && candidates.iter().any(|c| c == pin)
        {
            return Some(pin.clone());
        }
        candidates.first().cloned()
    }
}

#[cfg(test)]
mod default_selector_tests {
    use kurbo::Size;
    use mere_renderer_registry_types::{
        LodLevel, NodeContentKind, NodeIdentity, Placement, RendererId, SceneNodeRef,
    };

    use super::*;

    fn node_with_pin(pin: Option<RendererId>) -> SceneNodeRef {
        SceneNodeRef {
            identity: NodeIdentity::next(),
            placement: Placement::IDENTITY,
            lod: LodLevel::FullPane,
            size: Size::new(100.0, 100.0),
            content_kind: NodeContentKind::Panel,
            renderer_pin: pin,
        }
    }

    #[test]
    fn no_pin_picks_first_candidate() {
        let candidates = vec![RendererId::from_static("a"), RendererId::from_static("b")];
        let node = node_with_pin(None);
        let picked = DefaultSelector.select(&node, &candidates);
        assert_eq!(picked, Some(RendererId::from_static("a")));
    }

    #[test]
    fn matching_pin_overrides_first_candidate() {
        let candidates = vec![RendererId::from_static("a"), RendererId::from_static("b")];
        let node = node_with_pin(Some(RendererId::from_static("b")));
        let picked = DefaultSelector.select(&node, &candidates);
        assert_eq!(picked, Some(RendererId::from_static("b")));
    }

    #[test]
    fn unmatched_pin_falls_through_to_first_candidate() {
        let candidates = vec![RendererId::from_static("a"), RendererId::from_static("b")];
        let node = node_with_pin(Some(RendererId::from_static("nonexistent")));
        let picked = DefaultSelector.select(&node, &candidates);
        // Falls through to first candidate (step 5 of the chain).
        assert_eq!(picked, Some(RendererId::from_static("a")));
    }

    #[test]
    fn empty_candidates_returns_none_even_with_pin() {
        let node = node_with_pin(Some(RendererId::from_static("ghost")));
        let picked = DefaultSelector.select(&node, &[]);
        assert_eq!(picked, None);
    }
}
