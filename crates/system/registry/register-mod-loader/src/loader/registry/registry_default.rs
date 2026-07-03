use super::super::free_fns::NativeModRegistration;
use super::super::types::*;
use super::ModRegistry;

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
