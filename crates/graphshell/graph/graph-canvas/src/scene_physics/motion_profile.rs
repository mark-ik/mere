/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Simulation-feel motion profile and release-impulse decay.
//!
//! When a node is released after dragging, its drag velocity can be
//! captured as a release impulse that decays over subsequent frames. This
//! module owns the preset → profile mapping and the per-frame decay step.

use euclid::default::Vector2D;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;

/// Behavior preset for simulation-feel motion.
///
/// Each preset biases separation feel, containment response, and region
/// effect strength. The host maps these to a [`SimulateMotionProfile`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SimulateBehaviorPreset {
    /// Loose, gliding movement. Nodes coast longer after release.
    #[default]
    Float,
    /// Tight, snappy movement. Nodes settle quickly.
    Packed,
    /// Moderate coast with stronger region effects.
    Magnetic,
}

/// Motion parameters for release impulses.
///
/// When a node is released after dragging, its drag velocity can be captured as
/// a release impulse that decays over subsequent frames. This profile controls
/// the feel of that coasting behavior.
///
/// The host typically resolves this via a preset ([`SimulateBehaviorPreset`]
/// → [`SimulateMotionProfile::for_preset`]) but may also user-tune the
/// individual fields by setting a per-view or per-graph override.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SimulateMotionProfile {
    /// Scale factor applied to the captured drag impulse.
    pub release_impulse_scale: f32,
    /// Per-frame multiplicative decay applied to the impulse.
    pub release_decay: f32,
    /// Minimum impulse magnitude; below this the impulse is zeroed.
    pub min_impulse: f32,
}

impl Default for SimulateMotionProfile {
    fn default() -> Self {
        Self::for_preset(SimulateBehaviorPreset::default())
    }
}

impl SimulateMotionProfile {
    /// Get the canonical motion profile for a behavior preset.
    pub fn for_preset(preset: SimulateBehaviorPreset) -> Self {
        match preset {
            SimulateBehaviorPreset::Float => Self {
                release_impulse_scale: 1.15,
                release_decay: 0.84,
                min_impulse: 0.03,
            },
            SimulateBehaviorPreset::Packed => Self {
                release_impulse_scale: 0.45,
                release_decay: 0.45,
                min_impulse: 0.05,
            },
            SimulateBehaviorPreset::Magnetic => Self {
                release_impulse_scale: 0.7,
                release_decay: 0.62,
                min_impulse: 0.04,
            },
        }
    }
}

/// Compute one frame of release-impulse decay.
///
/// Takes a map of node ID → current impulse vector and returns the decayed
/// impulses for the next frame. Impulses below `min_impulse` are removed.
/// Also returns the position deltas to apply this frame.
///
/// The `remaining_frames` parameter controls a frame-based scale ramp
/// (nodes coast more at the start, less as the window closes). Set to the
/// remaining frames in the release window.
pub fn compute_release_impulse_frame<N: Clone + Eq + Hash>(
    impulses: &HashMap<N, Vector2D<f32>>,
    profile: &SimulateMotionProfile,
    remaining_frames: u32,
) -> (HashMap<N, Vector2D<f32>>, HashMap<N, Vector2D<f32>>) {
    if remaining_frames == 0 || impulses.is_empty() {
        return (HashMap::new(), HashMap::new());
    }

    let frame_scale = (remaining_frames as f32 / 10.0).clamp(0.1, 1.0);

    let deltas: HashMap<N, Vector2D<f32>> = impulses
        .iter()
        .filter_map(|(key, impulse)| {
            let delta = *impulse * frame_scale * profile.release_impulse_scale;
            (delta.square_length() > f32::EPSILON).then_some((key.clone(), delta))
        })
        .collect();

    let next_impulses: HashMap<N, Vector2D<f32>> = impulses
        .iter()
        .filter_map(|(key, impulse)| {
            let decayed = *impulse * profile.release_decay;
            (decayed.length() >= profile.min_impulse).then_some((key.clone(), decayed))
        })
        .collect();

    (deltas, next_impulses)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_profile_float_preset() {
        let profile = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Float);
        assert!(profile.release_impulse_scale > 1.0);
        assert!(profile.release_decay > 0.7);
    }

    #[test]
    fn motion_profile_packed_preset() {
        let profile = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Packed);
        assert!(profile.release_impulse_scale < 1.0);
        assert!(profile.release_decay < 0.5);
    }

    #[test]
    fn motion_profile_magnetic_preset() {
        let profile = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Magnetic);
        let float = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Float);
        let packed = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Packed);
        assert!(profile.release_impulse_scale < float.release_impulse_scale);
        assert!(profile.release_impulse_scale > packed.release_impulse_scale);
    }

    #[test]
    fn release_impulse_decays_over_frames() {
        let profile = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Float);
        let mut impulses = HashMap::new();
        impulses.insert(0u32, Vector2D::new(10.0, 0.0));

        let (deltas, next) = compute_release_impulse_frame(&impulses, &profile, 5);
        assert!(deltas.contains_key(&0));
        assert!(next.contains_key(&0));
        assert!(next[&0].length() < impulses[&0].length());
    }

    #[test]
    fn release_impulse_zeroes_when_small() {
        let profile = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Packed);
        let mut impulses = HashMap::new();
        impulses.insert(0u32, Vector2D::new(0.01, 0.0));

        let (_deltas, next) = compute_release_impulse_frame(&impulses, &profile, 5);
        assert!(next.is_empty());
    }

    #[test]
    fn release_impulse_empty_when_zero_frames() {
        let profile = SimulateMotionProfile::for_preset(SimulateBehaviorPreset::Float);
        let mut impulses = HashMap::new();
        impulses.insert(0u32, Vector2D::new(10.0, 0.0));

        let (deltas, next) = compute_release_impulse_frame(&impulses, &profile, 0);
        assert!(deltas.is_empty());
        assert!(next.is_empty());
    }

    #[test]
    fn behavior_preset_default_is_float() {
        assert_eq!(
            SimulateBehaviorPreset::default(),
            SimulateBehaviorPreset::Float
        );
    }
}
