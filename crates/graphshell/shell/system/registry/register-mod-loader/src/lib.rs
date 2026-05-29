/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Mod loader registry — discovery, manifest parsing, dependency
//! resolution, and lifecycle bookkeeping for native + WASM mods.
//!
//! Extracted from `registries/infrastructure/mod_loader.rs` per
//! Slice 68c. Mod registration is the canonical extension seam:
//! mods declare manifests, register themselves into a `ModRegistry`,
//! and provide capabilities that other registries (action, agent,
//! viewer, protocol) check against.
//!
//! ## DI seams
//!
//! Two host-side runtimes are injected at registry construction:
//!
//! - [`WasmModRuntime`] (Slice 68a): the host's WASM runtime
//!   (extism + wasmtime + WASI wiring). Without it, `load_all`
//!   errors out activation for `ModType::Wasm` mods.
//! - [`NativeModRuntime`] (Slice 68b): the host's native-mod
//!   activation table (function pointers to compiled-in mods).
//!   Without it, `load_all` silently no-ops native activation
//!   (matches the pre-Slice-68b "unknown mod ID" semantics).
//!
//! Hosts wire both at startup:
//!
//! ```ignore
//! ModRegistry::new()
//!     .with_wasm_runtime(Arc::new(GraphshellWasmRuntime))
//!     .with_native_runtime(Arc::new(GraphshellNativeRuntime))
//! ```
//!
//! ## What stays in tree
//!
//! `registries/infrastructure/mod_activation.rs` — the
//! `NativeModActivations` factory hardcodes function pointers to
//! `crate::mods::native::*::activate`, which are intrinsically
//! host-side. The factory stays in tree; the host wraps it with a
//! `NativeModRuntime` impl (`GraphshellNativeRuntime` in
//! `mods/native/mod.rs`).
//!
//! ## Diagnostics
//!
//! Lifecycle events emit through the portable
//! [`register_diagnostics::emit_event`] scaffold (Slice 59); channel
//! constants live in [`register_diagnostics::channels`].

pub mod loader;

pub use loader::{
    discover_mod_manifests, discover_native_mods, resolve_mod_load_order, runtime_has_capability,
    ModActivationError, ModCapability, ModDependencyError, ModExtensionRecord,
    ModLoadPathError, ModManifest, ModRegistry, ModStatus, ModType, ModUnloadError,
    NativeModRegistration, NativeModRuntime, WasmModRuntime, WasmModSource,
};

// `compute_active_capabilities_with_disabled` is test-utils gated in
// the loader module; not re-exported at the crate root.
