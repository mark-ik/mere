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
//! This module has two separate consumers:
//!
//! - a resident host can pass tensors backed by its own [`crate::resident`] planes
//!   and keep the result on that device;
//! - [`repulsion_wgpu_roundtrip`] and
//!   [`node_exclusion_wgpu_roundtrip`] are explicit staging helpers for experiments
//!   with a CPU-owned integrator. They upload positions and read force vectors back;
//!   they are not a resident simulation path and do not accept a host device.
//!
//! Both tensor laws materialize pairwise `[N, N]` working sets. They are suitable
//! for small and mid-sized semantic batches; the explicit resident kernel remains
//! the large-body simulation lane.

#[cfg(any(feature = "field-burn", feature = "field-gpu"))]
use burn::tensor::Tensor;

/// Input rejected before a staging helper creates a tensor program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepulsionInputError {
    PositionLengthMismatch { xs: usize, ys: usize },
}

impl std::fmt::Display for RepulsionInputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PositionLengthMismatch { xs, ys } => {
                write!(
                    formatter,
                    "repulsion needs equally-sized x/y positions, got {xs} and {ys}"
                )
            }
        }
    }
}

impl std::error::Error for RepulsionInputError {}

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

/// The hard-floor, finite-cutoff law used by a CPU graph layout.
///
/// This deliberately remains distinct from [`RepulsionParams`]. Softened
/// all-pairs fields are useful on their own, but substituting them for a layout
/// law changes its equilibrium at the routing threshold.
#[derive(Debug, Clone, Copy)]
pub struct NodeExclusionParams {
    pub strength: f32,
    pub cutoff: f32,
    pub min_distance: f32,
}

impl Default for NodeExclusionParams {
    fn default() -> Self {
        Self {
            strength: 220_000.0,
            cutoff: 1_000.0,
            min_distance: 8.0,
        }
    }
}

