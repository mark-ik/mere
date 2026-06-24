use super::*;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use super::registry::compute_active_capabilities;

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
    // A valid Component-Model preamble (\0asm + version 0x0d 0x00 + layer 0x01 0x00);
    // validate_wasm_binary now rejects a bare core-module header.
    fs::write(
        &module_path,
        [0x00, 0x61, 0x73, 0x6d, 0x0d, 0x00, 0x01, 0x00],
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
fn discover_wasm_mods_in_dir_surfaces_document_script_origins() {
    let temp_dir = tempfile::tempdir().expect("temp dir should create");
    // A document-script mod: declares origin globs in its sidecar manifest.
    write_wasm_fixture(
        &temp_dir,
        "weather",
        "mod_id = \"mod:weather\"\ndocument_script_origins = [\"example.com\", \"*.weather.test\"]\n",
    );
    // An ordinary mod: no document-script declaration.
    write_wasm_fixture(&temp_dir, "plain", "mod_id = \"mod:plain\"\n");

    let found = discover_wasm_mods_in_dir(temp_dir.path());
    assert_eq!(found.len(), 2, "both wasm mods discovered");

    let weather = found
        .iter()
        .find(|(m, _)| m.mod_id == "mod:weather")
        .expect("weather mod discovered");
    assert_eq!(
        weather.0.document_script_origins,
        vec!["example.com".to_string(), "*.weather.test".to_string()],
        "origin globs parse from the sidecar manifest",
    );
    assert!(
        weather.1.module_path.ends_with("weather.wasm"),
        "the bound component is the mod's own .wasm",
    );

    let plain = found
        .iter()
        .find(|(m, _)| m.mod_id == "mod:plain")
        .expect("plain mod discovered");
    assert!(
        plain.0.document_script_origins.is_empty(),
        "no declaration = not a document-script mod",
    );
}

#[test]
fn discover_wasm_mods_in_dir_empty_for_absent_dir() {
    let missing = std::path::Path::new("definitely/not/a/real/mods/dir/xyz");
    assert!(
        discover_wasm_mods_in_dir(missing).is_empty(),
        "an absent mods dir yields no bindings, not an error",
    );
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
