/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Physics tuning + extension configs — pure-data types for
//! `ForceDirectedState`. Promoted to graph-canvas in Slice 65 from
//! the root crate's `graph/physics.rs` so the lens registry (and
//! eventually `register-lens`) can reference them without dragging
//! the binary root into portable code.
//!
//! What stays in the root crate's `graph/physics.rs`: the
//! app-mutating functions that take `&mut GraphBrowserApp` and run
//! per-frame physics integration (`apply_graph_physics_extensions`,
//! `apply_hub_pull_forces`, etc.). Those are genuinely host-side.
//!
//! `apply_graph_physics_tuning` lives here because it's pure data
//! → pure data: it just copies tuning fields onto the
//! [`ForceDirectedState`] (alias `GraphPhysicsState` in the root
//! crate's namespace).

use serde::{Deserialize, Serialize};

use crate::layout::ForceDirectedState;

/// Top-level physics tuning — repulsion / attraction / gravity /
/// damping coefficients. The defaults match the historical
/// graphshell physics feel; lenses override these via tuning
/// presets.
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

/// Domain-clustering config — attracts nodes sharing the same
/// domain (e.g., URL host) toward each other.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DomainClusteringConfig {
    pub strength: f32,
}

/// Semantic-clustering config — attracts nodes whose semantic
/// classes overlap (UDC closeness above the similarity floor).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SemanticClusteringConfig {
    pub strength: f32,
    pub similarity_floor: f32,
}

/// Hub-pull config — high-degree nodes attract their neighbours
/// from a wider radius, anchoring local clusters around hubs.
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
/// `frame_affinity_enabled` toggles the post-physics frame-affinity
/// soft-attraction force (derived from `CanvasRegistry.zones_enabled`
/// at call site).
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

/// Apply a [`GraphPhysicsTuning`] preset to a [`ForceDirectedState`].
/// Pure-data → pure-data: copies the four tuning coefficients onto
/// the corresponding force-directed-state fields. The state's other
/// fields (k_scale, dt, max_step) are independent and unchanged.
pub fn apply_graph_physics_tuning(state: &mut ForceDirectedState, tuning: GraphPhysicsTuning) {
    state.c_repulse = tuning.repulsion_strength;
    state.c_attract = tuning.attraction_strength;
    state.damping = tuning.damping;
    state.c_gravity = tuning.gravity_strength;
}

/// Default `ForceDirectedState` with `GraphPhysicsTuning::default()`
/// applied + the canonical `k_scale` / `dt` / `max_step`.
pub fn default_graph_physics_state() -> ForceDirectedState {
    let mut state = ForceDirectedState::default();
    apply_graph_physics_tuning(&mut state, GraphPhysicsTuning::default());
    state.k_scale = 0.42;
    state.dt = 0.03;
    state.max_step = 3.0;
    state
}

/// Scene-level collision policy — node separation + viewport
/// containment toggles + scaling factors. Promoted from the root
/// crate's `graph/scene_runtime.rs` so lens configurations can
/// describe scene-collision intent without dragging the host
/// scene-runtime into portable code.
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

/// Default node-padding for [`SceneCollisionPolicy`] — matches the
/// historical in-tree value before Slice 65 promotion.
pub const DEFAULT_NODE_PADDING: f32 = 4.0;

impl Default for SceneCollisionPolicy {
    fn default() -> Self {
        Self {
            // Historical in-tree defaults: collision is opt-in;
            // lenses that want it explicitly enable both flags.
            node_separation_enabled: false,
            viewport_containment_enabled: false,
            node_padding: DEFAULT_NODE_PADDING,
            region_effect_scale: 1.0,
            containment_response_scale: 1.0,
        }
    }
}
