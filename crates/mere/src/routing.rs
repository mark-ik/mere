// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mere's routing vocabulary over inker's host-neutral policy.
//!
//! Inker's default policy carries only engine-shaped rules (genet rungs,
//! nematic formats, surface engines) plus the neutral hand-to-OS fallback.
//! The app-flavored, host-handled ids live here (moved out of inker with the
//! 2026-07-10 genet adoption): internal pages and the JSON-LD
//! graph-contribution marker. Hosts (meerkat today, merecat next) route with
//! [`route_policy`] instead of `EngineRoutePolicy::default()`.

use inker::routing::{EngineRoutePolicy, EngineRouteRule, SurfaceContractMode};

/// Mere's internal pages (`about:` / `graphshell:` / `mere:` addresses —
/// settings, node views). Host-handled, never dispatched to a render engine.
/// Keeps the legacy `graphshell.internal` value so existing per-node pins and
/// persisted decisions resolve.
pub const ENGINE_GRAPHSHELL_INTERNAL: &str = "graphshell.internal";

/// Marker route target for JSON-LD graph ingest (linked-data plan Phase 2).
/// **Not a render engine.** A host that receives this decision feeds the body
/// to `linked_data::from_jsonld` to produce a `GraphContribution` it merges
/// into the graph, instead of dispatching to the engine registry. Routed
/// Headless (no surface); recognize it with [`is_graph_contribution_route`].
pub const ENGINE_LINKED_DATA_INGEST: &str = "linked-data.ingest";

/// Whether a route decision's `engine_id` is the JSON-LD ingest marker
/// ([`ENGINE_LINKED_DATA_INGEST`]) rather than a render engine. A host checks
/// this before dispatching a decision to the engine registry.
pub fn is_graph_contribution_route(engine_id: &str) -> bool {
    engine_id == ENGINE_LINKED_DATA_INGEST
}

/// Inker's default engine policy plus mere's app rules: internal-page schemes
/// route to [`ENGINE_GRAPHSHELL_INTERNAL`] and `application/ld+json` routes to
/// [`ENGINE_LINKED_DATA_INGEST`], both Headless. Rule order within the policy
/// is immaterial here: scheme rules and content-type rules match disjoint
/// requests, and neither app rule overlaps an engine rule's matcher.
pub fn route_policy() -> EngineRoutePolicy {
    let mut policy = EngineRoutePolicy::default();
    policy.rules.push(EngineRouteRule::new(
        ["about", "graphshell", "mere"],
        ENGINE_GRAPHSHELL_INTERNAL,
        SurfaceContractMode::Headless,
    ));
    // JSON-LD is a graph contribution, not a render: the host feeds it to
    // linked-data ingest rather than an engine. Headless.
    policy.rules.push(EngineRouteRule::content_type(
        ["application/ld+json"],
        ENGINE_LINKED_DATA_INGEST,
        SurfaceContractMode::Headless,
    ));
    policy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(address: &str) -> inker::routing::EngineRouteRequest {
        inker::routing::EngineRouteRequest {
            workspace_id: inker::routing::WorkspaceRouteId::new("main"),
            view: None,
            node: None,
            address: address.to_string(),
            content_type: None,
            pinned_engine: None,
        }
    }

    #[test]
    fn policy_routes_internal_addresses_headless() {
        let decision = route_policy().route(&request("graphshell://settings"));

        assert_eq!(decision.engine_id, ENGINE_GRAPHSHELL_INTERNAL);
        assert_eq!(
            decision.surface_contract.mode,
            SurfaceContractMode::Headless
        );
    }

    #[test]
    fn ld_json_routes_to_graph_contribution_ingest() {
        let mut request = request("https://example.test/data");
        request.content_type = Some("application/ld+json".to_string());
        let decision = route_policy().route(&request);

        assert_eq!(decision.engine_id, ENGINE_LINKED_DATA_INGEST);
        // A graph contribution has no visible surface.
        assert_eq!(
            decision.surface_contract.mode,
            SurfaceContractMode::Headless
        );
        // And it is recognized as a non-render route the host handles itself.
        assert!(is_graph_contribution_route(&decision.engine_id));
        assert!(!is_graph_contribution_route(
            inker::routing::ENGINE_GENET_WEB
        ));
    }

    #[test]
    fn engine_rules_still_route_beneath_the_app_layer() {
        let decision = route_policy().route(&request("gemini://example.test"));
        assert_eq!(
            decision.engine_id,
            inker::routing::ENGINE_NEMATIC_GEMTEXT
        );
    }
}
