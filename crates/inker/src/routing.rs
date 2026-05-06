/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-neutral engine routing vocabulary.

use graphshell_core::graph::{GraphViewId, NodeKey};
use serde::{Deserialize, Serialize};
pub use verso_tile::SurfaceTargetId;

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
