// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use register_layout::ConformanceLevel;

#[test]
fn viewer_registry_core_seed_uses_plaintext_and_metadata() {
    let registry = ViewerRegistry::core_seed();

    let plaintext = registry.select_for_uri("file:///notes/readme.txt", Some("text/plain"));
    assert_eq!(plaintext.viewer_id, "viewer:plaintext");
    assert!(!plaintext.fallback_used);

    let fallback = registry.select_for_uri("file:///archive/blob.bin", None);
    assert_eq!(fallback.viewer_id, "viewer:metadata");
    assert!(fallback.fallback_used);

    assert_eq!(fallback.capabilities.history.level, ConformanceLevel::Full);
}

#[test]
fn viewer_registry_reports_registered_capabilities_in_selection() {
    let mut registry = ViewerRegistry::new("viewer:fallback");
    registry.register_mime("text/plain", "viewer:plaintext");
    registry.register_capabilities(
        "viewer:plaintext",
        ViewerSubsystemCapabilities {
            accessibility: CapabilityDeclaration::partial("access bridge disabled in test"),
            security: CapabilityDeclaration::full(),
            storage: CapabilityDeclaration::full(),
            history: CapabilityDeclaration::full(),
        },
    );

    let selection = registry.select_for_uri("file:///notes/readme.txt", Some("text/plain"));
    assert_eq!(selection.viewer_id, "viewer:plaintext");
    assert_eq!(
        selection.capabilities.accessibility.level,
        ConformanceLevel::Partial
    );
    assert_eq!(
        selection.capabilities.accessibility.reason.as_deref(),
        Some("access bridge disabled in test")
    );
}

#[test]
fn viewer_capabilities_round_trip_via_json() {
    let capabilities = ViewerSubsystemCapabilities {
        accessibility: CapabilityDeclaration::partial("access bridge degraded"),
        security: CapabilityDeclaration::full(),
        storage: CapabilityDeclaration::full(),
        history: CapabilityDeclaration::none("history replay unavailable"),
    };

    let json = serde_json::to_string(&capabilities).expect("capabilities should serialize");
    let restored: ViewerSubsystemCapabilities =
        serde_json::from_str(&json).expect("capabilities should deserialize");

    assert_eq!(restored.accessibility.level, ConformanceLevel::Partial);
    assert_eq!(
        restored.accessibility.reason.as_deref(),
        Some("access bridge degraded")
    );
    assert_eq!(restored.history.level, ConformanceLevel::None);
}

#[test]
fn viewer_render_mode_accepts_legacy_embedded_egui_payload() {
    let mode: ViewerRenderMode =
        serde_json::from_str("\"EmbeddedEgui\"").expect("legacy mode should deserialize");

    assert_eq!(mode, ViewerRenderMode::EmbeddedHost);
}

// --- select_for tests ---

#[test]
#[cfg(feature = "pdf")]
fn select_for_pdf_mime_routes_to_pdf_viewer() {
    let registry = ViewerRegistry::default();
    assert_eq!(
        registry.select_for(Some("application/pdf"), AddressKind::File),
        "viewer:pdf"
    );
}

#[test]
fn select_for_text_plain_routes_to_plaintext_viewer() {
    let registry = ViewerRegistry::default();
    assert_eq!(
        registry.select_for(Some("text/plain"), AddressKind::File),
        "viewer:plaintext"
    );
}

#[test]
fn viewer_registry_selects_middlenet_for_gemini_scheme_without_mime_hint() {
    let registry = ViewerRegistry::default();
    let selection = registry.select_for_uri("gemini://example.com/start", None);

    assert_eq!(selection.viewer_id, "viewer:middlenet");
    assert!(!selection.fallback_used);
    assert_eq!(selection.matched_by, "scheme");
}

#[test]
fn viewer_registry_selects_middlenet_for_spartan_scheme_without_mime_hint() {
    let registry = ViewerRegistry::default();
    let selection = registry.select_for_uri("spartan://example.com/start", None);

    assert_eq!(selection.viewer_id, "viewer:middlenet");
    assert!(!selection.fallback_used);
    assert_eq!(selection.matched_by, "scheme");
}

#[test]
fn viewer_registry_selects_middlenet_for_titan_scheme_without_mime_hint() {
    let registry = ViewerRegistry::default();
    let selection = registry.select_for_uri("titan://example.com/edit/start", None);

    assert_eq!(selection.viewer_id, "viewer:middlenet");
    assert!(!selection.fallback_used);
    assert_eq!(selection.matched_by, "scheme");
}

#[test]
fn viewer_registry_selects_middlenet_for_gemini_mime() {
    let registry = ViewerRegistry::default();
    let selection = registry.select_for_uri("https://example.com/capsule.gmi", Some("text/gemini"));

    assert_eq!(selection.viewer_id, "viewer:middlenet");
    assert!(!selection.fallback_used);
    assert_eq!(selection.matched_by, "mime");
}

