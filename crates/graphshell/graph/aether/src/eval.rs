/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pure-Rust evaluator for field expressions.
//!
//! Walks the AST recursively. Closed forms are used for known kernels
//! (Gaussian, Linear, Disk, analytic gradient where available); arbitrary
//! compositions fall through to a finite-difference gradient.
//!
//! This is the default backend. The future Burn-wgpu backend (gated by the
//! `field-burn` feature) will lower an entire expression to a fused tensor
//! program for vectorised evaluation across many points; see `lower_burn`.

use crate::ast::{Falloff, ScalarField, VectorField};
use crate::registry::{FieldDef, FieldRegistry};

/// Evaluate a scalar field at `(x, y, t)`.
///
/// Returns `0.0` for `Sample`s that point at a missing or wrong-typed entry
/// in the registry (warnings should be surfaced by the host's diagnostics
/// layer, not panics here).
pub fn eval_scalar(field: &ScalarField, registry: &FieldRegistry, x: f32, y: f32, t: f32) -> f32 {
    match field {
        ScalarField::Const(c) => *c,
        ScalarField::CoordX => x,
        ScalarField::CoordY => y,
        ScalarField::Time => t,
        ScalarField::Gaussian { center, sigma } => {
            let (cx, cy) = eval_vector(center, registry, x, y, t);
            let dx = x - cx;
            let dy = y - cy;
            let s2 = sigma * sigma;
            if s2 <= f32::EPSILON {
                return 0.0;
            }
            (-(dx * dx + dy * dy) / (2.0 * s2)).exp()
        }
        ScalarField::Disk {
            center,
            radius,
            falloff,
        } => {
            let (cx, cy) = eval_vector(center, registry, x, y, t);
            let dx = x - cx;
            let dy = y - cy;
            let d = (dx * dx + dy * dy).sqrt();
            if *radius <= 0.0 || d > *radius {
                return 0.0;
            }
            let u = (d / *radius).clamp(0.0, 1.0);
            apply_falloff(*falloff, u)
        }
        ScalarField::Linear { normal, offset } => {
            let (nx, ny) = eval_vector(normal, registry, x, y, t);
            nx * x + ny * y + offset
        }
        ScalarField::Add(a, b) => {
            eval_scalar(a, registry, x, y, t) + eval_scalar(b, registry, x, y, t)
        }
        ScalarField::Mul(a, b) => {
            eval_scalar(a, registry, x, y, t) * eval_scalar(b, registry, x, y, t)
        }
        ScalarField::Scale(a, k) => eval_scalar(a, registry, x, y, t) * k,
        ScalarField::Negate(a) => -eval_scalar(a, registry, x, y, t),
        ScalarField::Dot(a, b) => {
            let (ax, ay) = eval_vector(a, registry, x, y, t);
            let (bx, by) = eval_vector(b, registry, x, y, t);
            ax * bx + ay * by
        }
        ScalarField::Sample(id) => match registry.get(*id) {
            Some(FieldDef::Scalar(f)) => eval_scalar(f, registry, x, y, t),
            _ => 0.0,
        },
    }
}

/// Evaluate a vector field at `(x, y, t)`.
pub fn eval_vector(
    field: &VectorField,
    registry: &FieldRegistry,
    x: f32,
    y: f32,
    t: f32,
) -> (f32, f32) {
    match field {
        VectorField::ConstVec { x: vx, y: vy } => (*vx, *vy),
        VectorField::Coord => (x, y),
        VectorField::Gradient(s) => grad_scalar(s, registry, x, y, t),
        VectorField::Perp(v) => {
            let (vx, vy) = eval_vector(v, registry, x, y, t);
            (-vy, vx)
        }
        VectorField::Add(a, b) => {
            let (ax, ay) = eval_vector(a, registry, x, y, t);
            let (bx, by) = eval_vector(b, registry, x, y, t);
            (ax + bx, ay + by)
        }
        VectorField::Scale(v, s) => {
            let (vx, vy) = eval_vector(v, registry, x, y, t);
            let scale = eval_scalar(s, registry, x, y, t);
            (vx * scale, vy * scale)
        }
        VectorField::ScaleConst(v, k) => {
            let (vx, vy) = eval_vector(v, registry, x, y, t);
            (vx * k, vy * k)
        }
        VectorField::Sample(id) => match registry.get(*id) {
            Some(FieldDef::Vector(f)) => eval_vector(f, registry, x, y, t),
            _ => (0.0, 0.0),
        },
    }
}

