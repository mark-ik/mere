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

use mere_renderer_registry_types::{NodeContentKind, RendererId, SceneNodeRef};

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
}

impl RendererRegistry {
    /// Construct an empty registry with a custom selector.
    pub fn new(selector: Box<dyn RendererSelector>) -> Self {
        Self {
            renderers: HashMap::new(),
            by_kind: HashMap::new(),
            selector,
        }
    }

    /// Construct an empty registry with the default first-candidate selector.
    pub fn with_default_selector() -> Self {
        Self::new(Box::new(DefaultSelector))
    }

    /// Register a renderer.
    ///
    /// Returns [`RegistryError::DuplicateId`] if a renderer with the same
    /// `RendererId` is already registered.
    pub fn register(
        &mut self,
        renderer: Box<dyn NodeRenderer>,
    ) -> Result<(), RegistryError> {
        let id = renderer.renderer_id();
        if self.renderers.contains_key(&id) {
            return Err(RegistryError::DuplicateId(id));
        }
        for kind in renderer.handles().iter() {
            self.by_kind.entry(*kind).or_default().push(id.clone());
        }
        self.renderers.insert(id, renderer);
        Ok(())
    }

    /// Unregister a renderer; returns the boxed renderer if it was registered.
    pub fn unregister(&mut self, id: &RendererId) -> Option<Box<dyn NodeRenderer>> {
        let removed = self.renderers.remove(id)?;
        for ids in self.by_kind.values_mut() {
            ids.retain(|i| i != id);
        }
        self.by_kind.retain(|_, ids| !ids.is_empty());
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
}

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

/// First-candidate selector. v0 stub.
pub struct DefaultSelector;

impl RendererSelector for DefaultSelector {
    fn select(&self, _node: &SceneNodeRef, candidates: &[RendererId]) -> Option<RendererId> {
        candidates.first().cloned()
    }
}
