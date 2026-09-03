// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! **Distillery**, the model-works port of the Mere platform.
//!
//! A distillery takes a raw mash and runs it, batch by batch, through stills
//! into something concentrated. This port is that works for models: the
//! harness where a ring of devices runs inference, embedding, and eventually
//! training jobs. It splits in two, in the castellan mold:
//!
//! - an **embeddable half** any host app composes: the job board, lease and
//!   heartbeat tiles, the device roster, retention and device-policy panes,
//!   model-manifest browsing, a minimal streaming console. These views render
//!   what the supervisor reports, never a placebo.
//! - an **authority half** that lives with the device: the supervisor loop
//!   that owns mesh host ticks, device conditions, the resource registry, and
//!   blob custody. It answers to the owner; reclaim always wins.
//!
//! The boundaries are the point:
//!
//! - **Not esp.** The inference/embedding seam crate stays the portable burn
//!   boundary; distillery drives it.
//! - **Not mere-mesh or mere-mesh-host.** Job grammar, leases, and the
//!   supervisor are substrate; distillery embeds and renders them.
//! - **Not servitor.** Whether an admitted denizen may petition at all stays
//!   the gate's office.
//! - **Not turnstone.** The flagship embeds the same views; distillery is the
//!   standalone works.
//!
//! The trainer lane (Distillery-as-trainer, per the geist brief) lands here
//! later, behind its own plan: training is one more job the works runs.
//!
//! [`Distillery`] is the first real consumer of `mere-mesh-host`. Its D0
//! authority drives the non-blocking supervisor and owns checkpoint/collection.
//! [`ResidentAuthority`] is D1's long-lived process body: it owns persistent
//! collecting storage, the transport shutdown order, configurable tick and
//! maintenance cadence, and an ordered receipt stream. Views remain a later
//! slice; when they arrive they render those receipts rather than reconstructing
//! authority state.

#![doc(html_no_source)]
#![warn(missing_docs)]

mod authority;

#[cfg(feature = "flora")]
pub mod flora;
mod installed;
#[cfg(feature = "remote")]
mod remote;
mod resident;
mod surface;
#[cfg(feature = "trainer")]
mod trainer;

pub use authority::{
    BlobCustody, Distillery, DistilleryError, MaintenanceReport, RetentionSettings,
};
pub use installed::{
    DISTILLERY_MESH_SALT, DistilleryPaths, InstalledAuthority, InstalledError, InstalledSettings,
    InstalledSettingsError, distillery_settings_path,
};
#[cfg(feature = "remote")]
pub use remote::{
    BURN_REMOTE_RESOURCE, RemoteBurnResource, RemoteSessionError, RemoteSessionService,
    RemoteSessionSettings,
};
pub use resident::{
    ResidentAuthority, ResidentError, ResidentReceipt, ResidentSettings, ResidentStorage,
};
pub use surface::{
    DISTILLERY_INSTALLED_CSS, DistilleryInstalledSnapshotV1, DistilleryInstalledSurfaceState,
    DistilleryResidentSnapshotV1, distillery_installed_descriptor, distillery_installed_surface,
    distillery_installed_view,
};
#[cfg(feature = "trainer-gpu")]
pub use trainer::discrete_gpu_trainer_device;
#[cfg(feature = "trainer")]
pub use trainer::{
    TRAINER_REQUEST_INPUT, TRAINER_RESOURCE, TrainReceipt, TrainRequest, TrainerResource,
};

/// The trainer's own vocabulary, re-exported so a composition layer can wire
/// and drive the trainer without naming `esp` — the same courtesy esp already
/// pays `burn`.
///
/// These are not conveniences: [`TrainerResource::new`] takes a
/// [`TrainerDevice`], and [`TrainRequest`] carries a
/// [`LoraTrainerSettings`] and is fed by [`TrainingCase`] codicils, so a host
/// that cannot name them cannot compose the resource or post a job to it.
/// Re-exporting them here keeps the seam where it already is: distillery
/// drives esp, and its consumers drive distillery.
#[cfg(feature = "trainer")]
pub use esp::infer::decoder::{DecoderDevice as TrainerDevice, LoraTrainerSettings, TrainingCase};

/// The GPU half of that vocabulary, under `trainer-gpu`.
///
/// [`discrete_gpu_trainer_device`] returns [`GpuAdapterFacts`], and a host that
/// wants to log them, assert on them, or carry them into its own facts has to
/// be able to name the type and its [`GpuDeviceType`]. [`TrainerGpuKind`] is
/// there for a host that means to probe some other class itself rather than
/// take the discrete-GPU shorthand.
#[cfg(feature = "trainer-gpu")]
pub use esp::infer::decoder::{
    DecoderGpuKind as TrainerGpuKind, GpuAdapterFacts, GpuDeviceType, probe_gpu_adapter,
};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
