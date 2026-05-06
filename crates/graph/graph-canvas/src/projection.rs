/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! View dimension, projection mode, and z-source types.
//!
//! These are portable mirrors of the view-dimension types currently owned by
//! `app::graph_views`. They describe *how* a graph view projects its 2D layout
//! truth into a rendered scene — they do not own that layout truth.

use euclid::default::Point2D;
use serde::{Deserialize, Serialize};

/// How z-coordinates are assigned to nodes in a 3D-mode graph view.
///
/// z-positions are ephemeral: they are recomputed from this source plus node
/// metadata on every 2D-to-3D switch and are never persisted independently.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum ZSource {
    /// All nodes coplanar — soft 3D visual effect only.
    #[default]
    Zero,
    /// Recent nodes float to front; `max_depth` controls the maximum z offset.
    Recency { max_depth: f32 },
    /// Root nodes at z=0; deeper BFS nodes further back; `scale` controls
    /// layer spacing.
    BfsDepth { scale: f32 },
    /// UDC main class determines z layer; `scale` controls layer spacing.
    UdcLevel { scale: f32 },
    /// Per-node z override sourced from node metadata.
    Manual,
}

/// Sub-mode for a 3D graph view, ordered by implementation complexity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreeDMode {
    /// 2.5D: fixed top-down perspective; z is visual-only depth offset.
    /// Navigation remains 2D (pan/zoom). No camera tilt.
    TwoPointFive,
    /// Isometric: quantized z layers, fixed-angle projection.
    /// Layer separation reveals hierarchical/temporal structure.
    Isometric,
    /// Standard 3D: reorientable arcball camera, arbitrary z.
    /// Architecture-only until a later 3D program defines the full contract.
    Standard,
}

/// Dimension mode for a graph view.
///
/// Snapshot degradation rule: if a persisted snapshot contains `ThreeD` but 3D
/// rendering is unavailable (e.g. unsupported platform), the view falls back to
/// `TwoD`; (x, y) positions are preserved unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum ViewDimension {
    /// Standard 2D planar graph.
    #[default]
    TwoD,
    /// 3D graph with the given sub-mode and z-source.
    ThreeD { mode: ThreeDMode, z_source: ZSource },
}

/// Projection mode for scene derivation.
///
/// This is the canvas-facing projection instruction consumed by packet
/// derivation and backend rendering. It is derived from `ViewDimension` but
/// flattened for direct use in projection math.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum ProjectionMode {
    #[default]
    TwoD,
    TwoPointFive {
        z_source: ZSource,
    },
    Isometric {
        z_source: ZSource,
    },
    /// Architecture-only placeholder. Not renderable until a 3D program is
    /// defined.
    Standard,
}

impl ProjectionMode {
    /// Derive a `ProjectionMode` from a `ViewDimension`.
    pub fn from_view_dimension(dim: &ViewDimension) -> Self {
        match dim {
            ViewDimension::TwoD => Self::TwoD,
            ViewDimension::ThreeD { mode, z_source } => match mode {
                ThreeDMode::TwoPointFive => Self::TwoPointFive {
                    z_source: z_source.clone(),
                },
                ThreeDMode::Isometric => Self::Isometric {
                    z_source: z_source.clone(),
                },
                ThreeDMode::Standard => Self::Standard,
            },
        }
    }

    /// Whether this projection is currently renderable.
    /// `Standard` is architecture-only and returns `false`.
    pub fn is_renderable(&self) -> bool {
        !matches!(self, Self::Standard)
    }

    /// Degrade to `TwoD` if this projection is not renderable.
    pub fn degrade_if_needed(&self) -> Self {
        if self.is_renderable() {
            self.clone()
        } else {
            Self::TwoD
        }
    }
}

// ── Projection configuration ─────────────────────────────────────────────────

/// Tuning parameters for 2.5D perspective projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TwoPointFiveConfig {
    /// Perspective convergence rate. Larger = faster shrink with depth.
    pub perspective_k: f32,
    /// Minimum depth scale — nodes never shrink below this fraction.
    pub min_depth_scale: f32,
    /// Y shift per z unit (positive = further down on screen).
    pub y_shift: f32,
}

impl Default for TwoPointFiveConfig {
    fn default() -> Self {
        Self {
            perspective_k: 0.003,
            min_depth_scale: 0.4,
            y_shift: 0.15,
        }
    }
}

/// Tuning parameters for isometric projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IsometricConfig {
    /// X shift per z unit (positive = rightward).
    pub x_shift: f32,
    /// Y shift per z unit (positive = upward in screen space).
    pub y_shift: f32,
}

impl Default for IsometricConfig {
    fn default() -> Self {
        Self {
            x_shift: 0.5,
            y_shift: 0.35,
        }
    }
}

/// Projection configuration. Controls the visual behavior of each projection
/// mode. Serializable so it can be persisted in user preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectionConfig {
    pub two_point_five: TwoPointFiveConfig,
    pub isometric: IsometricConfig,
}

