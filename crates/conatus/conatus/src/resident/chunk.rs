// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Shared Conatus resident chunk planes and their zero-copy consumer views.
//!
//! Allocation has one direction here: [`ResidentClient`] allocates each
//! plane through CubeCL, then [`ResidentChunk`] lends Burn and raw wgpu
//! arrangements of that allocation. Neither view imports or copies the
//! plane, and this module deliberately offers no CPU whole-plane read.

use std::{collections::BTreeMap, fmt, mem::size_of_val, num::NonZeroU64};

use burn::tensor::{DType, Shape, Tensor};
use burn_wgpu::{
    CubeTensor, RuntimeOptions, Wgpu, WgpuDevice, WgpuRuntime, WgpuSetup, init_device,
};
use bytemuck::Pod;
use cubecl::{Runtime, client::ComputeClient, server::Handle};

/// Burn's view of one resident `f32` channel plane.
pub type ResidentTensor = Tensor<3>;

/// The host schedule epoch in which a materialized chunk is safe to read.
///
/// This is intentionally a host-issued number, not a frame counter hidden
/// inside Conatus. The shared device scheduler decides when a submitted write
/// becomes visible to its reader tenants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReadEpoch(u64);

impl ReadEpoch {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Integer world bounds for a resident voxel chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkBounds {
    pub origin: [i64; 3],
    pub extent: [u32; 3],
}

/// A chunk-local dirty box, in cell coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyRegion {
    pub origin: [u32; 3],
    pub extent: [u32; 3],
}

/// One aligned replacement range inside a resident plane.
///
/// Several patches can be validated and published under one [`ChunkStamp`]
/// with [`ResidentChunk::commit_plane_patches`]. The values are borrowed only
/// for the duration of that queue write.
#[derive(Clone, Copy, Debug)]
pub struct PlanePatch<'a, T> {
    pub element_offset: usize,
    pub values: &'a [T],
}

impl<'a, T> PlanePatch<'a, T> {
    pub const fn new(element_offset: usize, values: &'a [T]) -> Self {
        Self {
            element_offset,
            values,
        }
    }
}

/// The fact-bearing class of a channel plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneClass {
    /// Canonical values such as occupancy, palette id, provenance, or flags.
    Exact,
    /// Replay-exact numerical state stored as fixed point.
    FixedPoint,
    /// Learned or derived state whose drift does not alter the record.
    Derived,
    /// A work plane, candidate result, mask, or queue.
    Temporary,
}

/// The scalar representation held by one channel plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneElementType {
    U8,
    U16,
    U32,
    I8,
    I16,
    I32,
    F32,
}

impl PlaneElementType {
    pub const fn byte_width(self) -> usize {
        match self {
            Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 | Self::F32 => 4,
        }
    }
}

mod sealed {
    pub trait Sealed {}

    impl Sealed for u8 {}
    impl Sealed for u16 {}
    impl Sealed for u32 {}
    impl Sealed for i8 {}
    impl Sealed for i16 {}
    impl Sealed for i32 {}
    impl Sealed for f32 {}
}

/// A scalar that can be uploaded as a resident channel plane.
pub trait ResidentElement: Pod + sealed::Sealed {
    const ELEMENT_TYPE: PlaneElementType;
}

macro_rules! resident_elements {
    ($($ty:ty => $element:ident),+ $(,)?) => {
        $(impl ResidentElement for $ty {
            const ELEMENT_TYPE: PlaneElementType = PlaneElementType::$element;
        })+
    };
}

resident_elements! {
    u8 => U8,
    u16 => U16,
    u32 => U32,
    i8 => I8,
    i16 => I16,
    i32 => I32,
    f32 => F32,
}

/// A named plane in a resident bundle.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlaneId(String);

impl PlaneId {
    pub fn new(name: impl Into<String>) -> Result<Self, ResidentChunkError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(ResidentChunkError::EmptyPlaneId);
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PlaneId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Shape and scalar layout of one contiguous 3D plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaneLayout {
    pub shape: [usize; 3],
    pub strides: [usize; 3],
    pub element_type: PlaneElementType,
}

impl PlaneLayout {
    fn contiguous<T: ResidentElement>(shape: [usize; 3]) -> Result<Self, ResidentChunkError> {
        if shape.contains(&0) {
            return Err(ResidentChunkError::EmptyShape { shape });
        }
        let yz = shape[1]
            .checked_mul(shape[2])
            .ok_or(ResidentChunkError::ShapeOverflow { shape })?;
        shape[0]
            .checked_mul(yz)
            .and_then(|elements| elements.checked_mul(T::ELEMENT_TYPE.byte_width()))
            .ok_or(ResidentChunkError::ShapeOverflow { shape })?;
        Ok(Self {
            shape,
            strides: [yz, shape[2], 1],
            element_type: T::ELEMENT_TYPE,
        })
    }

