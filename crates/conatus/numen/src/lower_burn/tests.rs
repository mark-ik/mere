// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::ast::{ScalarField, VectorField};
use crate::eval;
use burn::tensor::Tensor;

// Backend chosen per call site via Device.

fn device() -> burn::tensor::Device {
    Default::default()
}

fn make_xs_ys(points: &[(f32, f32)]) -> (Tensor<1>, Tensor<1>) {
    let dev = device();
    let xs: Vec<f32> = points.iter().map(|(x, _)| *x).collect();
    let ys: Vec<f32> = points.iter().map(|(_, y)| *y).collect();
    (
        Tensor::<1>::from_floats(xs.as_slice(), &dev),
        Tensor::<1>::from_floats(ys.as_slice(), &dev),
    )
}

fn approx_slice(actual: &[f32], expected: &[f32], eps: f32) -> bool {
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected.iter())
            .all(|(a, b)| (a - b).abs() < eps)
}

#[test]
fn const_field_matches_analytic() {
    let reg = FieldRegistry::new();
    let (xs, ys) = make_xs_ys(&[(0.0, 0.0), (1.0, 2.0), (3.0, 4.0)]);
    let f = ScalarField::Const(2.5);
    let result = lower_scalar(&f, &reg, xs, ys, 0.0).unwrap();
    let actual = result.into_data().to_vec::<f32>().unwrap();
    assert!(approx_slice(&actual, &[2.5, 2.5, 2.5], 1.0e-6));
}

#[test]
fn coord_x_matches_input() {
    let reg = FieldRegistry::new();
    let (xs, ys) = make_xs_ys(&[(1.0, 10.0), (2.0, 20.0), (3.0, 30.0)]);
    let result = lower_scalar(&ScalarField::CoordX, &reg, xs, ys, 0.0).unwrap();
    let actual = result.into_data().to_vec::<f32>().unwrap();
    assert!(approx_slice(&actual, &[1.0, 2.0, 3.0], 1.0e-6));
}

#[test]
fn coord_y_matches_input() {
    let reg = FieldRegistry::new();
    let (xs, ys) = make_xs_ys(&[(1.0, 10.0), (2.0, 20.0)]);
    let result = lower_scalar(&ScalarField::CoordY, &reg, xs, ys, 0.0).unwrap();
    let actual = result.into_data().to_vec::<f32>().unwrap();
    assert!(approx_slice(&actual, &[10.0, 20.0], 1.0e-6));
}

#[test]
fn time_field_broadcasts() {
    let reg = FieldRegistry::new();
    let (xs, ys) = make_xs_ys(&[(0.0, 0.0), (1.0, 1.0)]);
    let result = lower_scalar(&ScalarField::Time, &reg, xs, ys, 1.5).unwrap();
    let actual = result.into_data().to_vec::<f32>().unwrap();
    assert!(approx_slice(&actual, &[1.5, 1.5], 1.0e-6));
}

#[test]
fn add_matches_analytic() {
    let reg = FieldRegistry::new();
    let pts = [(1.0, 2.0), (3.0, 4.0)];
    let (xs, ys) = make_xs_ys(&pts);
    let f = ScalarField::Add(Box::new(ScalarField::CoordX), Box::new(ScalarField::CoordY));
    let result = lower_scalar(&f, &reg, xs, ys, 0.0).unwrap();
    let actual = result.into_data().to_vec::<f32>().unwrap();
    let expected: Vec<f32> = pts
        .iter()
        .map(|(x, y)| eval::eval_scalar(&f, &reg, *x, *y, 0.0))
        .collect();
    assert!(approx_slice(&actual, &expected, 1.0e-6));
}

#[test]
fn gaussian_matches_analytic() {
    let reg = FieldRegistry::new();
    let pts = [(0.0, 0.0), (5.0, 0.0), (10.0, 0.0), (-3.0, 4.0)];
    let (xs, ys) = make_xs_ys(&pts);
    let f = ScalarField::gaussian_at(0.0, 0.0, 10.0);
    let result = lower_scalar(&f, &reg, xs, ys, 0.0).unwrap();
    let actual = result.into_data().to_vec::<f32>().unwrap();
    let expected: Vec<f32> = pts
        .iter()
        .map(|(x, y)| eval::eval_scalar(&f, &reg, *x, *y, 0.0))
        .collect();
    assert!(
        approx_slice(&actual, &expected, 1.0e-5),
        "actual={:?} expected={:?}",
        actual,
        expected
    );
}

