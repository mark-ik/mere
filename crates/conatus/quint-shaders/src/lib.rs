//! Conatus's resident field lane, as explicit GPU programs.
//!
//! Three kernels advance a padded-3D position buffer that never leaves
//! the device: repulsion between every pair, springs along an adjacency
//! list, and a damped integration step that also reports whether the
//! system has settled. They are written in Rust and compiled to SPIR-V
//! rather than authored in WGSL, so the force law reads the same way it
//! would on the CPU and the same source can be checked by eye against
//! `quint::forces`.
#![no_std]

use spirv_std::glam::{Vec3, Vec4, Vec4Swizzles};
use spirv_std::spirv;

/// What the kernels need to know about this step. Mirrors
/// `quint::resident::Params` word for word; the two are checked against
/// each other by a size assertion on the host side.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Params {
    pub n: u32,
    pub dt: f32,
    pub damping: f32,
    pub repulsion: f32,
    pub min_distance: f32,
    pub spring_k: f32,
    pub rest_length: f32,
    pub centering: f32,
}

/// Softened inverse-square repulsion, every pair.
///
/// The same law as `quint::forces::repulsion`: the self term vanishes
/// because its displacement is zero, so no diagonal mask is needed.
#[spirv(compute(threads(256)))]
pub fn repulse(
    #[spirv(global_invocation_id)] id: spirv_std::glam::UVec3,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &Params,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] positions: &[Vec4],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] forces: &mut [Vec4],
) {
    let i = id.x as usize;
    if i >= params.n as usize {
        return;
    }
    let mine = positions[i].xyz();
    let softening = params.min_distance * params.min_distance;
    let mut force = Vec3::ZERO;
    let mut j = 0usize;
    while j < params.n as usize {
        let d = mine - positions[j].xyz();
        let d2 = d.dot(d) + softening;
        force += d * (params.repulsion / (d2 * libm_sqrt(d2)));
        j += 1;
    }
    forces[i] = Vec4::new(force.x, force.y, force.z, 0.0);
}

/// Spring forces gathered along CSR adjacency.
///
/// A gather rather than a scatter: every write is owned by exactly one
/// invocation, which is what lets this run without float atomics.
#[spirv(compute(threads(256)))]
pub fn springs(
    #[spirv(global_invocation_id)] id: spirv_std::glam::UVec3,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &Params,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] positions: &[Vec4],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] forces: &mut [Vec4],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] offsets: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] targets: &[u32],
) {
    let i = id.x as usize;
    if i >= params.n as usize {
        return;
    }
    let p = positions[i].xyz();
    let mut f = forces[i].xyz();
    let mut e = offsets[i] as usize;
    let end = offsets[i + 1] as usize;
    while e < end {
        let q = positions[targets[e] as usize].xyz();
        let d = q - p;
        let length = libm_sqrt(d.dot(d)).max(1.0e-4);
        f += d * (params.spring_k * (length - params.rest_length) / length);
        e += 1;
    }
    forces[i] = Vec4::new(f.x, f.y, f.z, 0.0);
}

/// Damped symplectic Euler with a weak centering, plus the settle
/// reduction.
///
/// Speed is bitcast to `u32` for the atomic maximum: the bit pattern of
/// a non-negative float orders the same way the float does, so one
/// `atomic_u_max` is the whole convergence probe.
#[spirv(compute(threads(256)))]
pub fn integrate(
    #[spirv(global_invocation_id)] id: spirv_std::glam::UVec3,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] params: &Params,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] positions: &mut [Vec4],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] velocities: &mut [Vec4],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] forces: &[Vec4],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 6)] settle: &mut [u32],
) {
    let i = id.x as usize;
    if i >= params.n as usize {
        return;
    }
    let p = positions[i].xyz();
    let mut v = velocities[i].xyz();
    let f = forces[i].xyz() - p * params.centering;

    v = (v + f * params.dt) * params.damping;
    let np = p + v * params.dt;

    positions[i] = Vec4::new(np.x, np.y, np.z, positions[i].w);
    velocities[i] = Vec4::new(v.x, v.y, v.z, 0.0);

    let speed = libm_sqrt(v.dot(v));
    unsafe {
        // QueueFamily scope, not Device: everything that touches this
        // word is in one dispatch on one queue, and Device scope under
        // the Vulkan memory model needs a capability
        // (VulkanMemoryModelDeviceScopeKHR) this target does not
        // declare.
        spirv_std::arch::atomic_u_max::<
            u32,
            { spirv_std::memory::Scope::QueueFamily as u32 },
            { spirv_std::memory::Semantics::NONE.bits() },
        >(&mut settle[0], speed.to_bits());
    }
}

/// `f32::sqrt` is not in core on the SPIR-V target. spirv-std re-exports
/// `num_traits`, whose `Float` is implemented for the GPU float types
/// and lowers to the native instruction.
fn libm_sqrt(x: f32) -> f32 {
    spirv_std::num_traits::Float::sqrt(x)
}
