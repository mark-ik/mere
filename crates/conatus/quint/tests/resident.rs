//! The resident lane's receipt: the kernel computes quint's own force
//! law, and the lane settles.
//!
//! The anchor is `forces::repulsion_reference`, the naive host loop
//! that exists precisely so a fast path can be checked against the
//! formula rather than against another fast path. Two implementations
//! agreeing proves they match each other; this proves the kernel
//! matches the law.
//!
//! Needs a real adapter, so it skips rather than fails where there is
//! none (CI without a GPU, a headless box with no software rasterizer).

#![cfg(feature = "field-gpu")]

use quint::forces::{RepulsionParams, repulsion_reference};
use quint::resident::{Adjacency, Params, Resident, ResidentClient};

/// A deterministic scatter, seeded: the same cloud every run.
fn scatter(n: usize, extent: f32) -> Vec<[f32; 4]> {
    let mut state = 0x2026_0813u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 33) as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    (0..n)
        .map(|_| [next() * extent, next() * extent, 0.0, 0.0])
        .collect()
}

/// Boot a client for the test, or `None` where no adapter exists.
///
/// The lane allocates through CubeCL now, so a test needs the client
/// rather than a device pair. Booting it from an explicit setup keeps
/// the "device is the host's, never this crate's" rule visible: quint
/// adopts what it is handed.
fn client() -> Option<ResidentClient> {
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
        label: Some("quint resident lane receipt"),
        ..Default::default()
    }))
    .ok()?;
    Some(ResidentClient::init(burn::backend::wgpu::WgpuSetup {
        instance,
        adapter,
        device,
        queue,
        backend,
    }))
}

fn no_edges(n: usize) -> Vec<u32> {
    vec![0; n + 1]
}

#[test]
fn the_kernel_computes_quints_own_force_law() {
    let Some(client) = client() else {
        eprintln!("no wgpu adapter: skipping the resident lane receipt");
        return;
    };

    let n = 512;
    let positions = scatter(n, 200.0);
    let offsets = no_edges(n);
    let params = Params {
        repulsion: 4_000.0,
        min_distance: 4.0,
        ..Default::default()
    };

    let mut resident = Resident::new(
        &client,
        &positions,
        Adjacency {
            offsets: &offsets,
            targets: &[],
        },
        params,
    );
    println!("kernel source: CubeCL, authored in quint::resident::kernels");
    resident.repulse_only();
    let theirs = resident.read_forces();

    // The same law on the host, from the same positions and constants.
    let xs: Vec<f32> = positions.iter().map(|p| p[0]).collect();
    let ys: Vec<f32> = positions.iter().map(|p| p[1]).collect();
    let (fx, fy) = repulsion_reference(
        &xs,
        &ys,
        RepulsionParams {
            strength: params.repulsion,
            softening: params.min_distance,
        },
    );

    // Different summation orders, so the bar is relative error rather
    // than bits: the kernel sums in workgroup tiles, the anchor in one
    // ascending loop.
    let mut worst = 0.0f32;
    let mut mean = 0.0f64;
    for (i, force) in theirs.iter().enumerate() {
        let magnitude = (fx[i] * fx[i] + fy[i] * fy[i]).sqrt().max(1e-6);
        let dx = force[0] - fx[i];
        let dy = force[1] - fy[i];
        let relative = (dx * dx + dy * dy).sqrt() / magnitude;
        worst = worst.max(relative);
        mean += relative as f64;
    }
    mean /= n as f64;

    assert!(
        mean < 1e-3,
        "the kernel disagrees with quint's own law: mean relative error {mean:.2e}, worst {worst:.2e}"
    );
    // The z column must stay zero: these are 2D positions in a padded
    // 3D layout, and a force leaking into the spare axis would mean the
    // kernel is reading the padding as data.
    for force in &theirs {
        assert_eq!(force[2], 0.0, "repulsion leaked into the padded axis");
    }
}

