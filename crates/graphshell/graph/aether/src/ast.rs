/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Field expression AST.
//!
//! Two enums describe the algebra: [`ScalarField`] for `f: R^2 -> R` and
//! [`VectorField`] for `f: R^2 -> R^2`. The calculus operators (`Gradient`,
//! later `Curl` / `Divergence`) are first-class — that is the deliberate
//! departure from raw tensor IRs and lets the evaluator recognise closed
//! forms (e.g. `Gradient(Gaussian)`) before falling through to finite
//! differences.

use serde::{Deserialize, Serialize};

use crate::registry::FieldId;

/// Falloff shape for spatially-bounded scalar kernels (e.g. `Disk`).
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

/// A scalar field expression.
///
/// Recursive cases use `Box` to keep the enum sized.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScalarField {
    /// Constant value everywhere.
    Const(f32),
    /// The x-coordinate at the evaluation point.
    CoordX,
    /// The y-coordinate at the evaluation point.
    CoordY,
    /// The current time (seconds since some epoch the host chose).
    Time,
    /// `exp(-||p - center||^2 / (2 sigma^2))`.
    Gaussian {
        center: Box<VectorField>,
        sigma: f32,
    },
    /// `1.0` at center, `0.0` outside `radius`, with a falloff inside.
    Disk {
        center: Box<VectorField>,
        radius: f32,
        falloff: Falloff,
    },
    /// `dot(normal(p), p) + offset`.
    Linear {
        normal: Box<VectorField>,
        offset: f32,
    },
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
    /// Reference to a registered scalar field by id.
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
    /// Reference to a registered vector field by id.
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

    /// `Linear` with a constant normal vector.
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
    fn falloff_default_serde_roundtrip() {
        for f in [
            Falloff::Hard,
            Falloff::Linear,
            Falloff::Smoothstep,
            Falloff::Quadratic,
        ] {
            let s = serde_json::to_string(&f).unwrap();
            let back: Falloff = serde_json::from_str(&s).unwrap();
            assert_eq!(f, back);
        }
    }

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
    fn convenience_gaussian_at() {
        let f = ScalarField::gaussian_at(0.0, 0.0, 10.0);
        match f {
            ScalarField::Gaussian { center, sigma } => {
                assert_eq!(*center, VectorField::ConstVec { x: 0.0, y: 0.0 });
                assert_eq!(sigma, 10.0);
            }
            _ => panic!("expected Gaussian"),
        }
    }

    #[test]
    fn convenience_disk_at() {
        let f = ScalarField::disk_at(5.0, -3.0, 7.5, Falloff::Smoothstep);
        match f {
            ScalarField::Disk {
                center,
                radius,
                falloff,
            } => {
                assert_eq!(*center, VectorField::ConstVec { x: 5.0, y: -3.0 });
                assert_eq!(radius, 7.5);
                assert_eq!(falloff, Falloff::Smoothstep);
            }
            _ => panic!("expected Disk"),
        }
    }

    #[test]
    fn vector_const_and_gradient_constructors() {
        assert_eq!(
            VectorField::const_vec(1.0, 2.0),
            VectorField::ConstVec { x: 1.0, y: 2.0 }
        );
        let g = VectorField::gradient_of(ScalarField::CoordX);
        match g {
            VectorField::Gradient(s) => assert_eq!(*s, ScalarField::CoordX),
            _ => panic!("expected Gradient"),
        }
    }

    #[test]
    fn nested_serde_preserves_structure() {
        let f = ScalarField::Mul(
            Box::new(ScalarField::Negate(Box::new(ScalarField::CoordY))),
            Box::new(ScalarField::Dot(
                Box::new(VectorField::Coord),
                Box::new(VectorField::ConstVec { x: 1.0, y: 0.0 }),
            )),
        );
        let s = serde_json::to_string(&f).unwrap();
        let back: ScalarField = serde_json::from_str(&s).unwrap();
        assert_eq!(f, back);
    }
}