/// Gradient of a scalar field at `(x, y, t)`. Uses analytic forms where
/// known and falls back to central finite differences otherwise.
pub fn grad_scalar(
    field: &ScalarField,
    registry: &FieldRegistry,
    x: f32,
    y: f32,
    t: f32,
) -> (f32, f32) {
    match field {
        ScalarField::Const(_) => (0.0, 0.0),
        ScalarField::CoordX => (1.0, 0.0),
        ScalarField::CoordY => (0.0, 1.0),
        ScalarField::Time => (0.0, 0.0),
        ScalarField::Gaussian { center, sigma } => {
            let (cx, cy) = eval_vector(center, registry, x, y, t);
            let s2 = sigma * sigma;
            if s2 <= f32::EPSILON {
                return (0.0, 0.0);
            }
            let g = eval_scalar(field, registry, x, y, t);
            (-(x - cx) / s2 * g, -(y - cy) / s2 * g)
        }
        ScalarField::Linear { normal, .. } => eval_vector(normal, registry, x, y, t),
        ScalarField::Add(a, b) => {
            let (ax, ay) = grad_scalar(a, registry, x, y, t);
            let (bx, by) = grad_scalar(b, registry, x, y, t);
            (ax + bx, ay + by)
        }
        ScalarField::Scale(a, k) => {
            let (ax, ay) = grad_scalar(a, registry, x, y, t);
            (ax * k, ay * k)
        }
        ScalarField::Negate(a) => {
            let (ax, ay) = grad_scalar(a, registry, x, y, t);
            (-ax, -ay)
        }
        // Mul, Dot, Disk, Sample fall back to finite differences.
        _ => grad_finite_diff(field, registry, x, y, t),
    }
}

fn grad_finite_diff(
    field: &ScalarField,
    registry: &FieldRegistry,
    x: f32,
    y: f32,
    t: f32,
) -> (f32, f32) {
    const H: f32 = 1.0e-3;
    let dx = (eval_scalar(field, registry, x + H, y, t)
        - eval_scalar(field, registry, x - H, y, t))
        / (2.0 * H);
    let dy = (eval_scalar(field, registry, x, y + H, t)
        - eval_scalar(field, registry, x, y - H, t))
        / (2.0 * H);
    (dx, dy)
}

