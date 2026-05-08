/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Projection math: lifts `(world_pos, z, view)` into a [`ProjectedPosition`]
//! consumed by packet derivation.
//!
//! All projections preserve the canonical `(x, y)` graph layout truth — they
//! only add depth cues, screen-space offsets, and (for perspective) scale.
//! Callers are responsible for deriving `z` from the active z-field (see
//! [`crate::fields::FieldProjection::z_field`]) before calling
//! [`project_position`]. `TwoD` ignores `z`.

use euclid::default::Point2D;

use crate::projection::presets::TwoPointFiveProjection;
use crate::projection::types::{ProjectionConfig, ViewDimension};

/// Minimum depth scale for the `MildPerspective` 2.5D projection.
/// Nodes never shrink below this fraction of their world size.
const MILD_PERSPECTIVE_MIN_SCALE: f32 = 0.4;
/// Y-shift coefficient per z unit for the `MildPerspective` projection.
const MILD_PERSPECTIVE_Y_SHIFT: f32 = 0.15;

/// A projected position: the screen-space result of projecting a 2D graph
/// position through a view dimension, given an optional z value.
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

/// Project a 2D graph position into screen space through the given view
/// dimension.
///
/// - `world_pos`: canonical (x, y) in graph layout space
/// - `z`: derived z value for this node (0.0 if `TwoD`, or no z-field set)
/// - `view`: the active view dimension
/// - `_config`: reserved for future global projection settings; currently unused
pub fn project_position(
    world_pos: Point2D<f32>,
    z: f32,
    view: &ViewDimension,
    _config: &ProjectionConfig,
) -> ProjectedPosition {
    match view {
        ViewDimension::TwoD { .. } | ViewDimension::ThreeD => identity_projection(world_pos),
        ViewDimension::TwoPointFive { projection, .. } => match projection {
            TwoPointFiveProjection::Isometric { dimetric_angle } => {
                axonometric_project(world_pos, z, *dimetric_angle, 1.0)
            }
            TwoPointFiveProjection::Cabinet { z_scale, angle_deg } => {
                axonometric_project(world_pos, z, *angle_deg, *z_scale)
            }
            TwoPointFiveProjection::Cavalier { angle_deg } => {
                axonometric_project(world_pos, z, *angle_deg, 1.0)
            }
            TwoPointFiveProjection::Tilted { tilt_deg } => tilted_project(world_pos, z, *tilt_deg),
            TwoPointFiveProjection::MildPerspective { focal } => {
                mild_perspective_project(world_pos, z, *focal)
            }
        },
    }
}

fn identity_projection(world_pos: Point2D<f32>) -> ProjectedPosition {
    ProjectedPosition {
        x: world_pos.x,
        y: world_pos.y,
        depth: 0.0,
        depth_scale: 1.0,
    }
}

/// Axonometric projection (covers Isometric, Cabinet, Cavalier).
/// `angle_deg` is the angle the depth axis makes with the screen-x axis.
/// `z_scale` shrinks the depth axis (1.0 for cavalier/isometric, 0.5 for cabinet).
fn axonometric_project(
    world_pos: Point2D<f32>,
    z: f32,
    angle_deg: f32,
    z_scale: f32,
) -> ProjectedPosition {
    let angle = angle_deg.to_radians();
    let dx = z * z_scale * angle.cos();
    let dy = -z * z_scale * angle.sin();
    ProjectedPosition {
        x: world_pos.x + dx,
        y: world_pos.y + dy,
        depth: z,
        depth_scale: 1.0,
    }
}

fn tilted_project(world_pos: Point2D<f32>, z: f32, tilt_deg: f32) -> ProjectedPosition {
    let tilt = tilt_deg.to_radians();
    ProjectedPosition {
        x: world_pos.x,
        y: world_pos.y + z * tilt.sin(),
        depth: z,
        depth_scale: 1.0,
    }
}

