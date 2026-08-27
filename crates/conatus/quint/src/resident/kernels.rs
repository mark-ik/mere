// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The resident lane's kernels, authored in CubeCL.
//!
//! The three dispatches the field tier advances by (repulsion, springs,
//! integration) plus the settle reduction, written once here and
//! compiled by CubeCL to whatever the adapter takes. This is the engine
//! composition ruling's authored lane: our kernels are CubeCL, and the
//! buffers they run over are the same CubeCL allocations Burn
//! addresses, so a tensor pass and a kernel pass meet with no bridge.
//!
//! The force laws are the ones `quint::forces` states on the CPU, and
//! `forces::repulsion_reference` remains the anchor they are checked
//! against.
//!
//! Positions and velocities are padded 3D throughout, per the spatial
//! compute plan: four floats per body, xyz meaningful, w spare, and a
//! 2D canvas is a constrained case of the same layout rather than a
//! second format. The padding is addressed by stride rather than by a
//! vector type, since CubeCL 0.10 exposes no `vec4` for buffer
//! elements; the bytes on the wire are identical either way.
//!
//! Dialect notes, earned by the seed crystal and paid for in bisection:
//! the `cube` macro cannot expand `==` on floats, and a `let mut` local
//! that is later compared needs its type written, because the
//! comparison expansion erases the back-inference arithmetic keeps.

use cubecl::prelude::*;

/// Threads per cube, and the tile width the repulsion pass stages
/// through shared memory.
pub const CUBE_DIM: u32 = 256;
/// Floats per body: padded 3D.
pub const STRIDE: u32 = 4;

/// Softened inverse-square repulsion over every pair, tiled through
/// shared memory.
///
/// The self term contributes nothing because its displacement is zero,
/// so no diagonal mask is needed. Threads past `n` still load and
/// barrier: a cube that diverges at a barrier is undefined behaviour.
#[cube(launch_unchecked)]
pub fn repulse(
    positions: &[f32],
    forces: &mut [f32],
    n: u32,
    repulsion: f32,
    min_distance: f32,
    #[comptime] tile_floats: usize,
) {
    // Shared-memory extent is comptime, as is the stride: both are
    // shapes the compiler needs before it can lay the cube out.
    let mut tile = Shared::<[f32]>::new_slice(tile_floats);
    let stride = STRIDE as usize;
    let width = CUBE_DIM as usize;
    let count_n = n as usize;
    let i = ABSOLUTE_POS;
    let unit = UNIT_POS as usize;
    let last = count_n - 1;
    let mut mine = i;
    if mine > last {
        mine = last;
    }
    let base_i = mine * stride;
    let px = positions[base_i];
    let py = positions[base_i + 1];
    let pz = positions[base_i + 2];
    let softening = min_distance * min_distance;

    let mut fx = 0.0f32;
    let mut fy = 0.0f32;
    let mut fz = 0.0f32;
    let tiles = count_n.div_ceil(width);
    let mut t = 0usize;
    while t < tiles {
        let mut j = t * width + unit;
        if j > last {
            j = last;
        }
        let src = j * stride;
        let dst = unit * stride;
        tile[dst] = positions[src];
        tile[dst + 1] = positions[src + 1];
        tile[dst + 2] = positions[src + 2];
        sync_cube();

        let start = t * width;
        let mut count = width;
        if count_n - start < width {
            count = count_n - start;
        }
        let mut k = 0usize;
        while k < count {
            let other = k * stride;
            let dx = px - tile[other];
            let dy = py - tile[other + 1];
            let dz = pz - tile[other + 2];
            let d2 = dx * dx + dy * dy + dz * dz + softening;
            let inv = repulsion / (d2 * f32::sqrt(d2));
            fx += dx * inv;
            fy += dy * inv;
            fz += dz * inv;
            k += 1;
        }
        sync_cube();
        t += 1;
    }

    if i < count_n {
        let out = i * stride;
        forces[out] = fx;
        forces[out + 1] = fy;
        forces[out + 2] = fz;
        forces[out + 3] = 0.0f32;
    }
}

/// Spring forces gathered along CSR adjacency.
///
/// Every write is owned by one invocation, so no float atomics are
/// needed (and WGSL has none).
#[cube(launch_unchecked)]
pub fn springs(
    positions: &[f32],
    forces: &mut [f32],
    offsets: &[u32],
    targets: &[u32],
    n: u32,
    spring_k: f32,
    rest_length: f32,
) {
    let stride = STRIDE as usize;
    let i = ABSOLUTE_POS;
    if i < n as usize {
        let base = i * stride;
        let px = positions[base];
        let py = positions[base + 1];
        let pz = positions[base + 2];
        let mut fx = forces[base];
        let mut fy = forces[base + 1];
        let mut fz = forces[base + 2];

        let mut e = offsets[i] as usize;
        let end = offsets[i + 1] as usize;
        while e < end {
            let target = targets[e] as usize * stride;
            let dx = positions[target] - px;
            let dy = positions[target + 1] - py;
            let dz = positions[target + 2] - pz;
            let mut len = f32::sqrt(dx * dx + dy * dy + dz * dz);
            if len < 1.0e-4f32 {
                len = 1.0e-4f32;
            }
            let scale = spring_k * (len - rest_length) / len;
            fx += dx * scale;
            fy += dy * scale;
            fz += dz * scale;
            e += 1;
        }
        forces[base] = fx;
        forces[base + 1] = fy;
        forces[base + 2] = fz;
    }
}

/// Damped symplectic Euler, a weak centering, and the settle reduction.
///
/// Speed is reinterpreted as `u32` for the atomic maximum: the bit
/// pattern of a non-negative float orders the same way the float does,
/// so one atomic max over the whole grid is the entire convergence
/// probe, and the host reads four bytes rather than the cloud.
#[cube(launch_unchecked)]
pub fn integrate(
    positions: &mut [f32],
    velocities: &mut [f32],
    forces: &[f32],
    settle: &mut [Atomic<u32>],
    n: u32,
    dt: f32,
    damping: f32,
    centering: f32,
) {
    let stride = STRIDE as usize;
    let i = ABSOLUTE_POS;
    if i < n as usize {
        let base = i * stride;
        let px = positions[base];
        let py = positions[base + 1];
        let pz = positions[base + 2];

        let ax = forces[base] - px * centering;
        let ay = forces[base + 1] - py * centering;
        let az = forces[base + 2] - pz * centering;
        let vx = (velocities[base] + ax * dt) * damping;
        let vy = (velocities[base + 1] + ay * dt) * damping;
        let vz = (velocities[base + 2] + az * dt) * damping;

        positions[base] = px + vx * dt;
        positions[base + 1] = py + vy * dt;
        positions[base + 2] = pz + vz * dt;
        velocities[base] = vx;
        velocities[base + 1] = vy;
        velocities[base + 2] = vz;
        velocities[base + 3] = 0.0f32;

        let speed = f32::sqrt(vx * vx + vy * vy + vz * vz);
        Atomic::fetch_max(&settle[0], u32::reinterpret(speed));
    }
}

/// Reset the settle word before a step, so each step's maximum is its
/// own rather than the running maximum of every step so far.
#[cube(launch_unchecked)]
pub fn clear_settle(settle: &mut [Atomic<u32>]) {
    if ABSOLUTE_POS < 1 {
        Atomic::store(&settle[0], 0u32);
    }
}
