// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

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
// Durable content-addressed store for node preview imagery (favicons, previews,
// snapshots) — the sibling of content_store, keyed by BLAKE3 digest so identical
// images dedup. The pixels live here; the kernel Node holds only an ImageRef.
pub mod image_store;
// Per-node browser-state working set (scroll / form draft / viewer override /
// compat mode / content-on) — browser-runtime state that doesn't belong in
// graph truth (boundary pass slice C). Persistence converged onto web.* facets
// (web_facets); the module's browser_nodes.json IO remains as legacy read-only.
pub mod browser_node_state;
// The web.* facet namespace: browser_node_state's persistence boundary — one
// atomic facet per field (web.scroll / form_draft / viewer / compat / content)
// in facets.json, replacing the bespoke browser_nodes.json document.
pub mod web_facets;
// The denizen.* facet namespace: which graph nodes are denizens (servitor /
// agent / peer / scenario / pack) and where each one's nested graph lives —
// a facet bundle on the node, in facets.json. Supersedes the transitional
// denizen_bindings.json sidecar (removed before any host wrote one).
pub mod denizen_facets;
// Per-node facet-store sidecar (facets.json): the runtime tier of the one-node
// facet system — typed per-node metadata keyed by node UUID, persisted beside
// graph.json. The durable home the bespoke per-node sidecars (browser/denizen/
// arrangement) converge onto. Wraps chartulary's FacetStore.
pub mod facet_store;
// Mere-side adapter from eidetic SchemaDefinition engrams to chartulary's
// synchronous FacetValidator seam.
pub mod schema_facets;
// The arrangement.* facet namespace: cartography's per-node data (position
// first; size/sprite/hull/material/face follow) as facets in facets.json —
// born as facets, since the bespoke cartography sidecar was never wired.
pub mod arrangement_facets;
// The scene.* facet namespace: the graph-scene's own view settings (sizing
// mode, importance metric, physics damping) as facets of the CONTAINER node
// (keyed by the session's root_graph_id) — scene-scoped, not per-node.
pub mod engine_profile_store;
pub mod scene_facets;
// Freeze/thaw a live graph into an immutable, content-addressed graph engram over
// an eidetic Store (the Alembic memory spine; wasm-clean — store-agnostic, not
// filesystem). Save redacts private fields by default; open thaws read-only.
pub mod engram_seal;
pub mod graph_engram;
// Snapshot-level merge for engram compose (Alembic tail B7): union two graph
// snapshots by URL identity, retaining per-member provenance. Pure; the engram
// compose op (`graph_engram::compose_graph_engrams`) layers on top.
pub mod snapshot_merge;
// The three memory levels' read-model (Alembic slice C): classify a node as
// short-term vs long-term (a tag/pin promotes), and compute which short-term nodes
// an eviction policy would drop. Pure logic; the pane/settings wiring layers on top.
pub mod memory_levels;
// Athanor's forgetting pass (Alembic slice D): propose which short-term cached
// content to evict (pure, R0) and apply it by dropping content blobs (never graph
// truth or engrams). The pass logic; the armillary actor that schedules it layers on top.
pub mod athanor;
// The frame.json pane-layout store moved OUT with the pane model at
// meerkat's deletion (2026-07-18): it lives in turnstone's `frisket::store`
// now — the pane-coupled half of this crate, split exactly as the
// boundary-pass plan parked it.
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
// The tear-out payload types (PaneDragPayload/TileDragPayload) moved out
// with the pane model at meerkat's deletion: they name frisket::PaneId, so
// they live in turnstone's `frisket::tearout` now.
pub mod view_intent_store;
// Identity-level and persona-level wallet manifests (`identity/` + `personas/<id>/wallet.json`)
// for the carry layer. Storage only; pairing and crypto semantics layer on top.
pub mod wallet_grant;
pub mod wallet_store;