fn apply_falloff(falloff: Falloff, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match falloff {
        Falloff::Hard => 1.0,
        Falloff::Linear => 1.0 - t,
        Falloff::Smoothstep => {
            let u = 1.0 - t;
            (3.0 - 2.0 * u) * u * u
        }
        Falloff::Quadratic => (1.0 - t) * (1.0 - t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn const_evaluates_constant() {
        let reg = FieldRegistry::new();
        let f = ScalarField::Const(3.5);
        assert_eq!(eval_scalar(&f, &reg, 100.0, 200.0, 0.0), 3.5);
    }

    #[test]
    fn coord_returns_position() {
        let reg = FieldRegistry::new();
        assert_eq!(eval_scalar(&ScalarField::CoordX, &reg, 7.0, 11.0, 0.0), 7.0);
        assert_eq!(
            eval_scalar(&ScalarField::CoordY, &reg, 7.0, 11.0, 0.0),
            11.0
        );
    }

    #[test]
    fn time_returns_t() {
        let reg = FieldRegistry::new();
        assert_eq!(eval_scalar(&ScalarField::Time, &reg, 0.0, 0.0, 1.5), 1.5);
    }

    #[test]
    fn gaussian_peak_is_one_at_center() {
        let reg = FieldRegistry::new();
        let f = ScalarField::gaussian_at(50.0, 50.0, 10.0);
        let v = eval_scalar(&f, &reg, 50.0, 50.0, 0.0);
        assert!(approx(v, 1.0, 1.0e-6));
    }

    #[test]
    fn gaussian_decays_with_distance() {
        let reg = FieldRegistry::new();
        let f = ScalarField::gaussian_at(0.0, 0.0, 10.0);
        let near = eval_scalar(&f, &reg, 5.0, 0.0, 0.0);
        let far = eval_scalar(&f, &reg, 50.0, 0.0, 0.0);
        assert!(near > far);
        assert!(near > 0.0);
    }

    #[test]
    fn disk_is_zero_outside_radius() {
        let reg = FieldRegistry::new();
        let f = ScalarField::disk_at(0.0, 0.0, 10.0, Falloff::Linear);
        assert_eq!(eval_scalar(&f, &reg, 11.0, 0.0, 0.0), 0.0);
        assert_eq!(eval_scalar(&f, &reg, -11.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn disk_hard_is_one_inside() {
        let reg = FieldRegistry::new();
        let f = ScalarField::disk_at(0.0, 0.0, 10.0, Falloff::Hard);
        assert_eq!(eval_scalar(&f, &reg, 5.0, 0.0, 0.0), 1.0);
        assert_eq!(eval_scalar(&f, &reg, 0.0, 0.0, 0.0), 1.0);
    }

    #[test]
    fn disk_linear_falloff() {
        let reg = FieldRegistry::new();
        let f = ScalarField::disk_at(0.0, 0.0, 10.0, Falloff::Linear);
        let v = eval_scalar(&f, &reg, 5.0, 0.0, 0.0);
        assert!(approx(v, 0.5, 1.0e-6));
    }

    #[test]
    fn add_and_mul_compose() {
        let reg = FieldRegistry::new();
        let f = ScalarField::Add(
            Box::new(ScalarField::Const(2.0)),
            Box::new(ScalarField::Mul(
                Box::new(ScalarField::CoordX),
                Box::new(ScalarField::Const(3.0)),
            )),
        );
        // 2 + x * 3 at x=4 -> 14
        assert_eq!(eval_scalar(&f, &reg, 4.0, 0.0, 0.0), 14.0);
    }

    #[test]
    fn vector_coord_returns_position() {
        let reg = FieldRegistry::new();
        assert_eq!(
            eval_vector(&VectorField::Coord, &reg, 3.0, -7.0, 0.0),
            (3.0, -7.0)
        );
    }

    #[test]
    fn vector_perp_rotates_90() {
        let reg = FieldRegistry::new();
        let f = VectorField::Perp(Box::new(VectorField::ConstVec { x: 1.0, y: 0.0 }));
        assert_eq!(eval_vector(&f, &reg, 0.0, 0.0, 0.0), (0.0, 1.0));
    }

    #[test]
    fn analytic_gradient_of_coord_x() {
        let reg = FieldRegistry::new();
        let g = grad_scalar(&ScalarField::CoordX, &reg, 5.0, 7.0, 0.0);
        assert_eq!(g, (1.0, 0.0));
    }

    #[test]
    fn analytic_gradient_of_linear() {
        let reg = FieldRegistry::new();
        let f = ScalarField::linear(2.0, -3.0, 0.0);
        let g = grad_scalar(&f, &reg, 100.0, 100.0, 0.0);
        assert_eq!(g, (2.0, -3.0));
    }

    #[test]
    fn analytic_gradient_of_gaussian_points_inward() {
        let reg = FieldRegistry::new();
        let f = ScalarField::gaussian_at(0.0, 0.0, 10.0);
        // At x>0, gradient should point in -x direction (toward the peak at 0).
        let (gx, gy) = grad_scalar(&f, &reg, 5.0, 0.0, 0.0);
        assert!(gx < 0.0);
        assert!(approx(gy, 0.0, 1.0e-6));
    }

    #[test]
    fn analytic_gradient_of_add_is_sum() {
        let reg = FieldRegistry::new();
        let f = ScalarField::Add(
            Box::new(ScalarField::linear(1.0, 0.0, 0.0)),
            Box::new(ScalarField::linear(0.0, 2.0, 0.0)),
        );
        let g = grad_scalar(&f, &reg, 0.0, 0.0, 0.0);
        assert_eq!(g, (1.0, 2.0));
    }

    #[test]
    fn finite_diff_gradient_for_unknown_form() {
        // Mul(CoordX, CoordY) = x*y; gradient = (y, x)
        let reg = FieldRegistry::new();
        let f = ScalarField::Mul(Box::new(ScalarField::CoordX), Box::new(ScalarField::CoordY));
        let (gx, gy) = grad_scalar(&f, &reg, 3.0, 5.0, 0.0);
        assert!(approx(gx, 5.0, 1.0e-2));
        assert!(approx(gy, 3.0, 1.0e-2));
    }

    #[test]
    fn sample_resolves_through_registry() {
        let mut reg = FieldRegistry::new();
        let id = reg.insert_scalar("base", ScalarField::Const(7.0));
        let f = ScalarField::Add(
            Box::new(ScalarField::Sample(id)),
            Box::new(ScalarField::Const(1.0)),
        );
        assert_eq!(eval_scalar(&f, &reg, 0.0, 0.0, 0.0), 8.0);
    }

    #[test]
    fn sample_returns_zero_for_missing() {
        let reg = FieldRegistry::new();
        let f = ScalarField::Sample(crate::registry::FieldId::from_uuid(uuid::Uuid::from_u128(99)));
        assert_eq!(eval_scalar(&f, &reg, 0.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn vector_sample_resolves() {
        let mut reg = FieldRegistry::new();
        let id = reg.insert_vector("flow", VectorField::ConstVec { x: 1.0, y: 2.0 });
        let f = VectorField::Sample(id);
        assert_eq!(eval_vector(&f, &reg, 0.0, 0.0, 0.0), (1.0, 2.0));
    }

    #[test]
    fn dot_product_is_correct() {
        let reg = FieldRegistry::new();
        let f = ScalarField::Dot(
            Box::new(VectorField::ConstVec { x: 1.0, y: 2.0 }),
            Box::new(VectorField::ConstVec { x: 3.0, y: 4.0 }),
        );
        // 1*3 + 2*4 = 11
        assert_eq!(eval_scalar(&f, &reg, 0.0, 0.0, 0.0), 11.0);
    }

    #[test]
    fn negate_flips_sign() {
        let reg = FieldRegistry::new();
        let f = ScalarField::Negate(Box::new(ScalarField::Const(5.0)));
        assert_eq!(eval_scalar(&f, &reg, 0.0, 0.0, 0.0), -5.0);
    }
}
