/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-neutral engine routing vocabulary and default policy.

use graphshell_core::graph::{GraphViewId, NodeKey};
use serde::{Deserialize, Serialize};
pub use verso_tile::SurfaceTargetId;

pub const ENGINE_SERVAL_WEB: &str = "serval.web";
pub const ENGINE_NEMATIC_SMOLWEB: &str = "nematic.smolweb";
pub const ENGINE_NEMATIC_FILE: &str = "nematic.file";
pub const ENGINE_GRAPHSHELL_INTERNAL: &str = "graphshell.internal";
pub const ENGINE_EXTERNAL_PROTOCOL: &str = "host.external-protocol";

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceRouteId(pub String);

impl WorkspaceRouteId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineRouteRequest {
    pub workspace_id: WorkspaceRouteId,
    pub view: Option<GraphViewId>,
    pub node: Option<NodeKey>,
    pub address: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineRouteDecision {
    pub engine_id: String,
    pub surface_contract: SurfaceContract,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineRoutePolicy {
    pub rules: Vec<EngineRouteRule>,
    pub fallback: EngineRouteRule,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineRouteRule {
    pub schemes: Vec<String>,
    pub engine_id: String,
    pub mode: SurfaceContractMode,
}

impl EngineRoutePolicy {
    pub fn route(&self, request: &EngineRouteRequest) -> EngineRouteDecision {
        let scheme = address_scheme(&request.address);
        let rule = scheme
            .and_then(|scheme| self.rules.iter().find(|rule| rule.matches_scheme(scheme)))
            .unwrap_or(&self.fallback);

        EngineRouteDecision {
            engine_id: rule.engine_id.clone(),
            surface_contract: SurfaceContract {
                target: surface_target_for_request(request, scheme),
                mode: rule.mode,
            },
        }
    }
}

impl Default for EngineRoutePolicy {
    fn default() -> Self {
        Self {
            rules: vec![
                EngineRouteRule::new(
                    ["http", "https"],
                    ENGINE_SERVAL_WEB,
                    SurfaceContractMode::CompositedTexture,
                ),
                EngineRouteRule::new(
                    ["gemini", "gopher", "finger", "spartan"],
                    ENGINE_NEMATIC_SMOLWEB,
                    SurfaceContractMode::CompositedTexture,
                ),
                EngineRouteRule::new(
                    ["file"],
                    ENGINE_NEMATIC_FILE,
                    SurfaceContractMode::CompositedTexture,
                ),
                EngineRouteRule::new(
                    ["about", "graphshell", "mere"],
                    ENGINE_GRAPHSHELL_INTERNAL,
                    SurfaceContractMode::Headless,
                ),
            ],
            fallback: EngineRouteRule::new(
                Vec::<&str>::new(),
                ENGINE_EXTERNAL_PROTOCOL,
                SurfaceContractMode::Headless,
            ),
        }
    }
}

impl EngineRouteRule {
    pub fn new(
        schemes: impl IntoIterator<Item = impl Into<String>>,
        engine_id: impl Into<String>,
        mode: SurfaceContractMode,
    ) -> Self {
        Self {
            schemes: schemes.into_iter().map(Into::into).collect(),
            engine_id: engine_id.into(),
            mode,
        }
    }

    pub fn matches_scheme(&self, scheme: &str) -> bool {
        self.schemes
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(scheme))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceContract {
    pub target: SurfaceTargetId,
    pub mode: SurfaceContractMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SurfaceContractMode {
    CompositedTexture,
    NativeOverlay,
    EmbeddedHost,
    Headless,
}

pub fn address_scheme(address: &str) -> Option<&str> {
    let (scheme, _) = address.split_once(':')?;
    let first = scheme.as_bytes().first()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    if scheme
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
    {
        Some(scheme)
    } else {
        None
    }
}

fn surface_target_for_request(
    request: &EngineRouteRequest,
    scheme: Option<&str>,
) -> SurfaceTargetId {
    if let Some(node) = request.node {
        return SurfaceTargetId::new(format!("node:{}", node.index()));
    }
    if let Some(view) = request.view {
        return SurfaceTargetId::new(format!("view:{}", view.as_uuid()));
    }
    SurfaceTargetId::new(format!(
        "workspace:{}:{}",
        request.workspace_id.as_str(),
        scheme.unwrap_or("unknown")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(address: &str) -> EngineRouteRequest {
        EngineRouteRequest {
            workspace_id: WorkspaceRouteId::new("main"),
            view: None,
            node: None,
            address: address.to_string(),
        }
    }

    #[test]
    fn default_policy_routes_full_web_to_serval() {
        let decision = EngineRoutePolicy::default().route(&request("https://example.test"));

        assert_eq!(decision.engine_id, ENGINE_SERVAL_WEB);
        assert_eq!(
            decision.surface_contract.mode,
            SurfaceContractMode::CompositedTexture
        );
    }

    #[test]
    fn default_policy_routes_smolweb_to_nematic() {
        let decision = EngineRoutePolicy::default().route(&request("gemini://example.test"));

        assert_eq!(decision.engine_id, ENGINE_NEMATIC_SMOLWEB);
    }

    #[test]
    fn default_policy_routes_internal_addresses_headless() {
        let decision = EngineRoutePolicy::default().route(&request("graphshell://settings"));

        assert_eq!(decision.engine_id, ENGINE_GRAPHSHELL_INTERNAL);
        assert_eq!(
            decision.surface_contract.mode,
            SurfaceContractMode::Headless
        );
    }

    #[test]
    fn default_policy_does_not_guess_unknown_protocols() {
        let decision = EngineRoutePolicy::default().route(&request("mailto:hello@example.test"));

        assert_eq!(decision.engine_id, ENGINE_EXTERNAL_PROTOCOL);
        assert_eq!(
            decision.surface_contract.mode,
            SurfaceContractMode::Headless
        );
    }

    #[test]
    fn route_target_prefers_node_identity() {
        let mut request = request("https://example.test");
        request.node = Some(NodeKey::new(42));

        let decision = EngineRoutePolicy::default().route(&request);

        assert_eq!(decision.surface_contract.target.as_str(), "node:42");
    }
}
