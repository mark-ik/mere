// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The device-side Distillery authority.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use mesh::{BlobRef, JobBoard, MeshStoreError, RetentionCheckpoint, RetentionEffect};
use mesh_host::{HostError, MeshHost, Step, TransportBlobSpace};
use muniment::Backend;

type CustodyFuture<'a> = Pin<Box<dyn Future<Output = Result<u64, String>> + Send + 'a>>;

/// The blob owner Distillery asks to release settled mesh references.
///
/// `collect` returns how many custody claims were actually removed. The
/// custody operation itself is idempotent. Physical content may remain when
/// another mesh or subsystem retains the same content-addressed hash.
pub trait BlobCustody: Send + Sync {
    /// Release this authority's custody claims for `blobs`.
    fn collect<'a>(&'a self, blobs: &'a [BlobRef]) -> CustodyFuture<'a>;
}

impl BlobCustody for TransportBlobSpace {
    fn collect<'a>(&'a self, blobs: &'a [BlobRef]) -> CustodyFuture<'a> {
        Box::pin(async move { self.release(blobs).await.map_err(|error| error.to_string()) })
    }
}

/// Owner-controlled retention behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetentionSettings {
    /// Release this mesh's blob tags after an accepted checkpoint says every
    /// current reference is settled. Off by default: keeping bytes is the
    /// conservative owner policy.
    pub collect_after_checkpoint: bool,
}

/// What one explicit maintenance operation accomplished.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaintenanceReport {
    /// The accepted checkpoint this operation authored.
    pub checkpoint: RetentionCheckpoint,
    /// Distinct blob references that were safe to release at the accepted
    /// checkpoint and current replay tail.
    pub candidates: u64,
    /// Custody claims actually removed.
    pub collected: u64,
    /// User-facing durable effects produced by the operation.
    pub effects: Vec<RetentionEffect>,
}

/// A Distillery authority failure.
#[derive(Debug, thiserror::Error)]
pub enum DistilleryError {
    /// The mesh supervisor or its sync lane failed.
    #[error(transparent)]
    Host(#[from] HostError),
    /// The mesh retention store could not derive its safe release set.
    #[error(transparent)]
    Store(#[from] MeshStoreError),
    /// The configured blob custodian refused the release.
    #[error("blob custody: {0}")]
    Custody(String),
}

/// Type-erased board projection consumed by an optional remote-session lane.
#[cfg(feature = "remote")]
pub trait RemoteSessionProjection: Send + Sync {
    /// Replace the admission snapshot with the host's current folded board.
    fn refresh(&self, board: JobBoard);
}

/// The resident device-side model works.
///
/// This is deliberately a consumer of [`MeshHost`], not another scheduler. It
/// drives host ticks, exposes their exact [`Step`] receipts, and decides when
/// owner-governed retention maintenance runs. Mesh keeps job and lease truth;
/// Distillery composes that substrate into a resident service.
pub struct Distillery<B: Backend + Clone + Send + Sync + 'static> {
    host: MeshHost<B>,
    custody: Arc<dyn BlobCustody>,
    retention: RetentionSettings,
    #[cfg(feature = "remote")]
    remote: Option<Arc<dyn RemoteSessionProjection>>,
}

impl<B: Backend + Clone + Send + Sync + 'static> Distillery<B> {
    /// Compose a supervised mesh host with its blob custodian and owner
    /// retention settings.
    pub fn new(
        host: MeshHost<B>,
        custody: Arc<dyn BlobCustody>,
        retention: RetentionSettings,
    ) -> Self {
        Self {
            host,
            custody,
            retention,
            #[cfg(feature = "remote")]
            remote: None,
        }
    }

    /// Attach the remote-session authority composed into this host's resource
    /// registry and transport endpoint.
    #[cfg(feature = "remote")]
    pub fn attach_remote_sessions(&mut self, remote: Arc<dyn RemoteSessionProjection>) {
        self.remote = Some(remote);
    }

    /// Drive one non-blocking supervisor tick.
    pub async fn tick(&mut self) -> Result<Vec<Step>, DistilleryError> {
        #[cfg(feature = "remote")]
        self.refresh_remote().await?;
        let steps = self.host.tick().await?;
        #[cfg(feature = "remote")]
        self.refresh_remote().await?;
        Ok(steps)
    }

    #[cfg(feature = "remote")]
    async fn refresh_remote(&self) -> Result<(), DistilleryError> {
        if let Some(remote) = &self.remote {
            let board = self.host.synced().board().await?;
            remote.refresh(board);
        }
        Ok(())
    }

    /// The substrate host, for read-only board, progress, and sync projections.
    pub fn host(&self) -> &MeshHost<B> {
        &self.host
    }

    /// The substrate host's owner-controlled policy surface.
    pub fn host_mut(&mut self) -> &mut MeshHost<B> {
        &mut self.host
    }

    /// Current owner retention settings.
    pub fn retention(&self) -> RetentionSettings {
        self.retention
    }

    /// Replace the owner retention settings used by the next maintenance run.
    pub fn set_retention(&mut self, retention: RetentionSettings) {
        self.retention = retention;
    }

    /// Stop local work and wait until the joined mesh has released its store.
    pub async fn shutdown(self) -> Result<(), DistilleryError> {
        self.host.shutdown().await?;
        Ok(())
    }

    /// Author a checkpoint and, if enabled, release the mesh's settled blob
    /// custody claims.
    ///
    /// The mesh store refuses the checkpoint while a live lease needs its
    /// history. After acceptance, the safe set is evaluated against both that
    /// checkpoint and the current replay tail, so a later or unfinished job
    /// sharing a content hash keeps it protected.
    pub async fn maintain(&self) -> Result<MaintenanceReport, DistilleryError> {
        let checkpoint = self.host.checkpoint().await?;
        self.finish_maintenance(checkpoint).await
    }

    /// Run maintenance only when the mesh's event frontier has advanced.
    ///
    /// This is the resident-loop operation. [`Self::maintain`] remains the
    /// explicit owner command and always authors a checkpoint; a cadence uses
    /// this method so an idle mesh does not accumulate identical checkpoints.
    pub async fn maintain_if_advanced(&self) -> Result<Option<MaintenanceReport>, DistilleryError> {
        let Some(checkpoint) = self.host.checkpoint_if_advanced().await? else {
            return Ok(None);
        };
        self.finish_maintenance(checkpoint).await.map(Some)
    }

    async fn finish_maintenance(
        &self,
        checkpoint: RetentionCheckpoint,
    ) -> Result<MaintenanceReport, DistilleryError> {
        let synced = self.host.synced();
        let blobs = synced.store().collectable_blobs(synced.mesh_id()).await?;
        let collected = if self.retention.collect_after_checkpoint {
            self.custody
                .collect(&blobs)
                .await
                .map_err(DistilleryError::Custody)?
        } else {
            0
        };
        let effects = (collected != 0)
            .then_some(RetentionEffect::BlobCollected { count: collected })
            .into_iter()
            .collect();

        Ok(MaintenanceReport {
            checkpoint,
            candidates: blobs.len() as u64,
            collected,
            effects,
        })
    }
}
