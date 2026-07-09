/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Tensorized N-body forces (burn brief Lane 5).
//!
//! A repulsion pass computed on burn: for N canvas positions, the force on
//! each body is the softened inverse-square repulsion summed over every other
//! body. This does **not** fit the field-algebra AST (whose kernels take fixed
//! parameters — one Gaussian center, one Linear normal); an N-body force is
//! parameterized by all N *dynamic* source positions, so it is a dedicated
//! burn kernel living beside [`lower_burn`](crate::lower_burn), backend-generic
//! the same way (ndarray CPU, or wgpu GPU under `field-burn-wgpu`).
//!
//! seiche stays burn-free: this is the *field source*, computing forces that
//! seiche's integrator applies. seiche already has an O(N log N) Barnes-Hut CPU
//! path; this naive O(N²) GPU pass is meant to win only above a crossover N
//! (measured in the timing test), where the GPU's throughput beats the better
//! asymptotics on CPU.

use burn::tensor::{Tensor, backend::Backend};

/// Parameters for the repulsion pass.
#[derive(Debug, Clone, Copy)]
pub struct RepulsionParams {
    /// Overall force scale (Coulomb-like constant).
    pub strength: f32,
    /// Softening length ε: added in quadrature to every pairwise distance so
    /// the self term (distance 0) and near-coincident bodies stay finite. The
    /// self term contributes zero force by construction (its displacement is
    /// zero), so no diagonal masking is needed.
    pub softening: f32,
}

impl Default for RepulsionParams {
    fn default() -> Self {
        Self {
            strength: 1.0,
            softening: 1.0e-2,
        }
    }
}

/// Softened inverse-square repulsion forces for a batch of 2-D positions.
///
/// `xs` / `ys` are rank-1 `[N]` position components; the result is the rank-1
/// `[N]` force components `(fx, fy)`. Force on body i:
/// `strength · Σ_{j} (p_i − p_j) / (|p_i − p_j|² + ε²)^{3/2}`. The `j = i` term
/// vanishes (numerator zero), so it is summed over all j including self.
pub fn repulsion<B: Backend>(
    xs: Tensor<B, 1>,
    ys: Tensor<B, 1>,
    params: RepulsionParams,
) -> (Tensor<B, 1>, Tensor<B, 1>) {
    let n = xs.dims()[0];

    // Pairwise displacements by broadcasting [N,1] against [1,N]:
    // dx[i,j] = x_i − x_j.
    let dx = xs.clone().reshape([n, 1]) - xs.reshape([1, n]);
    let dy = ys.clone().reshape([n, 1]) - ys.reshape([1, n]);

    let eps2 = params.softening * params.softening;
    let dist_sq = (dx.clone() * dx.clone() + dy.clone() * dy.clone()).add_scalar(eps2);
    // (dist² )^{-3/2} = 1 / (sqrt(dist²) · dist²).
    let inv_dist_cubed = (dist_sq.clone().sqrt() * dist_sq).recip();

    let fx = (dx * inv_dist_cubed.clone())
        .sum_dim(1)
        .reshape([n])
        .mul_scalar(params.strength);
    let fy = (dy * inv_dist_cubed)
        .sum_dim(1)
        .reshape([n])
        .mul_scalar(params.strength);
    (fx, fy)
}

/// Repulsion on the wgpu backend — the host-facing entry point (hosts never
/// name `burn`). Takes/returns plain `f32` slices, so it drops straight into a
/// seiche `RepulsionSolver` closure. Positions in, `(fx, fy)` out.
#[cfg(feature = "field-burn-wgpu")]
pub fn repulsion_wgpu(xs: &[f32], ys: &[f32], params: RepulsionParams) -> (Vec<f32>, Vec<f32>) {
    use burn::backend::Wgpu;
    use burn::tensor::backend::BackendTypes;
    type B = Wgpu<f32, i32>;
    let dev = <B as BackendTypes>::Device::default();
    let (fx, fy) = repulsion::<B>(
        Tensor::<B, 1>::from_floats(xs, &dev),
        Tensor::<B, 1>::from_floats(ys, &dev),
        params,
    );
    (
        fx.into_data().to_vec::<f32>().expect("fx readback"),
        fy.into_data().to_vec::<f32>().expect("fy readback"),
    )
}

