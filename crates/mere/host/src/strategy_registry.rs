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
use graph_layout::adapters::{GridAdapter, PhyllotaxisAdapter};

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

    /// Pre-populated with the analytic adapters every v0a preset
    /// routes to. Streaming strategies (force-directed and friends)
    /// join when Path B (per-node substrate nodes) lands.
    pub fn with_defaults() -> Self {
        let mut registry = Self::empty();
        registry.register(Box::new(GridAdapter::default()));
        registry.register(Box::new(PhyllotaxisAdapter::default()));
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
