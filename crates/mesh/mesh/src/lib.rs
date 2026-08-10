// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # mesh
//!
//! The personal-space compute mesh — resource-coordination **milestone 2**:
//! one owned device posts a job into the shared space, another claims it and
//! runs it against a *restricted namespace*, returning a content-addressed
//! output; all of it replicated over the same signed-operation event-DAG
//! tessera and murm ride (LogSync underneath). **No economy**: every peer
//! holding the mesh id is trusted (the own-devices ring, where sharing is
//! scheduling and permissions, never verification markets). Trusted still does
//! not mean unbounded — a worker sees only the inputs and output slot one job
//! granted it.
//!
//! Layers, innermost first:
//!
//! - [`ident`] / [`spec`] — the V2 job vocabulary: extensible [`ResourceId`]s
//!   and the [`JobSpec`] manifest (named blob inputs, one granted output).
//! - [`wire`] — job events as signed `Operation<MeshExt>`s (the mesh id is the
//!   signed addressing extension, so an op cannot replay into another mesh).
//!   Two generations coexist: M1's inline pair and V2's spec/output pair.
//! - [`board`] — the [`JobBoard`]: a **deterministic, order-independent fold**
//!   of the op log into job state (`posted → claimed → done | committed`).
//!   Claim races resolve identically on every peer (lowest claim-op hash
//!   wins), only the winner's result is accepted, and a V2 result is accepted
//!   only when it honours the signed grant.
//! - [`namespace`] — the [`JobNamespaceView`]: the *only* door a resource has
//!   to data. Named reads, one granted writer, no ambient resolver. The host
//!   builds it; a `BlobRef` in a signed spec never authorizes anything by
//!   itself.
//! - [`resource`] / [`registry`] — the [`MeshResource`] adapter seam (async
//!   prepare + execute under a host-owned [`JobControl`]) and the one
//!   [`ResourceRegistry`] that maps a [`ResourceId`] to it. Adding a resource
//!   touches neither `wire.rs` nor `JobBoard::fold`.
//! - [`resources`] — the shipped adapters: `mesh.echo/v1`, `mesh.blake3/v1`
//!   (the M1 kinds, now behind one execution route) and
//!   `esp.embed.lexical/v1`.
//! - [`lease`] / [`projection`] — lending an owned device safely. The author
//!   signs a [`LeaseTerms`] envelope once; the deterministic claim winner
//!   grants itself a lease inside it. The fold keeps those facts **without a
//!   clock** (signed timestamp against signed timestamp); liveness is a
//!   separate question asked with an explicit observation time,
//!   `job.lease_at(now_ms, &policy)`.
//! - [`policy`] — [`DevicePolicy`] and [`DeviceConditions`]: what this device
//!   will lend and what it is doing right now. Host-supplied; `mere-mesh` never
//!   queries the OS. Owner reclaim outranks every job.
//! - [`worker`] — the pure decision function ([`next_action`]), which selects
//!   only work this host advertises capability for and hands the owner's
//!   reclaim priority ahead of everything else. The *host* drives the loop
//!   (the `mesh-peer` bin now; turnstone's compute actor later).
//! - [`store`] — the [`MeshStore`]: the shared muniment operation store behind
//!   one policy-before-insert path that validates, admits, and indexes each op
//!   atomically. Retention checkpoints live in a separate author log from job
//!   events, so an authorized event-prefix cut cannot delete its own trust
//!   root. Checkpoint acceptance can erase terminal input bodies in the same
//!   backend batch while retaining signed headers and compact results.
//! - [`drop_export`] — mesh-owned catch-up, archive, and radio selection over
//!   the shared native-drop exporter. Privacy and priority remain explicit
//!   settings; catch-up selects the current checkpoint plus its replay tail.
//! - [`sync`] — [`SyncedMesh`], mirroring tessera's `SyncedMoot`: the LogSync
//!   catch-up + live session over the store, plus the device's authoring
//!   path ([`SyncedMesh::author`]) and a real, non-placebo [`SyncStatus`].
//!
//! See the
//! [mesh M2 plan](https://github.com/merely-made/mere/blob/main/design_docs/archive_docs/2026-08-09_completed_plans/2026-06-30_personal_mesh_substrate_m2_plan.md)
//! (landed 2026-08-09) and the
//! [lease scheduler plan](https://github.com/merely-made/mere/blob/main/design_docs/archive_docs/2026-08-09_completed_plans/2026-06-30_mesh_lease_scheduler_plan.md)
//! that followed it (both landed 2026-08-09). The lanes both plans leaned on
//! and did not build — blob delivery above all — are the
//! [mesh host lanes plan](https://github.com/merely-made/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-08-09_mesh_host_lanes_plan.md).

pub mod board;
pub mod directory;
pub mod drop_export;
mod fold;
pub mod ident;
pub mod lease;
pub mod namespace;
pub mod policy;
pub mod projection;
pub mod registry;
pub mod resource;
pub mod resources;
pub mod retention;
pub mod spec;
pub mod store;
pub mod sync;
pub mod wire;
pub mod worker;

pub use board::{Job, JobBoard, JobId, JobState};
pub use directory::{DeviceDirectory, MESH_AUTHOR_SALT, attests};
pub use drop_export::{MeshDropPriorities, MeshDropPrivacy, MeshDropProfile, MeshDropSelector};
pub use ident::{IdentError, ImplementationId, ResourceId};
pub use lease::{
    LeaseActivity, LeaseEnd, LeaseEpoch, LeaseId, LeaseProgress, LeaseRecord, LeaseTerms,
    LeaseTermsError, ReclaimReason, ReleaseReason,
};
pub use namespace::{
    BlobSink, BlobSource, JobNamespaceView, MemoryBlobSpace, NamespaceError, OutputCommit,
};
pub use policy::{DeviceConditions, DevicePolicy, NetworkClass, QuietHours};
pub use projection::{LapseReason, LeasePhase, LeasePolicy};
pub use proofs::{BlobRef, Commitment, CommitmentDomain, CommitmentScheme, Digest, DigestAlg};
pub use registry::{RegistryError, ResourceRegistry, RunError, Verdict, run_job, verify_output};
pub use resource::{
    Cancelled, Checkpoint, ControlSignal, JobControl, JobControlHandle, MeshResource, Prepared,
    ResourceDescriptor, ResourceError,
};
pub use retention::{
    AvailabilityPolicy, CheckpointError, ErasurePolicy, JobBoardSnapshot, KeepBound, LogFrontier,
    MeshRetentionPolicy, PayloadRule, PolicyRevision, RetentionCheckpoint, RetentionEffect,
};
pub use spec::{
    CheckpointClass, ComputeClass, DeterminismClass, HostFacts, JobInput, JobOutput, JobSpec,
    OutputError, OutputGrant, ResourceRequirements, SpecError, VerificationClass,
};
pub use store::{MeshStore, MeshStoreError, StoredCheckpoint};
pub use sync::{MeshSyncError, SyncRound, SyncStatus, SyncedMesh};
pub use wire::{
    JobKind, MeshEvent, MeshExt, MeshLogId, WireError, from_operation, to_operation,
    to_prune_operation, verify,
};
pub use worker::{HostOffer, WorkerAction, next_action};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
