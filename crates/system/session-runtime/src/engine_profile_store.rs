/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Engine profile boundary — path resolution for the engine UDF
//! directories.
//!
//! Per the [framing brief §5.4](../../../../design_docs/mere_docs/research/2026-05-11_browser_multiplexer_framing.md#54-engine-profile--site-data-boundary)
//! and the [engine profile boundary plan](../../../../design_docs/mere_docs/implementation_strategy/2026-05-14_engine_profile_boundary_plan.md):
//!
//! > Graph truth is what the session knows. Profile state is what the
//! > engine knows about its content. The session references a profile
//! > binding; it does not own profile bytes.
//!
//! This module is the pure-function half of that contract — given a
//! data root, persona, engine id, and scope (persona / session /
//! graph), it composes the UDF path. v0a does not create or write
//! anything; the engine itself materialises the directory when it
//! launches. Per-engine wiring (scrying first, then Wry, then Servo)
//! follows in v0b.

use std::path::{Path, PathBuf};

use frame::{GraphId, SessionId};

use crate::manifest::{EngineProfileBinding, GraphSessionManifest, PersonaId};

/// Subdirectory holding per-engine UDFs, under whichever scope owns
/// them (persona, session, or graph).
pub const ENGINE_PROFILES_DIR: &str = "engine-profiles";

/// Top-level subdirectory holding per-persona state — engine UDFs,
/// future persona-scoped data (vaults, settings).
pub const PERSONAS_DIR: &str = "personas";

/// Top-level subdirectory holding per-session state. Matches the
/// `ManifestStore` layout convention.
pub const SESSIONS_DIR: &str = "sessions";

/// Subdirectory under a session for per-graph state. v0 doesn't write
/// here unless the session's binding is `GraphScoped`.
pub const GRAPHS_DIR: &str = "graphs";

/// Which identity tier owns the engine's UDF. Mirrors
/// [`EngineProfileBinding`] but is a non-`Default` enum so callers can
/// thread it through pure functions without a fallback variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineProfileScope {
    /// All of a persona's sessions share one UDF for this engine.
    Persona,
    /// This session gets its own UDF for this engine.
    Session,
    /// A specific graph inside this session gets its own UDF
    /// (reserved; rare in practice).
    Graph,
}

impl From<EngineProfileBinding> for EngineProfileScope {
    fn from(binding: EngineProfileBinding) -> Self {
        match binding {
            EngineProfileBinding::PersonaScoped => Self::Persona,
            EngineProfileBinding::SessionScoped => Self::Session,
            EngineProfileBinding::GraphScoped => Self::Graph,
        }
    }
}

/// Compose the engine UDF path for the given scope. Returns `None`
/// when the scope requires an id that the caller didn't supply
/// (e.g. `Session` without a `session_id`). Pure — no filesystem
/// touch. The engine itself runs `fs::create_dir_all` when it
/// initialises.
pub fn engine_profile_path(
    data_root: &Path,
    persona_id: PersonaId,
    engine_id: &str,
    scope: EngineProfileScope,
    session_id: Option<SessionId>,
    graph_id: Option<GraphId>,
) -> Option<PathBuf> {
    match scope {
        EngineProfileScope::Persona => Some(
            data_root
                .join(PERSONAS_DIR)
                .join(persona_id.as_uuid().to_string())
                .join(ENGINE_PROFILES_DIR)
                .join(engine_id),
        ),
        EngineProfileScope::Session => {
            let session_id = session_id?;
            Some(
                data_root
                    .join(SESSIONS_DIR)
                    .join(session_id.as_uuid().to_string())
                    .join(ENGINE_PROFILES_DIR)
                    .join(engine_id),
            )
        }
        EngineProfileScope::Graph => {
            let session_id = session_id?;
            let graph_id = graph_id?;
            Some(
                data_root
                    .join(SESSIONS_DIR)
                    .join(session_id.as_uuid().to_string())
                    .join(GRAPHS_DIR)
                    .join(graph_id.as_uuid().to_string())
                    .join(ENGINE_PROFILES_DIR)
                    .join(engine_id),
            )
        }
    }
}

