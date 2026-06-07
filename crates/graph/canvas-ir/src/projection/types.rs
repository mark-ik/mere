/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! View dimension and projection mode.
//!
//! As of the field-algebra "full-break" reshape, [`ViewDimension`] is the
//! single per-view geometric state. The legacy `ZSource` and `ThreeDMode`
//! enums are gone — z-derivation lives in [`aether::FieldProjection`]
//! as `z_field: Option<FieldId>`, and what used to be `ThreeDMode` collapses
//! into the [`crate::projection::TwoPointFiveProjection`] preset family
//! (Isometric / Cabinet / Cavalier / Tilted / MildPerspective).
//!
//! [`ProjectionMode`] is kept as a back-compat type alias of
//! [`ViewDimension`] so existing call sites at `*::ProjectionMode::TwoD` /
//! `::TwoPointFive { .. }` keep working without churn.

use serde::{Deserialize, Serialize};

use aether::FieldId;
use crate::projection::presets::{TwoDPreset, TwoPointFiveProjection};

/// Per-view dimension state.
///
/// `(x, y)` layout truth lives elsewhere; this enum chooses how that truth
/// is projected and what visual preset decorates the result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ViewDimension {
    /// Pure 2D with a visual preset (background, decoration).
    TwoD { preset: TwoDPreset },
    /// 2.5D with one of the projection-family presets.
    /// `z_field`, when `Some`, names the scalar field whose value at each
    /// node position drives that node's ephemeral z. `None` means z=0
    /// everywhere (visual preset only, no depth).
    TwoPointFive {
        projection: TwoPointFiveProjection,
        z_field: Option<FieldId>,
    },
    /// Architecture-only placeholder; not renderable until a 3D
    /// (free-camera) program is defined.
    ThreeD,
}

impl Default for ViewDimension {
    fn default() -> Self {
        Self::TwoD {
            preset: TwoDPreset::Plain,
        }
    }
}

impl ViewDimension {
    /// Whether this dimension is currently renderable.
    /// `ThreeD` is architecture-only and returns `false`.
    pub fn is_renderable(&self) -> bool {
        !matches!(self, Self::ThreeD)
    }

    /// Degrade to `TwoD { preset: Plain }` if this dimension is not
    /// renderable.
    pub fn degrade_if_needed(&self) -> Self {
        if self.is_renderable() {
            self.clone()
        } else {
            Self::default()
        }
    }

    /// Identity helper retained for back-compat with the previous
    /// `ProjectionMode::from_view_dimension` API.
    pub fn from_view_dimension(dim: &ViewDimension) -> Self {
        dim.clone()
    }
}

/// Back-compat alias: existing call sites that import `ProjectionMode`
/// continue to work. New code should refer to [`ViewDimension`] directly.
pub type ProjectionMode = ViewDimension;

/// Empty marker struct kept for back-compat with consumers that pass a
/// `ProjectionConfig` to [`crate::projection::project_position`].
/// Per-projection tuning now lives inside the
/// [`TwoPointFiveProjection`] variants directly, so there are no global
/// projection settings here. May regain fields if a future cycle adds
/// global projection state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectionConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_twod_plain() {
        assert_eq!(
            ViewDimension::default(),
            ViewDimension::TwoD {
                preset: TwoDPreset::Plain
            }
        );
    }

    #[test]
    fn default_is_renderable() {
        assert!(ViewDimension::default().is_renderable());
    }

    #[test]
    fn threed_is_not_renderable() {
        assert!(!ViewDimension::ThreeD.is_renderable());
    }

    #[test]
    fn twopointfive_is_renderable() {
        let v = ViewDimension::TwoPointFive {
            projection: TwoPointFiveProjection::isometric_default(),
            z_field: None,
        };
        assert!(v.is_renderable());
    }

    #[test]
    fn degrade_threed_to_default() {
        assert_eq!(
            ViewDimension::ThreeD.degrade_if_needed(),
            ViewDimension::default()
        );
    }

    #[test]
    fn degrade_twod_is_noop() {
        let v = ViewDimension::TwoD {
            preset: TwoDPreset::Paper,
        };
        assert_eq!(v.degrade_if_needed(), v);
    }

    #[test]
    fn projection_mode_is_view_dimension_alias() {
        let p: ProjectionMode = ProjectionMode::default();
        assert_eq!(p, ViewDimension::default());
    }

    #[test]
    fn from_view_dimension_is_identity() {
        let v = ViewDimension::TwoPointFive {
            projection: TwoPointFiveProjection::cabinet_default(),
            z_field: Some(FieldId::new()),
        };
        assert_eq!(ViewDimension::from_view_dimension(&v), v);
    }

    #[test]
    fn serde_roundtrip_view_dimension() {
        let dims = [
            ViewDimension::TwoD {
                preset: TwoDPreset::Heatmap { field: FieldId::new() },
            },
            ViewDimension::TwoPointFive {
                projection: TwoPointFiveProjection::isometric_default(),
                z_field: Some(FieldId::new()),
            },
            ViewDimension::ThreeD,
        ];
        for d in &dims {
            let json = serde_json::to_string(d).unwrap();
            let back: ViewDimension = serde_json::from_str(&json).unwrap();
            assert_eq!(*d, back);
        }
    }
}
