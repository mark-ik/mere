/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Scene physics: node separation, viewport containment, region effects,
//! and simulation-feel motion profiles.
//!
//! Pure functions that compute position deltas for a set of node snapshots;
//! the host applies the deltas to its own position storage. No framework
//! dependency, no mutation of graph truth.
//!
//! ## Module layout
//!
//! - [`separation`] — [`compute_node_separation`] (circle-circle pushout)
//! - [`containment`] — [`compute_viewport_containment`] (clamp to rect)
//! - [`region_effects`] — [`compute_region_effects`] (attractor / repulsor /
//!   dampener / wall over `SceneRegion`s)
//! - [`motion_profile`] — [`SimulateBehaviorPreset`],
//!   [`SimulateMotionProfile`], [`compute_release_impulse_frame`]
//!
//! Per the field-algebra plan, the per-region effect machinery here is the
//! embryo of the field-coupling system; Phase 6 will reground it as one
//! lowering of the field algebra.

pub mod containment;
pub mod motion_profile;
pub mod region_effects;
pub mod separation;

pub use containment::compute_viewport_containment;
pub use motion_profile::{
    SimulateBehaviorPreset, SimulateMotionProfile, compute_release_impulse_frame,
};
pub use region_effects::compute_region_effects;
pub use separation::compute_node_separation;

use euclid::default::Point2D;
use serde::{Deserialize, Serialize};

use crate::scene_region::SceneRegionId;

/// Configuration for the scene physics pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScenePhysicsConfig {
    /// Whether node-node separation is enabled.
    pub node_separation_enabled: bool,
    /// Whether viewport containment is enabled.
    pub viewport_containment_enabled: bool,
    /// Padding around nodes for collision detection.
    pub node_padding: f32,
    /// Scale factor for region effects.
    pub region_effect_scale: f32,
    /// Scale factor for containment response.
    pub containment_response_scale: f32,
    /// Number of separation passes per frame.
    pub separation_passes: u32,
    /// Maximum region-effect delta per pass.
    pub max_region_delta: f32,
}

impl Default for ScenePhysicsConfig {
    fn default() -> Self {
        Self {
            node_separation_enabled: false,
            viewport_containment_enabled: false,
            node_padding: 4.0,
            region_effect_scale: 1.0,
            containment_response_scale: 1.0,
            separation_passes: 3,
            max_region_delta: 18.0,
        }
    }
}

/// A snapshot of a node's spatial state for physics computation.
#[derive(Debug, Clone, Copy)]
pub struct NodeSnapshot<N> {
    pub id: N,
    pub position: Point2D<f32>,
    pub radius: f32,
    pub pinned: bool,
}

/// An event emitted by the scene physics system.
///
/// This portable crate does not emit events by itself; hosts can detect region
/// enter/exit by comparing snapshots, or bridge events from a simulation
/// adapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SceneEvent<N> {
    /// Two bodies began touching.
    ContactBegin { a: N, b: N },
    /// Two bodies stopped touching.
    ContactEnd { a: N, b: N },
    /// A node entered a region sensor.
    TriggerEnter { node: N, region: SceneRegionId },
    /// A node exited a region sensor.
    TriggerExit { node: N, region: SceneRegionId },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_event_serde_roundtrip() {
        let events: Vec<SceneEvent<u32>> = vec![
            SceneEvent::ContactBegin { a: 0, b: 1 },
            SceneEvent::ContactEnd { a: 0, b: 1 },
            SceneEvent::TriggerEnter {
                node: 0,
                region: SceneRegionId(42),
            },
            SceneEvent::TriggerExit {
                node: 1,
                region: SceneRegionId(99),
            },
        ];
        let json = serde_json::to_string(&events).unwrap();
        let back: Vec<SceneEvent<u32>> = serde_json::from_str(&json).unwrap();
        assert_eq!(events, back);
    }
}