fn mild_perspective_project(world_pos: Point2D<f32>, z: f32, focal: f32) -> ProjectedPosition {
    let focal = focal.max(1.0);
    let raw = focal / (focal + z.abs());
    let depth_factor = raw.max(MILD_PERSPECTIVE_MIN_SCALE);
    ProjectedPosition {
        x: world_pos.x * depth_factor,
        y: world_pos.y * depth_factor + z * MILD_PERSPECTIVE_Y_SHIFT,
        depth: z,
        depth_scale: depth_factor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::presets::{TwoDPreset, TwoPointFiveProjection};

    fn cfg() -> ProjectionConfig {
        ProjectionConfig::default()
    }

    #[test]
    fn twod_is_identity() {
        let pos = Point2D::new(100.0, 200.0);
        let r = project_position(
            pos,
            0.0,
            &ViewDimension::TwoD {
                preset: TwoDPreset::Plain,
            },
            &cfg(),
        );
        assert_eq!(r.x, 100.0);
        assert_eq!(r.y, 200.0);
        assert_eq!(r.depth, 0.0);
        assert_eq!(r.depth_scale, 1.0);
    }

    #[test]
    fn twod_ignores_z() {
        let pos = Point2D::new(50.0, 50.0);
        let r = project_position(
            pos,
            999.0,
            &ViewDimension::TwoD {
                preset: TwoDPreset::Paper,
            },
            &cfg(),
        );
        assert_eq!(r.x, 50.0);
        assert_eq!(r.y, 50.0);
        assert_eq!(r.depth, 0.0);
    }

    fn twopointfive(projection: TwoPointFiveProjection) -> ViewDimension {
        ViewDimension::TwoPointFive {
            projection,
            z_field: None,
        }
    }

    #[test]
    fn isometric_zero_z_is_identity() {
        let pos = Point2D::new(100.0, 200.0);
        let r = project_position(
            pos,
            0.0,
            &twopointfive(TwoPointFiveProjection::isometric_default()),
            &cfg(),
        );
        assert_eq!(r.x, 100.0);
        assert_eq!(r.y, 200.0);
        assert_eq!(r.depth_scale, 1.0);
    }

    #[test]
    fn isometric_positive_z_shifts_diagonal() {
        let pos = Point2D::new(100.0, 200.0);
        let r = project_position(
            pos,
            10.0,
            &twopointfive(TwoPointFiveProjection::isometric_default()),
            &cfg(),
        );
        assert!(r.x > 100.0); // 30° puts cos(30) > 0
        assert!(r.y < 200.0); // -sin(30) < 0
        assert_eq!(r.depth_scale, 1.0);
    }

    #[test]
    fn cabinet_half_depth_shrinks_more_than_cavalier() {
        let pos = Point2D::new(0.0, 0.0);
        let cab = project_position(
            pos,
            10.0,
            &twopointfive(TwoPointFiveProjection::cabinet_default()),
            &cfg(),
        );
        let cav = project_position(
            pos,
            10.0,
            &twopointfive(TwoPointFiveProjection::cavalier_default()),
            &cfg(),
        );
        assert!(cab.x.abs() < cav.x.abs());
    }

    #[test]
    fn tilted_only_shifts_y() {
        let pos = Point2D::new(100.0, 200.0);
        let r = project_position(
            pos,
            10.0,
            &twopointfive(TwoPointFiveProjection::Tilted { tilt_deg: 45.0 }),
            &cfg(),
        );
        assert_eq!(r.x, 100.0);
        assert!(r.y > 200.0);
    }

    #[test]
    fn mild_perspective_shrinks_with_depth() {
        let pos = Point2D::new(100.0, 200.0);
        let r0 = project_position(
            pos,
            0.0,
            &twopointfive(TwoPointFiveProjection::MildPerspective { focal: 333.0 }),
            &cfg(),
        );
        let r1 = project_position(
            pos,
            50.0,
            &twopointfive(TwoPointFiveProjection::MildPerspective { focal: 333.0 }),
            &cfg(),
        );
        assert_eq!(r0.depth_scale, 1.0);
        assert!(r1.depth_scale < 1.0);
        assert!(r1.depth_scale >= MILD_PERSPECTIVE_MIN_SCALE);
    }

    #[test]
    fn mild_perspective_scale_bounded_below() {
        let pos = Point2D::new(100.0, 200.0);
        let r = project_position(
            pos,
            10000.0,
            &twopointfive(TwoPointFiveProjection::MildPerspective { focal: 333.0 }),
            &cfg(),
        );
        assert!((r.depth_scale - MILD_PERSPECTIVE_MIN_SCALE).abs() < 0.01);
    }

    #[test]
    fn projection_determinism() {
        let pos = Point2D::new(73.5, -42.1);
        let z = 25.0;
        let views = [
            ViewDimension::TwoD {
                preset: TwoDPreset::Plain,
            },
            twopointfive(TwoPointFiveProjection::isometric_default()),
            twopointfive(TwoPointFiveProjection::cabinet_default()),
            twopointfive(TwoPointFiveProjection::cavalier_default()),
            twopointfive(TwoPointFiveProjection::Tilted { tilt_deg: 30.0 }),
            twopointfive(TwoPointFiveProjection::MildPerspective { focal: 333.0 }),
        ];
        for v in &views {
            let a = project_position(pos, z, v, &cfg());
            let b = project_position(pos, z, v, &cfg());
            assert_eq!(a.x, b.x);
            assert_eq!(a.y, b.y);
            assert_eq!(a.depth, b.depth);
            assert_eq!(a.depth_scale, b.depth_scale);
        }
    }

    #[test]
    fn xy_preserved_through_dimension_changes() {
        // Lossless ladder: (x, y) is never lost when the view dimension
        // changes (z is ephemeral).
        let pos = Point2D::new(123.4, -56.7);
        let twod = project_position(
            pos,
            0.0,
            &ViewDimension::TwoD {
                preset: TwoDPreset::Plain,
            },
            &cfg(),
        );
        let twopointfive = project_position(
            pos,
            0.0,
            &twopointfive(TwoPointFiveProjection::isometric_default()),
            &cfg(),
        );
        let back = project_position(
            pos,
            0.0,
            &ViewDimension::TwoD {
                preset: TwoDPreset::Paper,
            },
            &cfg(),
        );
        assert_eq!(twod.x, back.x);
        assert_eq!(twod.y, back.y);
        // At z=0, axonometric projections reduce to identity.
        assert_eq!(twopointfive.x, twod.x);
        assert_eq!(twopointfive.y, twod.y);
    }
}
