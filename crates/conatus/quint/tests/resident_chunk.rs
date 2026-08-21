//! Lane A receipt for the resident chunk seam.
//!
//! The load-bearing assertion is allocation identity: the Burn tensor's
//! CubeCL handle resolves to the same wgpu buffer range as the raw-kernel
//! view. No plane contents are read back to establish that fact.

#![cfg(feature = "field-gpu")]

use quint::resident::{
    ChunkBounds, ChunkStamp, DirtyRegion, PlaneClass, PlaneElementType, PlaneId, RawKernelView,
    ReadEpoch, ResidentChunk, ResidentChunkError, ResidentClient,
};

fn setup() -> Option<burn::backend::wgpu::WgpuSetup> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        flags: wgpu::InstanceFlags::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        // wgpu 30 limit bucketing, off so the test sees real limits.
        apply_limit_buckets: false,
    }))
    .ok()?;
    let backend = adapter.get_info().backend;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("quint resident chunk receipt"),
        ..Default::default()
    }))
    .ok()?;
    Some(burn::backend::wgpu::WgpuSetup {
        instance,
        adapter,
        device,
        queue,
        backend,
    })
}

fn readback(device: &wgpu::Device, queue: &wgpu::Queue, view: &RawKernelView) -> Vec<u8> {
    let lease = view.lease();
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("quint resident patch readback"),
        size: lease.byte_len(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("quint resident patch readback"),
    });
    encoder.copy_buffer_to_buffer(lease.buffer, lease.offset, &staging, 0, lease.byte_len());
    let submission = queue.submit([encoder.finish()]);
    staging.slice(..).map_async(wgpu::MapMode::Read, |_| ());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .unwrap();
    let bytes = staging
        .slice(..)
        .get_mapped_range()
        .expect("resident patch readback mapped")
        .to_vec();
    staging.unmap();
    bytes
}

#[test]
fn burn_and_raw_views_are_the_same_cubecl_allocation() {
    let Some(setup) = setup() else {
        eprintln!("no wgpu adapter: skipping the resident chunk receipt");
        return;
    };
    let client = ResidentClient::init(setup);
    let mut chunk = ResidentChunk::new(
        client,
        "synthetic",
        ChunkBounds {
            origin: [-8, 4, 12],
            extent: [2, 2, 2],
        },
        41,
        ReadEpoch::new(73),
        vec![DirtyRegion {
            origin: [0, 0, 0],
            extent: [2, 2, 2],
        }],
    );
    let temperature = PlaneId::new("temperature").unwrap();
    chunk
        .insert_plane(
            temperature.clone(),
            PlaneClass::Derived,
            [2, 2, 2],
            &[0.0f32, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0],
        )
        .unwrap();

    let burn = chunk.burn_f32_view(&temperature).unwrap();
    let raw = chunk.raw_kernel_view(&temperature).unwrap();

    assert_eq!(burn.allocation(), raw.allocation());
    assert!(burn.allocation().same_buffer(raw.allocation()));
    assert_eq!(burn.tensor().dims(), [2, 2, 2]);
    assert_eq!(burn.layout().strides, [4, 2, 1]);
    assert_eq!(burn.stamp().revision, 41);
    assert_eq!(burn.stamp().valid_read_epoch, ReadEpoch::new(73));
    assert_eq!(raw.stamp(), burn.stamp());

    // Resolve the primitive carried by the public Burn tensor, rather than
    // trusting two metadata values both minted by quint. Its CubeCL handle
    // must resolve to the exact raw buffer range.
    let primitive = burn
        .into_tensor()
        .try_into_primitive::<burn_wgpu::Wgpu>()
        .expect("resident tensor is a wgpu tensor");
    let managed = primitive
        .client
        .get_resource(primitive.handle.clone())
        .unwrap();
    let resource = managed.resource();
    assert_eq!(&resource.buffer, raw.allocation().buffer());
    assert_eq!(resource.offset, raw.allocation().offset());
    assert_eq!(resource.size, raw.allocation().size());
}

#[test]
fn exact_plane_keeps_its_integer_type_in_the_raw_view() {
    let Some(setup) = setup() else {
        eprintln!("no wgpu adapter: skipping the resident chunk type receipt");
        return;
    };
    let mut chunk = ResidentChunk::new(
        ResidentClient::init(setup),
        9u64,
        ChunkBounds {
            origin: [0, 0, 0],
            extent: [2, 1, 1],
        },
        5,
        ReadEpoch::new(8),
        Vec::new(),
    );
    let occupancy = PlaneId::new("occupancy").unwrap();
    chunk
        .insert_plane(occupancy.clone(), PlaneClass::Exact, [2, 1, 1], &[0u8, 1])
        .unwrap();

    let raw = chunk.raw_kernel_view(&occupancy).unwrap();
    assert_eq!(raw.layout().element_type, PlaneElementType::U8);
    assert!(matches!(
        chunk.burn_f32_view(&occupancy),
        Err(ResidentChunkError::BurnElementType {
            actual: PlaneElementType::U8,
            ..
        })
    ));
}