impl Default for ProjectionConfig {
    fn default() -> Self {
        Self {
            two_point_five: TwoPointFiveConfig::default(),
            isometric: IsometricConfig::default(),
        }
    }
}

// ── Projection math ──────────────────────────────────────────────────────────

/// A projected position: the screen-space result of projecting a 2D graph
/// position through a projection mode, given an optional z value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectedPosition {
    /// Screen-space x after projection.
    pub x: f32,
    /// Screen-space y after projection.
    pub y: f32,
    /// Depth value used for draw ordering and visual scaling. 0.0 = front.
    pub depth: f32,
    /// Scale factor derived from depth (1.0 = no scaling, <1.0 = further away).
    pub depth_scale: f32,
}

impl ProjectedPosition {
    pub fn to_point(&self) -> Point2D<f32> {
        Point2D::new(self.x, self.y)
    }
}

/// Project a 2D graph position into screen space through the given projection.
///
/// - `world_pos`: canonical (x, y) in graph layout space
/// - `z`: the derived z value for this node (0.0 if `ZSource::Zero` or TwoD)
/// - `projection`: which projection mode to apply
/// - `config`: tuning parameters for the active projection
///
/// All projections preserve the canonical (x, y) — they only add depth cues,
/// y-offset, and scale. The caller is responsible for deriving `z` from the
/// active `ZSource` before calling this function.
pub fn project_position(
    world_pos: Point2D<f32>,
    z: f32,
    projection: &ProjectionMode,
    config: &ProjectionConfig,
) -> ProjectedPosition {
    match projection {
        ProjectionMode::TwoD => ProjectedPosition {
            x: world_pos.x,
            y: world_pos.y,
            depth: 0.0,
            depth_scale: 1.0,
        },
        ProjectionMode::TwoPointFive { .. } => {
            let cfg = &config.two_point_five;
            let depth_factor = perspective_depth_factor(z, cfg);
            ProjectedPosition {
                x: world_pos.x * depth_factor,
                y: world_pos.y * depth_factor + z * cfg.y_shift,
                depth: z,
                depth_scale: depth_factor,
            }
        }
        ProjectionMode::Isometric { .. } => {
            let cfg = &config.isometric;
            ProjectedPosition {
                x: world_pos.x + z * cfg.x_shift,
                y: world_pos.y - z * cfg.y_shift,
                depth: z,
                depth_scale: 1.0,
            }
        }
        ProjectionMode::Standard => ProjectedPosition {
            x: world_pos.x,
            y: world_pos.y,
            depth: 0.0,
            depth_scale: 1.0,
        },
    }
}

