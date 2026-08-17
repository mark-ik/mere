//! The resident field lane: positions that never come home.
//!
//! Promoted from the spatial compute plan's P2 and P3 probes
//! (`2026-08-13_spatial_compute_plan.md`). Positions and velocities
//! live in GPU buffers as padded 3D `vec4f`; three dispatches per frame
//! advance them; the only per-frame readback is a four-byte settle
//! word. A consumer that wants to draw them binds
//! [`crate::resident::Resident::positions`] directly, or, if its renderer addresses
//! memory some other way, publishes from it with a kernel of its own.
//!
//! # Which lane this is
//!
//! quint has two GPU lanes now, and they are not rivals:
//!
//! - the **tensor lane** ([`crate::forces::repulsion`], Burn), for
//!   dense field evaluation, semantic couplings, and anything that
//!   benefits from fusion;
//! - the **explicit lane**, here, for the resident n-body step, where
//!   exact memory control is the whole point. The tensor formulation
//!   materializes `[n, n]` intermediates and cannot reach the sizes a
//!   canvas needs; this one holds `O(n)`.
//!
//! # Device
//!
//! The device is the host's, never this crate's. A second device on the
//! same adapter cannot share a buffer with the renderer, which is the
//! failure the wing's tenancy seam exists to prevent, so
//! [`crate::resident::Resident::new`] takes handles rather than booting.

use bytemuck::{Pod, Zeroable};
use cubecl::client::ComputeClient;
use cubecl::prelude::*;
use cubecl::server::Handle;
use cubecl::wgpu::WgpuRuntime;

mod chunk;
pub mod kernels;

pub use chunk::*;

/// One step's constants. Mirrors `quint-shaders`'s `Params` word for
/// word; [`Resident::new`] asserts the sizes agree, so a field added on
/// one side and forgotten on the other fails at construction rather
/// than by reading garbage.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Params {
    pub n: u32,
    pub dt: f32,
    /// Velocity retained per step. Below 1 the system settles.
    pub damping: f32,
    /// Coulomb-like constant on the pairwise repulsion.
    pub repulsion: f32,
    /// Softening length: added in quadrature so coincident bodies stay
    /// finite, and the self term stays zero.
    pub min_distance: f32,
    pub spring_k: f32,
    pub rest_length: f32,
    /// A weak pull toward the origin, so a cloud with no boundary
    /// cannot drift away under accumulated float bias.
    pub centering: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            n: 0,
            dt: 1.0 / 60.0,
            damping: 0.85,
            repulsion: 4_000.0,
            min_distance: 4.0,
            spring_k: 0.08,
            rest_length: 30.0,
            centering: 0.05,
        }
    }
}

/// The adjacency a spring pass gathers over: CSR, `offsets` of length
/// `n + 1` indexing `targets`.
pub struct Adjacency<'a> {
    pub offsets: &'a [u32],
    pub targets: &'a [u32],
}

/// A resident simulation: buffers on the host's device, and the three
/// dispatches that advance them.
pub struct Resident {
    client: ComputeClient<WgpuRuntime>,
    params: Params,
    /// Padded 3D positions, allocated on the shared CubeCL client and
    /// published to consumers as a lease rather than a raw buffer, so a
    /// renderer reads the exact range and its stamp together.
    positions: Handle,
    velocities: Handle,
    forces: Handle,
    offsets: Handle,
    targets: Handle,
    settle: Handle,
    revision: u64,
    /// The resolved wgpu ranges behind `positions` and `forces`.
    /// Resolved once at construction rather than per call, because a
    /// lease borrows the buffer and a freshly resolved resource is a
    /// temporary. A handle's range does not move for its lifetime.
    positions_alloc: (wgpu::Buffer, u64, u64),
    forces_alloc: (wgpu::Buffer, u64, u64),
}