pub use arrangement_facets::{
    ARRANGEMENT_FACE, ARRANGEMENT_MATERIAL, ARRANGEMENT_POSITION, ARRANGEMENT_SIZE,
    ARRANGEMENT_SPRITE, ARRANGEMENT_SPRITE_HULL, arrangement_position_facet,
    read_arrangement_faces, read_arrangement_materials, read_arrangement_positions,
    read_arrangement_sizes, read_arrangement_sprite_hulls, read_arrangement_sprites,
    retain_present_nodes, write_arrangement_faces, write_arrangement_materials,
    write_arrangement_positions, write_arrangement_sizes, write_arrangement_sprite_hulls,
    write_arrangement_sprites,
};
pub use denizen_facets::{
    DENIZEN_BINDING, DenizenBinding, DenizenKind, is_denizen, read_denizen_binding,
    read_denizen_bindings, remove_denizen_binding, write_denizen_binding,
};
pub use engine_profile_store::{
    ENGINE_PROFILES_DIR, EngineProfileScope, GRAPHS_DIR, PERSONAS_DIR, SESSIONS_DIR,
    engine_profile_path, engine_profile_path_for_session,
};
pub use engram_seal::WalletEpochSealer;
pub use facet_store::{
    AcceptAll, FacetError, FacetId, FacetValidator, NODE_FACETS_FILE, NodeFacetStore, NodeFacets,
    copy_node_facets, load_node_facets, node_facets_path, save_node_facets,
};
pub use identity::{StartupUnlockMode, auto_unlock_backend_available};
pub use manifest::{
    EngineProfileBinding, EngramId, GraphSessionManifest, MANIFEST_SCHEMA_VERSION, PersonaId,
    SessionPolicy, SessionPolicyOverride, WorkerKind,
};
pub use manifest_store::{LoadFailure, LoadReport, MANIFEST_FILE, ManifestStore, TRASH_DIR};
pub use persona_settings_store::{
    PERSONA_SETTINGS_DIR, PERSONA_UI_FILENAME, PersonaSettings, load_persona_settings,
    persona_settings_path, save_persona_settings,
};
pub use scene_facets::{
    DEFAULT_PHYSICS_DAMPING, SCENE_IMPORTANCE_METRIC, SCENE_PHYSICS_DAMPING, SCENE_SIZE_BY_DEGREE,
    SCENE_SIZE_BY_IMPORTANCE, SceneFacets, copy_scene_facets, read_scene_facets,
    write_scene_facets,
};
pub use schema_facets::{
    ContentClassEngram, SchemaFacetValidator, content_class_schema_definition,
    content_class_schema_ref, load_content_class, save_content_class,
};
#[cfg(not(target_arch = "wasm32"))]
pub use script_bindings_store::{SCRIPT_BINDINGS_FILENAME, ScriptBinding};
pub use session_service_runner::{
    InMemoryRunner, NullRunner, SessionServiceRunner, WorkerHandle, WorkerStartError, WorkerState,
    WorkerStatus, WorkerStopError,
};
pub use settings_store::{PersistedSettings, SETTINGS_FILENAME, ShellbarEdge};
pub use switcher_thumbnail::{
    SwitcherThumbnail, SwitcherThumbnailOptions, ThumbnailEdge, ThumbnailNode,
    build_switcher_thumbnail_with,
};
pub use view_intent_store::{CameraSnapshot, HiddenRelationRecord, VIEW_INTENT_DIR, ViewIntent};
pub use wallet_grant::{
    DEVICE_GRANT_SCHEMA_VERSION, DeviceGrantError, DeviceGrantPayload, DeviceGrantSignature,
    EnrollmentBundleError, PairedRemoteAuthGrantSpec, PairingCodeError, PairingMaterialError,
    PairingTicketError, PrivateEpochPlaintext, REMOTE_AUTH_ENROLLMENT_BUNDLE_SCHEMA_VERSION,
    REMOTE_AUTH_PAIRING_SAS_CONTEXT_V1, REMOTE_AUTH_PAIRING_SECRET_LEN,
    REMOTE_AUTH_PAIRING_TICKET_SCHEMA_VERSION, REMOTE_AUTH_PAIRING_WRAP_CONTEXT_V1,
    RemoteAuthEnrollmentBundle, RemoteAuthGrantSpec, RemoteAuthPairingMaterial,
    RemoteAuthPairingResponse, RemoteAuthPairingTicket, RemoteAuthPairingTicketRequest,
    RemoteAuthRevocationOutcome, SignedDeviceGrant, WRAPPED_PRIVATE_EPOCH_FORMAT_V1,
    WrappedEpochError, WrappedEpochMaterial, build_remote_auth_enrollment_bundle,
    decode_remote_auth_enrollment_bundle, decode_remote_auth_pairing_ticket,
    decode_signed_device_grant, derive_remote_auth_pairing_material, device_grant_ref,
    encode_remote_auth_enrollment_bundle, encode_remote_auth_pairing_ticket,
    encode_signed_device_grant, format_remote_auth_pairing_code,
    install_remote_auth_enrollment_bundle, install_remote_auth_enrollment_bundle_with_wrapping_key,
    issue_device_grant, issue_remote_auth_device_grant,
    issue_remote_auth_device_grant_from_pairing, issue_remote_auth_device_grant_from_ticket,
    load_signed_device_grant, mint_remote_auth_pairing_ticket, parse_remote_auth_pairing_code,
    revoke_remote_auth_device, save_signed_device_grant, signed_device_grant_path,
    unwrap_private_epoch_material, verify_device_grant, wrap_private_epoch_material,
};
pub use wallet_store::{
    CapabilitySlotRef, DEVICE_ROSTER_FILENAME, DeviceExposure, DeviceGrantRef, DeviceId,
    DeviceMode, DevicePublicKey, DeviceRecord, DeviceRoster, IDENTITY_DIR, IDENTITY_GRANTS_DIR,
    IDENTITY_SEED_FILENAME, IDENTITY_WALLET_FILENAME, IdentityWalletManifest, KeyEpochId,
    LOCAL_DEVICE_IDENTITY_FILENAME, LocalDeviceIdentity, PERSONA_EPOCH_BRIDGE_FILENAME,
    PERSONA_WALLET_FILENAME, PersonaChainRoot, PersonaEpochBridge, PersonaWalletManifest,
    PersonaWalletRef, PrivateEpochRecord, PrivateRoots, PublicRoots,
    REMOTE_AUTH_WRAPPING_KEYS_FILENAME, RecoveryPolicy, RemoteAuthWrappingKeyBridge,
    RemoteAuthWrappingKeyRecord, WALLET_SCHEMA_VERSION, WalletBootstrapMode,
    bootstrap_wallet_state, derive_persona_chain_root, device_grant_path, device_roster_path,
    device_roster_ref, ensure_local_device_identity, ensure_persona_epoch_bridge,
    ensure_wallet_state, identity_dir, identity_grants_dir, identity_seed_locked_at_startup,
    identity_seed_path, identity_wallet_path, load_current_private_epoch, load_device_grant,
    load_device_roster, load_identity_seed, load_identity_wallet, load_local_device_identity,
    load_persona_epoch_bridge, load_persona_wallet, load_remote_auth_wrapping_key_bridge,
    local_device_identity_path, persona_epoch_bridge_path, persona_wallet_path,
    persona_wallet_salt, relock_wallet_after_manual_unlock, remote_auth_wrapping_keys_path,
    save_device_grant, save_device_roster, save_identity_seed, save_identity_wallet,
    save_local_device_identity, save_persona_epoch_bridge, save_persona_wallet,
    save_remote_auth_wrapping_key_bridge, stage_persona_private_epoch, unlock_wallet_with_auto_os,
    wallet_local_secrets_locked,
};
pub use web_facets::{
    WEB_COMPAT, WEB_CONTENT, WEB_FORM_DRAFT, WEB_SCROLL, WEB_VIEWER, read_web_states,
    write_web_state, write_web_states,
};
