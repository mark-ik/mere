/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Burn-backed lowering of the field algebra.
//!
//! Walks a [`ScalarField`] / [`VectorField`] AST and emits a Burn tensor
//! program evaluated at a batch of positions, returning the per-position
//! values as rank-1 tensors. The default backend wired in by the
//! `field-burn` feature is `burn::backend::NdArray` (CPU); a wgpu backend
//! is a follow-up cycle.
//!
//! ## Inputs
//!
//! Callers split positions into two rank-1 tensors `xs` and `ys` of shape
//! `[N]`. Splitting at the boundary keeps the lowering free of rank-2
//! reshape/slice plumbing.
//!
//! ## Operator coverage
//!
//! Currently lowered:
//!
//! - **Scalar**: `Const`, `CoordX`, `CoordY`, `Time`, `Add`, `Mul`, `Scale`,
//!   `Negate`, `Gaussian` (const center), `Linear` (const normal), `Disk`
//!   (const center, all falloffs), `Dot`, `Sample` (transitive).
//! - **Vector**: `ConstVec`, `Coord`, `Add`, `Scale`, `ScaleConst`, `Perp`,
//!   `Sample` (transitive), `Gradient` of `Const`, `Coord{X,Y}`, `Time`,
//!   `Linear`, `Gaussian`, `Add`, `Scale`, `Negate`, `Mul` (product rule),
//!   `Sample` via analytic closed forms.
//!
//! Unsupported variants (`Disk` gradient — piecewise; same for any future
//! AST node without a closed form) return
//! [`LowerError::UnsupportedOperator`]. Unknown field-id samples return
//! [`LowerError::UnknownField`]; samples crossing scalar↔vector return
//! [`LowerError::SampleTypeMismatch`].

use crate::ast::{Falloff, ScalarField, VectorField};
use crate::registry::{FieldDef, FieldId, FieldRegistry};

#[cfg(feature = "field-burn")]
use burn::tensor::{Tensor, backend::Backend};

