//! P2.4 — the `register-mod-loader` `WasmModRuntime` bridge.
//!
//! Implements `register-mod-loader`'s `WasmModRuntime` DI trait **without editing
//! that crate** (it is deliberately runtime-free — the trait exists precisely so
//! Wasmtime stays host-side, §11.1). A host wires
//!
//! ```ignore
//! ModRegistry::new().with_wasm_runtime(Arc::new(DocumentScriptRuntime::new(allowed)))
//! ```
//!
//! and every manifest-described `ModType::Wasm` mod flows through the loader's
//! lifecycle into this host:
//!
//! - `activate` — host-policy capability check (refuse before instantiation), map
//!   the manifest to a [`Grant`], build the component instance under that grant,
//!   run the `activate` export, and retain the live instance keyed by `mod_id`.
//! - `deactivate` — run the `deactivate` export on the retained instance and drop
//!   it (store + instance freed).
//!
//! ## The two impedance mismatches, and how they are bridged
//!
//! - **sync trait vs async host.** The trait is sync `&self`; our `document-core`
//!   exports are invoked via `call_async` (so a future `fetch` can suspend a turn,
//!   §11.7-7). We bridge with a per-call [`pollster::block_on`] — a minimal
//!   executor with no global runtime, safe to call from any non-reentrant thread.
//!   With no async import yet, every call completes in a single poll.
//! - **`Send + Sync` trait vs the per-mod store.** The trait demands `Send + Sync`;
//!   we retain live `Store<ScriptHost>`s behind a `Mutex`. That compiles only
//!   because `ScriptHost` is `Send` (serval's `ScriptedDom` is an id-keyed arena of
//!   `HashMap`/`Vec`, its `QualName`s string-cache atoms — no `Rc`/`RefCell`). If
//!   that ever regresses, the fallback is a dedicated worker thread; for now the
//!   live store rides in the runtime directly.
//!
//! ## Capability policy (the load-bearing check)
//!
//! The `document-core` world exposes `log` + `document`; both are world-intrinsic
//! for a document mod, so [`grant_for`] grants both. The manifest's host
//! `ModCapability`s (Network / Filesystem / Identity / Clipboard / Exec) gate
//! *host* capabilities that map to future imports (Network → `fetch`); none has a
//! live import yet, so they do not change the grant. They are not inert, though:
//! a runtime is constructed with the set of capabilities its **surface** will
//! grant, and `activate` refuses — before any instantiation — a mod requesting a
//! capability outside that set. That is the host-policy enforcement point (§11.4):
//! a surface that will not grant `Network` rejects a `Network` mod up front.

use std::collections::HashMap;
use std::sync::Mutex;

use register_mod_loader::{ModCapability, ModManifest, WasmModRuntime, WasmModSource};
use wasmtime::{Engine, Store, StoreLimits};

use crate::{build_instance, seed_dom, DocumentCore, Grant, ScriptHost};

/// One activated wasm mod: the live store + bindings, retained so `deactivate` can
/// drive the teardown export before the store (and the instance it owns) is freed.
struct ActiveMod {
    store: Store<ScriptHost>,
    bindings: DocumentCore,
}

/// The document-host's implementation of `register-mod-loader`'s `WasmModRuntime`.
/// Owns one Wasmtime `Engine` (cheap, `Arc`-backed) and a `mod_id`-keyed table of
/// activated instances. Construct it with the set of host capabilities the surface
/// is willing to grant; `activate` enforces that set before instantiating.
pub struct DocumentScriptRuntime {
    engine: Engine,
    /// Host capability policy: which `ModCapability`s this surface will grant.
    allowed: Vec<ModCapability>,
    /// Live activated instances, keyed by `mod_id`.
    active: Mutex<HashMap<String, ActiveMod>>,
}

impl DocumentScriptRuntime {
    /// A runtime that grants exactly the `allowed` host capabilities. A mod whose
    /// manifest requests anything outside this set is refused at `activate`.
    pub fn new(allowed: Vec<ModCapability>) -> Self {
        Self { engine: Engine::default(), allowed, active: Mutex::new(HashMap::new()) }
    }

    /// A fully-trusted surface: every host capability granted.
    pub fn permissive() -> Self {
        Self::new(vec![
            ModCapability::Network,
            ModCapability::Filesystem,
            ModCapability::Identity,
            ModCapability::Clipboard,
            ModCapability::Exec,
        ])
    }

    /// The `mod_id`s currently active, in arbitrary order. (Test/inspection seam.)
    pub fn active_ids(&self) -> Vec<String> {
        match self.active.lock() {
            Ok(active) => active.keys().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Whether `mod_id` is currently active.
    pub fn is_active(&self, mod_id: &str) -> bool {
        self.active.lock().map(|a| a.contains_key(mod_id)).unwrap_or(false)
    }
}

/// Map a mod's manifest to the `document-core` import grant. Both world imports
/// (`log`, `document`) are intrinsic to a document mod, so both are granted; the
/// host `ModCapability`s are policy-checked in [`DocumentScriptRuntime::activate`]
/// (they gate future imports like `fetch`, which do not exist yet).
fn grant_for(_manifest: &ModManifest) -> Grant {
    Grant::allow_all()
}

impl WasmModRuntime for DocumentScriptRuntime {
    fn activate(&self, manifest: &ModManifest, source: &WasmModSource) -> Result<(), String> {
        // 1. Host-policy capability check — before any instantiation.
        if let Some(denied) = manifest.capabilities.iter().find(|c| !self.allowed.contains(c)) {
            return Err(format!(
                "mod '{}' requests capability {:?} not granted by this surface",
                manifest.mod_id, denied
            ));
        }

        // 2. No double-activation (the loader should not, but be defensive).
        {
            let active = self.active.lock().map_err(|_| "runtime lock poisoned".to_string())?;
            if active.contains_key(&manifest.mod_id) {
                return Err(format!("mod '{}' already active", manifest.mod_id));
            }
        }

        // 3. Build the instance under the grant + run the `activate` lifecycle
        //    export. Instantiation itself fails if the component requires an import
        //    the grant omitted (the runtime-enforced capability boundary).
        // Extension mods get a fresh seeded DOM (they are not page-attached) and are
        // unguarded for P2.4 (the page-script guards live in `DocumentScript`).
        let grant = grant_for(manifest);
        let (mut store, bindings) = pollster::block_on(build_instance(
            &self.engine,
            &source.module_path,
            seed_dom(),
            &grant,
            StoreLimits::default(),
        ))
        .map_err(|e| format!("instantiate '{}': {e}", manifest.mod_id))?;
        pollster::block_on(bindings.call_activate(&mut store))
            .map_err(|e| format!("activate '{}': {e}", manifest.mod_id))?;

        // 4. Retain the live instance for `deactivate`.
        self.active
            .lock()
            .map_err(|_| "runtime lock poisoned".to_string())?
            .insert(manifest.mod_id.clone(), ActiveMod { store, bindings });
        Ok(())
    }

    fn deactivate(&self, mod_id: &str) -> Result<(), String> {
        // Remove under the lock, then run the export with the lock released so a
        // teardown call never blocks other activations.
        let entry = {
            let mut active = self.active.lock().map_err(|_| "runtime lock poisoned".to_string())?;
            active.remove(mod_id)
        };
        let ActiveMod { mut store, bindings } =
            entry.ok_or_else(|| format!("mod '{mod_id}' not active"))?;
        pollster::block_on(bindings.call_deactivate(&mut store))
            .map_err(|e| format!("deactivate '{mod_id}': {e}"))?;
        // `store` (and the instance it owns) dropped here.
        Ok(())
    }
}
