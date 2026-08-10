// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # mesh-host
//!
//! The mesh host supervisor — gate **H0** of the host lanes plan.
//!
//! `mere-mesh` is a protocol floor: it folds signed facts without a clock,
//! decides what a device *should* do next, and refuses to look at the OS. What
//! it cannot do is run anything. This crate is the thing that runs it.
//!
//! The distinction that makes a lease possible at all: **execution happens off
//! the decision loop**. A host that awaits a job inside its own tick cannot
//! heartbeat while working and cannot stop when its owner wants the device
//! back — so it must not take a lease, and
//! [`DevicePolicy::supervises_leases`](mesh::DevicePolicy) exists to say so.
//! [`MeshHost::tick`] spawns runs, keeps their [`JobControlHandle`](mesh::JobControlHandle)s,
//! and never blocks.
//!
//! Five things this gets right that a loop-blocking host cannot:
//!
//! 1. A tick can heartbeat, reclaim, or claim while a job is running.
//! 2. Heartbeat progress comes from the running job's control handle, so it
//!    cannot claim work that did not happen.
//! 3. Owner reclaim **stops the run first and authors the revoke after**, so a
//!    peer reading the fact knows the hardware is genuinely free.
//! 4. A leased job completes with `JobCompletedUnderLease` naming its lease.
//! 5. A job that promised not to be interrupted gets its
//!    [`reclaim_grace_ms`](mesh::DevicePolicy) before the hard cancel — and
//!    loses the device anyway when it runs out.
//!
//! The OS enters through exactly two seams, both in [`sense`]: [`Clock`] and
//! [`ConditionSource`]. Nothing below this crate is allowed to care how a
//! device knows it is idle.
//!
//! ## What is not here
//!
//! Blob delivery (gate H1). A worker still needs the job's inputs already in
//! its own blob space; `NamespaceError::MissingBlob` is the honest failure, and
//! the supervisor reports it as [`ReleaseReason::InputUnavailable`](mesh::ReleaseReason)
//! rather than as an unreliable worker.
//!
//! See the
//! [mesh host lanes plan](https://github.com/merely-made/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-08-09_mesh_host_lanes_plan.md).

pub mod courier;
pub mod host;
pub mod inflight;
pub mod sense;

pub use courier::{
    BlobCourier, CourierError, NoCourier, TransportBlobSpace, TransportCourier, deliver_inputs,
};
pub use host::{BlobSpace, HostConfig, HostError, MeshHost, Step};
pub use inflight::{RunOutcome, still_held};
pub use sense::{Clock, ConditionSource, ManualClock, ObservedConditions, SystemClock};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
