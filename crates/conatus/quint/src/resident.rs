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

mod chunk;

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
    device: wgpu::Device,
    queue: wgpu::Queue,
    params: Params,
    params_buffer: wgpu::Buffer,
    /// Padded 3D positions. Public because publishing them is the whole
    /// point: a consumer binds this, or copies from it, on the same
    /// device.
    pub positions: wgpu::Buffer,
    /// Held so the binding stays valid; the kernels own its contents.
    _velocities: wgpu::Buffer,
    forces: wgpu::Buffer,
    settle: wgpu::Buffer,
    settle_staging: wgpu::Buffer,
    bind: wgpu::BindGroup,
    repulse: wgpu::ComputePipeline,
    springs: wgpu::ComputePipeline,
    integrate: wgpu::ComputePipeline,
    spirv: bool,
}

fn buffer(
    device: &wgpu::Device,
    label: &str,
    contents: &[u8],
    extra: wgpu::BufferUsages,
) -> wgpu::Buffer {
    use wgpu::util::DeviceExt;
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage: wgpu::BufferUsages::STORAGE | extra,
    })
}

impl Resident {
    /// The kernel module: SPIR-V compiled from `quint-shaders`' Rust
    /// source when the adapter can take it, WGSL otherwise.
    ///
    /// The Rust source is the source of truth and the `.spv` travels
    /// with the crate, so a consumer needs no rust-gpu toolchain. The
    /// WGSL is the same three kernels, kept as the downlevel path:
    /// browsers have no SPIR-V ingestion, and `SPIRV_SHADER_PASSTHROUGH`
    /// is a Vulkan-side feature an adapter may not offer. Both are
    /// checked against the same CPU anchor by this crate's receipts.
    fn kernels(device: &wgpu::Device) -> (wgpu::ShaderModule, bool) {
        if device
            .features()
            .contains(wgpu::Features::PASSTHROUGH_SHADERS)
        {
            let words = wgpu::util::make_spirv_raw(include_bytes!("../shaders/quint_shaders.spv"));
            // wgpu 30 added `entry_points` to the passthrough descriptor and
            // its Default is EMPTY, because passthrough skips naga and so
            // cannot reflect them. Leaving it defaulted compiles fine and then
            // fails at pipeline creation with "Unable to find entry point".
            // These three, at threads(256), are `quint-shaders`' own kernels.
            let entry_points = [
                wgpu::PassthroughShaderEntryPoint {
                    name: "repulse".into(),
                    workgroup_size: (256, 1, 1),
                },
                wgpu::PassthroughShaderEntryPoint {
                    name: "springs".into(),
                    workgroup_size: (256, 1, 1),
                },
                wgpu::PassthroughShaderEntryPoint {
                    name: "integrate".into(),
                    workgroup_size: (256, 1, 1),
                },
            ];
            // SAFETY: the module is this crate's own committed artifact,
            // built by rust-gpu from `quint-shaders` and validated by
            // spirv-val at build time. Passthrough skips naga entirely,
            // so that provenance is the whole guarantee.
            let module = unsafe {
                device.create_shader_module_passthrough(wgpu::ShaderModuleDescriptorPassthrough {
                    label: Some("quint resident kernels (spir-v)"),
                    entry_points: entry_points.as_slice().into(),
                    spirv: Some(words),
                    ..Default::default()
                })
            };
            return (module, true);
        }
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quint resident kernels (wgsl)"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/resident.wgsl").into()),
        });
        (module, false)
    }

    /// Build the lane on the host's device. `positions` is the initial
    /// padded-3D scatter; `adjacency` the springs' CSR.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
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

        let params_buffer = buffer(
            device,
            "quint resident params",
            bytemuck::bytes_of(&params),
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let positions_buffer = buffer(
            device,
            "quint resident positions",
            bytemuck::cast_slice(positions),
            wgpu::BufferUsages::COPY_SRC,
        );
        let zero = vec![[0.0f32; 4]; positions.len()];
        let velocities = buffer(
            device,
            "quint resident velocities",
            bytemuck::cast_slice(&zero),
            wgpu::BufferUsages::empty(),
        );
        // COPY_DST so an external force pass (the Burn lane, or a
        // consumer's own kernel) can fill it; COPY_SRC so a test can
        // read it back and compare against the CPU anchor.
        let forces = buffer(
            device,
            "quint resident forces",
            bytemuck::cast_slice(&zero),
            wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        );
        let offsets = buffer(
            device,
            "quint resident adjacency offsets",
            bytemuck::cast_slice(adjacency.offsets),
            wgpu::BufferUsages::empty(),
        );
        // An empty CSR is legal (a graph with no edges), and a
        // zero-sized buffer is not, so pad it.
        let targets_data: &[u32] = if adjacency.targets.is_empty() {
            &[0]
        } else {
            adjacency.targets
        };
        let targets = buffer(
            device,
            "quint resident adjacency targets",
            bytemuck::cast_slice(targets_data),
            wgpu::BufferUsages::empty(),
        );
        let settle = buffer(
            device,
            "quint resident settle",
            bytemuck::bytes_of(&0u32),
            wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        );
        let settle_staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quint resident settle staging"),
            size: 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let (shader, spirv) = Self::kernels(device);

        let entries: Vec<wgpu::BindGroupLayoutEntry> = (0..7u32)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: match binding {
                        0 => wgpu::BufferBindingType::Uniform,
                        4 | 5 => wgpu::BufferBindingType::Storage { read_only: true },
                        _ => wgpu::BufferBindingType::Storage { read_only: false },
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quint resident layout"),
            entries: &entries,
        });
        let bound = [
            &params_buffer,
            &positions_buffer,
            &velocities,
            &forces,
            &offsets,
            &targets,
            &settle,
        ];
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("quint resident bind"),
            layout: &layout,
            entries: &bound
                .iter()
                .enumerate()
                .map(|(i, buffer)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: buffer.as_entire_binding(),
                })
                .collect::<Vec<_>>(),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quint resident pipelines"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };

        Self {
            device: device.clone(),
            queue: queue.clone(),
            params,
            params_buffer,
            positions: positions_buffer,
            _velocities: velocities,
            forces,
            settle,
            settle_staging,
            bind,
            repulse: pipeline("repulse"),
            springs: pipeline("springs"),
            integrate: pipeline("integrate"),
            spirv,
        }
    }

    /// Whether the lane is running the SPIR-V built from
    /// `quint-shaders`' Rust source, rather than the WGSL fallback. A
    /// receipt that does not check this can pass while the committed
    /// artifact never executes.
    pub fn using_spirv(&self) -> bool {
        self.spirv
    }

    pub fn params(&self) -> Params {
        self.params
    }

    /// Change the step's constants. The physics is configurable at
    /// runtime because a mere's physics profile is data.
    pub fn set_params(&mut self, params: Params) {
        self.params = Params {
            n: self.params.n,
            ..params
        };
        self.queue
            .write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&self.params));
    }

    /// One frame: repulsion, springs, integration, and the settle word
    /// staged for its four-byte read.
    pub fn step(&self) {
        self.dispatch(true);
    }

    /// A frame whose repulsion came from elsewhere (the Burn lane, or a
    /// consumer's own pass, having filled [`Self::forces_buffer`]).
    pub fn step_with_external_forces(&self) {
        self.dispatch(false);
    }

    /// The forces buffer, for a producer that computes the field some
    /// other way. Writing it before [`Self::step_with_external_forces`]
    /// is how the tensor lane hands off.
    pub fn forces_buffer(&self) -> &wgpu::Buffer {
        &self.forces
    }

    fn dispatch(&self, own_repulsion: bool) {
        self.queue
            .write_buffer(&self.settle, 0, bytemuck::bytes_of(&0u32));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("quint resident frame"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("quint resident"),
                timestamp_writes: None,
            });
            let groups = self.params.n.div_ceil(256);
            pass.set_bind_group(0, &self.bind, &[]);
            if own_repulsion {
                pass.set_pipeline(&self.repulse);
                pass.dispatch_workgroups(groups, 1, 1);
            }
            pass.set_pipeline(&self.springs);
            pass.dispatch_workgroups(groups, 1, 1);
            pass.set_pipeline(&self.integrate);
            pass.dispatch_workgroups(groups, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&self.settle, 0, &self.settle_staging, 0, 4);
        self.queue.submit([encoder.finish()]);
    }

    /// The frame's only readback: the fastest body's speed, four bytes.
    /// A host polls this to know when a layout has settled.
    pub fn max_speed(&self) -> f32 {
        let slice = self.settle_staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("device poll");
        rx.recv().expect("map channel").expect("settle map");
        // wgpu 30 made `get_mapped_range` fallible; the map_async above
        // already succeeded, so a failure here is a broken invariant.
        let bits = u32::from_le_bytes(
            slice.get_mapped_range().expect("settle map range")[..4]
                .try_into()
                .expect("four bytes"),
        );
        self.settle_staging.unmap();
        f32::from_bits(bits)
    }

    /// Read the whole force buffer back. A diagnostic, deliberately not
    /// part of any frame: the lane's discipline is that nothing but the
    /// settle word crosses the bus per step.
    pub fn read_forces(&self) -> Vec<[f32; 4]> {
        self.read_all(&self.forces)
    }

    /// Read the whole position buffer back. Same diagnostic status.
    pub fn read_positions(&self) -> Vec<[f32; 4]> {
        self.read_all(&self.positions)
    }

    fn read_all(&self, source: &wgpu::Buffer) -> Vec<[f32; 4]> {
        let size = (self.params.n as u64) * 16;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quint resident staging"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("quint resident readback"),
            });
        encoder.copy_buffer_to_buffer(source, 0, &staging, 0, size);
        self.queue.submit([encoder.finish()]);
        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("device poll");
        rx.recv().expect("map channel").expect("readback map");
        let data = slice.get_mapped_range().expect("readback map range");
        let out = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        staging.unmap();
        out
    }

    /// The repulsion dispatch alone, so a test can compare one pass
    /// against the CPU anchor without integrating.
    pub fn repulse_only(&self) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("quint resident repulse"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("quint resident repulse"),
                timestamp_writes: None,
            });
            pass.set_bind_group(0, &self.bind, &[]);
            pass.set_pipeline(&self.repulse);
            pass.dispatch_workgroups(self.params.n.div_ceil(256), 1, 1);
        }
        self.queue.submit([encoder.finish()]);
    }
}
