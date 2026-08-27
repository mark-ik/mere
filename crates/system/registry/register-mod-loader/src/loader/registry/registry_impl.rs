// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::super::free_fns::{
    NativeModRuntime, WasmModRuntime, discover_mod_manifests, read_wasm_mod_from_path,
    resolve_mod_load_order,
};
use super::super::types::*;
use super::ModRegistry;
use super::parse_disabled_mod_ids_from_env;

impl ModRegistry {
    fn rollback_extension_records<F>(
        installed_records: &mut Vec<ModExtensionRecord>,
        rollback: &mut F,
    ) -> Result<(), String>
    where
        F: FnMut(ModExtensionRecord) -> Result<(), String>,
    {
        while let Some(record) = installed_records.pop() {
            if let Err(reason) = rollback(record.clone()) {
                installed_records.push(record);
                return Err(reason);
            }
        }

        Ok(())
    }

    fn from_manifests_with_disabled(
        manifests: Vec<ModManifest>,
        disabled_mod_ids: &HashSet<String>,
    ) -> Self {
        let manifests = manifests
            .into_iter()
            .map(|manifest| (manifest.mod_id.clone(), manifest))
            .collect::<HashMap<_, _>>();

        let status = manifests
            .keys()
            .map(|id| {
                if disabled_mod_ids.contains(id) {
                    (id.clone(), ModStatus::Unloaded)
                } else {
                    (id.clone(), ModStatus::Discovered)
                }
            })
            .collect();

        Self {
            manifests,
            status,
            load_order: Vec::new(),
            wasm_sources: HashMap::new(),
            disabled_mod_ids: disabled_mod_ids.clone(),
            extension_records: HashMap::new(),
            wasm_runtime: None,
            native_runtime: None,
        }
    }

    /// Builder-style setter for the host's WASM runtime. Without
    /// it, `load_all` errors out activation for `ModType::Wasm` mods
    /// (the manifests are still parsed and tracked). Slice 68a.
    pub fn with_wasm_runtime(mut self, runtime: std::sync::Arc<dyn WasmModRuntime>) -> Self {
        self.wasm_runtime = Some(runtime);
        self
    }