/// Naive O(N²) host reference, no burn — the correctness anchor. Two burn
/// backends agreeing proves they match each other, not that the formula is
/// right; this proves the formula.
pub fn repulsion_reference(
    xs: &[f32],
    ys: &[f32],
    params: RepulsionParams,
) -> (Vec<f32>, Vec<f32>) {
    let n = xs.len();
    let eps2 = params.softening * params.softening;
    let mut fx = vec![0.0f32; n];
    let mut fy = vec![0.0f32; n];
    for i in 0..n {
        for j in 0..n {
            let dx = xs[i] - xs[j];
            let dy = ys[i] - ys[j];
            let d2 = dx * dx + dy * dy + eps2;
            let inv = 1.0 / (d2.sqrt() * d2);
            fx[i] += dx * inv;
            fy[i] += dy * inv;
        }
        fx[i] *= params.strength;
        fy[i] *= params.strength;
    }
    (fx, fy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn::backend::NdArray;
    use burn::tensor::backend::BackendTypes;

    type B = NdArray<f32>;

    fn run<Bk: Backend>(xs: &[f32], ys: &[f32], p: RepulsionParams) -> (Vec<f32>, Vec<f32>) {
        let dev = <Bk as BackendTypes>::Device::default();
        let (fx, fy) = repulsion::<Bk>(
            Tensor::<Bk, 1>::from_floats(xs, &dev),
            Tensor::<Bk, 1>::from_floats(ys, &dev),
            p,
        );
        (
            fx.into_data().to_vec::<f32>().unwrap(),
            fy.into_data().to_vec::<f32>().unwrap(),
        )
    }

    #[test]
    fn matches_naive_reference() {
        let xs = [0.0f32, 1.0, -2.0, 3.5, 0.7];
        let ys = [0.0f32, 2.0, 1.0, -1.5, 4.0];
        let p = RepulsionParams::default();
        let (bx, by) = run::<B>(&xs, &ys, p);
        let (rx, ry) = repulsion_reference(&xs, &ys, p);
        for (a, b) in bx.iter().zip(&rx) {
            assert!((a - b).abs() < 1.0e-4, "fx {a} vs {b}");
        }
        for (a, b) in by.iter().zip(&ry) {
            assert!((a - b).abs() < 1.0e-4, "fy {a} vs {b}");
        }
    }

    #[test]
    fn two_bodies_repel_apart() {
        // Body 0 at origin, body 1 to its right: 0 pushed left (−x), 1 right (+x),
        // equal and opposite, no y component.
        let (fx, fy) = run::<B>(&[0.0, 1.0], &[0.0, 0.0], RepulsionParams::default());
        assert!(fx[0] < 0.0, "left body pushed left: {}", fx[0]);
        assert!(fx[1] > 0.0, "right body pushed right: {}", fx[1]);
        assert!((fx[0] + fx[1]).abs() < 1.0e-5, "equal and opposite");
        assert!(fy[0].abs() < 1.0e-5 && fy[1].abs() < 1.0e-5, "no y force");
    }

    #[test]
    fn self_term_contributes_zero() {
        // One body feels no force from itself.
        let (fx, fy) = run::<B>(&[2.0], &[3.0], RepulsionParams::default());
        assert!(fx[0].abs() < 1.0e-6 && fy[0].abs() < 1.0e-6);
    }
}

#[cfg(all(test, feature = "field-burn-wgpu"))]
mod tests_wgpu {
    use super::*;
    use burn::backend::{NdArray, Wgpu};
    use burn::tensor::backend::BackendTypes;

    type Cpu = NdArray<f32>;
    type Gpu = Wgpu<f32, i32>;

    /// Deterministic spread of N positions; no RNG (varies by index).
    fn positions(n: usize) -> (Vec<f32>, Vec<f32>) {
        (0..n)
            .map(|i| {
                let t = i as f32;
                ((t * 0.6180339).sin() * 100.0, (t * 0.7548776).cos() * 100.0)
            })
            .unzip()
    }

    fn run<B: Backend>(xs: &[f32], ys: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let dev = <B as BackendTypes>::Device::default();
        let (fx, fy) = repulsion::<B>(
            Tensor::<B, 1>::from_floats(xs, &dev),
            Tensor::<B, 1>::from_floats(ys, &dev),
            RepulsionParams::default(),
        );
        (
            fx.into_data().to_vec::<f32>().unwrap(),
            fy.into_data().to_vec::<f32>().unwrap(),
        )
    }

    #[test]
    fn parity_ndarray_wgpu() {
        let (xs, ys) = positions(257);
        let (cx, cy) = run::<Cpu>(&xs, &ys);
        let (gx, gy) = run::<Gpu>(&xs, &ys);
        let max = |a: &[f32], b: &[f32]| {
            a.iter()
                .zip(b)
                .map(|(x, y)| (x - y).abs())
                .fold(0.0f32, f32::max)
        };
        assert!(max(&cx, &gx) < 1.0e-3, "fx diverged");
        assert!(max(&cy, &gy) < 1.0e-3, "fy diverged");
    }

    /// CPU-vs-GPU across N, readback included, GPU warmed. Finds the crossover
    /// N where the O(N²) GPU pass beats the O(N²) CPU pass (the seiche Barnes-Hut
    /// comparison is P3). Run:
    /// `cargo test -p quint --features field-burn-wgpu --release -- --ignored timing --nocapture`
    #[test]
    #[ignore]
    fn timing_repulsion_cpu_vs_gpu() {
        for n in [256usize, 1_000, 4_000, 16_000] {
            let (xs, ys) = positions(n);
            let _warm = run::<Gpu>(&xs, &ys);

            let t = std::time::Instant::now();
            let _ = run::<Cpu>(&xs, &ys);
            let cpu_us = t.elapsed().as_micros();

            let t = std::time::Instant::now();
            let _ = run::<Gpu>(&xs, &ys);
            let gpu_us = t.elapsed().as_micros();

            println!("n={n}: ndarray={cpu_us}us wgpu={gpu_us}us");
        }
    }
}