#[test]
fn linear_matches_analytic() {
    let reg = FieldRegistry::new();
    let pts = [(0.0, 0.0), (1.0, 1.0), (2.0, -1.0)];
    let (xs, ys) = make_xs_ys(&pts);
    let f = ScalarField::linear(2.0, -3.0, 1.0);
    let result = lower_scalar(&f, &reg, xs, ys, 0.0).unwrap();
    let actual = result.into_data().to_vec::<f32>().unwrap();
    let expected: Vec<f32> = pts
        .iter()
        .map(|(x, y)| eval::eval_scalar(&f, &reg, *x, *y, 0.0))
        .collect();
    assert!(approx_slice(&actual, &expected, 1.0e-5));
}

#[test]
fn negate_flips_sign() {
    let reg = FieldRegistry::new();
    let (xs, ys) = make_xs_ys(&[(2.0, 0.0), (-3.0, 0.0)]);
    let f = ScalarField::Negate(Box::new(ScalarField::CoordX));
    let result = lower_scalar(&f, &reg, xs, ys, 0.0).unwrap();
    let actual = result.into_data().to_vec::<f32>().unwrap();
    assert!(approx_slice(&actual, &[-2.0, 3.0], 1.0e-6));
}

#[test]
fn scale_matches_analytic() {
    let reg = FieldRegistry::new();
    let (xs, ys) = make_xs_ys(&[(1.0, 0.0), (2.0, 0.0)]);
    let f = ScalarField::Scale(Box::new(ScalarField::CoordX), 5.0);
    let result = lower_scalar(&f, &reg, xs, ys, 0.0).unwrap();
    let actual = result.into_data().to_vec::<f32>().unwrap();
    assert!(approx_slice(&actual, &[5.0, 10.0], 1.0e-6));
}

#[test]
fn vector_const_matches_analytic() {
    let reg = FieldRegistry::new();
    let (xs, ys) = make_xs_ys(&[(0.0, 0.0), (1.0, 1.0)]);
    let f = VectorField::ConstVec { x: 3.0, y: 5.0 };
    let (rx, ry) = lower_vector(&f, &reg, xs, ys, 0.0).unwrap();
    let ax = rx.into_data().to_vec::<f32>().unwrap();
    let ay = ry.into_data().to_vec::<f32>().unwrap();
    assert!(approx_slice(&ax, &[3.0, 3.0], 1.0e-6));
    assert!(approx_slice(&ay, &[5.0, 5.0], 1.0e-6));
}

#[test]
fn vector_perp_rotates_90() {
    let reg = FieldRegistry::new();
    let (xs, ys) = make_xs_ys(&[(0.0, 0.0)]);
    let f = VectorField::Perp(Box::new(VectorField::ConstVec { x: 1.0, y: 0.0 }));
    let (rx, ry) = lower_vector(&f, &reg, xs, ys, 0.0).unwrap();
    assert!(approx_slice(
        &rx.into_data().to_vec::<f32>().unwrap(),
        &[0.0],
        1.0e-6
    ));
    assert!(approx_slice(
        &ry.into_data().to_vec::<f32>().unwrap(),
        &[1.0],
        1.0e-6
    ));
}

#[test]
fn gradient_of_gaussian_matches_analytic() {
    let reg = FieldRegistry::new();
    let pts = [(5.0, 0.0), (0.0, 5.0), (3.0, 4.0)];
    let (xs, ys) = make_xs_ys(&pts);
    let f = ScalarField::gaussian_at(0.0, 0.0, 10.0);
    let g = VectorField::Gradient(Box::new(f.clone()));
    let (rx, ry) = lower_vector(&g, &reg, xs, ys, 0.0).unwrap();
    let ax = rx.into_data().to_vec::<f32>().unwrap();
    let ay = ry.into_data().to_vec::<f32>().unwrap();
    let expected: Vec<(f32, f32)> = pts
        .iter()
        .map(|(x, y)| eval::grad_scalar(&f, &reg, *x, *y, 0.0))
        .collect();
    let ex: Vec<f32> = expected.iter().map(|(x, _)| *x).collect();
    let ey: Vec<f32> = expected.iter().map(|(_, y)| *y).collect();
    assert!(
        approx_slice(&ax, &ex, 1.0e-5),
        "actual_x={:?} expected_x={:?}",
        ax,
        ex
    );
    assert!(
        approx_slice(&ay, &ey, 1.0e-5),
        "actual_y={:?} expected_y={:?}",
        ay,
        ey
    );
}