/// The lease export is the producing side's single definition of what
/// crosses to a consumer, so its derived length must match the
/// allocation it describes. A consumer sizing a copy from `shape`
/// against a mismatched allocation would read whatever the pool placed
/// next to this plane.
#[test]
fn a_lease_reports_the_bytes_its_shape_describes() {
    let Some(setup) = setup() else {
        eprintln!("no wgpu adapter: skipping the lease receipt");
        return;
    };
    let mut chunk = ResidentChunk::new(
        ResidentClient::init(setup),
        "lease",
        ChunkBounds {
            origin: [0, 0, 0],
            extent: [2, 3, 4],
        },
        7,
        ReadEpoch::new(9),
        Vec::new(),
    );
    let id = PlaneId::new("occupancy").unwrap();
    chunk
        .insert_plane(id.clone(), PlaneClass::Exact, [2, 3, 4], &[1u8; 24])
        .unwrap();

    let view = chunk.raw_kernel_view(&id).unwrap();
    let lease = view.lease();
    assert_eq!(lease.byte_len(), 24);
    assert_eq!(lease.size, 24);
    assert!(lease.fits());
    assert_eq!(lease.shape, [2, 3, 4]);
    assert_eq!(lease.element_type, PlaneElementType::U8);
    assert_eq!(lease.stamp.revision, 7);
    assert_eq!(lease.stamp.valid_read_epoch, ReadEpoch::new(9));
    assert_eq!(lease.buffer, view.allocation().buffer());
    assert_eq!(lease.offset, view.allocation().offset());

    // The guard's negative case: a shape larger than the leased range
    // must report that it does not fit, rather than being copied.
    let overshot = quint::resident::SpatialLease {
        shape: [4, 3, 4],
        ..lease
    };
    assert_eq!(overshot.byte_len(), 48);
    assert!(!overshot.fits());
}

#[test]
fn committed_patch_retains_the_allocation_and_restamps_new_views() {
    let Some(setup) = setup() else {
        eprintln!("no wgpu adapter: skipping the resident patch receipt");
        return;
    };
    let device = setup.device.clone();
    let queue = setup.queue.clone();
    let mut chunk = ResidentChunk::new(
        ResidentClient::init(setup),
        "retained",
        ChunkBounds {
            origin: [0, 0, 0],
            extent: [8, 1, 1],
        },
        12,
        ReadEpoch::new(20),
        Vec::new(),
    );
    let id = PlaneId::new("material").unwrap();
    chunk
        .insert_plane(
            id.clone(),
            PlaneClass::Exact,
            [8, 1, 1],
            &[3u8, 7, 4, 5, 6, 7, 8, 9],
        )
        .unwrap();

    let before = chunk.raw_kernel_view(&id).unwrap();
    let expected = before.stamp();
    let committed = ChunkStamp {
        revision: 13,
        valid_read_epoch: ReadEpoch::new(21),
    };
    let dirty = vec![DirtyRegion {
        origin: [4, 0, 0],
        extent: [1, 1, 1],
    }];

    let unaligned = chunk
        .commit_plane_patch(
            &queue,
            &id,
            expected,
            1,
            &[0u8; 4],
            committed,
            dirty.clone(),
        )
        .unwrap_err();
    assert_eq!(
        unaligned,
        ResidentChunkError::UnalignedPatch {
            plane: id.clone(),
            byte_offset: 1,
            byte_len: 4,
            required_alignment: wgpu::COPY_BUFFER_ALIGNMENT as usize,
        }
    );
    assert_eq!(chunk.stamp(), expected);

    chunk
        .commit_plane_patch(
            &queue,
            &id,
            expected,
            4,
            &[11u8, 12, 13, 14],
            committed,
            dirty.clone(),
        )
        .unwrap();
    let after = chunk.raw_kernel_view(&id).unwrap();

    assert_eq!(before.allocation(), after.allocation());
    assert_eq!(
        before.stamp(),
        expected,
        "old views keep their source stamp"
    );
    assert_eq!(after.stamp(), committed);
    assert_eq!(chunk.dirty_regions(), dirty);
    let bytes = readback(&device, &queue, &after);
    assert_eq!(bytes, &[3, 7, 4, 5, 11, 12, 13, 14]);

    let auxiliary = PlaneId::new("auxiliary").unwrap();
    chunk
        .insert_plane(auxiliary, PlaneClass::Derived, [8, 1, 1], &[0u8; 8])
        .unwrap();
    let bundle_error = chunk
        .commit_plane_patch(
            &queue,
            &id,
            committed,
            0,
            &[5u8; 4],
            ChunkStamp {
                revision: 14,
                valid_read_epoch: ReadEpoch::new(22),
            },
            Vec::new(),
        )
        .unwrap_err();
    assert_eq!(
        bundle_error,
        ResidentChunkError::PatchRequiresSinglePlane { plane_count: 2 }
    );
    assert_eq!(chunk.stamp(), committed);

    let error = chunk
        .commit_plane_patch(
            &queue,
            &id,
            expected,
            0,
            &[5u8; 4],
            ChunkStamp {
                revision: 14,
                valid_read_epoch: ReadEpoch::new(22),
            },
            Vec::new(),
        )
        .unwrap_err();
    assert_eq!(
        error,
        ResidentChunkError::StaleStamp {
            expected,
            actual: committed,
        }
    );
    assert_eq!(chunk.stamp(), committed, "a stale patch changed the stamp");
}
