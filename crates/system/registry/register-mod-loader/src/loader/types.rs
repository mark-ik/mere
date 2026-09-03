// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::path::PathBuf;

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
    /// Origin globs this mod's wasm binds as a DocumentScript (the "installed
    /// extension" auto-attach form). Empty = not a document-script mod. When
    /// non-empty, the mod's `module_path` is the component a host attaches over a
    /// page whose origin matches one of these globs (exact host or `*.suffix`).
    /// Defaults empty; set by the manifest reader, not [`ModManifest::new`].
    pub document_script_origins: Vec<String>,
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
            document_script_origins: Vec::new(),
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
        previous_capabilities: Option<register_viewer::ViewerSubsystemCapabilities>,
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

    pub fn rollback(reason: impl Into<String>, applied_records: Vec<ModExtensionRecord>) -> Self {
        Self {
            reason: reason.into(),
            applied_records,
        }
    }

    pub(super) fn into_parts(self) -> (String, Vec<ModExtensionRecord>) {
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
