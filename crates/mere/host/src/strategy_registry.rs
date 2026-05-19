// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Host-side strategy registry — keyed by [`LayoutStrategy::projection_id`].
//!
//! [`ViewPreset`](crate::view_preset::ViewPreset)s name *which strategy*
//! a pane wants; this registry resolves that name to a boxed
//! [`cartography::LayoutStrategy`] the projection pass can call. Two
//! pieces stay separate on purpose:
//!
//! - Preset → strategy id is configuration (per-pane, per-session, per
//!   user preference). Lives on `ViewPreset`.
//! - Strategy id → strategy implementation is a registry the host
//!   populates once at startup. Lives here.
//!
//! The split lets future code add a preset (or remap one) without
//! touching the registry, and add a strategy without touching the
//! preset enum. Third-party / WASM strategies eventually register
//! through this same surface.

use std::collections::HashMap;

use cartography::LayoutStrategy;
use graph_layout::adapters::{
    GridAdapter, LSystemAdapter, PenroseAdapter, PhyllotaxisAdapter, RadialAdapter,
};

/// Resolves a `projection_id` to a registered [`LayoutStrategy`].
/// Defaults include the analytic adapters every v0a preset routes to
/// (`grid.default`, `phyllotaxis.default`); hosts add more through
/// [`Self::register`].
pub struct StrategyRegistry {
    strategies: HashMap<&'static str, Box<dyn LayoutStrategy>>,
}

impl StrategyRegistry {
    /// Empty registry. Use [`Self::with_defaults`] for the v0a
    /// analytic-strategy baseline.
    pub fn empty() -> Self {
        Self {
            strategies: HashMap::new(),
        }
    }

    /// Pre-populated with the analytic adapters that produce
    /// meaningful output from a bare graph + viewport — no host-
    /// supplied axis values, embeddings, or focus required (Radial
    /// degrades to an empty projection until a focus-setting preset
    /// drives it):
    ///
    /// - `grid.default` — row-major snap grid
    /// - `phyllotaxis.default` — sunflower / golden-angle packing
    /// - `penrose.default` — aperiodic P2/P3 tiling
    /// - `lsystem.default` — space-filling fractal path
    /// - `radial.default` — BFS rings around `ViewIntent::focus`
    ///
    /// Axial (Kanban / Timeline) and embedding (SemanticEmbedding)
    /// adapters are intentionally left out until the host supplies
    /// their inputs (axis values / precomputed embeddings) — they'd
    /// otherwise be dead routes. Streaming strategies (force-directed,
    /// Barnes-Hut, semantic-edge-weight) implement
    /// `StreamingLayoutStrategy`, not `LayoutStrategy`, so they join
    /// through a separate per-frame stepping path, not this registry.
    pub fn with_defaults() -> Self {
        let mut registry = Self::empty();
        registry.register(Box::new(GridAdapter::default()));
        registry.register(Box::new(PhyllotaxisAdapter::default()));
        registry.register(Box::new(PenroseAdapter::default()));
        registry.register(Box::new(LSystemAdapter::default()));
        registry.register(Box::new(RadialAdapter::default()));
        registry
    }

    /// Add a strategy. Keyed by [`LayoutStrategy::projection_id`];
    /// duplicate ids silently replace the prior registration so
    /// hosts can swap defaults without unregistering first.
    pub fn register(&mut self, strategy: Box<dyn LayoutStrategy>) {
        let id = strategy.projection_id();
        self.strategies.insert(id, strategy);
    }

    /// Resolve a strategy by id. Returns `None` if no registration
    /// matches — callers fall back to a default or skip the pane.
    pub fn resolve(&self, id: &str) -> Option<&dyn LayoutStrategy> {
        self.strategies.get(id).map(|boxed| boxed.as_ref())
    }

    /// Number of registered strategies.
    pub fn len(&self) -> usize {
        self.strategies.len()
    }

    /// True when nothing is registered.
    pub fn is_empty(&self) -> bool {
        self.strategies.is_empty()
    }
}

impl Default for StrategyRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_preset::ViewPreset;

    #[test]
    fn defaults_resolve_every_preset_id() {
        let registry = StrategyRegistry::with_defaults();
        for preset in [ViewPreset::Orrery, ViewPreset::Drift, ViewPreset::Minimap] {
            let id = preset.default_strategy_id();
            assert!(
                registry.resolve(id).is_some(),
                "preset {preset:?} routes to unregistered id {id}"
            );
        }
    }

    #[test]
    fn defaults_register_the_analytic_adapter_set() {
        let registry = StrategyRegistry::with_defaults();
        for id in [
            "grid.default",
            "phyllotaxis.default",
            "penrose.default",
            "lsystem.default",
            "radial.default",
        ] {
            assert!(registry.resolve(id).is_some(), "missing adapter {id}");
        }
        assert_eq!(registry.len(), 5);
    }

    #[test]
    fn empty_registry_resolves_nothing() {
        let registry = StrategyRegistry::empty();
        assert!(registry.is_empty());
        assert!(registry.resolve("grid.default").is_none());
    }

    #[test]
    fn register_replaces_duplicate_ids() {
        let mut registry = StrategyRegistry::empty();
        registry.register(Box::new(GridAdapter::default()));
        registry.register(Box::new(GridAdapter::default()));
        // Replacement, not duplication.
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn unknown_id_resolves_to_none() {
        let registry = StrategyRegistry::with_defaults();
        assert!(registry.resolve("does-not-exist").is_none());
    }
}