/// Manifest-driven convenience wrapper. The manifest carries every
/// id the resolver needs except the per-pane graph (only relevant
/// for `GraphScoped`); callers pass `graph_id = None` for the
/// non-graph scopes and the manifest's primary `root_graph_id`
/// otherwise (or an explicit override).
pub fn engine_profile_path_for_session(
    data_root: &Path,
    manifest: &GraphSessionManifest,
    engine_id: &str,
    graph_id: Option<GraphId>,
) -> Option<PathBuf> {
    let scope = EngineProfileScope::from(manifest.engine_profile);
    let session_id = Some(manifest.session_id);
    let graph_id = graph_id.or(match scope {
        EngineProfileScope::Graph => Some(manifest.root_graph_id),
        _ => None,
    });
    engine_profile_path(
        data_root,
        manifest.persona_id,
        engine_id,
        scope,
        session_id,
        graph_id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use uuid::Uuid;

    fn fixture_persona() -> PersonaId {
        PersonaId(Uuid::from_u128(0x1111))
    }

    fn fixture_session() -> SessionId {
        SessionId(Uuid::from_u128(0x2222))
    }

    fn fixture_graph() -> GraphId {
        GraphId(Uuid::from_u128(0x3333))
    }

    #[test]
    fn persona_scope_composes_under_personas_root() {
        let root = Path::new("/data");
        let path = engine_profile_path(
            root,
            fixture_persona(),
            "scrying.web",
            EngineProfileScope::Persona,
            None,
            None,
        )
        .unwrap();
        let expected = root
            .join("personas")
            .join(fixture_persona().as_uuid().to_string())
            .join("engine-profiles")
            .join("scrying.web");
        assert_eq!(path, expected);
    }

    #[test]
    fn session_scope_composes_under_sessions_root() {
        let root = Path::new("/data");
        let path = engine_profile_path(
            root,
            fixture_persona(),
            "scrying.web",
            EngineProfileScope::Session,
            Some(fixture_session()),
            None,
        )
        .unwrap();
        let expected = root
            .join("sessions")
            .join(fixture_session().as_uuid().to_string())
            .join("engine-profiles")
            .join("scrying.web");
        assert_eq!(path, expected);
    }

    #[test]
    fn graph_scope_composes_under_graphs_subdir() {
        let root = Path::new("/data");
        let path = engine_profile_path(
            root,
            fixture_persona(),
            "scrying.web",
            EngineProfileScope::Graph,
            Some(fixture_session()),
            Some(fixture_graph()),
        )
        .unwrap();
        let expected = root
            .join("sessions")
            .join(fixture_session().as_uuid().to_string())
            .join("graphs")
            .join(fixture_graph().as_uuid().to_string())
            .join("engine-profiles")
            .join("scrying.web");
        assert_eq!(path, expected);
    }

    #[test]
    fn session_scope_without_session_id_returns_none() {
        let path = engine_profile_path(
            Path::new("/data"),
            fixture_persona(),
            "scrying.web",
            EngineProfileScope::Session,
            None,
            None,
        );
        assert!(path.is_none());
    }

    #[test]
    fn graph_scope_without_session_id_returns_none() {
        let path = engine_profile_path(
            Path::new("/data"),
            fixture_persona(),
            "scrying.web",
            EngineProfileScope::Graph,
            None,
            Some(fixture_graph()),
        );
        assert!(path.is_none());
    }

    #[test]
    fn graph_scope_without_graph_id_returns_none() {
        let path = engine_profile_path(
            Path::new("/data"),
            fixture_persona(),
            "scrying.web",
            EngineProfileScope::Graph,
            Some(fixture_session()),
            None,
        );
        assert!(path.is_none());
    }

    #[test]
    fn binding_maps_to_scope_in_all_three_cases() {
        assert_eq!(
            EngineProfileScope::from(EngineProfileBinding::PersonaScoped),
            EngineProfileScope::Persona
        );
        assert_eq!(
            EngineProfileScope::from(EngineProfileBinding::SessionScoped),
            EngineProfileScope::Session
        );
        assert_eq!(
            EngineProfileScope::from(EngineProfileBinding::GraphScoped),
            EngineProfileScope::Graph
        );
    }

    #[test]
    fn convenience_wrapper_routes_through_manifest_fields() {
        let mut manifest = GraphSessionManifest::new(fixture_session(), fixture_graph());
        manifest.persona_id = fixture_persona();
        // Default binding is PersonaScoped.
        let path =
            engine_profile_path_for_session(Path::new("/data"), &manifest, "scrying.web", None)
                .unwrap();
        let expected_persona = Path::new("/data")
            .join("personas")
            .join(fixture_persona().as_uuid().to_string())
            .join("engine-profiles")
            .join("scrying.web");
        assert_eq!(path, expected_persona);

        // Flipping to SessionScoped picks up the manifest's session id.
        manifest.engine_profile = EngineProfileBinding::SessionScoped;
        let path =
            engine_profile_path_for_session(Path::new("/data"), &manifest, "scrying.web", None)
                .unwrap();
        let expected_session = Path::new("/data")
            .join("sessions")
            .join(fixture_session().as_uuid().to_string())
            .join("engine-profiles")
            .join("scrying.web");
        assert_eq!(path, expected_session);

        // GraphScoped — wrapper falls back to root_graph_id when the
        // caller doesn't pass an explicit graph id.
        manifest.engine_profile = EngineProfileBinding::GraphScoped;
        let path =
            engine_profile_path_for_session(Path::new("/data"), &manifest, "scrying.web", None)
                .unwrap();
        let expected_graph = Path::new("/data")
            .join("sessions")
            .join(fixture_session().as_uuid().to_string())
            .join("graphs")
            .join(fixture_graph().as_uuid().to_string())
            .join("engine-profiles")
            .join("scrying.web");
        assert_eq!(path, expected_graph);
    }

    #[test]
    fn engine_id_segment_is_used_verbatim() {
        // No munging — engine ids are caller-defined identifiers
        // matching `Engine::engine_id()`. A change here would silently
        // move every engine's UDF.
        let root = Path::new("/data");
        let path = engine_profile_path(
            root,
            fixture_persona(),
            "wry.web",
            EngineProfileScope::Persona,
            None,
            None,
        )
        .unwrap();
        assert!(path.ends_with(Path::new("engine-profiles/wry.web")));
    }
}
