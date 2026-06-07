/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Physics tuning + extension configs — pure-data preset types.
//!
//! These describe *what physics a lens wants*; they do not run physics.
//! In the donor they lived in `graph_canvas::physics_config` next to the
//! `ForceDirectedState` runtime they fed. Mere's graph-canvas uses a
//! different physics model (`scene_physics::ScenePhysicsConfig`), so the
//! lens registry owns these preset types itself and a consumer maps them
//! onto whatever runtime is active. The donor's `apply_graph_physics_tuning`
//! / `default_graph_physics_state` bridge (which mutated `ForceDirectedState`
//! directly) is intentionally not carried over — it targeted a runtime type
//! that no longer exists here.

use serde::{Deserialize, Serialize};

/// Top-level physics tuning — repulsion / attraction / gravity / damping
/// coefficients. Defaults match the historical graphshell physics feel;
/// lenses override these via tuning presets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphPhysicsTuning {
    pub repulsion_strength: f32,
    pub attraction_strength: f32,
    pub gravity_strength: f32,
    pub damping: f32,
}

impl Default for GraphPhysicsTuning {
    fn default() -> Self {
        Self {
            repulsion_strength: 0.28,
            attraction_strength: 0.22,
            gravity_strength: 0.18,
            damping: 0.55,
        }
    }
}

/// Degree-aware repulsion config — pushes high-degree nodes apart
/// proportionally to their connection count to prevent hub-of-hubs
/// crowding.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DegreeRepulsionConfig {
    pub radius_px: f32,
    pub strength: f32,
}

impl DegreeRepulsionConfig {
    pub const fn mild() -> Self {
        Self {
            radius_px: 220.0,
            strength: 4.0,
        }
    }

    pub const fn medium() -> Self {
        Self {
            radius_px: 220.0,
            strength: 8.0,
        }
    }
}

/// Domain-clustering config — attracts nodes sharing the same domain
/// (e.g., URL host) toward each other.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DomainClusteringConfig {
    pub strength: f32,
}

/// Semantic-clustering config — attracts nodes whose semantic classes
/// overlap (UDC closeness above the similarity floor).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SemanticClusteringConfig {
    pub strength: f32,
    pub similarity_floor: f32,
}

/// Hub-pull config — high-degree nodes attract their neighbours from a
/// wider radius, anchoring local clusters around hubs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HubPullConfig {
    pub radius_px: f32,
    pub strength: f32,
    pub degree_floor: usize,
}

impl Default for HubPullConfig {
    fn default() -> Self {
        Self {
            radius_px: 260.0,
            strength: 0.05,
            degree_floor: 3,
        }
    }
}

/// Aggregate of optional physics extensions a lens may enable.
/// `frame_affinity_enabled` toggles a post-physics frame-affinity
/// soft-attraction force at the consumer's call site.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GraphPhysicsExtensionConfig {
    pub degree_repulsion: Option<DegreeRepulsionConfig>,
    pub domain_clustering: Option<DomainClusteringConfig>,
    pub semantic_clustering: Option<SemanticClusteringConfig>,
    pub hub_pull: Option<HubPullConfig>,
    pub frame_affinity_enabled: bool,
}

impl GraphPhysicsExtensionConfig {
    pub fn any_enabled(self) -> bool {
        self.degree_repulsion.is_some()
            || self.domain_clustering.is_some()
            || self.semantic_clustering.is_some()
            || self.hub_pull.is_some()
            || self.frame_affinity_enabled
    }
}

/// Scene-level collision policy — node separation + viewport containment
/// toggles + scaling factors. Describes scene-collision intent without
/// owning a scene runtime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneCollisionPolicy {
    pub node_separation_enabled: bool,
    pub viewport_containment_enabled: bool,
    pub node_padding: f32,
    pub region_effect_scale: f32,
    pub containment_response_scale: f32,
}

impl SceneCollisionPolicy {
    pub fn enabled(self) -> bool {
        self.node_separation_enabled || self.viewport_containment_enabled
    }
}

/// Default node-padding for [`SceneCollisionPolicy`].
pub const DEFAULT_NODE_PADDING: f32 = 4.0;

impl Default for SceneCollisionPolicy {
    fn default() -> Self {
        Self {
            // Collision is opt-in; lenses that want it enable both flags.
            node_separation_enabled: false,
            viewport_containment_enabled: false,
            node_padding: DEFAULT_NODE_PADDING,
            region_effect_scale: 1.0,
            containment_response_scale: 1.0,
        }
    }
}
