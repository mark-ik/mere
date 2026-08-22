// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Long-lived Distillery lifecycle and its durable blob custody.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use mere_resident::{CloseAction, CloseFuture, close_all};
use mesh_host::{MeshHost, Step, TransportBlobSpace};
use muniment::Backend;
use tokio::time::{Instant, MissedTickBehavior, interval, interval_at};
use transport::{BlobError, BlobStore, P2pandaTransport};

use crate::{Distillery, DistilleryError, MaintenanceReport, RetentionSettings};

/// Owner-selected cadence and retention behavior for one resident authority.
///
/// There is deliberately no `Default`: a process entry point must project an
/// owner's settings into every cadence rather than quietly choosing policy in
/// the service layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResidentSettings {
    /// How often the non-blocking mesh supervisor gets a turn.
    pub tick_every: Duration,
    /// How often to checkpoint an advanced event frontier. `None` leaves
    /// maintenance as an explicit owner command.
    pub maintenance_every: Option<Duration>,
    /// How often the physical store looks for content with no custody tag.
    pub blob_gc_every: Duration,
    /// What an accepted maintenance checkpoint may release.
    pub retention: RetentionSettings,
}

impl ResidentSettings {
    /// Refuse cadences which would spin continuously or which Tokio cannot run.
    pub fn validate(&self) -> Result<(), ResidentError> {
        if self.tick_every.is_zero() {
            return Err(ResidentError::InvalidSettings(
                "supervisor tick cadence must be greater than zero",
            ));
        }
        if self.maintenance_every.is_some_and(|every| every.is_zero()) {
            return Err(ResidentError::InvalidSettings(
                "maintenance cadence must be greater than zero when enabled",
            ));
        }
        if self.blob_gc_every.is_zero() {
            return Err(ResidentError::InvalidSettings(
                "blob collection cadence must be greater than zero",
            ));
        }
        Ok(())
    }
}

/// Disk-backed bytes and the mesh-scoped custody view over them.
pub struct ResidentStorage {
    root: PathBuf,
    mesh_id: [u8; 32],
    gc_every: Duration,
    blobs: Arc<BlobStore>,
    space: Arc<TransportBlobSpace>,
}

impl ResidentStorage {
    /// Open the persistent collecting store used by a resident mesh host.
    pub async fn open(
        root: impl AsRef<Path>,
        mesh_id: [u8; 32],
        gc_every: Duration,
    ) -> Result<Self, ResidentError> {
        if gc_every.is_zero() {
            return Err(ResidentError::InvalidSettings(
                "blob collection cadence must be greater than zero",
            ));
        }
        let root = root.as_ref().to_path_buf();
        let blobs = Arc::new(BlobStore::open_collecting(&root, gc_every).await?);
        let space = Arc::new(TransportBlobSpace::for_mesh(blobs.clone(), mesh_id));
        Ok(Self {
            root,
            mesh_id,
            gc_every,
            blobs,
            space,
        })
    }

    /// Store root selected by the owner.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Mesh whose custody tags this storage view owns.
    pub fn mesh_id(&self) -> [u8; 32] {
        self.mesh_id
    }

    /// Physical collection cadence active in the opened store.
    pub fn gc_every(&self) -> Duration {
        self.gc_every
    }

    /// Persistent bytes to mount on the p2p transport.
    pub fn blobs(&self) -> Arc<BlobStore> {
        self.blobs.clone()
    }

    /// Mesh-scoped read, write, and custody access for [`MeshHost`].
    pub fn space(&self) -> Arc<TransportBlobSpace> {
        self.space.clone()
    }

    async fn shutdown(self) -> Result<(), BlobError> {
        let Self { blobs, space, .. } = self;
        drop(space);
        blobs.shutdown().await
    }
}

/// One exact observation from the resident authority lifecycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResidentReceipt {
    /// One supervisor turn and the substrate receipts it returned.
    Tick {
        /// Exact receipts returned by `MeshHost`.
        steps: Vec<Step>,
    },
    /// The event frontier advanced and maintenance committed.
    MaintenanceCompleted(Box<MaintenanceReport>),
    /// The cadence fired, but the frontier had not advanced.
    MaintenanceIdle,
    /// Maintenance was refused or its custody operation failed. This is
    /// observable and non-fatal; a live lease is an expected example.
    MaintenanceFailed {
        /// Display form of the maintenance refusal.
        error: String,
    },
    /// The supervisor itself failed, so the resident loop is ending.
    SupervisorFailed {
        /// Display form of the fatal supervisor error.
        error: String,
    },
    /// The caller's shutdown signal won the lifecycle race.
    StopRequested,
}

/// A resident Distillery failure.
#[derive(Debug, thiserror::Error)]
pub enum ResidentError {
    /// An owner setting cannot form a safe timer.
    #[error("resident settings: {0}")]
    InvalidSettings(&'static str),
    /// The durable custody scope and joined mesh disagree.
    #[error("resident storage belongs to another mesh")]
    MeshMismatch,
    /// The reported settings and already-opened storage cadence disagree.
    #[error("resident settings name another blob collection cadence than the open store")]
    StorageCadenceMismatch,
    /// The Distillery authority could not continue.
    #[error(transparent)]
    Authority(#[from] DistilleryError),
    /// The persistent blob store could not open.
    #[error(transparent)]
    Storage(#[from] BlobError),
    /// Shutdown tried all owned resources and reports every refusal.
    #[error(
        "resident shutdown failed (transport: {transport:?}, authority: {authority:?}, blob store: {storage:?})"
    )]
    Shutdown {
        /// Failure while closing the network endpoint.
        transport: Option<String>,
        /// Failure while stopping work and releasing the mesh store.
        authority: Option<String>,
        /// Failure while flushing and closing persistent blob storage.
        storage: Option<String>,
    },
}

/// The long-lived device authority, including resources whose shutdown order
/// matters.
///
/// Build `host` with [`ResidentStorage::space`] and mount
/// [`ResidentStorage::blobs`] on `transport` before moving all three here. The
/// resident then owns the endpoint lifetime, the joined mesh, and the final
/// blob-store flush as one lifecycle.
pub struct ResidentAuthority<B: Backend + Clone + Send + Sync + 'static> {
    authority: Distillery<B>,
    transport: Arc<P2pandaTransport>,
    storage: ResidentStorage,
    settings: ResidentSettings,
}