    pub fn element_count(self) -> usize {
        self.shape[0] * self.shape[1] * self.shape[2]
    }

    pub fn byte_len(self) -> usize {
        self.element_count() * self.element_type.byte_width()
    }
}

/// Authoritative snapshot metadata copied into every consumer view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkStamp {
    pub revision: u64,
    pub valid_read_epoch: ReadEpoch,
}

/// Identity of a byte range in one concrete wgpu buffer.
///
/// CubeCL may suballocate several planes from one buffer. Equality therefore
/// checks the wgpu buffer object and the allocation's offset and size, rather
/// than treating any two ranges in the same pooled buffer as one allocation.
#[derive(Clone, Debug)]
pub struct BufferIdentity {
    buffer: wgpu::Buffer,
    offset: u64,
    size: u64,
}

impl BufferIdentity {
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub const fn offset(&self) -> u64 {
        self.offset
    }

    pub const fn size(&self) -> u64 {
        self.size
    }

    pub fn same_buffer(&self, other: &Self) -> bool {
        self.buffer == other.buffer
    }
}

impl PartialEq for BufferIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.buffer == other.buffer && self.offset == other.offset && self.size == other.size
    }
}

impl Eq for BufferIdentity {}

/// One raw-kernel arrangement of a resident plane.
///
/// The private CubeCL handle keeps this allocation range leased for as long
/// as the raw view exists, even if its parent chunk is dropped.
#[derive(Debug)]
pub struct RawKernelView {
    allocation: BufferIdentity,
    layout: PlaneLayout,
    class: PlaneClass,
    stamp: ChunkStamp,
    _lease: Handle,
}

impl RawKernelView {
    pub fn allocation(&self) -> &BufferIdentity {
        &self.allocation
    }

    pub const fn layout(&self) -> PlaneLayout {
        self.layout
    }

    pub const fn class(&self) -> PlaneClass {
        self.class
    }

    pub const fn stamp(&self) -> ChunkStamp {
        self.stamp
    }

    /// Everything a consumer outside this crate needs to read the plane,
    /// and nothing it does not.
    ///
    /// Consumers that must not depend on a compute stack (a renderer, a
    /// tracer) spell the lease in their own plain-wgpu terms; this is the
    /// producing side's single definition of what those terms mean, so a
    /// host assembling one cannot invent a field or drop the stamp. The
    /// byte length is derived here rather than restated, which is the
    /// error the consumer-side spellings cannot make on their own.
    pub fn lease(&self) -> SpatialLease<'_> {
        SpatialLease {
            buffer: self.allocation.buffer(),
            offset: self.allocation.offset(),
            size: self.allocation.size(),
            shape: self.layout.shape,
            element_type: self.layout.element_type,
            stamp: self.stamp,
        }
    }

    /// A storage-buffer binding for the exact CubeCL allocation range.
    pub fn binding(&self) -> wgpu::BufferBinding<'_> {
        wgpu::BufferBinding {
            buffer: self.allocation.buffer(),
            offset: self.allocation.offset(),
            size: NonZeroU64::new(self.allocation.size().next_multiple_of(4)),
        }
    }
}

/// The producing side's definition of a resident plane handed outward.
///
/// Deliberately plain: a buffer range, the shape and element type that
/// range holds, and the stamp it was materialized at. A consumer needs
/// exactly this much to copy or bind the plane, and needs no part of
/// CubeCL or Burn to understand it.
///
/// `byte_len` is the load-bearing method. Consumers size their copies
/// from `shape`, and a shape that disagrees with the allocation would
/// walk off the end of the lease into whatever the pool put next to it,
/// which is silent corruption rather than a fault. Checking it is the
/// consumer's obligation and this makes it one line.
#[derive(Clone, Copy, Debug)]
pub struct SpatialLease<'a> {
    pub buffer: &'a wgpu::Buffer,
    pub offset: u64,
    pub size: u64,
    pub shape: [usize; 3],
    pub element_type: PlaneElementType,
    pub stamp: ChunkStamp,
}

