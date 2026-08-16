//! Lane A receipt for the resident chunk seam.
//!
//! The load-bearing assertion is allocation identity: the Burn tensor's
//! CubeCL handle resolves to the same wgpu buffer range as the raw-kernel
//! view. No plane contents are read back to establish that fact.

#![cfg(feature = "field-gpu")]

use quint::resident::{
    ChunkBounds, DirtyRegion, PlaneClass, PlaneElementType, PlaneId, ReadEpoch, ResidentChunk,
    ResidentChunkError, ResidentClient,
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
    let primitive = burn.into_tensor().into_primitive().tensor();
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
