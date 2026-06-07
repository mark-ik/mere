/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Mod-based protocol-handler provider hook. Mods implement
//! [`ProtocolHandlerProvider`] to register their handlers into a
//! [`ProtocolContractRegistry`](super::contract::ProtocolContractRegistry)
//! during activation; the host collects provider closures into a
//! [`ProtocolHandlerProviders`] bundle and applies them all at runtime
//! startup.
//!
//! Folded into the same crate as the contract registry (Slice 51)
//! because providers register *into* the registry — they belong in the
//! same crate even though the file split is preserved for navigation.

use super::contract::ProtocolContractRegistry;

/// Trait mods implement to register their protocol handlers during
/// activation. The provider receives a mutable reference to the host's
/// registry and calls `register_scheme` for each scheme it owns.
pub trait ProtocolHandlerProvider {
    fn register(&self, registry: &mut ProtocolContractRegistry);
}

/// Bundle of provider closures collected during mod activation. The
/// host populates a `ProtocolHandlerProviders` during startup, then
/// calls [`Self::apply_all`] against the freshly-constructed
/// [`ProtocolContractRegistry`] to wire all registered handlers.
pub struct ProtocolHandlerProviders {
    providers: Vec<Box<dyn Fn(&mut ProtocolContractRegistry)>>,
}

impl ProtocolHandlerProviders {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Register a free-function provider closure. Used by mods that
    /// don't have a struct to hang the provider trait on.
    pub fn register_fn<F>(&mut self, f: F)
    where
        F: Fn(&mut ProtocolContractRegistry) + 'static,
    {
        self.providers.push(Box::new(f));
    }

    /// Apply every registered provider to the given registry. Called
    /// once during runtime startup after all mods have been loaded.
    pub fn apply_all(&self, registry: &mut ProtocolContractRegistry) {
        for provider in &self.providers {
            provider(registry);
        }
    }
}

impl Default for ProtocolHandlerProviders {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_all_runs_each_registered_provider() {
        let mut providers = ProtocolHandlerProviders::new();
        providers.register_fn(|registry| registry.register_scheme("gemini", "protocol:gemini"));
        providers.register_fn(|registry| registry.register_scheme("gopher", "protocol:gopher"));

        let mut registry = ProtocolContractRegistry::core_seed();
        providers.apply_all(&mut registry);

        assert!(registry.has_scheme("gemini"));
        assert!(registry.has_scheme("gopher"));
        assert!(registry.has_scheme("file"));
    }
}