#[test]
fn the_lane_settles_and_reports_it() {
    let Some(client) = client() else {
        eprintln!("no wgpu adapter: skipping the settle receipt");
        return;
    };

    let n = 256;
    let positions = scatter(n, 150.0);
    let offsets = no_edges(n);
    let mut resident = Resident::new(
        &client,
        &positions,
        Adjacency {
            offsets: &offsets,
            targets: &[],
        },
        Params::default(),
    );

    // A few steps in, the cloud is moving.
    for _ in 0..10 {
        resident.step();
    }
    let early = resident.max_speed();
    assert!(early > 0.0, "nothing moved at all");

    // Left alone under damping and centering, it comes to rest, and the
    // four-byte settle word is how a host knows without reading
    // positions.
    for _ in 0..600 {
        resident.step();
    }
    let late = resident.max_speed();
    assert!(late < early, "the lane never settled: {early} then {late}");

    // And the positions are finite: a diverging integrator shows up
    // here rather than as a blank screen much later.
    for position in resident.read_positions() {
        assert!(
            position[0].is_finite() && position[1].is_finite(),
            "the integrator diverged: {position:?}"
        );
    }
}

#[test]
fn springs_pull_two_bodies_to_their_rest_length() {
    let Some(client) = client() else {
        eprintln!("no wgpu adapter: skipping the spring receipt");
        return;
    };

    // Two bodies, one edge, far apart. With repulsion off, the spring
    // is the only force and its rest length is where they should end.
    let positions = vec![[-200.0, 0.0, 0.0, 0.0], [200.0, 0.0, 0.0, 0.0]];
    let mut resident = Resident::new(
        &client,
        &positions,
        Adjacency {
            offsets: &[0, 1, 2],
            targets: &[1, 0],
        },
        Params {
            repulsion: 0.0,
            centering: 0.0,
            spring_k: 0.5,
            rest_length: 60.0,
            ..Default::default()
        },
    );

    // Run until the lane says it has settled rather than for a fixed
    // count: a spring approaches its rest length asymptotically, and
    // guessing the step count is how a receipt ends up asserting on a
    // system still in motion (it was at 74 of 60 after 2,000 steps).
    // This also uses the settle word for the job it exists to do.
    let mut steps = 0;
    loop {
        resident.step();
        steps += 1;
        if resident.max_speed() < 1e-3 || steps > 200_000 {
            break;
        }
    }
    assert!(steps < 200_000, "the spring never came to rest");
    let settled = resident.read_positions();
    let separation = (settled[1][0] - settled[0][0]).abs();
    assert!(
        (separation - 60.0).abs() < 2.0,
        "the spring settled at {separation}, not its 60 rest length"
    );
}

/// The migration's own receipt: the lane's positions are a CubeCL
/// allocation, published as a lease whose stamp advances with the step.
///
/// This replaces the receipt that asserted a committed `.spv` was what
/// ran. There is no `.spv` any more: the kernels are CubeCL and compile
/// at first launch, so the question "did the artifact execute or did a
/// fallback" no longer exists. What is worth asserting instead is that
/// the field tier publishes the same contract the voxel tier does.
#[test]
fn the_lane_publishes_its_positions_as_a_stamped_lease() {
    let Some(client) = client() else {
        eprintln!("no wgpu adapter: skipping the resident lease receipt");
        return;
    };

    let n = 256;
    let positions = scatter(n, 120.0);
    let offsets = no_edges(n);
    let mut resident = Resident::new(
        &client,
        &positions,
        Adjacency {
            offsets: &offsets,
            targets: &[],
        },
        Params::default(),
    );

    // Read the lease's facts and let the borrow end: a lease borrows
    // the lane, and stepping it is a mutation.
    let (shape, byte_len, fits, revision) = {
        let lease = resident.positions_lease();
        (
            lease.shape,
            lease.byte_len(),
            lease.fits(),
            lease.stamp.revision,
        )
    };
    assert_eq!(shape, [n, 4, 1]);
    assert_eq!(byte_len, (n * 4 * 4) as u64);
    assert!(fits, "the lane published a lease it overruns");
    assert_eq!(revision, 0);

    resident.step();
    assert_eq!(
        resident.positions_lease().stamp.revision,
        1,
        "a step must advance the published revision, or a reader on          another cadence cannot tell fresh from stale"
    );

    // The forces allocation is published the same way, which is what
    // the tensor lane fills before an external-force step.
    let forces = resident.forces_lease();
    assert!(forces.fits());
    assert_eq!(forces.shape, shape);
}