impl Resident {
    /// Build the lane on the host's client. `positions` is the initial
    /// padded-3D scatter; `adjacency` the springs' CSR.
    ///
    /// Every allocation is made through the caller's
    /// [`ResidentClient`], which is what lets Burn address these same
    /// buffers with no bridge: the tensor lane and this one share an
    /// allocator, not merely a device.
    pub fn new(
        client: &ResidentClient,
        positions: &[[f32; 4]],
        adjacency: Adjacency<'_>,
        params: Params,
    ) -> Self {
        let n = positions.len() as u32;
        assert_eq!(
            adjacency.offsets.len(),
            positions.len() + 1,
            "CSR offsets must be one longer than the node count"
        );
        let params = Params { n, ..params };
        let compute = client.compute_client().clone();

        let zero = vec![[0.0f32; 4]; positions.len()];
        let positions_handle = compute.create_from_slice(bytemuck::cast_slice(positions));
        let velocities = compute.create_from_slice(bytemuck::cast_slice(&zero));
        let forces = compute.create_from_slice(bytemuck::cast_slice(&zero));
        let offsets = compute.create_from_slice(bytemuck::cast_slice(adjacency.offsets));
        // An empty CSR still needs a bound allocation. One zero keeps
        // the binding valid without implying an edge, since every
        // offset pair is then empty.
        let targets = if adjacency.targets.is_empty() {
            compute.create_from_slice(bytemuck::cast_slice(&[0u32]))
        } else {
            compute.create_from_slice(bytemuck::cast_slice(adjacency.targets))
        };
        let settle = compute.create_from_slice(bytemuck::bytes_of(&0u32));

        let resolve = |handle: &Handle| {
            let managed = compute
                .get_resource(handle.clone())
                .expect("resident allocation");
            let resource = managed.resource();
            (resource.buffer.clone(), resource.offset, resource.size)
        };
        let positions_alloc = resolve(&positions_handle);
        let forces_alloc = resolve(&forces);

        Self {
            client: compute,
            params,
            positions: positions_handle,
            velocities,
            forces,
            offsets,
            targets,
            settle,
            revision: 0,
            positions_alloc,
            forces_alloc,
        }
    }

    pub fn params(&self) -> Params {
        self.params
    }

    /// Change the step's constants. The physics is configurable at
    /// runtime because a mere's physics profile is data.
    ///
    /// Constants ride as kernel arguments rather than in a uniform
    /// buffer, so this is a plain field write with nothing to upload.
    pub fn set_params(&mut self, params: Params) {
        self.params = Params {
            n: self.params.n,
            ..params
        };
    }

    /// How many times this lane has advanced. A consumer stamps its
    /// lease with this, so a reader on another cadence can tell a
    /// refreshed field from a stale one.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// One frame: the settle word cleared, then repulsion, springs, and
    /// integration.
    pub fn step(&mut self) {
        self.dispatch(true);
    }

    /// A frame whose repulsion came from elsewhere (the tensor lane, or
    /// a consumer's own pass, having filled the forces allocation).
    pub fn step_with_external_forces(&mut self) {
        self.dispatch(false);
    }

    /// The repulsion dispatch alone, so a test can compare one pass
    /// against the CPU anchor without integrating.
    pub fn repulse_only(&self) {
        let cubes = self.params.n.div_ceil(kernels::CUBE_DIM).max(1);
        let count = self.params.n as usize;
        unsafe {
            kernels::repulse::launch_unchecked::<WgpuRuntime>(
                &self.client,
                CubeCount::Static(cubes, 1, 1),
                CubeDim::new_1d(kernels::CUBE_DIM),
                ArrayArg::from_raw_parts(self.positions.clone(), count * 4),
                ArrayArg::from_raw_parts(self.forces.clone(), count * 4),
                self.params.n,
                self.params.repulsion,
                self.params.min_distance,
                (kernels::CUBE_DIM * kernels::STRIDE) as usize,
            );
        }
    }

