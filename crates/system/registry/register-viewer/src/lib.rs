// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Viewer registry — late-binding map of viewer IDs to viewer
//! capabilities (mime/extension routing, render mode, conformance
//! declarations) plus the `ViewerHandler` trait that concrete
//! viewers implement.
//!
//! Extracted from `registries/atomic/viewer.rs` per Slice 67. Only
//! the portable parts (ViewerRegistry + capabilities + descriptor +
//! handler trait + selection logic + render mode) move here. The
//! egui-host-gated `EmbeddedViewer*` trait family
//! (EmbeddedViewerOutput, EmbeddedViewerContext, EmbeddedViewerRegistry,
//! SettingsViewer, FallbackViewer) stays in tree at the original
//! path because it depends on `crate::app::GraphIntent`,
//! `crate::app::AppCommand`, `crate::prefs::FileAccessPolicy`, and
//! `crate::shell::desktop::workbench::*` — all binary-root concerns
//! that haven't been promoted.
//!
//! Dependencies in mere:
//! - `register_layout::CapabilityDeclaration` (conformance vocabulary)
//! - `kernel::address::{AddressKind, address_kind_from_url}`
//!
//! The donor also routed internal `verso://settings/...` / `verso://frame/...`
//! addresses through a `VersoAddress` type. That internal-address scheme was
//! retired in the move to mere (only the clip route survives, as
//! `AddressKind::GraphshellClip`); settings/frame routing is now a host /
//! mere-domain chrome concern, not the portable viewer registry's. The
//! `select_for_uri` internal branch was dropped accordingly — scheme / MIME /
//! extension / magic-byte / fallback selection is unchanged.

use std::collections::HashMap;

use kernel::address::{AddressKind, address_kind_from_url};
use register_layout::CapabilityDeclaration;

