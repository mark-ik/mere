use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use super::types::*;

#[derive(Debug, Clone, serde::Deserialize)]
struct DiskModManifest {
    mod_id: String,
    display_name: Option<String>,
    #[serde(default)]
    provides: Vec<String>,
    #[serde(default)]
    requires: Vec<String>,
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NativeModRegistration {
    pub manifest: fn() -> ModManifest,
}

inventory::collect!(NativeModRegistration);

pub fn discover_native_mods() -> Vec<ModManifest> {
    inventory::iter::<NativeModRegistration>
        .into_iter()
        .map(|registration| (registration.manifest)())
        .collect()
}

pub fn discover_mod_manifests(
    additional_manifests: impl IntoIterator<Item = ModManifest>,
) -> Vec<ModManifest> {
    let mut manifests = discover_native_mods();
    manifests.extend(additional_manifests);
    manifests
}

fn parse_mod_capability(raw: &str) -> Result<ModCapability, ModLoadPathError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "network" => Ok(ModCapability::Network),
        "filesystem" | "fs" => Ok(ModCapability::Filesystem),
        "identity" => Ok(ModCapability::Identity),
        "clipboard" => Ok(ModCapability::Clipboard),
        "exec" => Ok(ModCapability::Exec),
        other => Err(ModLoadPathError::InvalidCapability {
            capability: other.to_string(),
        }),
    }
}

fn candidate_manifest_paths(path: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![path.with_extension("wasm.toml")];
    if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
        candidates.push(path.with_file_name(format!("{stem}.mod.toml")));
    }
    candidates
}

fn validate_wasm_binary(path: &Path) -> Result<(), ModLoadPathError> {
    let bytes = std::fs::read(path).map_err(|error| ModLoadPathError::Io {
        path: path.to_path_buf(),
        reason: error.to_string(),
    })?;
    if bytes.len() < 4 || bytes[..4] != [0x00, 0x61, 0x73, 0x6d] {
        return Err(ModLoadPathError::InvalidWasmBinary(path.to_path_buf()));
    }
    Ok(())
}

pub(super) fn read_wasm_mod_from_path(path: &Path) -> Result<(ModManifest, WasmModSource), ModLoadPathError> {
    if path.extension().and_then(|ext| ext.to_str()) != Some("wasm") {
        return Err(ModLoadPathError::UnsupportedModPath(path.to_path_buf()));
    }

    validate_wasm_binary(path)?;

    let manifest_path = candidate_manifest_paths(path)
        .into_iter()
        .find(|candidate| candidate.exists())
        .ok_or_else(|| ModLoadPathError::MissingManifest(path.to_path_buf()))?;
    let manifest_raw =
        std::fs::read_to_string(&manifest_path).map_err(|error| ModLoadPathError::Io {
            path: manifest_path.clone(),
            reason: error.to_string(),
        })?;
    let disk_manifest: DiskModManifest =
        toml::from_str(&manifest_raw).map_err(|error| ModLoadPathError::InvalidManifest {
            path: manifest_path.clone(),
            reason: error.to_string(),
        })?;

    let capabilities = disk_manifest
        .capabilities
        .iter()
        .map(|entry| parse_mod_capability(entry))
        .collect::<Result<Vec<_>, _>>()?;
    let mod_id = disk_manifest.mod_id;
    let display_name = disk_manifest.display_name.unwrap_or_else(|| mod_id.clone());

    Ok((
        ModManifest::new(
            mod_id,
            display_name,
            ModType::Wasm,
            disk_manifest.provides,
            disk_manifest.requires,
            capabilities,
        ),
        WasmModSource {
            module_path: path.to_path_buf(),
            manifest_path,
        },
    ))
}