    fn dispatch(&mut self, own_repulsion: bool) {
        let cubes = self.params.n.div_ceil(kernels::CUBE_DIM).max(1);
        let count = self.params.n as usize;
        let dim = CubeDim::new_1d(kernels::CUBE_DIM);
        unsafe {
            kernels::clear_settle::launch_unchecked::<WgpuRuntime>(
                &self.client,
                CubeCount::Static(1, 1, 1),
                CubeDim::new_1d(1),
                ArrayArg::from_raw_parts(self.settle.clone(), 1),
            );
        }
        if own_repulsion {
            self.repulse_only();
        }
        unsafe {
            kernels::springs::launch_unchecked::<WgpuRuntime>(
                &self.client,
                CubeCount::Static(cubes, 1, 1),
                dim,
                ArrayArg::from_raw_parts(self.positions.clone(), count * 4),
                ArrayArg::from_raw_parts(self.forces.clone(), count * 4),
                ArrayArg::from_raw_parts(self.offsets.clone(), count + 1),
                ArrayArg::from_raw_parts(self.targets.clone(), 1),
                self.params.n,
                self.params.spring_k,
                self.params.rest_length,
            );
            kernels::integrate::launch_unchecked::<WgpuRuntime>(
                &self.client,
                CubeCount::Static(cubes, 1, 1),
                dim,
                ArrayArg::from_raw_parts(self.positions.clone(), count * 4),
                ArrayArg::from_raw_parts(self.velocities.clone(), count * 4),
                ArrayArg::from_raw_parts(self.forces.clone(), count * 4),
                ArrayArg::from_raw_parts(self.settle.clone(), 1),
                self.params.n,
                self.params.dt,
                self.params.damping,
                self.params.centering,
            );
        }
        self.revision += 1;
    }

    /// The frame's only readback: the fastest body's speed, four bytes.
    /// A host polls this to know when a layout has settled.
    pub fn max_speed(&self) -> f32 {
        let bytes = self
            .client
            .read_one(self.settle.clone())
            .expect("settle readback");
        f32::from_bits(u32::from_le_bytes(
            bytes[..4].try_into().expect("four bytes"),
        ))
    }

    /// Diagnostic reads of the whole field. Not the per-frame path: the
    /// steady loop reads four bytes, and these exist so a test can
    /// compare against the CPU anchor.
    pub fn read_positions(&self) -> Vec<[f32; 4]> {
        self.read_all(&self.positions)
    }

    pub fn read_forces(&self) -> Vec<[f32; 4]> {
        self.read_all(&self.forces)
    }

    fn read_all(&self, handle: &Handle) -> Vec<[f32; 4]> {
        let bytes = self.client.read_one(handle.clone()).expect("field readback");
        bytemuck::cast_slice(&bytes[..(self.params.n as usize) * 16]).to_vec()
    }

    /// The positions, published for a consumer to draw from.
    ///
    /// A lease rather than a raw buffer: the range, the shape it holds,
    /// and the revision it was advanced to travel together, so a reader
    /// on another cadence cannot bind a stale or ill-fitting view. The
    /// chunk bundles publish the same contract, which is the point: the
    /// field tier and the voxel tier say one thing to their consumers.
    pub fn positions_lease(&self) -> SpatialLease<'_> {
        self.lease_of(&self.positions_alloc)
    }

    /// The forces, for a producer that computes the field some other
    /// way. Filling this before [`Self::step_with_external_forces`] is
    /// how the tensor lane hands off, and that handoff is now a shared
    /// allocation rather than a copy.
    pub fn forces_lease(&self) -> SpatialLease<'_> {
        self.lease_of(&self.forces_alloc)
    }

    fn lease_of<'a>(&'a self, alloc: &'a (wgpu::Buffer, u64, u64)) -> SpatialLease<'a> {
        SpatialLease {
            buffer: &alloc.0,
            offset: alloc.1,
            size: alloc.2,
            shape: [self.params.n as usize, 4, 1],
            element_type: PlaneElementType::F32,
            stamp: ChunkStamp {
                revision: self.revision,
                valid_read_epoch: ReadEpoch::new(self.revision),
            },
        }
    }
}