/// Perspective scale factor for 2.5D depth.
fn perspective_depth_factor(z: f32, cfg: &TwoPointFiveConfig) -> f32 {
    let raw = 1.0 / (1.0 + z.abs() * cfg.perspective_k);
    raw.max(cfg.min_depth_scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_twod() {
        assert_eq!(ViewDimension::default(), ViewDimension::TwoD);
        assert_eq!(ProjectionMode::default(), ProjectionMode::TwoD);
    }

    #[test]
    fn projection_from_twod() {
        let dim = ViewDimension::TwoD;
        assert_eq!(
            ProjectionMode::from_view_dimension(&dim),
            ProjectionMode::TwoD
        );
    }

    #[test]
    fn projection_from_twopointfive() {
        let dim = ViewDimension::ThreeD {
            mode: ThreeDMode::TwoPointFive,
            z_source: ZSource::Recency { max_depth: 100.0 },
        };
        let proj = ProjectionMode::from_view_dimension(&dim);
        assert!(matches!(proj, ProjectionMode::TwoPointFive { .. }));
        assert!(proj.is_renderable());
    }

    #[test]
    fn projection_standard_not_renderable() {
        let dim = ViewDimension::ThreeD {
            mode: ThreeDMode::Standard,
            z_source: ZSource::Zero,
        };
        let proj = ProjectionMode::from_view_dimension(&dim);
        assert!(!proj.is_renderable());
    }

    #[test]
    fn serde_roundtrip_view_dimension() {
        let dims = [
            ViewDimension::TwoD,
            ViewDimension::ThreeD {
                mode: ThreeDMode::Isometric,
                z_source: ZSource::BfsDepth { scale: 2.5 },
            },
        ];
        for dim in &dims {
            let json = serde_json::to_string(dim).unwrap();
            let back: ViewDimension = serde_json::from_str(&json).unwrap();
            assert_eq!(dim, &back);
        }
    }

    #[test]
    fn degrade_standard_to_twod() {
        let mode = ProjectionMode::Standard;
        let degraded = mode.degrade_if_needed();
        assert_eq!(degraded, ProjectionMode::TwoD);
    }

    #[test]
    fn degrade_twod_is_noop() {
        let mode = ProjectionMode::TwoD;
        assert_eq!(mode.degrade_if_needed(), ProjectionMode::TwoD);
    }

    fn cfg() -> ProjectionConfig {
        ProjectionConfig::default()
    }

    // ── Projection math tests ────────────────────────────────────────────────

    #[test]
    fn twod_projection_is_identity() {
        let pos = Point2D::new(100.0, 200.0);
        let result = super::project_position(pos, 0.0, &ProjectionMode::TwoD, &cfg());
        assert_eq!(result.x, 100.0);
        assert_eq!(result.y, 200.0);
        assert_eq!(result.depth, 0.0);
        assert_eq!(result.depth_scale, 1.0);
    }

    #[test]
    fn twod_projection_ignores_z() {
        let pos = Point2D::new(50.0, 50.0);
        let result = super::project_position(pos, 999.0, &ProjectionMode::TwoD, &cfg());
        assert_eq!(result.x, 50.0);
        assert_eq!(result.y, 50.0);
        assert_eq!(result.depth, 0.0);
    }

    #[test]
    fn twopointfive_zero_z_is_identity() {
        let pos = Point2D::new(100.0, 200.0);
        let mode = ProjectionMode::TwoPointFive {
            z_source: ZSource::Zero,
        };
        let result = super::project_position(pos, 0.0, &mode, &cfg());
        assert_eq!(result.x, 100.0);
        assert_eq!(result.y, 200.0);
        assert_eq!(result.depth_scale, 1.0);
    }

    #[test]
    fn twopointfive_positive_z_shrinks_and_shifts_down() {
        let pos = Point2D::new(100.0, 200.0);
        let mode = ProjectionMode::TwoPointFive {
            z_source: ZSource::Recency { max_depth: 100.0 },
        };
        let result = super::project_position(pos, 50.0, &mode, &cfg());
        // Deeper node should be smaller.
        assert!(result.depth_scale < 1.0);
        assert!(result.depth_scale >= cfg().two_point_five.min_depth_scale);
        // Y should shift down relative to identity.
        assert!(result.y > 200.0 * result.depth_scale);
        assert_eq!(result.depth, 50.0);
    }

    #[test]
    fn twopointfive_depth_scale_is_bounded() {
        let pos = Point2D::new(100.0, 200.0);
        let mode = ProjectionMode::TwoPointFive {
            z_source: ZSource::Zero,
        };
        // Even at extreme z, scale doesn't go below MIN_DEPTH_SCALE.
        let result = super::project_position(pos, 10000.0, &mode, &cfg());
        assert!((result.depth_scale - cfg().two_point_five.min_depth_scale).abs() < 0.01);
    }

    #[test]
    fn isometric_zero_z_is_identity() {
        let pos = Point2D::new(100.0, 200.0);
        let mode = ProjectionMode::Isometric {
            z_source: ZSource::Zero,
        };
        let result = super::project_position(pos, 0.0, &mode, &cfg());
        assert_eq!(result.x, 100.0);
        assert_eq!(result.y, 200.0);
        assert_eq!(result.depth_scale, 1.0);
    }

    #[test]
    fn isometric_positive_z_shifts_diagonal() {
        let pos = Point2D::new(100.0, 200.0);
        let mode = ProjectionMode::Isometric {
            z_source: ZSource::BfsDepth { scale: 1.0 },
        };
        let result = super::project_position(pos, 10.0, &mode, &cfg());
        // X shifts right, Y shifts up.
        assert!(result.x > 100.0);
        assert!(result.y < 200.0);
        assert_eq!(result.depth_scale, 1.0); // No perspective in isometric.
    }

    #[test]
    fn projection_determinism() {
        // Same inputs must always produce the same outputs.
        let pos = Point2D::new(73.5, -42.1);
        let z = 25.0;
        let modes = [
            ProjectionMode::TwoD,
            ProjectionMode::TwoPointFive {
                z_source: ZSource::Recency { max_depth: 100.0 },
            },
            ProjectionMode::Isometric {
                z_source: ZSource::BfsDepth { scale: 2.0 },
            },
        ];
        for mode in &modes {
            let a = super::project_position(pos, z, mode, &cfg());
            let b = super::project_position(pos, z, mode, &cfg());
            assert_eq!(a.x, b.x);
            assert_eq!(a.y, b.y);
            assert_eq!(a.depth, b.depth);
            assert_eq!(a.depth_scale, b.depth_scale);
        }
    }

    #[test]
    fn serde_roundtrip_projection_mode() {
        let modes = [
            ProjectionMode::TwoD,
            ProjectionMode::TwoPointFive {
                z_source: ZSource::UdcLevel { scale: 1.0 },
            },
            ProjectionMode::Isometric {
                z_source: ZSource::Manual,
            },
            ProjectionMode::Standard,
        ];
        for mode in &modes {
            let json = serde_json::to_string(mode).unwrap();
            let back: ProjectionMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, &back);
        }
    }
}