impl SpatialLease<'_> {
    /// The bytes `shape` and `element_type` actually describe.
    pub fn byte_len(&self) -> u64 {
        (self.shape[0] * self.shape[1] * self.shape[2] * self.element_type.byte_width()) as u64
    }

    /// Whether the described shape fits the leased range. A consumer that
    /// copies without asking this can read a neighbouring allocation.
    pub fn fits(&self) -> bool {
        self.byte_len() <= self.size
    }
}

/// One Burn arrangement of a resident `f32` plane.
#[derive(Clone, Debug)]
pub struct BurnTensorView {
    tensor: ResidentTensor,
    allocation: BufferIdentity,
    layout: PlaneLayout,
    class: PlaneClass,
    stamp: ChunkStamp,
}

impl BurnTensorView {
    pub fn tensor(&self) -> &ResidentTensor {
        &self.tensor
    }

    pub fn into_tensor(self) -> ResidentTensor {
        self.tensor
    }

    pub fn allocation(&self) -> &BufferIdentity {
        &self.allocation
    }

    pub const fn layout(&self) -> PlaneLayout {
        self.layout
    }

    pub const fn class(&self) -> PlaneClass {
        self.class
    }

    pub const fn stamp(&self) -> ChunkStamp {
        self.stamp
    }
}

/// CubeCL registered on a host-owned wgpu device and queue.
///
/// [`Self::init`] consumes clones in [`WgpuSetup`]; it does not create a
/// second wgpu device. A host that already registered CubeCL can use
/// [`Self::from_registered_device`] instead.
#[derive(Clone)]
pub struct ResidentClient {
    compute: ComputeClient<WgpuRuntime>,
    device: WgpuDevice,
}

impl fmt::Debug for ResidentClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResidentClient")
            .field("device", &self.device)
            .finish_non_exhaustive()
    }
}

impl ResidentClient {
    pub fn init(setup: WgpuSetup) -> Self {
        let device = init_device(setup, RuntimeOptions::default());
        Self::from_registered_device(device)
    }

    pub fn from_registered_device(device: WgpuDevice) -> Self {
        let compute = WgpuRuntime::client(&device);
        Self { compute, device }
    }

    /// The CubeCL client every resident allocation is made through.
    ///
    /// Public because the field lane allocates its own buffers on the
    /// same client the chunk bundles use: one allocator is what lets a
    /// kernel pass and a tensor pass meet without a bridge.
    pub fn compute_client(&self) -> &ComputeClient<WgpuRuntime> {
        &self.compute
    }

    pub fn device(&self) -> &WgpuDevice {
        &self.device
    }
}

#[derive(Debug)]
struct ResidentPlane {
    handle: Handle,
    layout: PlaneLayout,
    class: PlaneClass,
}

/// A bundle of typed, GPU-resident channel planes for one world chunk.
///
/// `I` is the world's own chunk identity type. Conatus does not mint a parallel
/// identity model; the wing can carry its existing region key here directly.
#[derive(Debug)]
pub struct ResidentChunk<I> {
    client: ResidentClient,
    identity: I,
    bounds: ChunkBounds,
    stamp: ChunkStamp,
    dirty_regions: Vec<DirtyRegion>,
    planes: BTreeMap<PlaneId, ResidentPlane>,
}

impl<I> ResidentChunk<I> {
    pub fn new(
        client: ResidentClient,
        identity: I,
        bounds: ChunkBounds,
        revision: u64,
        valid_read_epoch: ReadEpoch,
        dirty_regions: Vec<DirtyRegion>,
    ) -> Self {
        Self {
            client,
            identity,
            bounds,
            stamp: ChunkStamp {
                revision,
                valid_read_epoch,
            },
            dirty_regions,
            planes: BTreeMap::new(),
        }
    }

    pub fn identity(&self) -> &I {
        &self.identity
    }

    pub const fn bounds(&self) -> ChunkBounds {
        self.bounds
    }

    pub const fn stamp(&self) -> ChunkStamp {
        self.stamp
    }

    pub fn dirty_regions(&self) -> &[DirtyRegion] {
        &self.dirty_regions
    }

    pub fn plane_ids(&self) -> impl ExactSizeIterator<Item = &PlaneId> {
        self.planes.keys()
    }

