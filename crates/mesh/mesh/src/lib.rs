/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

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
//! - [`store`] — the [`MeshStore`]: p2panda-store's SQLite backend behind one
//!   write path that persists *and* sync-indexes each op atomically.
//! - [`sync`] — [`SyncedMesh`], mirroring tessera's `SyncedMoot`: the LogSync
//!   catch-up + live session over the store, plus the device's authoring
//!   path ([`SyncedMesh::author`]) and a real, non-placebo [`SyncStatus`].
//!
//! See the
//! [mesh M1 plan](https://github.com/mark-ik/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-06-12_mesh_m1_plan.md).

pub mod board;
pub mod store;
pub mod sync;
pub mod wire;
pub mod worker;

pub use board::{Job, JobBoard, JobId, JobState};
pub use store::{MeshStore, MeshStoreError};
pub use sync::{MeshSyncError, SyncRound, SyncStatus, SyncedMesh};
pub use wire::{from_operation, to_operation, verify, JobKind, MeshEvent, MeshExt, WireError};
pub use worker::{execute, next_action, WorkerAction};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
