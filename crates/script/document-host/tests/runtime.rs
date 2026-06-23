//! P2.4 verification: the `register-mod-loader` `WasmModRuntime` bridge.
//!
//! A manifest-described wasm mod flows through the trait into the document-host:
//! `activate` instantiates under a capability grant and runs the `activate` export
//! and retains the instance; `deactivate` runs the teardown export and drops it.
//! The host-policy capability check refuses an over-asking mod *before* any
//! instantiation (proven by pointing a refused mod at a path that does not exist —
//! the error is the policy refusal, not a file-not-found).

use std::path::PathBuf;

use document_host::runtime::DocumentScriptRuntime;
use register_mod_loader::{ModCapability, ModManifest, ModType, WasmModRuntime, WasmModSource};

fn doc_wasm() -> PathBuf {
    let p = std::env::var("DOC_HOST_GUEST_WASM").unwrap_or_else(|_| {
        "guest/target/wasm32-wasip2/release/document_core_guest.wasm".to_string()
    });
    let path = PathBuf::from(&p);
    assert!(
        path.exists(),
        "guest component missing at {p}; build it: cd guest && cargo build --target wasm32-wasip2 --release"
    );
    path
}

fn source_for(wasm: PathBuf) -> WasmModSource {
    // The manifest path is unused by the bridge (the loader has already parsed it);
    // point it next to the module for shape.
    WasmModSource { module_path: wasm, manifest_path: PathBuf::from("mod.toml") }
}

fn manifest(mod_id: &str, capabilities: Vec<ModCapability>) -> ModManifest {
    ModManifest::new(
        mod_id,
        format!("Test mod {mod_id}"),
        ModType::Wasm,
        vec![],
        vec![],
        capabilities,
    )
}

#[test]
fn activate_then_deactivate_runs_the_full_lifecycle() {
    // Empty policy still activates a mod that requests no host capabilities.
    let rt = DocumentScriptRuntime::new(vec![]);
    let m = manifest("doc.basic", vec![]);
    let src = source_for(doc_wasm());

    rt.activate(&m, &src).expect("activate should succeed");
    assert!(rt.is_active("doc.basic"), "mod should be active after activate");
    assert_eq!(rt.active_ids(), vec!["doc.basic".to_string()]);

    rt.deactivate("doc.basic").expect("deactivate should succeed");
    assert!(!rt.is_active("doc.basic"), "mod should be gone after deactivate");
    assert!(rt.active_ids().is_empty());
}

#[test]
fn denied_capability_is_refused_before_instantiation() {
    // Grants nothing; the mod asks for Network. The source path does not exist, so
    // *if* the bridge tried to instantiate, it would fail with a load error. Instead
    // it must fail with the policy refusal — proving the check precedes any load.
    let rt = DocumentScriptRuntime::new(vec![]);
    let m = manifest("doc.netty", vec![ModCapability::Network]);
    let src = source_for(PathBuf::from("does/not/exist.wasm"));

    let err = rt.activate(&m, &src).expect_err("over-asking mod must be refused");
    assert!(
        err.contains("Network") && err.contains("not granted"),
        "refusal should name the denied capability, got: {err}"
    );
    assert!(!rt.is_active("doc.netty"), "a refused mod must not be retained");
}

#[test]
fn within_policy_capability_is_admitted() {
    // The mod asks for Network and the surface grants it; the policy check passes
    // and the mod activates (the world has no `fetch` import yet, so Network does
    // not yet gate a live import — but the host-policy gate is exercised).
    let rt = DocumentScriptRuntime::new(vec![ModCapability::Network]);
    let m = manifest("doc.allowed", vec![ModCapability::Network]);
    rt.activate(&m, &source_for(doc_wasm())).expect("within-policy mod should activate");
    assert!(rt.is_active("doc.allowed"));
}

#[test]
fn double_activate_is_refused() {
    let rt = DocumentScriptRuntime::new(vec![]);
    let m = manifest("doc.dup", vec![]);
    let src = source_for(doc_wasm());
    rt.activate(&m, &src).expect("first activate should succeed");
    let err = rt.activate(&m, &src).expect_err("second activate must be refused");
    assert!(err.contains("already active"), "got: {err}");
    // Still active (the duplicate did not disturb the live instance).
    assert!(rt.is_active("doc.dup"));
}

#[test]
fn deactivate_unknown_mod_is_an_error() {
    let rt = DocumentScriptRuntime::new(vec![]);
    let err = rt.deactivate("nope").expect_err("deactivating an unknown mod must error");
    assert!(err.contains("not active"), "got: {err}");
}
