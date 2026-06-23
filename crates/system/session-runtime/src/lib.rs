/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # graphshell session runtime
//!
//! Portable runtime state + types Mere hosts attach to. Per the
//! multiplexer framing thesis:
//!
//! > **Mere multiplexes durable graph sessions. Engines are
//! > replaceable content producers. Hosts are attach clients.
//! > Graph truth is never browser profile state.**
//!
//! This crate is the **session** half of that picture — durable
//! session manifests, session graph/view sidecars, and service-runner
//! state. Everything here compiles wasm32-clean: no gpui, no
//! iced, no winit, no wgpu.
//!
//! Ownership split:
//!
//! - `graphshell/shell/system/control-plane` owns the action bus.
//! - this crate owns session manifests, session lifecycle storage,
//!   graph/view sidecars, and session worker declarations.
//!
//! Adjacent crates:
//!
//! - [`host-contract`](../host_ports/index.html) —
//!   the **vocabulary** half: port traits hosts satisfy
//!   (HostPaintPort, HostInputPort, HostSurfacePort, HostAccessibility
//!   Port, …) + frame-projection inputs.
//! - `graphshell-control-plane` owns the action-bus vocabulary.
//! - `host` and future host adapters consume these portable surfaces
//!   without owning their schemas.
//!
//! See:
//! - `design_docs/mere_docs/research/2026-05-11_browser_multiplexer_framing.md`
//! - `design_docs/mere_docs/implementation_strategy/2026-05-11_graph_session_manifest_plan.md`

#![doc(html_root_url = "https://docs.rs/host-runtime/0.0.1")]

// Durable web-content cache over an eidetic Store (wasm-clean; the host supplies
// the concrete backend). Persists fetched pages / subresources so a reload need
// not re-fetch.
pub mod content_store;
pub mod engine_profile_store;
// Filesystem persistence of the content frame's pane layout (frame.json);
// native-only so the crate stays wasm-clean.
#[cfg(not(target_arch = "wasm32"))]
pub mod frame_layout_store;
pub mod manifest;
pub mod manifest_store;
// Filesystem persistence of the session graph (graph.json); native-only so the
// crate stays wasm-clean (wasm hosts use a different storage backend).
#[cfg(not(target_arch = "wasm32"))]
pub mod session_graph_store;
pub mod session_service_runner;
// Session-wide settings sidecar (settings.json). A flat JSON document beside
// graph.json; the host loads it on launch and saves on change.
pub mod settings_store;
// DocumentScript origin->component bindings sidecar (script-bindings.json): the
// auto-attach list (§11.4 follow-on #2). Native-only (filesystem).
#[cfg(not(target_arch = "wasm32"))]
pub mod script_bindings_store;
// Per-persona UI settings (`personas/<id>/settings/ui.json`) — persona-scoped config
// distinct from the app-scoped settings_store; first field is the configurable menu.
pub mod persona_settings_store;
pub mod switcher_thumbnail;
pub mod tearout;
pub mod view_intent_store;

pub use engine_profile_store::{
    ENGINE_PROFILES_DIR, EngineProfileScope, GRAPHS_DIR, PERSONAS_DIR, SESSIONS_DIR,
    engine_profile_path, engine_profile_path_for_session,
};
pub use manifest::{
    EngineProfileBinding, EngramId, GraphSessionManifest, MANIFEST_SCHEMA_VERSION, PersonaId,
    SessionPolicy, SessionPolicyOverride, WorkerKind,
};
pub use manifest_store::{LoadFailure, LoadReport, MANIFEST_FILE, ManifestStore, TRASH_DIR};
pub use session_service_runner::{
    InMemoryRunner, NullRunner, SessionServiceRunner, WorkerHandle, WorkerStartError, WorkerState,
    WorkerStatus, WorkerStopError,
};
pub use switcher_thumbnail::{
    SwitcherThumbnail, SwitcherThumbnailOptions, ThumbnailEdge, ThumbnailNode,
    build_switcher_thumbnail, build_switcher_thumbnail_with,
};
pub use persona_settings_store::{
    PERSONA_SETTINGS_DIR, PERSONA_UI_FILENAME, PersonaSettings, load_persona_settings,
    persona_settings_path, save_persona_settings,
};
pub use settings_store::{PersistedSettings, SETTINGS_FILENAME, ShellbarEdge};
#[cfg(not(target_arch = "wasm32"))]
pub use script_bindings_store::{SCRIPT_BINDINGS_FILENAME, ScriptBinding};
pub use tearout::{PaneDragPayload, TileDragPayload};
pub use view_intent_store::{CameraSnapshot, HiddenRelationRecord, VIEW_INTENT_DIR, ViewIntent};