pub const VIEWER_ID_FALLBACK: &str = "viewer:webview";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ViewerRenderMode {
    CompositedTexture,
    NativeOverlay,
    #[serde(alias = "EmbeddedEgui")]
    EmbeddedHost,
    Placeholder,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ViewerCapability {
    pub viewer_id: String,
    pub supported_mime_types: Vec<String>,
    pub supported_extensions: Vec<String>,
    pub render_mode: ViewerRenderMode,
    pub overlay_affordance: bool,
    #[serde(flatten)]
    pub subsystems: ViewerSubsystemCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ViewerSubsystemCapabilities {
    pub accessibility: CapabilityDeclaration,
    pub security: CapabilityDeclaration,
    pub storage: CapabilityDeclaration,
    pub history: CapabilityDeclaration,
}

impl ViewerSubsystemCapabilities {
    pub fn full() -> Self {
        Self {
            accessibility: CapabilityDeclaration::full(),
            security: CapabilityDeclaration::full(),
            storage: CapabilityDeclaration::full(),
            history: CapabilityDeclaration::full(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ViewerDescriptor {
    pub uri: String,
    pub mime_hint: Option<String>,
}

pub trait ViewerHandler: Send + Sync {
    fn viewer_id(&self) -> &'static str;
    fn can_render(&self, descriptor: &ViewerDescriptor) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ViewerSelection {
    pub viewer_id: &'static str,
    pub fallback_used: bool,
    pub matched_by: &'static str,
    pub capabilities: ViewerSubsystemCapabilities,
}

#[derive(Debug, Clone)]
pub struct ViewerRegistry {
    mime_handlers: HashMap<String, &'static str>,
    extension_handlers: HashMap<String, &'static str>,
    capabilities: HashMap<&'static str, ViewerSubsystemCapabilities>,
    fallback_viewer_id: &'static str,
}

impl ViewerRegistry {
    pub fn new(fallback_viewer_id: &'static str) -> Self {
        Self {
            mime_handlers: HashMap::new(),
            extension_handlers: HashMap::new(),
            capabilities: HashMap::new(),
            fallback_viewer_id,
        }
    }

    pub fn register_capabilities(
        &mut self,
        viewer_id: &'static str,
        capabilities: ViewerSubsystemCapabilities,
    ) -> Option<ViewerSubsystemCapabilities> {
        self.capabilities.insert(viewer_id, capabilities)
    }

    pub fn capabilities_for(&self, viewer_id: &'static str) -> ViewerSubsystemCapabilities {
        self.capabilities
            .get(viewer_id)
            .cloned()
            .unwrap_or_else(ViewerSubsystemCapabilities::full)
    }

    pub fn describe_viewer(&self, viewer_id: &str) -> Option<ViewerCapability> {
        let normalized = viewer_id.trim();
        if normalized.is_empty() {
            return None;
        }

        let known = self.capabilities.contains_key(normalized)
            || self
                .mime_handlers
                .values()
                .any(|registered| *registered == normalized)
            || self
                .extension_handlers
                .values()
                .any(|registered| *registered == normalized);
        if !known {
            return None;
        }

        let mut supported_mime_types = self
            .mime_handlers
            .iter()
            .filter_map(|(mime, registered)| (*registered == normalized).then_some(mime.clone()))
            .collect::<Vec<_>>();
        supported_mime_types.sort();
        supported_mime_types.dedup();

        let mut supported_extensions = self
            .extension_handlers
            .iter()
            .filter_map(|(extension, registered)| {
                (*registered == normalized).then_some(extension.clone())
            })
            .collect::<Vec<_>>();
        supported_extensions.sort();
        supported_extensions.dedup();

        Some(ViewerCapability {
            viewer_id: normalized.to_string(),
            supported_mime_types,
            supported_extensions,
            render_mode: render_mode_for_viewer_id(normalized),
            overlay_affordance: overlay_affordance_for_viewer_id(normalized),
            subsystems: self
                .capabilities
                .get(normalized)
                .cloned()
                .unwrap_or_else(ViewerSubsystemCapabilities::full),
        })
    }

    fn selection(
        &self,
        viewer_id: &'static str,
        fallback_used: bool,
        matched_by: &'static str,
    ) -> ViewerSelection {
        ViewerSelection {
            viewer_id,
            fallback_used,
            matched_by,
            capabilities: self.capabilities_for(viewer_id),
        }
    }

    pub fn register_mime(&mut self, mime: &str, viewer_id: &'static str) -> Option<&'static str> {
        self.mime_handlers
            .insert(mime.to_ascii_lowercase(), viewer_id)
    }

    pub fn unregister_mime(&mut self, mime: &str) -> Option<&'static str> {
        self.mime_handlers.remove(&mime.to_ascii_lowercase())
    }

    pub fn register_extension(
        &mut self,
        extension: &str,
        viewer_id: &'static str,
    ) -> Option<&'static str> {
        self.extension_handlers
            .insert(extension.to_ascii_lowercase(), viewer_id)
    }

    pub fn unregister_extension(&mut self, extension: &str) -> Option<&'static str> {
        self.extension_handlers
            .remove(&extension.to_ascii_lowercase())
    }

    pub fn unregister_capabilities(
        &mut self,
        viewer_id: &'static str,
    ) -> Option<ViewerSubsystemCapabilities> {
        self.capabilities.remove(viewer_id)
    }

    pub fn select_for_uri(&self, uri: &str, mime_hint: Option<&str>) -> ViewerSelection {
        // Internal `verso://settings`/`verso://frame` routing was dropped in
        // the mere port (retired scheme; host/mere-domain chrome owns it now).
        if let Some(viewer_id) = middlenet_viewer_for_uri_scheme(uri) {
            return self.selection(viewer_id, false, "scheme");
        }

        if let Some(mime) = mime_hint.map(|m| m.to_ascii_lowercase())
            && let Some(viewer_id) = self.mime_handlers.get(&mime)
        {
            return self.selection(viewer_id, false, "mime");
        }

        if let Some(ext) = extract_extension(uri)
            && let Some(viewer_id) = self.extension_handlers.get(ext)
        {
            return self.selection(viewer_id, false, "extension");
        }

        // Magic-byte fallback for local files when no MIME hint and no extension match.
        if mime_hint.is_none() {
            if let AddressKind::File = address_kind_from_url(uri) {
                // Slice 67: inlined the file:// → local-path conversion
                // (was crate::shell::desktop::workbench::local_file_access::file_path_from_node_url).
                // Pure URL parsing, no host coupling.
                if let Ok(path) = url::Url::parse(uri)
                    .map_err(|_| ())
                    .and_then(|u| u.to_file_path().map_err(|_| ()))
                {
                    if let Ok(mut file) = std::fs::File::open(&path) {
                        let mut buf = [0u8; 512];
                        let n = std::io::Read::read(&mut file, &mut buf).unwrap_or(0);
                        if let Some(kind) = infer::get(&buf[..n]) {
                            let detected_mime = kind.mime_type().to_ascii_lowercase();
                            if let Some(viewer_id) = self.mime_handlers.get(&detected_mime) {
                                return self.selection(viewer_id, false, "magic");
                            }
                        }
                    }
                }
            }
        }

        // For non-HTTP address kinds (local files, custom schemes), avoid falling
        // back to the web renderer. Use plaintext only if the configured fallback
        // is the composited viewer; otherwise respect the registry's own fallback.
        let fallback = match address_kind_from_url(uri) {
            AddressKind::File | AddressKind::Unknown
                if self.fallback_viewer_id == "viewer:webview" =>
            {
                "viewer:plaintext"
            }
            _ => self.fallback_viewer_id,
        };
        self.selection(fallback, true, "fallback")
    }

    /// Select a viewer based on MIME hint and address kind.
    ///
    /// Selection order:
    /// 1. MIME-based lookup (highest priority when a hint is available).
    /// 2. Address-kind heuristic — `Http` falls back to the registry default (Servo webview);
    ///    `File` and `Custom` fall back to `viewer:plaintext` as a safe last resort.
    ///
    /// This method does **not** consult `viewer_id_override` or workspace defaults;
    /// those are the caller's responsibility and should be applied before calling this.
    pub fn select_for(&self, mime: Option<&str>, kind: AddressKind) -> &'static str {
        // 1. MIME-based lookup.
        if let Some(mime_val) = mime.map(|m| m.to_ascii_lowercase())
            && let Some(viewer_id) = self.mime_handlers.get(&mime_val)
        {
            return viewer_id;
        }

        // 2. Address-kind heuristic fallback.
        match kind {
            // HTTP/HTTPS: use the registry's configured default (normally viewer:webview).
            AddressKind::Http => self.fallback_viewer_id,
            // Local files and unknown/non-web schemes: plaintext is the safe fallback.
            AddressKind::File
            | AddressKind::Unknown
            | AddressKind::Data
            | AddressKind::GraphshellClip
            | AddressKind::Directory => "viewer:plaintext",
        }
    }

    pub fn core_seed() -> Self {
        let mut registry = Self::new("viewer:metadata");
        registry.register_mime("text/plain", "viewer:plaintext");
        registry.register_mime("application/octet-stream", "viewer:metadata");
        registry.register_extension("txt", "viewer:plaintext");
        registry.register_capabilities("viewer:plaintext", ViewerSubsystemCapabilities::full());
        registry.register_capabilities("viewer:metadata", ViewerSubsystemCapabilities::full());
        registry
    }
}

impl Default for ViewerRegistry {
    fn default() -> Self {
        let mut registry = Self::new(VIEWER_ID_FALLBACK);
        registry.register_mime("application/x-graphshell-settings", "viewer:settings");
        registry.register_mime("application/x-graphshell-internal", "viewer:webview");
        registry.register_mime("text/html", "viewer:webview");
        registry.register_mime("text/gemini", "viewer:middlenet");
        registry.register_mime("text/x-gemini", "viewer:middlenet");
        registry.register_mime("application/gophermap", "viewer:middlenet");
        registry.register_mime("application/x-gophermap", "viewer:middlenet");
        registry.register_mime("text/x-gophermap", "viewer:middlenet");
        registry.register_mime("application/x-finger", "viewer:middlenet");
        registry.register_mime("application/rss+xml", "viewer:middlenet");
        registry.register_mime("application/atom+xml", "viewer:middlenet");
        registry.register_mime("application/feed+json", "viewer:middlenet");
        registry.register_mime("text/plain", "viewer:plaintext");
        registry.register_mime("text/markdown", "viewer:markdown");
        registry.register_mime("text/x-markdown", "viewer:markdown");
        registry.register_mime("application/json", "viewer:plaintext");
        registry.register_mime("application/toml", "viewer:plaintext");
        registry.register_mime("application/yaml", "viewer:plaintext");
        registry.register_mime("application/x-yaml", "viewer:plaintext");
        #[cfg(feature = "pdf")]
        registry.register_mime("application/pdf", "viewer:pdf");
        registry.register_mime("text/csv", "viewer:csv");
        #[cfg(feature = "audio")]
        {
            registry.register_mime("audio/mpeg", "viewer:audio");
            registry.register_mime("audio/ogg", "viewer:audio");
            registry.register_mime("audio/flac", "viewer:audio");
            registry.register_mime("audio/wav", "viewer:audio");
            registry.register_mime("audio/x-wav", "viewer:audio");
            registry.register_mime("audio/aac", "viewer:audio");
        }
        registry.register_extension("md", "viewer:markdown");
        registry.register_extension("gmi", "viewer:middlenet");
        registry.register_extension("gemini", "viewer:middlenet");
        registry.register_extension("gophermap", "viewer:middlenet");
        registry.register_extension("rss", "viewer:middlenet");
        registry.register_extension("atom", "viewer:middlenet");
        registry.register_extension("jsonfeed", "viewer:middlenet");
        #[cfg(feature = "pdf")]
        registry.register_extension("pdf", "viewer:pdf");
        registry.register_extension("csv", "viewer:csv");
        registry.register_extension("txt", "viewer:plaintext");
        registry.register_extension("json", "viewer:plaintext");
        registry.register_extension("toml", "viewer:plaintext");
        registry.register_extension("yaml", "viewer:plaintext");
        registry.register_extension("yml", "viewer:plaintext");
        registry.register_extension("rs", "viewer:plaintext");
        registry.register_extension("py", "viewer:plaintext");
        registry.register_extension("js", "viewer:plaintext");
        registry.register_extension("ts", "viewer:plaintext");
        #[cfg(feature = "audio")]
        {
            registry.register_extension("mp3", "viewer:audio");
            registry.register_extension("ogg", "viewer:audio");
            registry.register_extension("flac", "viewer:audio");
            registry.register_extension("wav", "viewer:audio");
            registry.register_extension("aac", "viewer:audio");
        }
        registry.register_capabilities(
            "viewer:webview",
            ViewerSubsystemCapabilities {
                accessibility: CapabilityDeclaration::full(),
                security: CapabilityDeclaration::full(),
                storage: CapabilityDeclaration::full(),
                history: CapabilityDeclaration::full(),
            },
        );
        registry.register_capabilities("viewer:middlenet", ViewerSubsystemCapabilities::full());
        registry.register_capabilities("viewer:settings", ViewerSubsystemCapabilities::full());
        registry.register_capabilities("viewer:metadata", ViewerSubsystemCapabilities::full());
        registry.register_capabilities("viewer:plaintext", ViewerSubsystemCapabilities::full());
        registry.register_capabilities("viewer:markdown", ViewerSubsystemCapabilities::full());
        #[cfg(feature = "pdf")]
        registry.register_capabilities("viewer:pdf", ViewerSubsystemCapabilities::full());
        registry.register_capabilities("viewer:csv", ViewerSubsystemCapabilities::full());
        #[cfg(feature = "audio")]
        registry.register_capabilities("viewer:audio", ViewerSubsystemCapabilities::full());
        registry
    }
}

fn extract_extension(uri: &str) -> Option<&str> {
    let no_fragment = uri.split('#').next().unwrap_or(uri);
    let no_query = no_fragment.split('?').next().unwrap_or(no_fragment);
    no_query.rsplit_once('.').map(|(_, ext)| ext)
}

fn render_mode_for_viewer_id(viewer_id: &str) -> ViewerRenderMode {
    match viewer_id {
        "viewer:webview" => ViewerRenderMode::CompositedTexture,
        "viewer:wry" => ViewerRenderMode::NativeOverlay,
        "viewer:middlenet" | "viewer:plaintext" | "viewer:markdown" | "viewer:pdf"
        | "viewer:csv" | "viewer:settings" | "viewer:metadata" | "viewer:audio" => {
            ViewerRenderMode::EmbeddedHost
        }
        _ => ViewerRenderMode::Placeholder,
    }
}

fn middlenet_viewer_for_uri_scheme(uri: &str) -> Option<&'static str> {
    match uri
        .split_once(':')
        .map(|(scheme, _)| scheme.to_ascii_lowercase())
    {
        Some(scheme)
            if matches!(
                scheme.as_str(),
                "gemini" | "titan" | "gopher" | "finger" | "spartan" | "misfin"
            ) =>
        {
            Some("viewer:middlenet")
        }
        _ => None,
    }
}

fn overlay_affordance_for_viewer_id(viewer_id: &str) -> bool {
    !matches!(
        render_mode_for_viewer_id(viewer_id),
        ViewerRenderMode::Placeholder
    )
}

/// Baseline plaintext viewer handler.
///
/// Handles all `text/*` MIME types plus common structured-text formats
/// (`application/json`, `application/toml`, `application/yaml`).
/// This is the last-resort embedded renderer for local files and custom-scheme
/// content — it always accepts rather than falling through to the web renderer.
pub struct PlaintextViewerHandler;

impl ViewerHandler for PlaintextViewerHandler {
    fn viewer_id(&self) -> &'static str {
        "viewer:plaintext"
    }

    fn can_render(&self, descriptor: &ViewerDescriptor) -> bool {
        if let Some(ref mime) = descriptor.mime_hint {
            let lower = mime.to_ascii_lowercase();
            return lower.starts_with("text/")
                || lower == "application/json"
                || lower == "application/toml"
                || lower == "application/yaml"
                || lower == "application/x-yaml";
        }
        // No MIME hint — check the URI extension.
        matches!(
            extract_extension(&descriptor.uri)
                .map(|e| e.to_ascii_lowercase())
                .as_deref(),
            Some(
                "txt"
                    | "md"
                    | "rs"
                    | "py"
                    | "js"
                    | "ts"
                    | "json"
                    | "toml"
                    | "yaml"
                    | "yml"
                    | "html"
                    | "css"
                    | "sh"
                    | "bash"
                    | "zsh"
                    | "fish"
                    | "csv"
                    | "xml"
                    | "log"
                    | "ini"
                    | "cfg"
                    | "conf"
            )
        )
    }
}

#[cfg(test)]
mod tests;