#[test]
fn viewer_registry_selects_middlenet_for_json_feed_mime() {
    let registry = ViewerRegistry::default();
    let selection = registry.select_for_uri(
        "https://example.com/feed.jsonfeed",
        Some("application/feed+json"),
    );

    assert_eq!(selection.viewer_id, "viewer:middlenet");
    assert!(!selection.fallback_used);
    assert_eq!(selection.matched_by, "mime");
}

#[test]
fn select_for_http_no_mime_routes_to_webview_fallback() {
    let registry = ViewerRegistry::default();
    assert_eq!(
        registry.select_for(None, AddressKind::Http),
        "viewer:webview"
    );
}

#[test]
fn select_for_file_no_mime_routes_to_plaintext_fallback() {
    let registry = ViewerRegistry::default();
    assert_eq!(
        registry.select_for(None, AddressKind::File),
        "viewer:plaintext"
    );
}

#[test]
fn select_for_unknown_scheme_no_mime_routes_to_plaintext_fallback() {
    let registry = ViewerRegistry::default();
    assert_eq!(
        registry.select_for(None, AddressKind::Unknown),
        "viewer:plaintext"
    );
}

#[test]
fn select_for_html_mime_routes_to_webview() {
    let registry = ViewerRegistry::default();
    assert_eq!(
        registry.select_for(Some("text/html"), AddressKind::Http),
        "viewer:webview"
    );
}

#[test]
fn select_for_json_routes_to_plaintext() {
    let registry = ViewerRegistry::default();
    assert_eq!(
        registry.select_for(Some("application/json"), AddressKind::File),
        "viewer:plaintext"
    );
}

#[test]
fn describe_viewer_returns_capability_payload_for_registered_viewer() {
    let registry = ViewerRegistry::default();
    let capability = registry
        .describe_viewer("viewer:webview")
        .expect("viewer:webview should be described");

    assert_eq!(capability.viewer_id, "viewer:webview");
    assert_eq!(capability.render_mode, ViewerRenderMode::CompositedTexture);
    assert_eq!(
        capability.subsystems.accessibility.level,
        ConformanceLevel::Full
    );
    assert_eq!(capability.subsystems.accessibility.reason, None);
    assert!(capability.overlay_affordance);
    assert!(
        capability
            .supported_mime_types
            .iter()
            .any(|mime| mime == "text/html")
    );
}

#[test]
fn describe_viewer_returns_none_for_unknown_viewer() {
    let registry = ViewerRegistry::default();
    assert!(registry.describe_viewer("viewer:unknown").is_none());
}

#[test]
fn select_for_unknown_mime_uses_canonical_runtime_fallback() {
    let registry = ViewerRegistry::default();
    let selection = registry.select_for_uri("https://example.com/file.bin", Some(""));

    assert_eq!(selection.viewer_id, VIEWER_ID_FALLBACK);
    assert!(selection.fallback_used);
}

// --- PlaintextViewerHandler tests ---

#[test]
fn plaintext_handler_id_is_viewer_plaintext() {
    let handler = PlaintextViewerHandler;
    assert_eq!(handler.viewer_id(), "viewer:plaintext");
}

#[test]
fn plaintext_handler_can_render_text_plain() {
    let handler = PlaintextViewerHandler;
    assert!(handler.can_render(&ViewerDescriptor {
        uri: "file:///foo.txt".to_string(),
        mime_hint: Some("text/plain".to_string()),
    }));
}

#[test]
fn plaintext_handler_can_render_text_markdown() {
    let handler = PlaintextViewerHandler;
    assert!(handler.can_render(&ViewerDescriptor {
        uri: "file:///doc.md".to_string(),
        mime_hint: Some("text/markdown".to_string()),
    }));
}

#[test]
fn plaintext_handler_can_render_application_json() {
    let handler = PlaintextViewerHandler;
    assert!(handler.can_render(&ViewerDescriptor {
        uri: "file:///data.json".to_string(),
        mime_hint: Some("application/json".to_string()),
    }));
}

#[test]
fn plaintext_handler_can_render_rs_by_extension_without_mime() {
    let handler = PlaintextViewerHandler;
    assert!(handler.can_render(&ViewerDescriptor {
        uri: "file:///src/main.rs".to_string(),
        mime_hint: None,
    }));
}

#[test]
fn plaintext_handler_cannot_render_binary_without_mime() {
    let handler = PlaintextViewerHandler;
    assert!(!handler.can_render(&ViewerDescriptor {
        uri: "file:///archive.zip".to_string(),
        mime_hint: None,
    }));
}

#[test]
fn plaintext_handler_cannot_render_image_mime() {
    let handler = PlaintextViewerHandler;
    assert!(!handler.can_render(&ViewerDescriptor {
        uri: "file:///photo.png".to_string(),
        mime_hint: Some("image/png".to_string()),
    }));
}