#[test]
fn gradient_of_linear_returns_normal() {
    let reg = FieldRegistry::new();
    let (xs, ys) = make_xs_ys(&[(0.0, 0.0), (10.0, 20.0)]);
    let f = ScalarField::linear(2.0, -3.0, 1.0);
    let g = VectorField::Gradient(Box::new(f));
    let (rx, ry) = lower_vector(&g, &reg, xs, ys, 0.0).unwrap();
    let ax = rx.into_data().to_vec::<f32>().unwrap();
    let ay = ry.into_data().to_vec::<f32>().unwrap();
    assert!(approx_slice(&ax, &[2.0, 2.0], 1.0e-6));
    assert!(approx_slice(&ay, &[-3.0, -3.0], 1.0e-6));
}

#[test]
fn sample_resolves_through_registry() {
    let mut reg = FieldRegistry::new();
    let id = reg.insert_scalar("base", ScalarField::Const(7.0));
    let (xs, ys) = make_xs_ys(&[(0.0, 0.0), (1.0, 1.0)]);
    let result = lower_scalar(&ScalarField::Sample(id), &reg, xs, ys, 0.0).unwrap();
    let actual = result.into_data().to_vec::<f32>().unwrap();
    assert!(approx_slice(&actual, &[7.0, 7.0], 1.0e-6));
}

#[test]
fn sample_unknown_id_errors() {
    let reg = FieldRegistry::new();
    let (xs, ys) = make_xs_ys(&[(0.0, 0.0)]);
    let missing = FieldId::from_uuid(uuid::Uuid::from_u128(99));
    let err = lower_scalar(&ScalarField::Sample(missing), &reg, xs, ys, 0.0).unwrap_err();
    assert_eq!(err, LowerError::UnknownField(missing));
}

#[test]
fn disk_hard_matches_analytic() {
    let reg = FieldRegistry::new();
    let pts = [(0.0, 0.0), (5.0, 0.0), (11.0, 0.0), (-3.0, 4.0)];
    let (xs, ys) = make_xs_ys(&pts);
    let f = ScalarField::disk_at(0.0, 0.0, 10.0, Falloff::Hard);
    let result = lower_scalar(&f, &reg, xs, ys, 0.0).unwrap();
    let actual = result.into_data().to_vec::<f32>().unwrap();
    let expected: Vec<f32> = pts
        .iter()
        .map(|(x, y)| eval::eval_scalar(&f, &reg, *x, *y, 0.0))
        .collect();
    assert!(
        approx_slice(&actual, &expected, 1.0e-5),
        "actual={:?} expected={:?}",
        actual,
        expected
    );
}

#[test]
fn disk_linear_matches_analytic() {
    let reg = FieldRegistry::new();
    let pts = [(0.0, 0.0), (5.0, 0.0), (10.0, 0.0), (11.0, 0.0)];
    let (xs, ys) = make_xs_ys(&pts);
    let f = ScalarField::disk_at(0.0, 0.0, 10.0, Falloff::Linear);
    let result = lower_scalar(&f, &reg, xs, ys, 0.0).unwrap();
    let actual = result.into_data().to_vec::<f32>().unwrap();
    let expected: Vec<f32> = pts
        .iter()
        .map(|(x, y)| eval::eval_scalar(&f, &reg, *x, *y, 0.0))
        .collect();
    assert!(approx_slice(&actual, &expected, 1.0e-5));
}

#[test]
fn disk_smoothstep_matches_analytic() {
    let reg = FieldRegistry::new();
    let pts = [(0.0, 0.0), (3.0, 4.0), (6.0, 8.0), (12.0, 0.0)];
    let (xs, ys) = make_xs_ys(&pts);
    let f = ScalarField::disk_at(0.0, 0.0, 10.0, Falloff::Smoothstep);
    let result = lower_scalar(&f, &reg, xs, ys, 0.0).unwrap();
    let actual = result.into_data().to_vec::<f32>().unwrap();
    let expected: Vec<f32> = pts
        .iter()
        .map(|(x, y)| eval::eval_scalar(&f, &reg, *x, *y, 0.0))
        .collect();
    assert!(
        approx_slice(&actual, &expected, 1.0e-5),
        "actual={:?} expected={:?}",
        actual,
        expected
    );
}