    /// Builder-style setter for the host's native runtime. Without
    /// it, `load_all` errors out activation for `ModType::Native`
    /// mods (the manifests are still parsed and tracked).
    /// Slice 68b.
    pub fn with_native_runtime(mut self, runtime: std::sync::Arc<dyn NativeModRuntime>) -> Self {
        self.native_runtime = Some(runtime);
        self
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn from_manifests_for_tests(manifests: Vec<ModManifest>) -> Self {
        Self::from_manifests_with_disabled(manifests, &HashSet::new())
    }

    pub(crate) fn new_with_disabled(disabled_mod_ids: &HashSet<String>) -> Self {
        Self::from_manifests_with_disabled(discover_mod_manifests([]), disabled_mod_ids)
    }

    /// Create a new ModRegistry and discover all native mods.
    /// Does not perform dependency resolution or loading yet.
    pub fn new() -> Self {
        let disabled_mod_ids = parse_disabled_mod_ids_from_env();
        Self::new_with_disabled(&disabled_mod_ids)
    }

    /// Resolve dependencies and compute load order.
    /// Returns error if dependencies are missing or cyclic.
    pub fn resolve_dependencies(&mut self) -> Result<(), ModDependencyError> {
        use register_diagnostics::channels::CHANNEL_MOD_DEPENDENCY_MISSING;
        use register_diagnostics::{DiagnosticEvent, emit_event};

        let manifests_vec: Vec<_> = self
            .manifests
            .values()
            .filter(|manifest| !self.disabled_mod_ids.contains(&manifest.mod_id))
            .cloned()
            .collect();
        match resolve_mod_load_order(&manifests_vec) {
            Ok(ordered) => {
                self.load_order = ordered.iter().map(|m| m.mod_id.clone()).collect();
                Ok(())
            }
            Err(err) => {
                // Emit diagnostics for missing dependencies
                if let ModDependencyError::MissingRequirement {
                    mod_id,
                    requirement,
                } = &err
                {
                    emit_event(DiagnosticEvent::MessageSent {
                        channel_id: CHANNEL_MOD_DEPENDENCY_MISSING,
                        byte_len: mod_id.len() + requirement.len(),
                    });
                }
                Err(err)
            }
        }
    }

    pub fn load_mod(&mut self, path: impl AsRef<Path>) -> Result<String, ModLoadPathError> {
        let (manifest, source) = read_wasm_mod_from_path(path.as_ref())?;
        if self.manifests.contains_key(&manifest.mod_id) {
            return Err(ModLoadPathError::DuplicateModId(manifest.mod_id));
        }

        let mod_id = manifest.mod_id.clone();
        let initial_status = if self.disabled_mod_ids.contains(&mod_id) {
            ModStatus::Unloaded
        } else {
            ModStatus::Discovered
        };

        self.manifests.insert(mod_id.clone(), manifest);
        self.wasm_sources.insert(mod_id.clone(), source);
        self.status.insert(mod_id.clone(), initial_status);
        self.load_order.clear();

        Ok(mod_id)
    }

    /// Load all mods in dependency order.
    /// Emits lifecycle diagnostics for each mod.
    ///
    /// Slice 68: WASM activation routes through the registry's
    /// host-injected [`WasmModRuntime`]. Builds without a runtime
    /// (`with_wasm_runtime` never called) skip wasm activation and
    /// log a warning per affected mod.
    pub fn load_all(&mut self) -> Vec<String> {
        let wasm_runtime = self.wasm_runtime.clone();
        let native_runtime = self.native_runtime.clone();
        self.load_all_with_extensions(
            |manifest, wasm_source| match manifest.mod_type {
                ModType::Native => {
                    Self::activate_native_mod(native_runtime.as_ref(), &manifest.mod_id)
                        .map_err(ModActivationError::failed)?;
                    Ok(Vec::new())
                }
                ModType::Wasm => {
                    let source = wasm_source.ok_or_else(|| {
                        ModActivationError::failed(format!(
                            "missing wasm source for {}",
                            manifest.mod_id
                        ))
                    })?;
                    let Some(runtime) = wasm_runtime.as_ref() else {
                        return Err(ModActivationError::failed(format!(
                            "no WasmModRuntime injected; skipping wasm mod '{}'",
                            manifest.mod_id
                        )));
                    };
                    runtime
                        .activate(manifest, source)
                        .map_err(ModActivationError::failed)?;
                    Ok(vec![ModExtensionRecord::WasmRuntime {
                        mod_id: manifest.mod_id.clone(),
                    }])
                }
            },
            |record| match record {
                ModExtensionRecord::WasmRuntime { mod_id } => {
                    if let Some(runtime) = wasm_runtime.as_ref() {
                        runtime.deactivate(&mod_id)
                    } else {
                        Ok(())
                    }
                }
                ModExtensionRecord::ProtocolScheme { .. }
                | ModExtensionRecord::ViewerMime { .. }
                | ModExtensionRecord::ViewerExtension { .. }
                | ModExtensionRecord::ViewerCapabilities { .. }
                | ModExtensionRecord::Action { .. }
                | ModExtensionRecord::IndexProvider { .. }
                | ModExtensionRecord::Lens { .. }
                | ModExtensionRecord::Theme { .. } => Ok(()),
            },
        )
    }

    pub fn load_all_with_extensions<F, R>(
        &mut self,
        mut activate: F,
        mut rollback: R,
    ) -> Vec<String>
    where
        F: FnMut(
            &ModManifest,
            Option<&WasmModSource>,
        ) -> Result<Vec<ModExtensionRecord>, ModActivationError>,
        R: FnMut(ModExtensionRecord) -> Result<(), String>,
    {
        use register_diagnostics::channels::{
            CHANNEL_MOD_LOAD_FAILED, CHANNEL_MOD_LOAD_STARTED, CHANNEL_MOD_LOAD_SUCCEEDED,
            CHANNEL_MOD_QUARANTINED, CHANNEL_MOD_ROLLBACK_FAILED, CHANNEL_MOD_ROLLBACK_SUCCEEDED,
        };
        use register_diagnostics::{DiagnosticEvent, emit_event};

        let mut loaded = Vec::new();

        for mod_id in &self.load_order {
            if self.disabled_mod_ids.contains(mod_id) {
                continue;
            }
            let manifest = match self.manifests.get(mod_id) {
                Some(m) => m,
                None => continue,
            };

            // Emit load started
            emit_event(DiagnosticEvent::MessageSent {
                channel_id: CHANNEL_MOD_LOAD_STARTED,
                byte_len: mod_id.len() + manifest.display_name.len(),
            });

            self.status.insert(mod_id.clone(), ModStatus::Loading);

            let load_result = activate(manifest, self.wasm_sources.get(mod_id));

            match load_result {
                Ok(extension_records) => {
                    self.status.insert(mod_id.clone(), ModStatus::Active);
                    self.extension_records
                        .insert(mod_id.clone(), extension_records);
                    emit_event(DiagnosticEvent::MessageSent {
                        channel_id: CHANNEL_MOD_LOAD_SUCCEEDED,
                        byte_len: mod_id.len()
                            + manifest.provides.iter().map(|s| s.len()).sum::<usize>(),
                    });
                    loaded.push(mod_id.clone());
                }
                Err(error) => {
                    let (reason, mut applied_records) = error.into_parts();
                    let failure_reason = if applied_records.is_empty() {
                        self.status.insert(mod_id.clone(), ModStatus::Failed);
                        reason
                    } else {
                        match Self::rollback_extension_records(&mut applied_records, &mut rollback)
                        {
                            Ok(()) => {
                                self.status.insert(mod_id.clone(), ModStatus::Failed);
                                emit_event(DiagnosticEvent::MessageSent {
                                    channel_id: CHANNEL_MOD_ROLLBACK_SUCCEEDED,
                                    byte_len: mod_id.len() + reason.len(),
                                });
                                reason
                            }
                            Err(rollback_reason) => {
                                self.status.insert(mod_id.clone(), ModStatus::Quarantined);
                                self.extension_records
                                    .insert(mod_id.clone(), applied_records);
                                emit_event(DiagnosticEvent::MessageSent {
                                    channel_id: CHANNEL_MOD_ROLLBACK_FAILED,
                                    byte_len: mod_id.len() + rollback_reason.len(),
                                });
                                emit_event(DiagnosticEvent::MessageSent {
                                    channel_id: CHANNEL_MOD_QUARANTINED,
                                    byte_len: mod_id.len() + rollback_reason.len(),
                                });
                                format!("{reason}; rollback failed: {rollback_reason}")
                            }
                        }
                    };
                    emit_event(DiagnosticEvent::MessageSent {
                        channel_id: CHANNEL_MOD_LOAD_FAILED,
                        byte_len: mod_id.len() + failure_reason.len(),
                    });
                }
            }
        }

        for mod_id in &self.disabled_mod_ids {
            self.status.insert(mod_id.clone(), ModStatus::Unloaded);
        }

        loaded
    }

    pub fn unload_mod_with<F>(
        &mut self,
        mod_id: &str,
        mut remove_extension: F,
    ) -> Result<(), ModUnloadError>
    where
        F: FnMut(ModExtensionRecord) -> Result<(), String>,
    {
        use register_diagnostics::channels::{CHANNEL_MOD_QUARANTINED, CHANNEL_MOD_UNLOAD_FAILED};
        use register_diagnostics::{DiagnosticEvent, emit_event};

        let normalized = mod_id.trim().to_ascii_lowercase();
        let Some(status) = self.status.get(&normalized).copied() else {
            return Err(ModUnloadError::UnknownMod(normalized));
        };
        if status != ModStatus::Active {
            return Err(ModUnloadError::NotActive(normalized));
        }

        let Some(manifest) = self.manifests.get(&normalized).cloned() else {
            return Err(ModUnloadError::UnknownMod(normalized));
        };
        if let Some(dependent) = self.active_dependent_of(&manifest.mod_id) {
            return Err(ModUnloadError::DependencyActive {
                mod_id: manifest.mod_id,
                dependent_id: dependent,
            });
        }

        let mut remove_entry = false;
        if let Some(records) = self.extension_records.get_mut(&manifest.mod_id) {
            while let Some(record) = records.pop() {
                if let Err(reason) = remove_extension(record.clone()) {
                    records.push(record);
                    self.status
                        .insert(manifest.mod_id.clone(), ModStatus::Quarantined);
                    emit_event(DiagnosticEvent::MessageSent {
                        channel_id: CHANNEL_MOD_UNLOAD_FAILED,
                        byte_len: manifest.mod_id.len() + reason.len(),
                    });
                    emit_event(DiagnosticEvent::MessageSent {
                        channel_id: CHANNEL_MOD_QUARANTINED,
                        byte_len: manifest.mod_id.len() + reason.len(),
                    });
                    return Err(ModUnloadError::ExtensionRemovalFailed {
                        mod_id: manifest.mod_id,
                        reason,
                    });
                }
            }
            remove_entry = true;
        }

        if remove_entry {
            self.extension_records.remove(&manifest.mod_id);
        }

        self.status.insert(manifest.mod_id, ModStatus::Unloaded);
        Ok(())
    }

    /// Activate a native mod by dispatching through the host-injected
    /// [`NativeModRuntime`]. Pre-Slice-68b this called
    /// `super::NativeModActivations::new()` directly (which hardcoded
    /// `crate::mods::native::*::activate` function pointers); the
    /// indirection lets the mod loader extract to its own crate.
    ///
    /// The `None` case returns `Ok(())` — matches the pre-Slice-68b
    /// behaviour where `NativeModActivations::activate` silently
    /// no-op'd for unknown mod IDs. This keeps tests that don't
    /// inject a runtime working (they use synthetic mod IDs that
    /// no real activation hook would match).
    fn activate_native_mod(
        runtime: Option<&std::sync::Arc<dyn NativeModRuntime>>,
        mod_id: &str,
    ) -> Result<(), String> {
        match runtime {
            Some(runtime) => runtime.activate(mod_id),
            None => Ok(()),
        }
    }

    fn active_dependent_of(&self, mod_id: &str) -> Option<String> {
        let manifest = self.manifests.get(mod_id)?;
        self.manifests
            .values()
            .filter(|candidate| candidate.mod_id != mod_id)
            .filter(|candidate| {
                self.status
                    .get(&candidate.mod_id)
                    .copied()
                    .is_some_and(|status| status == ModStatus::Active)
            })
            .filter(|candidate| {
                candidate.requires.iter().any(|requirement| {
                    manifest
                        .provides
                        .iter()
                        .any(|provided| provided == requirement)
                })
            })
            .map(|candidate| candidate.mod_id.clone())
            .min()
    }

    /// Get the status of a mod
    pub fn get_status(&self, mod_id: &str) -> Option<ModStatus> {
        self.status.get(mod_id).copied()
    }

    /// Get the manifest for a mod
    pub fn get_manifest(&self, mod_id: &str) -> Option<&ModManifest> {
        self.manifests.get(mod_id)
    }

    /// List all mod IDs in load order
    pub fn list_mods(&self) -> &[String] {
        &self.load_order
    }

    pub fn extension_records_for(&self, mod_id: &str) -> Option<&[ModExtensionRecord]> {
        self.extension_records.get(mod_id).map(Vec::as_slice)
    }

    pub fn wasm_source(&self, mod_id: &str) -> Option<&WasmModSource> {
        self.wasm_sources.get(mod_id)
    }

    /// Check if a specific capability is provided by any loaded mod
    pub fn is_capability_available(&self, capability_id: &str) -> bool {
        self.manifests.values().any(|m| {
            if self.disabled_mod_ids.contains(&m.mod_id) {
                return false;
            }
            let mod_active = self
                .status
                .get(&m.mod_id)
                .is_some_and(|s| *s == ModStatus::Active);
            mod_active && m.provides.iter().any(|p| p == capability_id)
        })
    }

    pub fn active_capability_ids(&self) -> HashSet<String> {
        self.manifests
            .values()
            .filter(|manifest| {
                self.status
                    .get(&manifest.mod_id)
                    .map(|status| *status == ModStatus::Active)
                    .unwrap_or(false)
            })
            .flat_map(|manifest| manifest.provides.iter().cloned())
            .collect()
    }
}
