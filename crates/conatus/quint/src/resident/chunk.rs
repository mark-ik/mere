//! Shared resident chunk planes and their zero-copy consumer views.
//!
//! Allocation has one direction here: [`ResidentClient`] allocates each
//! plane through CubeCL, then [`ResidentChunk`] lends Burn and raw wgpu
//! arrangements of that allocation. Neither view imports or copies the
//! plane, and this module deliberately offers no CPU whole-plane read.

use std::{collections::BTreeMap, fmt, num::NonZeroU64};

use burn::{
    backend::{
        Wgpu,
        wgpu::{CubeTensor, RuntimeOptions, WgpuDevice, WgpuRuntime, WgpuSetup, init_device},
    },
    tensor::{DType, Shape, Tensor, TensorPrimitive},
};
use bytemuck::Pod;
use cubecl::{Runtime, client::ComputeClient, server::Handle};

/// Burn's view of one resident `f32` channel plane.
pub type ResidentTensor = Tensor<Wgpu<f32, i32>, 3>;

/// The host schedule epoch in which a materialized chunk is safe to read.
///
/// This is intentionally a host-issued number, not a frame counter hidden
/// inside quint. The shared device scheduler decides when a submitted write
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
/// `I` is the world's own chunk identity type. quint does not mint a parallel
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
        let tensor = Tensor::from_primitive(TensorPrimitive::Float(primitive));
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
            Self::Resource(error) => write!(formatter, "CubeCL resource export failed: {error}"),
        }
    }
}

impl std::error::Error for ResidentChunkError {}