impl<B: Backend + Clone + Send + Sync + 'static> ResidentAuthority<B> {
    /// Take ownership of a fully composed resident host.
    pub fn new(
        host: MeshHost<B>,
        transport: Arc<P2pandaTransport>,
        storage: ResidentStorage,
        settings: ResidentSettings,
    ) -> Result<Self, ResidentError> {
        settings.validate()?;
        if host.synced().mesh_id() != storage.mesh_id() {
            return Err(ResidentError::MeshMismatch);
        }
        if settings.blob_gc_every != storage.gc_every() {
            return Err(ResidentError::StorageCadenceMismatch);
        }
        let authority = Distillery::new(host, storage.space(), settings.retention);
        Ok(Self {
            authority,
            transport,
            storage,
            settings,
        })
    }

    /// The owner settings projected into this run.
    pub fn settings(&self) -> ResidentSettings {
        self.settings
    }

    /// The product authority for posting jobs and read-only projections.
    pub fn authority(&self) -> &Distillery<B> {
        &self.authority
    }

    /// Mutable owner policy access between resident runs.
    pub fn authority_mut(&mut self) -> &mut Distillery<B> {
        &mut self.authority
    }

    /// The live transport, for pairing and reachability projection.
    pub fn transport(&self) -> &Arc<P2pandaTransport> {
        &self.transport
    }

    /// The persistent custody surface.
    pub fn storage(&self) -> &ResidentStorage {
        &self.storage
    }

    /// Drive the authority until `shutdown` resolves.
    ///
    /// Receipts are delivered synchronously and in authority order, so an
    /// embeddable view can project them without rebuilding mesh truth. A
    /// maintenance failure remains visible and the supervisor keeps running;
    /// a supervisor failure emits its receipt and ends the run.
    pub async fn run_until<S, O>(
        &mut self,
        shutdown: S,
        mut observe: O,
    ) -> Result<(), ResidentError>
    where
        S: Future<Output = ()>,
        O: FnMut(ResidentReceipt),
    {
        let mut shutdown = Box::pin(shutdown);
        let mut ticks = interval(self.settings.tick_every);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut maintenance = self.settings.maintenance_every.map(|every| {
            let mut interval = interval_at(Instant::now() + every, every);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            interval
        });

        loop {
            tokio::select! {
                biased;
                _ = &mut shutdown => {
                    observe(ResidentReceipt::StopRequested);
                    return Ok(());
                }
                _ = ticks.tick() => {
                    match self.authority.tick().await {
                        Ok(steps) => observe(ResidentReceipt::Tick { steps }),
                        Err(error) => {
                            observe(ResidentReceipt::SupervisorFailed {
                                error: error.to_string(),
                            });
                            return Err(error.into());
                        }
                    }
                }
                _ = async {
                    match maintenance.as_mut() {
                        Some(interval) => {
                            interval.tick().await;
                        }
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    match self.authority.maintain_if_advanced().await {
                        Ok(Some(report)) => {
                            observe(ResidentReceipt::MaintenanceCompleted(Box::new(report)));
                        }
                        Ok(None) => observe(ResidentReceipt::MaintenanceIdle),
                        Err(error) => observe(ResidentReceipt::MaintenanceFailed {
                            error: error.to_string(),
                        }),
                    }
                }
            }
        }
    }

    /// Close the endpoint, drop the joined mesh, then flush and close blob
    /// storage. All close attempts run even when an earlier one fails.
    pub async fn shutdown(self) -> Result<(), ResidentError> {
        let Self {
            authority,
            transport,
            storage,
            ..
        } = self;
        let report = close_all(vec![
            (
                "transport",
                Box::new(move || {
                    Box::pin(
                        async move { transport.close().await.map_err(|error| error.to_string()) },
                    ) as CloseFuture<'static>
                }) as CloseAction<'static>,
            ),
            (
                "authority",
                Box::new(move || {
                    Box::pin(async move {
                        authority
                            .shutdown()
                            .await
                            .map_err(|error| error.to_string())
                    }) as CloseFuture<'static>
                }) as CloseAction<'static>,
            ),
            (
                "storage",
                Box::new(move || {
                    Box::pin(
                        async move { storage.shutdown().await.map_err(|error| error.to_string()) },
                    ) as CloseFuture<'static>
                }) as CloseAction<'static>,
            ),
        ])
        .await;
        let transport_error = report.failure("transport").map(str::to_owned);
        let authority_error = report.failure("authority").map(str::to_owned);
        let storage_error = report.failure("storage").map(str::to_owned);
        if transport_error.is_none() && authority_error.is_none() && storage_error.is_none() {
            Ok(())
        } else {
            Err(ResidentError::Shutdown {
                transport: transport_error,
                authority: authority_error,
                storage: storage_error,
            })
        }
    }
}
