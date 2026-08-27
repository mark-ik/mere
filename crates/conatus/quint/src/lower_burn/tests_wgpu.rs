// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Wgpu ↔ NdArray parity and timing for the burn lowering (burn brief
//! Lane 1, `field-burn-wgpu`). Parity tests need a working GPU adapter;
//! the timing test is `#[ignore]`d — run it explicitly:
//!
//! ```bash
//! cargo test -p quint --features field-burn-wgpu --release \
//!     -- --ignored timing --nocapture
//! ```

use super::*;
use crate::ast::{ScalarField, VectorField};
use burn::tensor::Device;
use burn::tensor::Tensor;

fn cpu_device() -> Device {
    Device::ndarray()
}
fn gpu_device() -> Device {
    Device::wgpu(burn::tensor::DeviceKind::DiscreteGpu(0))
}

/// A composition that exercises Gaussian, Linear, Mul, Scale, and Add in
/// one program: gaussian(0,0,σ=10) + 0.25·(x·linear(2,-3,1)).
fn scalar_program() -> ScalarField {
    ScalarField::Add(
        Box::new(ScalarField::gaussian_at(0.0, 0.0, 10.0)),
        Box::new(ScalarField::Scale(
            Box::new(ScalarField::Mul(
                Box::new(ScalarField::CoordX),
                Box::new(ScalarField::linear(2.0, -3.0, 1.0)),
            )),
            0.25,
        )),
    )
}

/// Gradient of a gaussian plus a perpendicular constant swirl.
fn vector_program() -> VectorField {
    VectorField::Add(
        Box::new(VectorField::Gradient(Box::new(ScalarField::gaussian_at(
            0.0, 0.0, 10.0,
        )))),
        Box::new(VectorField::Perp(Box::new(VectorField::ConstVec {
            x: 0.5,
            y: 0.0,
        }))),
    )
}

fn points(n: usize) -> Vec<(f32, f32)> {
    // Deterministic spread; no RNG needed for parity or timing.
    (0..n)
        .map(|i| {
            let t = i as f32;
            ((t * 0.37) % 40.0 - 20.0, (t * 0.73) % 40.0 - 20.0)
        })
        .collect()
}

fn tensors(pts: &[(f32, f32)], dev: &Device) -> (Tensor<1>, Tensor<1>) {
    let xs: Vec<f32> = pts.iter().map(|(x, _)| *x).collect();
    let ys: Vec<f32> = pts.iter().map(|(_, y)| *y).collect();
    (
        Tensor::<1>::from_floats(xs.as_slice(), dev),
        Tensor::<1>::from_floats(ys.as_slice(), dev),
    )
}

fn run_scalar(f: &ScalarField, pts: &[(f32, f32)], dev: &Device) -> Vec<f32> {
    let reg = FieldRegistry::new();
    let (xs, ys) = tensors(pts, dev);
    lower_scalar(f, &reg, xs, ys, 0.0)
        .unwrap()
        .into_data()
        .to_vec::<f32>()
        .unwrap()
}

fn run_vector(f: &VectorField, pts: &[(f32, f32)], dev: &Device) -> (Vec<f32>, Vec<f32>) {
    let reg = FieldRegistry::new();
    let (xs, ys) = tensors(pts, dev);
    let (rx, ry) = lower_vector(f, &reg, xs, ys, 0.0).unwrap();
    (
        rx.into_data().to_vec::<f32>().unwrap(),
        ry.into_data().to_vec::<f32>().unwrap(),
    )
}

fn approx(a: &[f32], b: &[f32], eps: f32) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < eps)
}

#[test]
fn scalar_parity_ndarray_wgpu() {
    let f = scalar_program();
    let pts = points(257); // odd size: no accidental tile alignment
    let cpu = run_scalar(&f, &pts, &cpu_device());
    let gpu = run_scalar(&f, &pts, &gpu_device());
    assert!(
        approx(&cpu, &gpu, 1.0e-3),
        "cpu[..4]={:?} gpu[..4]={:?}",
        &cpu[..4],
        &gpu[..4]
    );
}

#[test]
fn vector_parity_ndarray_wgpu() {
    let f = vector_program();
    let pts = points(257);
    let (cx, cy) = run_vector(&f, &pts, &cpu_device());
    let (gx, gy) = run_vector(&f, &pts, &gpu_device());
    assert!(approx(&cx, &gx, 1.0e-3), "x diverged");
    assert!(approx(&cy, &gy, 1.0e-3), "y diverged");
}

/// CPU-vs-GPU timing across batch sizes, including device→host readback
/// (forces come back to the host in today's architecture, so readback is
/// part of the honest number). GPU gets one warmup pass per size so kernel
/// compilation is not billed to the steady state.
#[test]
#[ignore]
fn timing_scalar_cpu_vs_gpu() {
    let f = scalar_program();
    for n in [1_000usize, 10_000, 100_000] {
        let pts = points(n);
        let _warm = run_scalar(&f, &pts, &gpu_device());

        let t = std::time::Instant::now();
        let _cpu = run_scalar(&f, &pts, &cpu_device());
        let cpu_us = t.elapsed().as_micros();

        let t = std::time::Instant::now();
        let _gpu = run_scalar(&f, &pts, &gpu_device());
        let gpu_us = t.elapsed().as_micros();

        println!("n={n}: ndarray={cpu_us}us wgpu={gpu_us}us");
    }
}