    /// Allocate and initialize one contiguous 3D plane through CubeCL.
    pub fn insert_plane<T: ResidentElement>(
        &mut self,
        id: PlaneId,
        class: PlaneClass,
        shape: [usize; 3],
        values: &[T],
    ) -> Result<(), ResidentChunkError> {
        if self.planes.contains_key(&id) {
            return Err(ResidentChunkError::DuplicatePlane(id));
        }
        let layout = PlaneLayout::contiguous::<T>(shape)?;
        let expected = layout.element_count();
        if values.len() != expected {
            return Err(ResidentChunkError::ElementCount {
                plane: id,
                expected,
                actual: values.len(),
            });
        }

        let handle = self
            .compute()
            .create_from_slice(bytemuck::cast_slice(values));
        self.planes.insert(
            id,
            ResidentPlane {
                handle,
                layout,
                class,
            },
        );
        Ok(())
    }

    /// Apply one authority-approved patch without replacing the CubeCL allocation.
    ///
    /// The caller owns the record and the shared-device schedule. Quint checks that
    /// the patch was derived from the resident stamp still being replaced, writes
    /// only the supplied aligned range, then publishes the committed stamp and dirty
    /// regions together. Views minted before this call retain their old stamp and
    /// must be rejected by revision-aware consumers.
    ///
    /// This narrow form accepts a single-plane chunk. A bundle-wide stamp cannot
    /// truthfully advance after one of several planes changes; that case needs one
    /// validated batch commit covering every changed plane.
    #[allow(
        clippy::too_many_arguments,
        reason = "the queue, plane, two stamps, byte range, and dirty publication are distinct validation inputs"
    )]
    pub fn commit_plane_patch<T: ResidentElement>(
        &mut self,
        queue: &wgpu::Queue,
        id: &PlaneId,
        expected_stamp: ChunkStamp,
        element_offset: usize,
        values: &[T],
        committed_stamp: ChunkStamp,
        dirty_regions: Vec<DirtyRegion>,
    ) -> Result<(), ResidentChunkError> {
        self.commit_plane_patches(
            queue,
            id,
            expected_stamp,
            &[PlanePatch::new(element_offset, values)],
            committed_stamp,
            dirty_regions,
        )
    }

    /// Apply several authority-approved ranges as one published resident revision.
    ///
    /// Every range, overlap, stamp, and dirty region is checked before the first
    /// queue write. A validation failure therefore leaves both the allocation and
    /// its stamp unchanged. The queue observes the accepted writes in patch order,
    /// then readers may use `committed_stamp` after the host's submission boundary.
    ///
    /// Like [`Self::commit_plane_patch`], this narrow operation accepts a
    /// single-plane chunk. Updating several planes truthfully requires a separate
    /// bundle-wide commit primitive.
    #[allow(
        clippy::too_many_arguments,
        reason = "the queue, plane, two stamps, sparse ranges, and dirty publication are distinct validation inputs"
    )]
    pub fn commit_plane_patches<T: ResidentElement>(
        &mut self,
        queue: &wgpu::Queue,
        id: &PlaneId,
        expected_stamp: ChunkStamp,
        patches: &[PlanePatch<'_, T>],
        committed_stamp: ChunkStamp,
        dirty_regions: Vec<DirtyRegion>,
    ) -> Result<(), ResidentChunkError> {
        if self.stamp != expected_stamp {
            return Err(ResidentChunkError::StaleStamp {
                expected: expected_stamp,
                actual: self.stamp,
            });
        }
        if committed_stamp.revision <= expected_stamp.revision
            || committed_stamp.valid_read_epoch <= expected_stamp.valid_read_epoch
        {
            return Err(ResidentChunkError::NonAdvancingStamp {
                previous: expected_stamp,
                committed: committed_stamp,
            });
        }
        for region in &dirty_regions {
            let fits = !region.extent.contains(&0)
                && (0..3).all(|axis| {
                    region.origin[axis]
                        .checked_add(region.extent[axis])
                        .is_some_and(|end| end <= self.bounds.extent[axis])
                });
            if !fits {
                return Err(ResidentChunkError::DirtyRegionOutOfBounds {
                    region: *region,
                    chunk_extent: self.bounds.extent,
                });
            }
        }

        if patches.is_empty() {
            return Err(ResidentChunkError::EmptyPatchBatch { plane: id.clone() });
        }

        let plane = self.plane(id)?;
        if self.planes.len() != 1 {
            return Err(ResidentChunkError::PatchRequiresSinglePlane {
                plane_count: self.planes.len(),
            });
        }
        if plane.layout.element_type != T::ELEMENT_TYPE {
            return Err(ResidentChunkError::PatchElementType {
                plane: id.clone(),
                expected: plane.layout.element_type,
                actual: T::ELEMENT_TYPE,
            });
        }

        let alignment = wgpu::COPY_BUFFER_ALIGNMENT as usize;
        let mut ranges = Vec::with_capacity(patches.len());
        for (index, patch) in patches.iter().enumerate() {
            let byte_offset = patch
                .element_offset
                .checked_mul(T::ELEMENT_TYPE.byte_width())
                .ok_or_else(|| ResidentChunkError::PatchRangeOverflow { plane: id.clone() })?;
            let byte_len = size_of_val(patch.values);
            let byte_end = byte_offset
                .checked_add(byte_len)
                .ok_or_else(|| ResidentChunkError::PatchRangeOverflow { plane: id.clone() })?;
            if byte_len == 0 || byte_end > plane.layout.byte_len() {
                return Err(ResidentChunkError::PatchRange {
                    plane: id.clone(),
                    byte_offset,
                    byte_len,
                    plane_byte_len: plane.layout.byte_len(),
                });
            }
            if !byte_offset.is_multiple_of(alignment) || !byte_len.is_multiple_of(alignment) {
                return Err(ResidentChunkError::UnalignedPatch {
                    plane: id.clone(),
                    byte_offset,
                    byte_len,
                    required_alignment: alignment,
                });
            }
            ranges.push((byte_offset, byte_end, index));
        }
        ranges.sort_unstable_by_key(|range| (range.0, range.1));
        for pair in ranges.windows(2) {
            let first = pair[0];
            let second = pair[1];
            if first.1 > second.0 {
                return Err(ResidentChunkError::OverlappingPatches {
                    plane: id.clone(),
                    first_index: first.2,
                    second_index: second.2,
                });
            }
        }

        let allocation = self.allocation(&plane.handle)?;
        for patch in patches {
            let byte_offset = patch.element_offset * T::ELEMENT_TYPE.byte_width();
            queue.write_buffer(
                allocation.buffer(),
                allocation.offset() + byte_offset as u64,
                bytemuck::cast_slice(patch.values),
            );
        }
        self.stamp = committed_stamp;
        self.dirty_regions = dirty_regions;
        Ok(())
    }

    /// Construct a raw storage-buffer view without copying the plane.
    pub fn raw_kernel_view(&self, id: &PlaneId) -> Result<RawKernelView, ResidentChunkError> {
        let plane = self.plane(id)?;
        let allocation = self.allocation(&plane.handle)?;
        Ok(RawKernelView {
            allocation,
            layout: plane.layout,
            class: plane.class,
            stamp: self.stamp,
            _lease: plane.handle.clone(),
        })
    }

    /// Construct a Burn `f32` tensor over the same CubeCL handle.
    ///
    /// This constructor only accepts an `F32` plane. Exact integer planes stay
    /// exact rather than acquiring a lossy float interpretation for convenience.
    pub fn burn_f32_view(&self, id: &PlaneId) -> Result<BurnTensorView, ResidentChunkError> {
        let plane = self.plane(id)?;
        if plane.layout.element_type != PlaneElementType::F32 {
            return Err(ResidentChunkError::BurnElementType {
                plane: id.clone(),
                actual: plane.layout.element_type,
            });
        }
        let allocation = self.allocation(&plane.handle)?;
        let primitive = CubeTensor::<WgpuRuntime>::new_contiguous(
            self.compute().clone(),
            self.client.device.clone(),
            Shape::new(plane.layout.shape),
            plane.handle.clone(),
            DType::F32,
        );
        let tensor = Tensor::from_primitive::<Wgpu>(primitive);
        Ok(BurnTensorView {
            tensor,
            allocation,
            layout: plane.layout,
            class: plane.class,
            stamp: self.stamp,
        })
    }

    fn compute(&self) -> &ComputeClient<WgpuRuntime> {
        &self.client.compute
    }

    fn plane(&self, id: &PlaneId) -> Result<&ResidentPlane, ResidentChunkError> {
        self.planes
            .get(id)
            .ok_or_else(|| ResidentChunkError::UnknownPlane(id.clone()))
    }

    fn allocation(&self, handle: &Handle) -> Result<BufferIdentity, ResidentChunkError> {
        let managed = self
            .compute()
            .get_resource(handle.clone())
            .map_err(|error| ResidentChunkError::Resource(error.to_string()))?;
        let resource = managed.resource();
        Ok(BufferIdentity {
            buffer: resource.buffer.clone(),
            offset: resource.offset,
            size: resource.size,
        })
    }
}