/// Reasons a field expression cannot be lowered to Burn.
#[derive(Debug, Clone, PartialEq)]
pub enum LowerError {
    /// AST variant not yet supported by the Burn backend.
    UnsupportedOperator(&'static str),
    /// `Sample(id)` referenced a field id that is not in the registry.
    UnknownField(FieldId),
    /// `Sample(id)` resolved to the wrong-shaped field (scalar vs vector).
    SampleTypeMismatch { wanted_scalar: bool },
}

// ── Scalar lowering ─────────────────────────────────────────────────────────

/// Lower a scalar field to a Burn tensor program evaluated at `(xs, ys, t)`.
///
/// `xs` and `ys` are rank-1 tensors of shape `[N]`. The result is a rank-1
/// tensor of shape `[N]`.
#[cfg(feature = "field-burn")]
pub fn lower_scalar<B: Backend>(
    field: &ScalarField,
    registry: &FieldRegistry,
    xs: Tensor<B, 1>,
    ys: Tensor<B, 1>,
    time: f32,
) -> Result<Tensor<B, 1>, LowerError> {
    match field {
        ScalarField::Const(c) => Ok(xs.zeros_like().add_scalar(*c)),
        ScalarField::CoordX => Ok(xs),
        ScalarField::CoordY => Ok(ys),
        ScalarField::Time => Ok(xs.zeros_like().add_scalar(time)),
        ScalarField::Add(a, b) => {
            let ta = lower_scalar(a, registry, xs.clone(), ys.clone(), time)?;
            let tb = lower_scalar(b, registry, xs, ys, time)?;
            Ok(ta + tb)
        }
        ScalarField::Mul(a, b) => {
            let ta = lower_scalar(a, registry, xs.clone(), ys.clone(), time)?;
            let tb = lower_scalar(b, registry, xs, ys, time)?;
            Ok(ta * tb)
        }
        ScalarField::Scale(a, k) => {
            let ta = lower_scalar(a, registry, xs, ys, time)?;
            Ok(ta.mul_scalar(*k))
        }
        ScalarField::Negate(a) => {
            let ta = lower_scalar(a, registry, xs, ys, time)?;
            Ok(ta.neg())
        }
        ScalarField::Gaussian { center, sigma } => {
            let (cx, cy) = expect_const_vec(center, "Gaussian center must be ConstVec")?;
            let s2 = sigma * sigma;
            if s2 <= f32::EPSILON {
                return Ok(xs.zeros_like());
            }
            let dx = xs.sub_scalar(cx);
            let dy = ys.sub_scalar(cy);
            let dist_sq = dx.clone() * dx + dy.clone() * dy;
            // exp(-dist_sq / (2 σ²)) = exp(dist_sq / -2σ²)
            Ok(dist_sq.div_scalar(-2.0 * s2).exp())
        }
        ScalarField::Linear { normal, offset } => {
            let (nx, ny) = expect_const_vec(normal, "Linear normal must be ConstVec")?;
            let result = xs.mul_scalar(nx) + ys.mul_scalar(ny);
            Ok(result.add_scalar(*offset))
        }
        ScalarField::Sample(id) => match registry.get(*id) {
            Some(FieldDef::Scalar(f)) => lower_scalar(f, registry, xs, ys, time),
            Some(FieldDef::Vector(_)) => Err(LowerError::SampleTypeMismatch {
                wanted_scalar: true,
            }),
            None => Err(LowerError::UnknownField(*id)),
        },
        ScalarField::Disk {
            center,
            radius,
            falloff,
        } => {
            let (cx, cy) = expect_const_vec(center, "Disk center must be ConstVec")?;
            if *radius <= 0.0 {
                return Ok(xs.zeros_like());
            }
            let dx = xs.sub_scalar(cx);
            let dy = ys.sub_scalar(cy);
            let dist_sq = dx.clone() * dx + dy.clone() * dy;
            let d = dist_sq.sqrt();
            let u = d.clone().div_scalar(*radius).clamp(0.0, 1.0);
            let f_val = apply_falloff_tensor(u, *falloff);
            // Mask zero outside the radius.
            let outside = d.greater_elem(*radius);
            Ok(f_val.mask_fill(outside, 0.0))
        }
        ScalarField::Dot(a, b) => {
            let (ax, ay) = lower_vector(a, registry, xs.clone(), ys.clone(), time)?;
            let (bx, by) = lower_vector(b, registry, xs, ys, time)?;
            Ok(ax * bx + ay * by)
        }
    }
}

#[cfg(feature = "field-burn")]
fn apply_falloff_tensor<B: Backend>(u: Tensor<B, 1>, falloff: Falloff) -> Tensor<B, 1> {
    match falloff {
        Falloff::Hard => u.zeros_like().add_scalar(1.0),
        Falloff::Linear => u.neg().add_scalar(1.0),
        Falloff::Smoothstep => {
            let v = u.neg().add_scalar(1.0);
            let v2 = v.clone() * v.clone();
            let factor = v.mul_scalar(-2.0).add_scalar(3.0);
            factor * v2
        }
        Falloff::Quadratic => {
            let v = u.neg().add_scalar(1.0);
            v.clone() * v
        }
    }
}

// ── Vector lowering ─────────────────────────────────────────────────────────

/// Lower a vector field to a pair of Burn tensor programs `(vx, vy)`
/// evaluated at `(xs, ys, t)`.
#[cfg(feature = "field-burn")]
pub fn lower_vector<B: Backend>(
    field: &VectorField,
    registry: &FieldRegistry,
    xs: Tensor<B, 1>,
    ys: Tensor<B, 1>,
    time: f32,
) -> Result<(Tensor<B, 1>, Tensor<B, 1>), LowerError> {
    match field {
        VectorField::ConstVec { x, y } => Ok((
            xs.zeros_like().add_scalar(*x),
            ys.zeros_like().add_scalar(*y),
        )),
        VectorField::Coord => Ok((xs, ys)),
        VectorField::Perp(v) => {
            // Perp((x, y)) = (-y, x)
            let (vx, vy) = lower_vector(v, registry, xs, ys, time)?;
            Ok((vy.neg(), vx))
        }
        VectorField::Add(a, b) => {
            let (ax, ay) = lower_vector(a, registry, xs.clone(), ys.clone(), time)?;
            let (bx, by) = lower_vector(b, registry, xs, ys, time)?;
            Ok((ax + bx, ay + by))
        }
        VectorField::ScaleConst(v, k) => {
            let (vx, vy) = lower_vector(v, registry, xs, ys, time)?;
            Ok((vx.mul_scalar(*k), vy.mul_scalar(*k)))
        }
        VectorField::Scale(v, s) => {
            let (vx, vy) = lower_vector(v, registry, xs.clone(), ys.clone(), time)?;
            let scalar = lower_scalar(s, registry, xs, ys, time)?;
            Ok((vx * scalar.clone(), vy * scalar))
        }
        VectorField::Gradient(scalar) => lower_gradient(scalar, registry, xs, ys, time),
        VectorField::Sample(id) => match registry.get(*id) {
            Some(FieldDef::Vector(f)) => lower_vector(f, registry, xs, ys, time),
            Some(FieldDef::Scalar(_)) => Err(LowerError::SampleTypeMismatch {
                wanted_scalar: false,
            }),
            None => Err(LowerError::UnknownField(*id)),
        },
    }
}

// ── Gradient closed forms ───────────────────────────────────────────────────

/// Closed-form gradient of selected scalar fields. Returns
/// [`LowerError::UnsupportedOperator`] for variants without a known analytic
/// gradient (the host can fall back to the CPU `grad_scalar` evaluator on
/// the same expression).
#[cfg(feature = "field-burn")]
fn lower_gradient<B: Backend>(
    scalar: &ScalarField,
    registry: &FieldRegistry,
    xs: Tensor<B, 1>,
    ys: Tensor<B, 1>,
    time: f32,
) -> Result<(Tensor<B, 1>, Tensor<B, 1>), LowerError> {
    match scalar {
        ScalarField::Const(_) | ScalarField::Time => Ok((xs.zeros_like(), ys.zeros_like())),
        ScalarField::CoordX => Ok((xs.zeros_like().add_scalar(1.0), ys.zeros_like())),
        ScalarField::CoordY => Ok((xs.zeros_like(), ys.zeros_like().add_scalar(1.0))),
        ScalarField::Linear { normal, .. } => {
            let (nx, ny) = expect_const_vec(normal, "Linear normal must be ConstVec")?;
            Ok((
                xs.zeros_like().add_scalar(nx),
                ys.zeros_like().add_scalar(ny),
            ))
        }
        ScalarField::Add(a, b) => {
            let (ax, ay) = lower_gradient(a, registry, xs.clone(), ys.clone(), time)?;
            let (bx, by) = lower_gradient(b, registry, xs, ys, time)?;
            Ok((ax + bx, ay + by))
        }
        ScalarField::Scale(a, k) => {
            let (ax, ay) = lower_gradient(a, registry, xs, ys, time)?;
            Ok((ax.mul_scalar(*k), ay.mul_scalar(*k)))
        }
        ScalarField::Negate(a) => {
            let (ax, ay) = lower_gradient(a, registry, xs, ys, time)?;
            Ok((ax.neg(), ay.neg()))
        }
        ScalarField::Gaussian { center, sigma } => {
            // ∇gaussian = -(p - c)/σ² · gaussian(p)
            let (cx, cy) = expect_const_vec(center, "Gaussian center must be ConstVec")?;
            let s2 = sigma * sigma;
            if s2 <= f32::EPSILON {
                return Ok((xs.zeros_like(), ys.zeros_like()));
            }
            let dx = xs.clone().sub_scalar(cx);
            let dy = ys.clone().sub_scalar(cy);
            let dist_sq = dx.clone() * dx.clone() + dy.clone() * dy.clone();
            let g = dist_sq.div_scalar(-2.0 * s2).exp();
            let factor = g.div_scalar(-s2);
            Ok((dx * factor.clone(), dy * factor))
        }
        ScalarField::Sample(id) => match registry.get(*id) {
            Some(FieldDef::Scalar(f)) => lower_gradient(f, registry, xs, ys, time),
            Some(FieldDef::Vector(_)) => Err(LowerError::SampleTypeMismatch {
                wanted_scalar: true,
            }),
            None => Err(LowerError::UnknownField(*id)),
        },
        ScalarField::Mul(a, b) => {
            // Product rule: ∇(a·b) = a·∇b + b·∇a
            let av = lower_scalar(a, registry, xs.clone(), ys.clone(), time)?;
            let bv = lower_scalar(b, registry, xs.clone(), ys.clone(), time)?;
            let (agx, agy) = lower_gradient(a, registry, xs.clone(), ys.clone(), time)?;
            let (bgx, bgy) = lower_gradient(b, registry, xs, ys, time)?;
            Ok((av.clone() * bgx + bv.clone() * agx, av * bgy + bv * agy))
        }
        ScalarField::Dot(_, _) => Err(LowerError::UnsupportedOperator("Gradient(Dot)")),
        ScalarField::Disk { .. } => Err(LowerError::UnsupportedOperator("Gradient(Disk)")),
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn expect_const_vec(v: &VectorField, msg: &'static str) -> Result<(f32, f32), LowerError> {
    match v {
        VectorField::ConstVec { x, y } => Ok((*x, *y)),
        _ => Err(LowerError::UnsupportedOperator(msg)),
    }
}

#[cfg(feature = "field-burn")]

#[cfg(test)]
mod tests;