pub fn resolve_mod_load_order(
    manifests: &[ModManifest],
) -> Result<Vec<ModManifest>, ModDependencyError> {
    let mut id_to_manifest = HashMap::<String, ModManifest>::new();
    let mut provided_by = HashMap::<String, String>::new();

    for manifest in manifests {
        if id_to_manifest
            .insert(manifest.mod_id.clone(), manifest.clone())
            .is_some()
        {
            return Err(ModDependencyError::DuplicateModId(manifest.mod_id.clone()));
        }
        for provided in &manifest.provides {
            provided_by
                .entry(provided.clone())
                .or_insert_with(|| manifest.mod_id.clone());
        }
    }

    let mut indegree = HashMap::<String, usize>::new();
    let mut edges = HashMap::<String, HashSet<String>>::new();
    for id in id_to_manifest.keys() {
        indegree.insert(id.clone(), 0);
        edges.insert(id.clone(), HashSet::new());
    }

    for manifest in manifests {
        for requirement in &manifest.requires {
            let dependency_mod = provided_by.get(requirement).ok_or_else(|| {
                ModDependencyError::MissingRequirement {
                    mod_id: manifest.mod_id.clone(),
                    requirement: requirement.clone(),
                }
            })?;

            if dependency_mod == &manifest.mod_id {
                continue;
            }

            let deps = edges
                .get_mut(dependency_mod)
                .expect("dependency mod must exist");
            if deps.insert(manifest.mod_id.clone()) {
                *indegree
                    .get_mut(&manifest.mod_id)
                    .expect("mod indegree entry must exist") += 1;
            }
        }
    }

    let mut queue = VecDeque::new();
    for (id, degree) in &indegree {
        if *degree == 0 {
            queue.push_back(id.clone());
        }
    }

    let mut ordered = Vec::new();
    while let Some(id) = queue.pop_front() {
        let manifest = id_to_manifest
            .get(&id)
            .expect("mod id in queue must exist")
            .clone();
        ordered.push(manifest);

        if let Some(dependents) = edges.get(&id) {
            for dependent in dependents {
                let degree = indegree
                    .get_mut(dependent)
                    .expect("dependent indegree entry must exist");
                *degree = degree.saturating_sub(1);
                if *degree == 0 {
                    queue.push_back(dependent.clone());
                }
            }
        }
    }

    if ordered.len() != manifests.len() {
        let unresolved = indegree
            .into_iter()
            .filter_map(|(id, degree)| if degree > 0 { Some(id) } else { None })
            .collect::<Vec<_>>();
        return Err(ModDependencyError::DependencyCycle(unresolved));
    }

    Ok(ordered)
}

/// DI seam for the WASM mod runtime. The mod loader needs to
/// activate / deactivate WASM mods but the actual runtime
/// (wasmtime, wasmer, etc.) is host-side. This trait lets the
/// host inject the runtime as a dependency at registry construction
/// time, so the loader stays portable.
///
/// Slice 68. The default is `None` — a `ModRegistry` constructed
/// without a runtime can still discover + parse manifests, but
/// `load_all` returns an activation error for `ModType::Wasm` mods.
/// Hosts that support WASM call [`ModRegistry::with_wasm_runtime`]
/// at construction.
pub trait WasmModRuntime: Send + Sync {
    /// Activate a WASM mod. The runtime owns instantiation, WASI
    /// wiring, and any per-mod sandbox state.
    fn activate(
        &self,
        manifest: &ModManifest,
        source: &WasmModSource,
    ) -> Result<(), String>;
    /// Deactivate a previously-activated WASM mod by ID. Called on
    /// rollback (activation failed midway through a load batch) and
    /// on explicit unload.
    fn deactivate(&self, mod_id: &str) -> Result<(), String>;
}

/// DI seam for the native mod runtime. Native mods are functions
/// compiled into the binary (e.g. `crate::mods::native::nostrcore::activate`)
/// — the activation table is intrinsically host-side. This trait
/// lets the host expose the dispatch entrypoint (`activate(mod_id)`)
/// without the mod loader needing to know which native mods are
/// linked in.
///
/// Slice 68b. Like [`WasmModRuntime`], the default is `None` and
/// `load_all` returns an activation error for `ModType::Native`
/// mods until the host calls [`ModRegistry::with_native_runtime`].
pub trait NativeModRuntime: Send + Sync {
    /// Activate a native mod by ID. The host's activation table
    /// looks up the mod_id and dispatches to the mod's `activate` fn.
    fn activate(&self, mod_id: &str) -> Result<(), String>;
}

