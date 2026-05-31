/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Lossless dimension-ladder presets.
//!
//! The current [`crate::projection::ProjectionMode`] keeps `Isometric` as a
//! sibling of `TwoPointFive`. This module introduces preset families that
//! reframe `Isometric` as **one preset within the 2.5D projection family**,
//! per the field-algebra plan §3.8.
//!
//! Today these enums ship alongside `ProjectionMode` as additive types.
//! A follow-up cycle will wire them into a `ViewDimension` variant; until
//! then, callers can opt in by storing a preset alongside their existing
//! [`ProjectionMode`] state and reading both at draw time.
//!
//! ## Lossless ladder semantics
//!
//! - **2D ↔ 2D preset swap**: visual change only; no state mutation.
//! - **2D → 2.5D**: derive z by evaluating the active z-field
//!   (or legacy `ZSource`) at every node position; pick a 2.5D projection
//!   preset.
//! - **2.5D ↔ 2.5D preset swap**: swap projection params; z stays.
//! - **2.5D → 3D**: unlock camera at the projection preset's current pose;
//!   z stays; couplings unchanged.
//! - **Any → 2D**: discard z; `(x, y)` preserved.

use aether::FieldId;
use serde::{Deserialize, Serialize};

/// Visual presets for the 2D projection family. Pure styling — `(x, y)`
/// layout truth is unaffected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum TwoDPreset {
    /// Plain background, no decoration.
    #[default]
    Plain,
    /// Subtle warm background with soft drop shadows on nodes.
    Paper,
    /// Contour lines from a designated scalar field.
    Terrain { field: FieldId },
    /// False-color a scalar field as a heatmap background.
    Heatmap { field: FieldId },
    /// Light grid overlay.
    Grid,
}

/// Projection variants within the 2.5D family.
///
/// The plan reframes `Isometric` as a member of this family rather than a
/// sibling of `TwoPointFive`. Each variant preserves the canonical 2D
/// `(x, y)` and uses an ephemeral `z` for depth cueing only.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TwoPointFiveProjection {
    /// Classic axonometric isometric — equal foreshortening on x/y/z.
    /// `dimetric_angle` controls deviation from the canonical 30°/30°.
    Isometric { dimetric_angle: f32 },
    /// CAD cabinet projection — depth axis at half scale and a tilt angle.
    Cabinet { z_scale: f32, angle_deg: f32 },
    /// CAD cavalier projection — depth axis at full scale.
    Cavalier { angle_deg: f32 },
    /// Top-down camera tilted by a small angle for depth hint.
    Tilted { tilt_deg: f32 },
    /// Soft vanishing-point hint without unlocking the camera.
    MildPerspective { focal: f32 },
}

impl TwoPointFiveProjection {
    /// The canonical 30°/30° axonometric isometric.
    pub fn isometric_default() -> Self {
        Self::Isometric {
            dimetric_angle: 30.0,
        }
    }

    /// Classic CAD cabinet projection.
    pub fn cabinet_default() -> Self {
        Self::Cabinet {
            z_scale: 0.5,
            angle_deg: 45.0,
        }
    }

    /// Classic CAD cavalier projection.
    pub fn cavalier_default() -> Self {
        Self::Cavalier { angle_deg: 30.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twod_preset_default_is_plain() {
        assert_eq!(TwoDPreset::default(), TwoDPreset::Plain);
    }

    #[test]
    fn twod_preset_serde_roundtrip() {
        let presets = [
            TwoDPreset::Plain,
            TwoDPreset::Paper,
            TwoDPreset::Terrain { field: FieldId(3) },
            TwoDPreset::Heatmap { field: FieldId(7) },
            TwoDPreset::Grid,
        ];
        for p in &presets {
            let json = serde_json::to_string(p).unwrap();
            let back: TwoDPreset = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, back);
        }
    }

    #[test]
    fn two_point_five_projection_serde_roundtrip() {
        let projections = [
            TwoPointFiveProjection::isometric_default(),
            TwoPointFiveProjection::cabinet_default(),
            TwoPointFiveProjection::cavalier_default(),
            TwoPointFiveProjection::Tilted { tilt_deg: 15.0 },
            TwoPointFiveProjection::MildPerspective { focal: 800.0 },
        ];
        for p in &projections {
            let json = serde_json::to_string(p).unwrap();
            let back: TwoPointFiveProjection = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, back);
        }
    }

    #[test]
    fn isometric_default_is_thirty_degrees() {
        match TwoPointFiveProjection::isometric_default() {
            TwoPointFiveProjection::Isometric { dimetric_angle } => {
                assert_eq!(dimetric_angle, 30.0);
            }
            _ => panic!("expected Isometric"),
        }
    }

    #[test]
    fn cabinet_default_is_half_scale() {
        match TwoPointFiveProjection::cabinet_default() {
            TwoPointFiveProjection::Cabinet { z_scale, angle_deg } => {
                assert_eq!(z_scale, 0.5);
                assert_eq!(angle_deg, 45.0);
            }
            _ => panic!("expected Cabinet"),
        }
    }

    #[test]
    fn presets_are_distinct() {
        // Sanity: structurally distinct presets serialise to distinct JSON.
        let iso = serde_json::to_string(&TwoPointFiveProjection::isometric_default()).unwrap();
        let cab = serde_json::to_string(&TwoPointFiveProjection::cabinet_default()).unwrap();
        let cav = serde_json::to_string(&TwoPointFiveProjection::cavalier_default()).unwrap();
        assert_ne!(iso, cab);
        assert_ne!(cab, cav);
        assert_ne!(iso, cav);
    }
}
