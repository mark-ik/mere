/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable field-expression AST — kernel truth.
//!
//! Two mutually-recursive enums describe the algebra: [`ScalarField`] for
//! `f: R^2 -> R` and [`VectorField`] for `f: R^2 -> R^2`. This is the
//! *definition* the kernel owns and persists; **the kernel does not evaluate
//! it** — `aether` does (closed forms + finite differences, optionally Burn).
//!
//! Ported from `aether::ast` (the field-system step-3 plan, 2026-05-31): the
//! kernel becomes the owner of the portable definition, `aether` gains a
//! `kernel` dependency and uses these types for evaluation. Two differences
//! from the aether original: [`ScalarField::Sample`] / [`VectorField::Sample`]
//! reference the kernel-stable [`FieldId`](super::field::FieldId) (UUID) rather
//! than aether's registry-local `FieldId(u64)`; and there is no Rhai/Burn here.
//!
//! Derives are serde only for now. rkyv archiving is handled at the
//! `Persisted*` DTO layer (plan Phase 2), where the recursive-Box rkyv decision
//! (omit_bounds vs serde-blob) is settled against `snapshot/{to,from}.rs`.
//!
//! WASM-clean: plain data, no host dependencies.

use serde::{Deserialize, Serialize};

use super::field::FieldId;

/// Falloff shape for spatially-bounded scalar kernels (e.g. [`ScalarField::Disk`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Falloff {
    /// Constant `1.0` inside, `0.0` outside (a step).
    Hard,
    /// Linear ramp from `1.0` at center to `0.0` at boundary.
    Linear,
    /// `1 - smoothstep(0,1,t)` ramp; smooth derivative at both ends.
    Smoothstep,
    /// `(1 - t)^2` quadratic ramp.
    Quadratic,
}

/// A scalar field expression. Recursive cases use `Box` to keep the enum sized.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScalarField {
    /// Constant value everywhere.
    Const(f32),
    /// The x-coordinate at the evaluation point.
    CoordX,
    /// The y-coordinate at the evaluation point.
    CoordY,
    /// The current time (seconds since a host-chosen epoch).
    Time,
    /// `exp(-||p - center||^2 / (2 sigma^2))`.
    Gaussian { center: Box<VectorField>, sigma: f32 },
    /// `1.0` at center, `0.0` outside `radius`, with a falloff inside.
    Disk {
        center: Box<VectorField>,
        radius: f32,
        falloff: Falloff,
    },
    /// `dot(normal(p), p) + offset`.
    Linear { normal: Box<VectorField>, offset: f32 },
    /// Pointwise sum.
    Add(Box<ScalarField>, Box<ScalarField>),
    /// Pointwise product.
    Mul(Box<ScalarField>, Box<ScalarField>),
    /// Pointwise multiply by a constant scalar.
    Scale(Box<ScalarField>, f32),
    /// Negation.
    Negate(Box<ScalarField>),
    /// Dot product of two vector fields.
    Dot(Box<VectorField>, Box<VectorField>),
    /// Reference to another registered scalar field by id.
    Sample(FieldId),
}

/// A vector field expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VectorField {
    /// Constant vector everywhere.
    ConstVec { x: f32, y: f32 },
    /// The identity field returning `(x, y)` at the evaluation point.
    Coord,
    /// `grad(scalar)` — first-class so the evaluator can use closed forms.
    Gradient(Box<ScalarField>),
    /// 2D 90-degree rotation: `Perp((x, y)) = (-y, x)`.
    Perp(Box<VectorField>),
    /// Pointwise sum.
    Add(Box<VectorField>, Box<VectorField>),
    /// Pointwise scalar-times-vector.
    Scale(Box<VectorField>, Box<ScalarField>),
    /// Pointwise multiply by a constant scalar.
    ScaleConst(Box<VectorField>, f32),
    /// Reference to another registered vector field by id.
    Sample(FieldId),
}

// ── Convenience constructors ────────────────────────────────────────────────

impl ScalarField {
    /// `Gaussian` centered at a constant point.
    pub fn gaussian_at(cx: f32, cy: f32, sigma: f32) -> Self {
        Self::Gaussian {
            center: Box::new(VectorField::ConstVec { x: cx, y: cy }),
            sigma,
        }
    }

    /// `Disk` centered at a constant point.
    pub fn disk_at(cx: f32, cy: f32, radius: f32, falloff: Falloff) -> Self {
        Self::Disk {
            center: Box::new(VectorField::ConstVec { x: cx, y: cy }),
            radius,
            falloff,
        }
    }

    /// `Linear` with a constant normal: `dot((nx, ny), p) + offset`.
    pub fn linear(nx: f32, ny: f32, offset: f32) -> Self {
        Self::Linear {
            normal: Box::new(VectorField::ConstVec { x: nx, y: ny }),
            offset,
        }
    }
}

impl VectorField {
    pub fn const_vec(x: f32, y: f32) -> Self {
        Self::ConstVec { x, y }
    }

    pub fn gradient_of(scalar: ScalarField) -> Self {
        Self::Gradient(Box::new(scalar))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_field_serde_roundtrip() {
        let field = ScalarField::Add(
            Box::new(ScalarField::gaussian_at(10.0, 20.0, 50.0)),
            Box::new(ScalarField::Scale(Box::new(ScalarField::CoordX), 0.5)),
        );
        let s = serde_json::to_string(&field).unwrap();
        let back: ScalarField = serde_json::from_str(&s).unwrap();
        assert_eq!(field, back);
    }

    #[test]
    fn vector_field_serde_roundtrip() {
        let field = VectorField::Add(
            Box::new(VectorField::Coord),
            Box::new(VectorField::Gradient(Box::new(ScalarField::CoordX))),
        );
        let s = serde_json::to_string(&field).unwrap();
        let back: VectorField = serde_json::from_str(&s).unwrap();
        assert_eq!(field, back);
    }

    #[test]
    fn disk_falloff_roundtrip() {
        let f = ScalarField::disk_at(5.0, -3.0, 7.5, Falloff::Smoothstep);
        let s = serde_json::to_string(&f).unwrap();
        assert_eq!(f, serde_json::from_str::<ScalarField>(&s).unwrap());
    }
}
