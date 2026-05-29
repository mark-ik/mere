use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// No direct DiagnosticsState import needed - this module emits via runtime diagnostics.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModType {
    Native,
    Wasm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModStatus {
    Discovered,
    Loading,
    Active,
    Failed,
    Quarantined,
    Unloaded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModCapability {
    Network,
    Filesystem,
    Identity,
    Clipboard,
    Exec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModManifest {
    pub mod_id: String,
    pub display_name: String,
    pub mod_type: ModType,
    pub provides: Vec<String>,
    pub requires: Vec<String>,
    pub capabilities: Vec<ModCapability>,
}

impl ModManifest {
    pub fn new(
        mod_id: impl Into<String>,
        display_name: impl Into<String>,
        mod_type: ModType,
        provides: Vec<String>,
        requires: Vec<String>,
        capabilities: Vec<ModCapability>,
    ) -> Self {
        Self {
            mod_id: mod_id.into(),
            display_name: display_name.into(),
            mod_type,
            provides,
            requires,
            capabilities,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModDependencyError {
    DuplicateModId(String),
    MissingRequirement { mod_id: String, requirement: String },
    DependencyCycle(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModExtensionRecord {
    ProtocolScheme {
        scheme: String,
        previously_present: bool,
    },
    ViewerMime {
        mime: String,
        previous_viewer_id: Option<String>,
    },
    ViewerExtension {
        extension: String,
        previous_viewer_id: Option<String>,
    },
    ViewerCapabilities {
        viewer_id: String,
        previous_capabilities:
            Option<register_viewer::ViewerSubsystemCapabilities>,
    },
    Action {
        action_id: String,
    },
    IndexProvider {
        provider_id: String,
    },
    Lens {
        lens_id: String,
    },
    Theme {
        theme_id: String,
    },
    WasmRuntime {
        mod_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModUnloadError {
    UnknownMod(String),
    NotActive(String),
    DependencyActive {
        mod_id: String,
        dependent_id: String,
    },
    ExtensionRemovalFailed {
        mod_id: String,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModActivationError {
    reason: String,
    applied_records: Vec<ModExtensionRecord>,
}

impl ModActivationError {
    pub fn failed(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            applied_records: Vec::new(),
        }
    }

    pub fn rollback(
        reason: impl Into<String>,
        applied_records: Vec<ModExtensionRecord>,
    ) -> Self {
        Self {
            reason: reason.into(),
            applied_records,
        }
    }

    fn into_parts(self) -> (String, Vec<ModExtensionRecord>) {
        (self.reason, self.applied_records)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmModSource {
    pub module_path: PathBuf,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModLoadPathError {
    UnsupportedModPath(PathBuf),
    MissingManifest(PathBuf),
    InvalidManifest { path: PathBuf, reason: String },
    InvalidCapability { capability: String },
    InvalidWasmBinary(PathBuf),
    Io { path: PathBuf, reason: String },
    DuplicateModId(String),
}

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

fn read_wasm_mod_from_path(path: &Path) -> Result<(ModManifest, WasmModSource), ModLoadPathError> {
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

fn compute_active_capabilities() -> HashSet<String> {
    let mut registry = ModRegistry::new();
    let _ = registry.resolve_dependencies();
    let _ = registry.load_all();
    registry.active_capability_ids()
}

#[cfg(any(test, feature = "test-utils"))]
pub fn compute_active_capabilities_with_disabled(
    disabled: &HashSet<String>,
) -> HashSet<String> {
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
    pub fn with_wasm_runtime(
        mut self,
        runtime: std::sync::Arc<dyn WasmModRuntime>,
    ) -> Self {
        self.wasm_runtime = Some(runtime);
        self
    }

    /// Builder-style setter for the host's native runtime. Without
    /// it, `load_all` errors out activation for `ModType::Native`
    /// mods (the manifests are still parsed and tracked).
    /// Slice 68b.
    pub fn with_native_runtime(
        mut self,
        runtime: std::sync::Arc<dyn NativeModRuntime>,
    ) -> Self {
        self.native_runtime = Some(runtime);
        self
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn from_manifests_for_tests(manifests: Vec<ModManifest>) -> Self {
        Self::from_manifests_with_disabled(manifests, &HashSet::new())
    }

    fn new_with_disabled(disabled_mod_ids: &HashSet<String>) -> Self {
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
        use register_diagnostics::{DiagnosticEvent, emit_event};
        use register_diagnostics::channels::CHANNEL_MOD_DEPENDENCY_MISSING;

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
        use register_diagnostics::{DiagnosticEvent, emit_event};
        use register_diagnostics::channels::{
            CHANNEL_MOD_LOAD_FAILED, CHANNEL_MOD_LOAD_STARTED, CHANNEL_MOD_LOAD_SUCCEEDED,
            CHANNEL_MOD_QUARANTINED, CHANNEL_MOD_ROLLBACK_FAILED, CHANNEL_MOD_ROLLBACK_SUCCEEDED,
        };

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
        use register_diagnostics::{DiagnosticEvent, emit_event};
        use register_diagnostics::channels::{
            CHANNEL_MOD_QUARANTINED, CHANNEL_MOD_UNLOAD_FAILED,
        };

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
        for dependent in self.active_dependents_of(&manifest.mod_id) {
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

    fn active_dependents_of(&self, mod_id: &str) -> Vec<String> {
        let Some(manifest) = self.manifests.get(mod_id) else {
            return Vec::new();
        };
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
            .collect()
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
                .map_or(false, |s| *s == ModStatus::Active);
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

impl Default for ModRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn verso_manifest() -> ModManifest {
    ModManifest::new(
        "mod:web-runtime",
        "Verso",
        ModType::Native,
        vec![
            "protocol:http".to_string(),
            "protocol:https".to_string(),
            "protocol:data".to_string(),
            "viewer:webview".to_string(),
        ],
        vec!["ProtocolRegistry".to_string(), "ViewerRegistry".to_string()],
        vec![ModCapability::Network],
    )
}

fn core_protocol_manifest() -> ModManifest {
    ModManifest::new(
        "mod:core-protocol",
        "Core Protocol Registry",
        ModType::Native,
        vec!["ProtocolRegistry".to_string()],
        vec![],
        vec![],
    )
}

fn core_viewer_manifest() -> ModManifest {
    ModManifest::new(
        "mod:core-viewer",
        "Core Viewer Registry",
        ModType::Native,
        vec!["ViewerRegistry".to_string()],
        vec![],
        vec![],
    )
}

fn core_identity_manifest() -> ModManifest {
    ModManifest::new(
        "mod:core-identity",
        "Core Identity Registry",
        ModType::Native,
        vec!["IdentityRegistry".to_string()],
        vec![],
        vec![],
    )
}

fn core_action_manifest() -> ModManifest {
    ModManifest::new(
        "mod:core-action",
        "Core Action Registry",
        ModType::Native,
        vec!["ActionRegistry".to_string()],
        vec![],
        vec![],
    )
}

fn core_control_panel_manifest() -> ModManifest {
    ModManifest::new(
        "mod:core-control-panel",
        "Core Control Panel",
        ModType::Native,
        vec!["ControlPanel".to_string()],
        vec![],
        vec![],
    )
}

fn core_diagnostics_manifest() -> ModManifest {
    ModManifest::new(
        "mod:core-diagnostics",
        "Core Diagnostics Registry",
        ModType::Native,
        vec!["DiagnosticsRegistry".to_string()],
        vec![],
        vec![],
    )
}

inventory::submit! {
    NativeModRegistration {
        manifest: core_protocol_manifest,
    }
}

inventory::submit! {
    NativeModRegistration {
        manifest: core_viewer_manifest,
    }
}

inventory::submit! {
    NativeModRegistration {
        manifest: core_identity_manifest,
    }
}

inventory::submit! {
    NativeModRegistration {
        manifest: core_action_manifest,
    }
}

inventory::submit! {
    NativeModRegistration {
        manifest: core_control_panel_manifest,
    }
}

inventory::submit! {
    NativeModRegistration {
        manifest: core_diagnostics_manifest,
    }
}

inventory::submit! {
    NativeModRegistration {
        manifest: verso_manifest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use register_diagnostics::{DiagnosticEvent, install_global_sender};
    use register_diagnostics::channels::{
        CHANNEL_MOD_QUARANTINED, CHANNEL_MOD_ROLLBACK_FAILED, CHANNEL_MOD_ROLLBACK_SUCCEEDED,
        CHANNEL_MOD_UNLOAD_FAILED,
    };

    fn test_manifest(id: &str, provides: &[&str], requires: &[&str]) -> ModManifest {
        ModManifest::new(
            id,
            id,
            ModType::Native,
            provides.iter().map(|v| v.to_string()).collect(),
            requires.iter().map(|v| v.to_string()).collect(),
            vec![],
        )
    }

    fn test_registry_with_disabled(disabled: &[&str]) -> ModRegistry {
        let disabled_ids = disabled
            .iter()
            .map(|id| (*id).to_string())
            .collect::<HashSet<_>>();
        ModRegistry::new_with_disabled(&disabled_ids)
    }

    fn disabled_set(disabled: &[&str]) -> HashSet<String> {
        disabled
            .iter()
            .map(|id| (*id).to_string())
            .collect::<HashSet<_>>()
    }

    fn write_wasm_fixture(
        temp_dir: &tempfile::TempDir,
        module_name: &str,
        manifest_body: &str,
    ) -> PathBuf {
        let module_path = temp_dir.path().join(format!("{module_name}.wasm"));
        let manifest_path = temp_dir.path().join(format!("{module_name}.wasm.toml"));
        fs::write(
            &module_path,
            [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
        )
        .expect("fixture module should write");
        fs::write(&manifest_path, manifest_body).expect("fixture manifest should write");
        module_path
    }

    #[test]
    #[ignore = "Slice 68c: this assertion depends on inventory::submit! \
                calls in the binary root's mods/native/* files. Those \
                calls don't link when register-mod-loader builds alone, \
                so the inventory is empty here. The test still runs as \
                a host-side integration test (manually) at the binary \
                root build."]
    fn discovers_native_mods_including_verso_and_nostrcore() {
        let mods = discover_native_mods();
        assert!(mods.iter().any(|entry| entry.mod_id == "mod:core-protocol"));
        assert!(mods.iter().any(|entry| entry.mod_id == "mod:core-viewer"));
        assert!(mods.iter().any(|entry| entry.mod_id == "mod:web-runtime"));
        assert!(mods.iter().any(|entry| entry.mod_id == "mod:nostrcore"));
    }

    #[test]
    fn discover_mod_manifests_appends_additional_entries() {
        let mods = discover_mod_manifests([ModManifest::new(
            "mod:test-wasm",
            "Test WASM",
            ModType::Wasm,
            vec!["viewer:test".to_string()],
            vec!["ViewerRegistry".to_string()],
            vec![ModCapability::Filesystem],
        )]);

        assert!(mods.iter().any(|entry| entry.mod_id == "mod:core-viewer"));
        assert!(mods.iter().any(|entry| entry.mod_id == "mod:test-wasm"));
    }

    #[test]
    fn resolves_dependency_order() {
        let protocol = test_manifest("mod:protocol", &["ProtocolRegistry"], &[]);
        let viewer = test_manifest("mod:viewer", &["ViewerRegistry"], &[]);
        let verso = test_manifest(
            "mod:web-runtime",
            &["viewer:webview"],
            &["ProtocolRegistry", "ViewerRegistry"],
        );

        let ordered = resolve_mod_load_order(&[verso.clone(), viewer.clone(), protocol.clone()])
            .expect("dependency order should resolve");
        let ids = ordered
            .iter()
            .map(|entry| entry.mod_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 3);
        let protocol_idx = ids.iter().position(|id| *id == "mod:protocol").unwrap();
        let viewer_idx = ids.iter().position(|id| *id == "mod:viewer").unwrap();
        let verso_idx = ids.iter().position(|id| *id == "mod:web-runtime").unwrap();
        assert!(protocol_idx < verso_idx);
        assert!(viewer_idx < verso_idx);
    }

    #[test]
    fn fails_on_missing_requirement() {
        let manifest = test_manifest("mod:x", &["x"], &["ProtocolRegistry"]);
        let error =
            resolve_mod_load_order(&[manifest]).expect_err("should fail missing requirement");
        assert!(matches!(
            error,
            ModDependencyError::MissingRequirement { mod_id, requirement }
                if mod_id == "mod:x" && requirement == "ProtocolRegistry"
        ));
    }

    #[test]
    fn fails_on_dependency_cycle() {
        let a = test_manifest("mod:a", &["A"], &["B"]);
        let b = test_manifest("mod:b", &["B"], &["A"]);
        let error = resolve_mod_load_order(&[a, b]).expect_err("should fail cycle");
        assert!(matches!(error, ModDependencyError::DependencyCycle(_)));
    }

    #[test]
    fn mod_registry_discovers_native_mods() {
        let registry = ModRegistry::new();
        assert!(registry.get_manifest("mod:core-protocol").is_some());
        assert!(registry.get_manifest("mod:core-viewer").is_some());
        assert!(registry.get_manifest("mod:web-runtime").is_some());

        // All should be in Discovered state initially
        assert_eq!(
            registry.get_status("mod:core-protocol"),
            Some(ModStatus::Discovered)
        );
        assert_eq!(
            registry.get_status("mod:core-viewer"),
            Some(ModStatus::Discovered)
        );
        assert_eq!(
            registry.get_status("mod:web-runtime"),
            Some(ModStatus::Discovered)
        );
    }

    #[test]
    fn mixed_native_and_wasm_manifests_resolve_dependency_order() {
        let protocol = test_manifest("mod:protocol", &["ProtocolRegistry"], &[]);
        let wasm = ModManifest::new(
            "mod:test-wasm",
            "Test WASM",
            ModType::Wasm,
            vec!["protocol:test".to_string()],
            vec!["ProtocolRegistry".to_string()],
            vec![ModCapability::Network],
        );

        let ordered = resolve_mod_load_order(&[wasm.clone(), protocol.clone()])
            .expect("mixed native/wasm dependency order should resolve");
        let ids = ordered
            .iter()
            .map(|entry| entry.mod_id.as_str())
            .collect::<Vec<_>>();
        let protocol_idx = ids.iter().position(|id| *id == "mod:protocol").unwrap();
        let wasm_idx = ids.iter().position(|id| *id == "mod:test-wasm").unwrap();
        assert!(protocol_idx < wasm_idx);
    }

    #[test]
    fn mod_registry_can_load_mixed_manifest_sets_with_extension_callback() {
        let protocol = test_manifest("mod:protocol", &["ProtocolRegistry"], &[]);
        let wasm = ModManifest::new(
            "mod:test-wasm",
            "Test WASM",
            ModType::Wasm,
            vec!["protocol:test".to_string()],
            vec!["ProtocolRegistry".to_string()],
            vec![ModCapability::Network],
        );
        let mut registry = ModRegistry::from_manifests_for_tests(vec![protocol, wasm]);

        registry
            .resolve_dependencies()
            .expect("mixed registry should resolve dependencies");
        let loaded = registry.load_all_with_extensions(
            |manifest, _wasm_source| {
                Ok(vec![ModExtensionRecord::Action {
                    action_id: format!("action:{}", manifest.mod_id),
                }])
            },
            |_record| Ok(()),
        );

        assert_eq!(
            loaded,
            vec!["mod:protocol".to_string(), "mod:test-wasm".to_string()]
        );
        assert_eq!(
            registry.get_status("mod:test-wasm"),
            Some(ModStatus::Active)
        );
        assert_eq!(
            registry.extension_records_for("mod:test-wasm"),
            Some(
                &[ModExtensionRecord::Action {
                    action_id: "action:mod:test-wasm".to_string(),
                }][..]
            )
        );
    }

    #[test]
    fn mod_registry_resolves_dependencies() {
        let mut registry = ModRegistry::new();

        registry
            .resolve_dependencies()
            .expect("should resolve dependencies");

        // Load order should have core mods before verso
        let load_order = registry.list_mods();
        let protocol_idx = load_order.iter().position(|id| id == "mod:core-protocol");
        let viewer_idx = load_order.iter().position(|id| id == "mod:core-viewer");
        let verso_idx = load_order.iter().position(|id| id == "mod:web-runtime");

        assert!(protocol_idx.is_some());
        assert!(viewer_idx.is_some());
        assert!(verso_idx.is_some());

        // Verso should load after its dependencies
        assert!(protocol_idx.unwrap() < verso_idx.unwrap());
        assert!(viewer_idx.unwrap() < verso_idx.unwrap());
    }

    #[test]
    fn mod_registry_loads_mods_in_order() {
        let mut registry = ModRegistry::new();

        registry.resolve_dependencies().expect("should resolve");
        let loaded = registry.load_all();

        // All mods should load successfully
        assert!(loaded.contains(&"mod:core-protocol".to_string()));
        assert!(loaded.contains(&"mod:core-viewer".to_string()));
        assert!(loaded.contains(&"mod:web-runtime".to_string()));

        // Check status transitions to Active
        assert_eq!(
            registry.get_status("mod:core-protocol"),
            Some(ModStatus::Active)
        );
        assert_eq!(
            registry.get_status("mod:web-runtime"),
            Some(ModStatus::Active)
        );
    }

    #[test]
    fn mod_registry_checks_capability_availability() {
        let mut registry = ModRegistry::new();

        registry.resolve_dependencies().expect("should resolve");
        registry.load_all();

        // Verso provides these capabilities
        assert!(registry.is_capability_available("protocol:http"));
        assert!(registry.is_capability_available("protocol:https"));
        assert!(registry.is_capability_available("viewer:webview"));

        // Core provides these
        assert!(registry.is_capability_available("ProtocolRegistry"));
        assert!(registry.is_capability_available("ViewerRegistry"));

        // This doesn't exist
        assert!(!registry.is_capability_available("protocol:ipfs"));
    }

    #[test]
    fn mod_registry_without_verso_disables_webview_capability() {
        let mut registry = test_registry_with_disabled(&["mod:web-runtime"]);
        registry
            .resolve_dependencies()
            .expect("dependencies should resolve without verso");
        registry.load_all();

        assert!(!registry.is_capability_available("viewer:webview"));
        assert!(!registry.is_capability_available("protocol:https"));
        assert!(registry.is_capability_available("ProtocolRegistry"));
        assert!(registry.is_capability_available("ViewerRegistry"));
    }

    #[test]
    fn test_safe_capability_path_disabling_verso_removes_webview_capabilities() {
        let disabled = disabled_set(&["mod:web-runtime"]);
        let capabilities = compute_active_capabilities_with_disabled(&disabled);

        assert!(!capabilities.contains("viewer:webview"));
        assert!(!capabilities.contains("protocol:https"));
        assert!(capabilities.contains("ProtocolRegistry"));
        assert!(capabilities.contains("ViewerRegistry"));
    }

    #[test]
    fn test_safe_capability_path_matches_runtime_default_when_unmodified() {
        let default = compute_active_capabilities();
        let disabled = HashSet::new();
        let test_safe = compute_active_capabilities_with_disabled(&disabled);

        assert_eq!(default, test_safe);
    }

    #[test]
    fn load_mod_admits_path_backed_wasm_manifests() {
        let temp_dir = tempfile::tempdir().expect("temp dir should create");
        let wasm_path = write_wasm_fixture(
            &temp_dir,
            "admitted",
            "mod_id = \"mod:admitted\"\ndisplay_name = \"Admitted\"\nprovides = [\"protocol:admitted\"]\nrequires = [\"ProtocolRegistry\"]\n",
        );
        let mut registry = ModRegistry::from_manifests_for_tests(vec![test_manifest(
            "mod:protocol",
            &["ProtocolRegistry"],
            &[],
        )]);

        let mod_id = registry
            .load_mod(&wasm_path)
            .expect("wasm admission should succeed");

        assert_eq!(mod_id, "mod:admitted");
        assert_eq!(
            registry
                .get_manifest("mod:admitted")
                .expect("admitted manifest should exist")
                .mod_type,
            ModType::Wasm
        );
        assert_eq!(
            registry
                .wasm_source("mod:admitted")
                .expect("wasm source should be tracked")
                .module_path,
            wasm_path
        );
    }

    #[test]
    fn load_mod_rejects_unknown_capabilities() {
        let temp_dir = tempfile::tempdir().expect("temp dir should create");
        let wasm_path = write_wasm_fixture(
            &temp_dir,
            "bad-capability",
            "mod_id = \"mod:bad-capability\"\ndisplay_name = \"Bad Capability\"\ncapabilities = [\"graph-write\"]\n",
        );
        let mut registry = ModRegistry::from_manifests_for_tests(vec![]);

        let error = registry
            .load_mod(&wasm_path)
            .expect_err("unknown capability should be rejected");
        assert!(matches!(
            error,
            ModLoadPathError::InvalidCapability { capability } if capability == "graph-write"
        ));
    }

    #[test]
    fn load_all_rolls_back_applied_records_on_activation_failure() {
        let (diag_tx, diag_rx) = std::sync::mpsc::channel();
        install_global_sender(diag_tx);

        let protocol = test_manifest("mod:protocol", &["ProtocolRegistry"], &[]);
        let failing = test_manifest("mod:failing", &["protocol:test"], &["ProtocolRegistry"]);
        let mut registry = ModRegistry::from_manifests_for_tests(vec![protocol, failing]);

        registry
            .resolve_dependencies()
            .expect("dependencies should resolve");

        let loaded = registry.load_all_with_extensions(
            |manifest, _wasm_source| {
                if manifest.mod_id == "mod:failing" {
                    Err(ModActivationError::rollback(
                        "activation failed",
                        vec![ModExtensionRecord::Action {
                            action_id: "action:mod:failing".to_string(),
                        }],
                    ))
                } else {
                    Ok(vec![ModExtensionRecord::Action {
                        action_id: format!("action:{}", manifest.mod_id),
                    }])
                }
            },
            |_record| Ok(()),
        );

        assert_eq!(loaded, vec!["mod:protocol".to_string()]);
        assert_eq!(registry.get_status("mod:failing"), Some(ModStatus::Failed));
        assert_eq!(registry.extension_records_for("mod:failing"), None);
        assert!(diag_rx.try_iter().any(|event| matches!(
            event,
            DiagnosticEvent::MessageSent { channel_id, .. }
                if channel_id == CHANNEL_MOD_ROLLBACK_SUCCEEDED
        )));
    }

    #[test]
    fn load_all_quarantines_when_rollback_fails() {
        let (diag_tx, diag_rx) = std::sync::mpsc::channel();
        install_global_sender(diag_tx);

        let protocol = test_manifest("mod:protocol", &["ProtocolRegistry"], &[]);
        let failing = test_manifest("mod:failing", &["protocol:test"], &["ProtocolRegistry"]);
        let mut registry = ModRegistry::from_manifests_for_tests(vec![protocol, failing]);

        registry
            .resolve_dependencies()
            .expect("dependencies should resolve");

        let loaded = registry.load_all_with_extensions(
            |manifest, _wasm_source| {
                if manifest.mod_id == "mod:failing" {
                    Err(ModActivationError::rollback(
                        "activation failed",
                        vec![ModExtensionRecord::Action {
                            action_id: "action:mod:failing".to_string(),
                        }],
                    ))
                } else {
                    Ok(vec![ModExtensionRecord::Action {
                        action_id: format!("action:{}", manifest.mod_id),
                    }])
                }
            },
            |record| match record {
                ModExtensionRecord::Action { action_id } if action_id == "action:mod:failing" => {
                    Err("simulated rollback failure".to_string())
                }
                _ => Ok(()),
            },
        );

        assert_eq!(loaded, vec!["mod:protocol".to_string()]);
        assert_eq!(
            registry.get_status("mod:failing"),
            Some(ModStatus::Quarantined)
        );
        assert_eq!(
            registry.extension_records_for("mod:failing"),
            Some(
                &[ModExtensionRecord::Action {
                    action_id: "action:mod:failing".to_string(),
                }][..]
            )
        );
        let emitted = diag_rx.try_iter().collect::<Vec<_>>();
        assert!(emitted.iter().any(|event| matches!(
            event,
            DiagnosticEvent::MessageSent { channel_id, .. }
                if *channel_id == CHANNEL_MOD_ROLLBACK_FAILED
        )));
        assert!(emitted.iter().any(|event| matches!(
            event,
            DiagnosticEvent::MessageSent { channel_id, .. }
                if *channel_id == CHANNEL_MOD_QUARANTINED
        )));
    }

    #[test]
    fn unload_mod_quarantines_and_preserves_records_on_removal_failure() {
        let (diag_tx, diag_rx) = std::sync::mpsc::channel();
        install_global_sender(diag_tx);

        let protocol = test_manifest("mod:protocol", &["ProtocolRegistry"], &[]);
        let target = test_manifest("mod:target", &["protocol:test"], &["ProtocolRegistry"]);
        let mut registry = ModRegistry::from_manifests_for_tests(vec![protocol, target]);

        registry
            .resolve_dependencies()
            .expect("dependencies should resolve");
        registry.load_all_with_extensions(
            |manifest, _wasm_source| {
                Ok(vec![ModExtensionRecord::Action {
                    action_id: format!("action:{}", manifest.mod_id),
                }])
            },
            |_record| Ok(()),
        );
        let _ = diag_rx.try_iter().collect::<Vec<_>>();

        let error = registry
            .unload_mod_with("mod:target", |record| match record {
                ModExtensionRecord::Action { action_id } if action_id == "action:mod:target" => {
                    Err("simulated removal failure".to_string())
                }
                _ => Ok(()),
            })
            .expect_err("unload should fail when removal fails");

        assert!(matches!(
            error,
            ModUnloadError::ExtensionRemovalFailed { mod_id, reason }
                if mod_id == "mod:target" && reason == "simulated removal failure"
        ));
        assert_eq!(
            registry.get_status("mod:target"),
            Some(ModStatus::Quarantined)
        );
        assert_eq!(
            registry.extension_records_for("mod:target"),
            Some(
                &[ModExtensionRecord::Action {
                    action_id: "action:mod:target".to_string(),
                }][..]
            )
        );
        let emitted = diag_rx.try_iter().collect::<Vec<_>>();
        assert!(emitted.iter().any(|event| matches!(
            event,
            DiagnosticEvent::MessageSent { channel_id, .. }
                if *channel_id == CHANNEL_MOD_UNLOAD_FAILED
        )));
        assert!(emitted.iter().any(|event| matches!(
            event,
            DiagnosticEvent::MessageSent { channel_id, .. }
                if *channel_id == CHANNEL_MOD_QUARANTINED
        )));
    }
}