/// Construction and view errors for [`ResidentChunk`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResidentChunkError {
    EmptyPlaneId,
    EmptyShape {
        shape: [usize; 3],
    },
    ShapeOverflow {
        shape: [usize; 3],
    },
    DuplicatePlane(PlaneId),
    UnknownPlane(PlaneId),
    ElementCount {
        plane: PlaneId,
        expected: usize,
        actual: usize,
    },
    BurnElementType {
        plane: PlaneId,
        actual: PlaneElementType,
    },
    StaleStamp {
        expected: ChunkStamp,
        actual: ChunkStamp,
    },
    NonAdvancingStamp {
        previous: ChunkStamp,
        committed: ChunkStamp,
    },
    DirtyRegionOutOfBounds {
        region: DirtyRegion,
        chunk_extent: [u32; 3],
    },
    PatchRequiresSinglePlane {
        plane_count: usize,
    },
    EmptyPatchBatch {
        plane: PlaneId,
    },
    OverlappingPatches {
        plane: PlaneId,
        first_index: usize,
        second_index: usize,
    },
    PatchElementType {
        plane: PlaneId,
        expected: PlaneElementType,
        actual: PlaneElementType,
    },
    PatchRangeOverflow {
        plane: PlaneId,
    },
    PatchRange {
        plane: PlaneId,
        byte_offset: usize,
        byte_len: usize,
        plane_byte_len: usize,
    },
    UnalignedPatch {
        plane: PlaneId,
        byte_offset: usize,
        byte_len: usize,
        required_alignment: usize,
    },
    Resource(String),
}