#[test]
fn disk_quadratic_matches_analytic() {
    let reg = FieldRegistry::new();
    let pts = [(0.0, 0.0), (5.0, 0.0), (10.0, 0.0), (15.0, 0.0)];
    let (xs, ys) = make_xs_ys(&pts);
    let f = ScalarField::disk_at(0.0, 0.0, 10.0, Falloff::Quadratic);
    let result = lower_scalar(&f, &reg, xs, ys, 0.0).unwrap();
    let actual = result.into_data().to_vec::<f32>().unwrap();
    let expected: Vec<f32> = pts
        .iter()
        .map(|(x, y)| eval::eval_scalar(&f, &reg, *x, *y, 0.0))
        .collect();
    assert!(approx_slice(&actual, &expected, 1.0e-5));
}

#[test]
fn dot_matches_analytic() {
    let reg = FieldRegistry::new();
    let pts = [(0.0, 0.0), (1.0, 0.0), (3.0, 4.0)];
    let (xs, ys) = make_xs_ys(&pts);
    // dot(coord, const(1, 2)) = x*1 + y*2
    let f = ScalarField::Dot(
        Box::new(VectorField::Coord),
        Box::new(VectorField::ConstVec { x: 1.0, y: 2.0 }),
    );
    let result = lower_scalar(&f, &reg, xs, ys, 0.0).unwrap();
    let actual = result.into_data().to_vec::<f32>().unwrap();
    let expected = vec![0.0, 1.0, 11.0];
    assert!(approx_slice(&actual, &expected, 1.0e-5));
}

#[test]
fn vector_scale_by_scalar_field() {
    let reg = FieldRegistry::new();
    let (xs, ys) = make_xs_ys(&[(2.0, 3.0), (4.0, 6.0)]);
    // Scale ConstVec(1, 0) by CoordX -> (x, 0)
    let f = VectorField::Scale(
        Box::new(VectorField::ConstVec { x: 1.0, y: 0.0 }),
        Box::new(ScalarField::CoordX),
    );
    let (rx, ry) = lower_vector(&f, &reg, xs, ys, 0.0).unwrap();
    let ax = rx.into_data().to_vec::<f32>().unwrap();
    let ay = ry.into_data().to_vec::<f32>().unwrap();
    assert!(approx_slice(&ax, &[2.0, 4.0], 1.0e-6));
    assert!(approx_slice(&ay, &[0.0, 0.0], 1.0e-6));
}

#[test]
fn gradient_of_mul_uses_product_rule() {
    let reg = FieldRegistry::new();
    // f = x * y; ∇f = (y, x)
    let f = ScalarField::Mul(Box::new(ScalarField::CoordX), Box::new(ScalarField::CoordY));
    let g = VectorField::Gradient(Box::new(f));
    let pts = [(0.0, 0.0), (3.0, 5.0), (-2.0, 7.0)];
    let (xs, ys) = make_xs_ys(&pts);
    let (rx, ry) = lower_vector(&g, &reg, xs, ys, 0.0).unwrap();
    let ax = rx.into_data().to_vec::<f32>().unwrap();
    let ay = ry.into_data().to_vec::<f32>().unwrap();
    // Expected: (y, x)
    assert!(approx_slice(&ax, &[0.0, 5.0, 7.0], 1.0e-5));
    assert!(approx_slice(&ay, &[0.0, 3.0, -2.0], 1.0e-5));
}

#[test]
fn gradient_of_disk_returns_unsupported() {
    let reg = FieldRegistry::new();
    let (xs, ys) = make_xs_ys(&[(0.0, 0.0)]);
    let f = ScalarField::disk_at(0.0, 0.0, 10.0, Falloff::Linear);
    let g = VectorField::Gradient(Box::new(f));
    let err = lower_vector(&g, &reg, xs, ys, 0.0).unwrap_err();
    assert_eq!(err, LowerError::UnsupportedOperator("Gradient(Disk)"));
}