/// Softened inverse-square repulsion forces for a batch of 2-D positions.
///
/// `xs` / `ys` are rank-1 `[N]` position components; the result is the rank-1
/// `[N]` force components `(fx, fy)`. Force on body i:
/// `strength · Σ_{j} (p_i − p_j) / (|p_i − p_j|² + ε²)^{3/2}`. The `j = i` term
/// vanishes (numerator zero), so it is summed over all j including self.
#[cfg(any(feature = "field-burn", feature = "field-gpu"))]
pub fn repulsion(
    xs: Tensor<1>,
    ys: Tensor<1>,
    params: RepulsionParams,
) -> (Tensor<1>, Tensor<1>) {
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

/// The exact hard-floor and cutoff law represented by [`NodeExclusionParams`].
///
/// The returned tensors stay on their backend. In particular, a host may pass
/// [`crate::resident::ResidentTensor`] inputs and publish the output buffer to its
/// next resident consumer without bringing force bytes to the CPU.
#[cfg(any(feature = "field-burn", feature = "field-gpu"))]
pub fn node_exclusion(
    xs: Tensor<1>,
    ys: Tensor<1>,
    params: NodeExclusionParams,
) -> (Tensor<1>, Tensor<1>) {
    let n = xs.dims()[0];
    let dx = xs.clone().reshape([n, 1]) - xs.reshape([1, n]);
    let dy = ys.clone().reshape([n, 1]) - ys.reshape([1, n]);
    let distance_sq = dx.clone() * dx.clone() + dy.clone() * dy.clone();
    let outside_cutoff = distance_sq
        .clone()
        .greater_elem(params.cutoff * params.cutoff);
    let distance = distance_sq.sqrt().clamp_min(params.min_distance);
    let inv_distance_cubed = (distance.clone() * distance.clone() * distance).recip();

    let fx = (dx * inv_distance_cubed.clone())
        .mask_fill(outside_cutoff.clone(), 0.0)
        .sum_dim(1)
        .reshape([n])
        .mul_scalar(params.strength);
    let fy = (dy * inv_distance_cubed)
        .mask_fill(outside_cutoff, 0.0)
        .sum_dim(1)
        .reshape([n])
        .mul_scalar(params.strength);
    (fx, fy)
}

/// Runs softened repulsion on Burn's default WGPU device and returns host vectors.
///
/// This is intentionally named for the CPU→GPU→CPU round trip it performs. It is
/// useful for a benchmark or a downlevel experiment, but a renderer cannot share
/// the default device or these returned vectors as resident state.
#[cfg(feature = "field-burn-wgpu")]
pub fn repulsion_wgpu_roundtrip(
    xs: &[f32],
    ys: &[f32],
    params: RepulsionParams,
) -> Result<(Vec<f32>, Vec<f32>), RepulsionInputError> {
    if xs.len() != ys.len() {
        return Err(RepulsionInputError::PositionLengthMismatch {
            xs: xs.len(),
            ys: ys.len(),
        });
    }
    let dev = burn::tensor::Device::wgpu(burn::tensor::DeviceKind::DiscreteGpu(0));
    let (fx, fy) = repulsion(
        Tensor::<1>::from_floats(xs, &dev),
        Tensor::<1>::from_floats(ys, &dev),
        params,
    );
    Ok((
        fx.into_data().to_vec::<f32>().expect("fx readback"),
        fy.into_data().to_vec::<f32>().expect("fy readback"),
    ))
}

/// Runs the [`NodeExclusionParams`] law on Burn's default WGPU device and reads
/// the force vectors back for a CPU-owned integrator.
///
/// Prefer [`node_exclusion`] with resident tensors when the simulation itself is
/// GPU-owned. This function exists so a staging adapter cannot accidentally swap
/// the layout law for smooth all-pairs repulsion.
#[cfg(feature = "field-burn-wgpu")]
pub fn node_exclusion_wgpu_roundtrip(
    xs: &[f32],
    ys: &[f32],
    params: NodeExclusionParams,
) -> Result<(Vec<f32>, Vec<f32>), RepulsionInputError> {
    if xs.len() != ys.len() {
        return Err(RepulsionInputError::PositionLengthMismatch {
            xs: xs.len(),
            ys: ys.len(),
        });
    }
    let dev = burn::tensor::Device::wgpu(burn::tensor::DeviceKind::DiscreteGpu(0));
    let (fx, fy) = node_exclusion(
        Tensor::<1>::from_floats(xs, &dev),
        Tensor::<1>::from_floats(ys, &dev),
        params,
    );
    Ok((
        fx.into_data().to_vec::<f32>().expect("fx readback"),
        fy.into_data().to_vec::<f32>().expect("fy readback"),
    ))
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

/// Naive host anchor for [`node_exclusion`].
pub fn node_exclusion_reference(
    xs: &[f32],
    ys: &[f32],
    params: NodeExclusionParams,
) -> Result<(Vec<f32>, Vec<f32>), RepulsionInputError> {
    if xs.len() != ys.len() {
        return Err(RepulsionInputError::PositionLengthMismatch {
            xs: xs.len(),
            ys: ys.len(),
        });
    }
    let mut fx = vec![0.0f32; xs.len()];
    let mut fy = vec![0.0f32; xs.len()];
    let cutoff_sq = params.cutoff * params.cutoff;
    for i in 0..xs.len() {
        for j in 0..xs.len() {
            if i == j {
                continue;
            }
            let dx = xs[i] - xs[j];
            let dy = ys[i] - ys[j];
            let distance_sq = dx * dx + dy * dy;
            if distance_sq > cutoff_sq {
                continue;
            }
            let distance = distance_sq.sqrt().max(params.min_distance);
            let scale = params.strength / (distance * distance * distance);
            fx[i] += dx * scale;
            fy[i] += dy * scale;
        }
    }
    Ok((fx, fy))
}

// Every test in here drives the burn lane; the burn-free anchor they
// compare against is exercised by the resident lane's receipt in
// `tests/resident.rs`, which needs no burn at all.
#[cfg(all(test, feature = "field-burn"))]
mod tests {
    use super::*;

    // backend chosen per call site via Device

    fn run(xs: &[f32], ys: &[f32], p: RepulsionParams) -> (Vec<f32>, Vec<f32>) {
        let dev = burn::tensor::Device::ndarray();
        let (fx, fy) = repulsion(
            Tensor::<1>::from_floats(xs, &dev),
            Tensor::<1>::from_floats(ys, &dev),
            p,
        );
        (
            fx.into_data().to_vec::<f32>().unwrap(),
            fy.into_data().to_vec::<f32>().unwrap(),
        )
    }

    fn run_node_exclusion(
        xs: &[f32],
        ys: &[f32],
        p: NodeExclusionParams,
    ) -> (Vec<f32>, Vec<f32>) {
        let dev = burn::tensor::Device::ndarray();
        let (fx, fy) = node_exclusion(
            Tensor::<1>::from_floats(xs, &dev),
            Tensor::<1>::from_floats(ys, &dev),
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
        let (bx, by) = run(&xs, &ys, p);
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
        let (fx, fy) = run(&[0.0, 1.0], &[0.0, 0.0], RepulsionParams::default());
        assert!(fx[0] < 0.0, "left body pushed left: {}", fx[0]);
        assert!(fx[1] > 0.0, "right body pushed right: {}", fx[1]);
        assert!((fx[0] + fx[1]).abs() < 1.0e-5, "equal and opposite");
        assert!(fy[0].abs() < 1.0e-5 && fy[1].abs() < 1.0e-5, "no y force");
    }

    #[test]
    fn self_term_contributes_zero() {
        // One body feels no force from itself.
        let (fx, fy) = run(&[2.0], &[3.0], RepulsionParams::default());
        assert!(fx[0].abs() < 1.0e-6 && fy[0].abs() < 1.0e-6);
    }

    #[test]
    fn node_exclusion_matches_its_hard_floor_and_cutoff_anchor() {
        // Includes a sub-floor pair and a pair beyond the cutoff. These are
        // exactly the cases smooth all-pairs repulsion cannot stand in for.
        let xs = [0.0f32, 2.0, 9.0, 30.0];
        let ys = [0.0f32, 0.0, 0.0, 0.0];
        let params = NodeExclusionParams {
            strength: 64.0,
            cutoff: 10.0,
            min_distance: 4.0,
        };
        let (bx, by) = run_node_exclusion(&xs, &ys, params);
        let (rx, ry) = node_exclusion_reference(&xs, &ys, params).unwrap();
        for (actual, expected) in bx.iter().zip(&rx) {
            assert!(
                (actual - expected).abs() < 1.0e-4,
                "fx {actual} vs {expected}"
            );
        }
        for (actual, expected) in by.iter().zip(&ry) {
            assert!(
                (actual - expected).abs() < 1.0e-4,
                "fy {actual} vs {expected}"
            );
        }
    }

    #[test]
    fn node_exclusion_rejects_mismatched_position_components() {
        assert!(matches!(
            node_exclusion_reference(&[0.0], &[0.0, 1.0], NodeExclusionParams::default()),
            Err(RepulsionInputError::PositionLengthMismatch { xs: 1, ys: 2 })
        ));
    }
}

#[cfg(all(test, feature = "field-burn-wgpu"))]
mod tests_wgpu {
    use super::*;


    /// Deterministic spread of N positions; no RNG (varies by index).
    fn positions(n: usize) -> (Vec<f32>, Vec<f32>) {
        (0..n)
            .map(|i| {
                let t = i as f32;
                ((t * 0.6180339).sin() * 100.0, (t * 0.7548776).cos() * 100.0)
            })
            .unzip()
    }

    fn run(xs: &[f32], ys: &[f32], dev: &burn::tensor::Device) -> (Vec<f32>, Vec<f32>) {
        let dev = dev.clone();
        let (fx, fy) = repulsion(
            Tensor::<1>::from_floats(xs, &dev),
            Tensor::<1>::from_floats(ys, &dev),
            RepulsionParams::default(),
        );
        (
            fx.into_data().to_vec::<f32>().unwrap(),
            fy.into_data().to_vec::<f32>().unwrap(),
        )
    }

    fn run_node_exclusion(
        xs: &[f32],
        ys: &[f32],
        params: NodeExclusionParams,
        dev_param: &burn::tensor::Device,
    ) -> (Vec<f32>, Vec<f32>) {
        let dev = dev_param.clone();
        let (fx, fy) = node_exclusion(
            Tensor::<1>::from_floats(xs, &dev),
            Tensor::<1>::from_floats(ys, &dev),
            params,
        );
        (
            fx.into_data().to_vec::<f32>().unwrap(),
            fy.into_data().to_vec::<f32>().unwrap(),
        )
    }

    #[test]
    fn parity_ndarray_wgpu() {
        let (xs, ys) = positions(257);
        let (cx, cy) = run(&xs, &ys, &burn::tensor::Device::ndarray());
        let (gx, gy) = run(&xs, &ys, &burn::tensor::Device::wgpu(burn::tensor::DeviceKind::DiscreteGpu(0)));
        let max = |a: &[f32], b: &[f32]| {
            a.iter()
                .zip(b)
                .map(|(x, y)| (x - y).abs())
                .fold(0.0f32, f32::max)
        };
        assert!(max(&cx, &gx) < 1.0e-3, "fx diverged");
        assert!(max(&cy, &gy) < 1.0e-3, "fy diverged");
    }

    #[test]
    fn node_exclusion_parity_ndarray_wgpu() {
        let (xs, ys) = positions(257);
        let params = NodeExclusionParams {
            strength: 220_000.0,
            cutoff: 100.0,
            min_distance: 8.0,
        };
        let (cx, cy) = run_node_exclusion(&xs, &ys, params, &burn::tensor::Device::ndarray());
        let (gx, gy) = run_node_exclusion(&xs, &ys, params, &burn::tensor::Device::wgpu(burn::tensor::DeviceKind::DiscreteGpu(0)));
        let max_relative = |a: &[f32], b: &[f32]| {
            a.iter()
                .zip(b)
                .map(|(x, y)| (x - y).abs() / x.abs().max(y.abs()).max(1.0))
                .fold(0.0f32, f32::max)
        };
        let x_error = max_relative(&cx, &gx);
        let y_error = max_relative(&cy, &gy);
        // Pairwise reductions use a different order on the GPU. At this force
        // scale, the observed drift is below 0.1 percent while the cutoff and
        // hard-floor branches agree exactly.
        assert!(x_error < 1.0e-3, "fx relative error too large: {x_error:e}");
        assert!(y_error < 1.0e-3, "fy relative error too large: {y_error:e}");
    }

    /// CPU-vs-GPU staging across bounded N, readback included and GPU warmed.
    /// This measures only the smooth all-pairs helper, not a resident or
    /// Barnes–Hut crossover. The cap prevents an ignored receipt from allocating
    /// multi-gibibyte pairwise intermediates. Run:
    /// `cargo test -p quint --features field-burn-wgpu --release -- --ignored timing --nocapture`
    #[test]
    #[ignore]
    fn timing_repulsion_cpu_vs_gpu() {
        for n in [256usize, 1_000, 4_000] {
            let (xs, ys) = positions(n);
            let gpu_dev = burn::tensor::Device::wgpu(burn::tensor::DeviceKind::DiscreteGpu(0));
            let _warm = run(&xs, &ys, &gpu_dev);

            let t = std::time::Instant::now();
            let _ = run(&xs, &ys, &burn::tensor::Device::ndarray());
            let cpu_us = t.elapsed().as_micros();

            let t = std::time::Instant::now();
            let _ = run(&xs, &ys, &gpu_dev);
            let gpu_us = t.elapsed().as_micros();

            println!("n={n}: ndarray={cpu_us}us wgpu={gpu_us}us");
        }
    }
}
