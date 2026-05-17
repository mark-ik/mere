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
    /// Named default policies (selector chain step 4): for each
    /// `NodeContentKind`, the renderer id the chain prefers when no
    /// per-node pin matches. The mapping is the host's "if I don't
    /// say otherwise, render WebPage with scrying.web" knob.
    default_policies: HashMap<NodeContentKind, RendererId>,
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
            default_policies: HashMap::new(),
        }
    }

    /// Set the default-policy renderer for a content kind (selector
    /// chain step 4). When no per-node pin matches and `id` is
    /// registered + handles `kind`, the chain picks `id` over the
    /// first-candidate fallback.
    pub fn set_default_policy(&mut self, kind: NodeContentKind, id: RendererId) {
        self.default_policies.insert(kind, id);
    }

    /// Remove the default-policy mapping for a content kind. Returns
    /// the previous mapping if any.
    pub fn clear_default_policy(&mut self, kind: NodeContentKind) -> Option<RendererId> {
        self.default_policies.remove(&kind)
    }

    /// The default-policy renderer for `kind`, if one is set.
    pub fn default_policy(&self, kind: NodeContentKind) -> Option<&RendererId> {
        self.default_policies.get(&kind)
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

    /// Resolve which renderer should handle this node, applying the
    /// selector chain:
    ///
    /// 1. **Per-node pin**: if `node.renderer_pin` is in candidates,
    ///    pick it.
    /// 2–3. (Profile-binding constraint, host capability filter —
    ///    deferred; need host-side surfaces those consult.)
    /// 4. **Named default policy**: if a default policy is set for
    ///    `node.content_kind` and the policy renderer is in
    ///    candidates, pick it.
    /// 5. **Selector tie-break**: hand the remaining candidates to
    ///    the configured `RendererSelector` (default:
    ///    [`DefaultSelector`] = first candidate).
    ///
    /// Returns `None` if no registered renderer handles the node's
    /// content kind — host paints a placeholder.
    pub fn select(&self, node: &SceneNodeRef) -> Option<RendererId> {
        let candidates = self.by_kind.get(&node.content_kind)?;
        // Step 1: per-node pin.
        if let Some(pin) = &node.renderer_pin
            && candidates.iter().any(|c| c == pin)
        {
            return Some(pin.clone());
        }
        // Step 4: named default policy.
        if let Some(policy) = self.default_policies.get(&node.content_kind)
            && candidates.iter().any(|c| c == policy)
        {
            return Some(policy.clone());
        }
        // Step 5: selector tie-break.
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

/// v0 selector implementing step 5 (last-resort: first candidate)
/// of the [contract brief §5] five-step chain. Steps 1 (per-node
/// pin) and 4 (named default policy) are applied by
/// [`RendererRegistry::select`] *before* delegating here; steps 2–3
/// (profile-binding constraint, host capability filter) wait on the
/// host-side surfaces those consult.
///
/// Pure tie-breaker: hand back the first candidate.
///
/// [contract brief §5]: ../../../../design_docs/mere_docs/research/2026-05-15_renderer_registry_contract_brief.md
pub struct DefaultSelector;

impl RendererSelector for DefaultSelector {
    fn select(&self, _node: &SceneNodeRef, candidates: &[RendererId]) -> Option<RendererId> {
        candidates.first().cloned()
    }
}

#[cfg(test)]
mod selector_chain_tests {
    use kurbo::Size;
    use mere_renderer_registry_types::{
        LodLevel, NodeContentKind, NodeIdentity, Placement, RendererId, SceneNodeRef,
    };

    use super::*;
    use crate::NodeRenderer;
    use crate::renderer::{InScenePaintRenderer, OverlayRenderer};
    use mere_renderer_registry_types::{
        CompositionMode, InputDisposition, InputEvent, NodeContentKindSet,
        ProfileBindingExpectation, RendererCapabilities,
    };
    use crate::paint::{PaintCtx, PaintResult};

    /// Tiny renderer just for registry-chain tests. Doesn't paint;
    /// reports the configured id + kind.
    struct StubRenderer {
        id: RendererId,
        kind: NodeContentKind,
    }

    impl StubRenderer {
        fn new(id: &'static str, kind: NodeContentKind) -> Self {
            Self {
                id: RendererId::from_static(id),
                kind,
            }
        }
    }

    impl NodeRenderer for StubRenderer {
        fn renderer_id(&self) -> RendererId {
            self.id.clone()
        }
        fn handles(&self) -> NodeContentKindSet {
            NodeContentKindSet::from_one(self.kind)
        }
        fn composition_mode(&self) -> CompositionMode {
            CompositionMode::InScenePaint
        }
        fn capabilities(&self) -> RendererCapabilities {
            RendererCapabilities {
                accepts_input: false,
                handles_ime: false,
                handles_a11y: false,
                scrollable: false,
                hit_testable_subregions: false,
                profile_binding: ProfileBindingExpectation::None,
                supports_capture: false,
            }
        }
        fn as_in_scene_paint(&mut self) -> Option<&mut dyn InScenePaintRenderer> {
            Some(self)
        }
    }

    impl InScenePaintRenderer for StubRenderer {
        fn paint(&mut self, _node: &SceneNodeRef, _ctx: &mut PaintCtx<'_>) -> PaintResult {
            Ok(())
        }
        fn input(&mut self, _node: &SceneNodeRef, _event: &InputEvent) -> InputDisposition {
            InputDisposition::Passthrough
        }
    }

    /// Stub overlay renderer — verifies tie-break (no pin, no policy)
    /// also makes sure we can't accidentally double-count it as
    /// InScene tie-break for `Panel`.
    struct OverlayStub {
        id: RendererId,
    }

    impl NodeRenderer for OverlayStub {
        fn renderer_id(&self) -> RendererId {
            self.id.clone()
        }
        fn handles(&self) -> NodeContentKindSet {
            NodeContentKindSet::from_one(NodeContentKind::WebPage)
        }
        fn composition_mode(&self) -> CompositionMode {
            CompositionMode::Overlay
        }
        fn capabilities(&self) -> RendererCapabilities {
            RendererCapabilities::NONE
        }
        fn as_overlay(&mut self) -> Option<&mut dyn OverlayRenderer> {
            Some(self)
        }
    }

    impl OverlayRenderer for OverlayStub {
        fn ensure_overlay(
            &mut self,
            _node: &SceneNodeRef,
        ) -> mere_renderer_registry_types::OverlayHandle {
            mere_renderer_registry_types::OverlayHandle::next()
        }
        fn position(
            &mut self,
            _h: mere_renderer_registry_types::OverlayHandle,
            _r: mere_renderer_registry_types::ScreenRect,
            _z: i32,
        ) {
        }
        fn deliver_input(
            &mut self,
            _h: mere_renderer_registry_types::OverlayHandle,
            _e: &InputEvent,
        ) -> InputDisposition {
            InputDisposition::Passthrough
        }
        fn release(&mut self, _h: mere_renderer_registry_types::OverlayHandle) {}
    }

    fn node_with_pin(kind: NodeContentKind, pin: Option<RendererId>) -> SceneNodeRef {
        SceneNodeRef {
            identity: NodeIdentity::next(),
            placement: Placement::IDENTITY,
            lod: LodLevel::FullPane,
            size: Size::new(100.0, 100.0),
            content_kind: kind,
            renderer_pin: pin,
        }
    }

    #[test]
    fn chain_no_pin_no_policy_picks_first_registered() {
        let mut reg = RendererRegistry::with_default_selector();
        reg.register(Box::new(StubRenderer::new("a", NodeContentKind::Panel)))
            .unwrap();
        reg.register(Box::new(StubRenderer::new("b", NodeContentKind::Panel)))
            .unwrap();
        let node = node_with_pin(NodeContentKind::Panel, None);
        assert_eq!(reg.select(&node), Some(RendererId::from_static("a")));
    }

    #[test]
    fn chain_matching_pin_wins() {
        let mut reg = RendererRegistry::with_default_selector();
        reg.register(Box::new(StubRenderer::new("a", NodeContentKind::Panel)))
            .unwrap();
        reg.register(Box::new(StubRenderer::new("b", NodeContentKind::Panel)))
            .unwrap();
        let node = node_with_pin(NodeContentKind::Panel, Some(RendererId::from_static("b")));
        assert_eq!(reg.select(&node), Some(RendererId::from_static("b")));
    }

    #[test]
    fn chain_default_policy_overrides_first_candidate() {
        let mut reg = RendererRegistry::with_default_selector();
        reg.register(Box::new(StubRenderer::new("a", NodeContentKind::Panel)))
            .unwrap();
        reg.register(Box::new(StubRenderer::new("b", NodeContentKind::Panel)))
            .unwrap();
        reg.set_default_policy(NodeContentKind::Panel, RendererId::from_static("b"));
        let node = node_with_pin(NodeContentKind::Panel, None);
        assert_eq!(reg.select(&node), Some(RendererId::from_static("b")));
    }

    #[test]
    fn chain_pin_beats_default_policy() {
        let mut reg = RendererRegistry::with_default_selector();
        reg.register(Box::new(StubRenderer::new("a", NodeContentKind::Panel)))
            .unwrap();
        reg.register(Box::new(StubRenderer::new("b", NodeContentKind::Panel)))
            .unwrap();
        reg.register(Box::new(StubRenderer::new("c", NodeContentKind::Panel)))
            .unwrap();
        reg.set_default_policy(NodeContentKind::Panel, RendererId::from_static("b"));
        let node = node_with_pin(NodeContentKind::Panel, Some(RendererId::from_static("c")));
        assert_eq!(reg.select(&node), Some(RendererId::from_static("c")));
    }

    #[test]
    fn chain_default_policy_skipped_when_not_registered() {
        let mut reg = RendererRegistry::with_default_selector();
        reg.register(Box::new(StubRenderer::new("a", NodeContentKind::Panel)))
            .unwrap();
        reg.set_default_policy(NodeContentKind::Panel, RendererId::from_static("ghost"));
        let node = node_with_pin(NodeContentKind::Panel, None);
        // Falls through to first-candidate (step 5).
        assert_eq!(reg.select(&node), Some(RendererId::from_static("a")));
    }

    #[test]
    fn clear_default_policy_returns_previous() {
        let mut reg = RendererRegistry::with_default_selector();
        reg.set_default_policy(NodeContentKind::WebPage, RendererId::from_static("scrying"));
        let prev = reg.clear_default_policy(NodeContentKind::WebPage);
        assert_eq!(prev, Some(RendererId::from_static("scrying")));
        assert_eq!(reg.default_policy(NodeContentKind::WebPage), None);
    }

    #[test]
    fn empty_candidates_returns_none() {
        // No renderer for the kind → None regardless of pin/policy.
        let mut reg = RendererRegistry::with_default_selector();
        reg.set_default_policy(NodeContentKind::WebPage, RendererId::from_static("ghost"));
        let node = node_with_pin(
            NodeContentKind::WebPage,
            Some(RendererId::from_static("ghost")),
        );
        assert_eq!(reg.select(&node), None);
        // Use OverlayStub so the import isn't dead — keeps coverage of
        // the second composition mode in this test module.
        reg.register(Box::new(OverlayStub {
            id: RendererId::from_static("overlay"),
        }))
        .unwrap();
        // Now the WebPage kind has a candidate, but `ghost` is still
        // a fake pin — falls through.
        assert_eq!(reg.select(&node), Some(RendererId::from_static("overlay")));
    }
}
