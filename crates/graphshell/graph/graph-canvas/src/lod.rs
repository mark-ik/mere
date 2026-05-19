/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Level-of-detail and culling policy types.
//!
//! These types describe how the canvas decides what to draw at a given zoom
//! level. Actual LOD logic arrives in Phase 1 (packet derivation); Phase 0
//! defines the policy carriers.

use serde::{Deserialize, Serialize};

/// LOD level for a node or edge at the current zoom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum LodLevel {
    /// Full detail: label, badge, thumbnail, full stroke.
    Full,
    /// Reduced: no label, simplified shape.
    Reduced,
    /// Minimal: dot or single-pixel representation.
    Minimal,
    /// Culled: not drawn at all (outside viewport or too small).
    Culled,
}

impl LodLevel {
    /// Whether this level results in the item being drawn at all.
    pub fn is_visible(&self) -> bool {
        !matches!(self, Self::Culled)
    }
}

impl Default for LodLevel {
    fn default() -> Self {
        Self::Full
    }
}

/// Policy for how LOD levels are assigned based on zoom and viewport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LodPolicy {
    /// Zoom threshold below which nodes degrade from Full to Reduced.
    pub reduced_threshold: f32,
    /// Zoom threshold below which nodes degrade from Reduced to Minimal.
    pub minimal_threshold: f32,
    /// Whether to cull nodes entirely when their projected radius is below
    /// this pixel count.
    pub cull_below_px: f32,
}

impl Default for LodPolicy {
    fn default() -> Self {
        Self {
            reduced_threshold: 0.4,
            minimal_threshold: 0.15,
            cull_below_px: 1.0,
        }
    }
}

impl LodPolicy {
    /// Determine the LOD level for a node given the current zoom and its
    /// world-space radius.
    pub fn level_for_node(&self, zoom: f32, world_radius: f32) -> LodLevel {
        let screen_radius = world_radius * zoom;
        if screen_radius < self.cull_below_px {
            LodLevel::Culled
        } else if zoom < self.minimal_threshold {
            LodLevel::Minimal
        } else if zoom < self.reduced_threshold {
            LodLevel::Reduced
        } else {
            LodLevel::Full
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_full_at_normal_zoom() {
        let policy = LodPolicy::default();
        assert_eq!(policy.level_for_node(1.0, 16.0), LodLevel::Full);
    }

    #[test]
    fn reduced_at_low_zoom() {
        let policy = LodPolicy::default();
        assert_eq!(policy.level_for_node(0.3, 16.0), LodLevel::Reduced);
    }

    #[test]
    fn minimal_at_very_low_zoom() {
        let policy = LodPolicy::default();
        assert_eq!(policy.level_for_node(0.1, 16.0), LodLevel::Minimal);
    }

    #[test]
    fn culled_when_too_small() {
        let policy = LodPolicy::default();
        assert_eq!(policy.level_for_node(0.01, 0.5), LodLevel::Culled);
    }

    #[test]
    fn culled_is_not_visible() {
        assert!(!LodLevel::Culled.is_visible());
        assert!(LodLevel::Full.is_visible());
        assert!(LodLevel::Minimal.is_visible());
    }

    #[test]
    fn serde_roundtrip_lod_policy() {
        let policy = LodPolicy {
            reduced_threshold: 0.5,
            minimal_threshold: 0.2,
            cull_below_px: 2.0,
        };
        let json = serde_json::to_string(&policy).unwrap();
        let back: LodPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy, back);
    }
}
