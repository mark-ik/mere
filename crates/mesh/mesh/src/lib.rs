// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # mesh
//!
//! The personal-space compute mesh — resource-coordination **milestone 1**:
//! one owned device posts a job into the shared space, another claims it and
//! returns the result, replicated over the same signed-operation event-DAG
//! tessera and murm ride (LogSync underneath). **No economy**: every peer
//! holding the mesh id is trusted (the own-devices ring, where sharing is
//! scheduling and permissions, never verification markets).
//!
//! Five layers, innermost first:
//!
//! - [`wire`] — job events as signed `Operation<MeshExt>`s (the mesh id is the
//!   signed addressing extension, so an op cannot replay into another mesh).
//! - [`board`] — the [`JobBoard`]: a **deterministic, order-independent fold**
//!   of the op log into job state (`posted → claimed → done`). Claim races
//!   resolve identically on every peer (lowest claim-op hash wins), and only
//!   the winner's result is accepted.
//! - [`worker`] — the pure decision function ([`next_action`]) + the M1 job
//!   executors ([`execute`]: `Echo`, `Blake3`). The *host* drives the loop
//!   (the `mesh-peer` bin now; meerkat's P6 compute actor later).
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
//! [mesh M1 plan](https://github.com/mark-ik/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-06-12_mesh_m1_plan.md).

pub mod board;
pub mod drop_export;
pub mod retention;
pub mod store;
pub mod sync;
pub mod wire;
pub mod worker;

pub use board::{Job, JobBoard, JobId, JobState};
pub use drop_export::{MeshDropPriorities, MeshDropPrivacy, MeshDropProfile, MeshDropSelector};
pub use proofs::{BlobRef, Commitment, CommitmentDomain, CommitmentScheme, Digest, DigestAlg};
pub use retention::{
    AvailabilityPolicy, CheckpointError, ErasurePolicy, JobBoardSnapshot, KeepBound, LogFrontier,
    MeshRetentionPolicy, PayloadRule, PolicyRevision, RetentionCheckpoint, RetentionEffect,
};
pub use store::{MeshStore, MeshStoreError, StoredCheckpoint};
pub use sync::{MeshSyncError, SyncRound, SyncStatus, SyncedMesh};
pub use wire::{
    JobKind, MeshEvent, MeshExt, MeshLogId, WireError, from_operation, to_operation,
    to_prune_operation, verify,
};
pub use worker::{WorkerAction, execute, next_action};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