impl fmt::Display for ResidentChunkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPlaneId => formatter.write_str("resident plane id cannot be empty"),
            Self::EmptyShape { shape } => {
                write!(formatter, "resident plane shape {shape:?} is empty")
            }
            Self::ShapeOverflow { shape } => {
                write!(
                    formatter,
                    "resident plane shape {shape:?} overflows its allocation"
                )
            }
            Self::DuplicatePlane(plane) => {
                write!(formatter, "resident plane {plane} already exists")
            }
            Self::UnknownPlane(plane) => write!(formatter, "resident plane {plane} does not exist"),
            Self::ElementCount {
                plane,
                expected,
                actual,
            } => write!(
                formatter,
                "resident plane {plane} needs {expected} elements, got {actual}"
            ),
            Self::BurnElementType { plane, actual } => write!(
                formatter,
                "resident plane {plane} is {actual:?}, not F32, so it has no Burn float view"
            ),
            Self::StaleStamp { expected, actual } => write!(
                formatter,
                "resident patch expected stamp {expected:?}, but the chunk is at {actual:?}"
            ),
            Self::NonAdvancingStamp {
                previous,
                committed,
            } => write!(
                formatter,
                "resident patch must advance revision and read epoch beyond {previous:?}, got {committed:?}"
            ),
            Self::DirtyRegionOutOfBounds {
                region,
                chunk_extent,
            } => write!(
                formatter,
                "resident dirty region {region:?} does not fit chunk extent {chunk_extent:?}"
            ),
            Self::PatchRequiresSinglePlane { plane_count } => write!(
                formatter,
                "single-plane resident patch cannot restamp a bundle containing {plane_count} planes"
            ),
            Self::EmptyPatchBatch { plane } => {
                write!(formatter, "resident patch batch for plane {plane} is empty")
            }
            Self::OverlappingPatches {
                plane,
                first_index,
                second_index,
            } => write!(
                formatter,
                "resident patches {first_index} and {second_index} overlap in plane {plane}"
            ),
            Self::PatchElementType {
                plane,
                expected,
                actual,
            } => write!(
                formatter,
                "resident plane {plane} stores {expected:?}, not patch element type {actual:?}"
            ),
            Self::PatchRangeOverflow { plane } => {
                write!(
                    formatter,
                    "resident patch range for plane {plane} overflowed"
                )
            }
            Self::PatchRange {
                plane,
                byte_offset,
                byte_len,
                plane_byte_len,
            } => write!(
                formatter,
                "resident patch [{byte_offset}..{}) does not fit plane {plane}'s {plane_byte_len} bytes",
                byte_offset + byte_len
            ),
            Self::UnalignedPatch {
                plane,
                byte_offset,
                byte_len,
                required_alignment,
            } => write!(
                formatter,
                "resident patch for plane {plane} has byte offset {byte_offset} and length {byte_len}; both must be multiples of {required_alignment}"
            ),
            Self::Resource(error) => write!(formatter, "CubeCL resource export failed: {error}"),
        }
    }
}

impl std::error::Error for ResidentChunkError {}
