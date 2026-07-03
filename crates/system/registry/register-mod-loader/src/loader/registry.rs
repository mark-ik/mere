use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use super::free_fns::{NativeModRuntime, WasmModRuntime};
use super::types::*;

mod registry_default;
mod registry_impl;

/// Runtime registry managing mod lifecycle and status.
/// Handles discovery, dependency resolution, and activation of both native and WASM mods.
pub struct ModRegistry {
    /// All discovered mods (native + future WASM)
    manifests: HashMap<String, ModManifest>,
    /// Current status of each mod
    status: HashMap<String, ModStatus>,
    /// Resolved load order (topologically sorted)
    load_order: Vec<String>,
    /// File-backed sources for admitted WASM mods.
    wasm_sources: HashMap<String, WasmModSource>,
    /// Disabled mods for this registry instance.
    disabled_mod_ids: HashSet<String>,
    /// Registry surface extensions installed by each active mod.
    extension_records: HashMap<String, Vec<ModExtensionRecord>>,
    /// Host-injected WASM runtime. `None` builds skip wasm
    /// activation (manifests still parse). Slice 68a.
    wasm_runtime: Option<std::sync::Arc<dyn WasmModRuntime>>,
    /// Host-injected native runtime. `None` builds skip native
    /// activation (manifests still parse). Slice 68b.
    native_runtime: Option<std::sync::Arc<dyn NativeModRuntime>>,
}

static ACTIVE_CAPABILITIES: OnceLock<HashSet<String>> = OnceLock::new();

fn parse_disabled_mod_ids_from_env() -> HashSet<String> {
    let mut disabled = HashSet::new();
    if let Ok(raw) = std::env::var("GRAPHSHELL_DISABLE_MODS") {
        for entry in raw.split([',', ';']) {
            let trimmed = entry.trim();
            if !trimmed.is_empty() {
                disabled.insert(trimmed.to_string());
            }
        }
    }
    if std::env::var("GRAPHSHELL_DISABLE_VERSO")
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes"
        })
        .unwrap_or(false)
    {
        disabled.insert("mod:web-runtime".to_string());
    }
    disabled
}

pub(crate) fn compute_active_capabilities() -> HashSet<String> {
    let mut registry = ModRegistry::new();
    let _ = registry.resolve_dependencies();
    let _ = registry.load_all();
    registry.active_capability_ids()
}

#[cfg(any(test, feature = "test-utils"))]
pub fn compute_active_capabilities_with_disabled(disabled: &HashSet<String>) -> HashSet<String> {
    let mut registry = ModRegistry::new_with_disabled(disabled);
    let _ = registry.resolve_dependencies();
    let _ = registry.load_all();
    registry.active_capability_ids()
}

pub fn runtime_has_capability(capability_id: &str) -> bool {
    ACTIVE_CAPABILITIES
        .get_or_init(compute_active_capabilities)
        .contains(capability_id)
}
