// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The supervisor: one tick, and everything it is allowed to do.

use std::collections::BTreeMap;
use std::sync::Arc;

use identity::{DerivedKeyAttestation, Ed25519Keypair};
use mesh::{
    BlobSink, BlobSource, DevicePolicy, HostFacts, HostOffer, JobControl, JobId, LeaseId,
    LeasePolicy, LeaseProgress, MeshEvent, MeshStoreError, MeshSyncError, ReclaimReason,
    ReleaseReason, ResourceRegistry, SyncedMesh, WorkerAction, next_action,
    registry::{run_job, run_legacy},
    spec::CheckpointClass,
};
use muniment::Backend;

use crate::courier::{BlobCourier, NoCourier, deliver_inputs};
use crate::inflight::{InFlight, Reclaiming, RunOutcome, release_reason, still_held};
use crate::sense::{Clock, ConditionSource};

/// Read and write access to this device's blobs, as one object.
///
/// The accessors are explicit rather than relying on trait upcasting so the
/// crate does not quietly depend on a recent language feature.
pub trait BlobSpace: BlobSource + BlobSink + Send + Sync {
    fn as_source(&self) -> &dyn BlobSource;
    fn as_sink(&self) -> &dyn BlobSink;
}

impl<T: BlobSource + BlobSink + Send + Sync> BlobSpace for T {
    fn as_source(&self) -> &dyn BlobSource {
        self
    }
    fn as_sink(&self) -> &dyn BlobSink {
        self
    }
}

/// Everything the supervisor needs that is not the sync lane itself.
pub struct HostConfig {
    pub registry: ResourceRegistry,
    pub blobs: Arc<dyn BlobSpace>,
    /// How this device gets a job's inputs when it does not already hold them.
    pub courier: Arc<dyn BlobCourier>,
    pub clock: Arc<dyn Clock>,
    pub conditions: Arc<dyn ConditionSource>,
    pub facts: HostFacts,
    pub policy: DevicePolicy,
    pub lease: LeasePolicy,
}

impl HostConfig {
    /// A supervised device: every built-in resource, the wall clock, an idle
    /// machine, and a policy that lends freely. Override the fields a real host
    /// actually knows.
    pub fn supervised(blobs: Arc<dyn BlobSpace>) -> Self {
        Self {
            registry: ResourceRegistry::builtin(),
            blobs,
            // No delivery lane by default: a host wires `TransportCourier` when
            // it has a transport to pull over.
            courier: Arc::new(NoCourier),
            clock: Arc::new(crate::sense::SystemClock),
            conditions: Arc::new(crate::sense::ObservedConditions::spare()),
            facts: HostFacts::cpu(4096),
            policy: DevicePolicy::permissive(),
            lease: LeasePolicy::default(),
        }
    }
}

/// What one tick did. Returned rather than logged so a caller — a test, a UI,
/// a status pane — can see the supervisor's actual behaviour instead of
/// inferring it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Claimed {
        job: JobId,
    },
    Granted {
        job: JobId,
        epoch: u32,
        lease: LeaseId,
    },
    Heartbeat {
        job: JobId,
        lease: LeaseId,
        progress: LeaseProgress,
    },
    /// A run started, off the decision loop.
    Started {
        job: JobId,
        lease: Option<LeaseId>,
    },
    Completed {
        job: JobId,
        lease: Option<LeaseId>,
    },
    Released {
        job: JobId,
        lease: LeaseId,
        reason: ReleaseReason,
    },
    /// The owner asked for the device back; the run has been signalled but has
    /// not stopped yet. No revoke is authored while this is the answer.
    AwaitingStop {
        job: JobId,
    },
    /// The run stopped and the revoke is now on the wire. `stopped_at` is what
    /// the run had actually done when it let go.
    Reclaimed {
        job: JobId,
        lease: LeaseId,
        reason: ReclaimReason,
        stopped_at: LeaseProgress,
    },
    /// This device was running under a lease it no longer holds, so the run was
    /// abandoned. Nothing is authored: a lease that lost is a lease that was
    /// never this device's to report on.
    LeaseLost {
        job: JobId,
        lease: LeaseId,
    },
    Idle,
}

/// A supervisor failure. Job failures are not errors — they are [`Step`]s.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error(transparent)]
    Sync(#[from] MeshSyncError),
    #[error(transparent)]
    Store(#[from] MeshStoreError),
}

/// The host supervisor.
///
/// Owns the in-flight job map and their control handles, the registry, blob
/// access, and the OS-facing seams. Drives [`next_action`] on a tick that never
/// blocks on execution, which is what makes heartbeats and owner reclaim
/// possible at all.
pub struct MeshHost<B: Backend + Clone + Send + Sync + 'static> {
    synced: SyncedMesh<B>,
    keypair: Ed25519Keypair,
    me: [u8; 32],
    config: HostConfig,
    inflight: BTreeMap<JobId, InFlight>,
}

impl<B: Backend + Clone + Send + Sync + 'static> MeshHost<B> {
    pub fn new(synced: SyncedMesh<B>, keypair: Ed25519Keypair, config: HostConfig) -> Self {
        let me = keypair.public_key().to_bytes();
        Self {
            synced,
            keypair,
            me,
            config,
            inflight: BTreeMap::new(),
        }
    }

    pub fn me(&self) -> [u8; 32] {
        self.me
    }

    pub fn synced(&self) -> &SyncedMesh<B> {
        &self.synced
    }

    /// Jobs running on this device right now.
    pub fn running(&self) -> impl Iterator<Item = JobId> + '_ {
        self.inflight.keys().copied()
    }

    /// What a running job last reported.
    pub fn progress(&self, job: JobId) -> Option<LeaseProgress> {
        self.inflight.get(&job).map(|f| f.handle.progress())
    }

    pub fn policy(&self) -> &DevicePolicy {
        &self.config.policy
    }

    /// Replace the owner's settings. The new policy governs the very next tick:
    /// it changes what this device will *offer* immediately, and — because the
    /// same settings decide whether running work must stop — can reclaim a job
    /// already in flight.
    pub fn set_policy(&mut self, policy: DevicePolicy) {
        self.config.policy = policy;
    }

    /// Author an owner-governed retention checkpoint at this host's current
    /// clock reading.
    ///
    /// This is explicit rather than part of [`tick`](Self::tick): retention is
    /// an owner setting and may be refused while a live lease needs the event
    /// prefix. A product host such as Distillery decides when maintenance runs.
    pub async fn checkpoint(&self) -> Result<mesh::RetentionCheckpoint, HostError> {
        let (_, checkpoint) = self
            .synced
            .checkpoint(&self.keypair, self.config.clock.now_ms())
            .await?;
        Ok(checkpoint)
    }

    /// Tell the ring which persona master key authorized this device's mesh
    /// authoring key, so peers can turn its jobs into a transport address.
    ///
    /// The attestation must be minted by the identity provider that owns the
    /// master — the supervisor holds only the derived key, and deliberately
    /// cannot forge one. Mint it with
    /// `provider.attest_derived_key(mesh::MESH_AUTHOR_SALT)`.
    pub async fn announce(&self, attestation: DerivedKeyAttestation) -> Result<(), HostError> {
        self.author(&MeshEvent::DeviceAttested {
            attestation: Box::new(attestation),
        })
        .await?;
        Ok(())
    }

    /// One pass: reap finished runs, escalate any overdue reclaim, then take at
    /// most one new action. Never awaits a job's execution.
    pub async fn tick(&mut self) -> Result<Vec<Step>, HostError> {
        let now_ms = self.config.clock.now_ms();
        let mut steps = self.reap(now_ms).await?;
        let board = self.synced.board().await?;
        steps.extend(self.abandon_lost_leases(&board, now_ms));
        self.escalate_reclaims(now_ms);

        let action = {
            let mut conditions = self.config.conditions.conditions();
            // The supervisor owns this number; a source that guessed it could
            // talk the device past its own concurrency limit.
            conditions.running_jobs = self.inflight.len() as u32;
            let running: Vec<JobId> = self.inflight.keys().copied().collect();
            let offer = HostOffer::new(
                &self.config.registry,
                self.config.facts,
                &self.config.policy,
            )
            .at(now_ms)
            .observing(conditions)
            .running(&running)
            .with_lease_policy(self.config.lease);
            next_action(&board, &self.me, &offer)
        };

        match action {
            WorkerAction::Idle => steps.push(Step::Idle),
            WorkerAction::Claim(job) => {
                self.author(&MeshEvent::JobClaimed {
                    job: job.0,
                    at_ms: now_ms,
                })
                .await?;
                steps.push(Step::Claimed { job });
            }
            WorkerAction::Grant {
                job,
                epoch,
                granted_at_ms,
                expires_at_ms,
            } => {
                let op = self
                    .author(&MeshEvent::LeaseGranted {
                        job: job.0,
                        epoch,
                        granted_at_ms,
                        expires_at_ms,
                    })
                    .await?;
                steps.push(Step::Granted {
                    job,
                    epoch,
                    lease: LeaseId(op),
                });
            }
            WorkerAction::Heartbeat { job, lease } => {
                // The payload comes from the run itself, so a heartbeat cannot
                // claim progress that did not happen.
                let progress = self
                    .inflight
                    .get(&job)
                    .map(|f| f.handle.progress())
                    .unwrap_or_default();
                self.author(&MeshEvent::LeaseHeartbeat {
                    job: job.0,
                    lease: lease.0,
                    progress,
                    at_ms: now_ms,
                })
                .await?;
                steps.push(Step::Heartbeat {
                    job,
                    lease,
                    progress,
                });
            }
            WorkerAction::Execute(job) => {
                if let Some(step) = self.start(&board, job, now_ms) {
                    steps.push(step);
                }
            }
            WorkerAction::Reclaim { job, lease, reason } => {
                let class = board
                    .job(job)
                    .and_then(|j| j.spec.as_ref())
                    .map_or(CheckpointClass::Restart, |spec| spec.checkpoint);
                self.begin_reclaim(job, reason, class, now_ms);
                steps.push(Step::AwaitingStop { job });
                let _ = lease;
            }
        }
        Ok(steps)
    }

    /// Stop running anything this device no longer holds the lease for.
    ///
    /// A device can grant itself a lease on a board that has not caught up, win
    /// locally, and start work — and then lose to a peer's earlier grant once
    /// the facts arrive. That is the protocol working (the fold picks one
    /// winner), but the loser must notice: its run is wasted work, and a
    /// completion under a lease that never survived would be silently dropped
    /// by every peer. The same check covers an expired lease and a revoke
    /// authored elsewhere.
    ///
    /// Nothing is authored — a lease this device does not hold is not its to
    /// release. The cancelled task detaches and exits at its next cooperative
    /// point.
    fn abandon_lost_leases(&mut self, board: &mesh::JobBoard, now_ms: u64) -> Vec<Step> {
        let lost: Vec<(JobId, LeaseId)> = self
            .inflight
            .iter()
            .filter_map(|(job, flight)| {
                let held = flight.lease?;
                let ours = still_held(board, *job, held, &self.me, now_ms, &self.config.lease);
                (!ours).then_some((*job, held))
            })
            .collect();

        lost.into_iter()
            .map(|(job, lease)| {
                if let Some(flight) = self.inflight.remove(&job) {
                    flight.handle.cancel();
                }
                Step::LeaseLost { job, lease }
            })
            .collect()
    }

    /// Who to ask for a job's inputs, best first.
    ///
    /// The poster is the obvious holder — it named the bytes, so it had them.
    /// Every other attested device follows, because a peer that already fetched
    /// the blob is just as good a source and the directory knows who they are.
    /// This device is never in the list: it has already looked locally.
    fn blob_sources(&self, board: &mesh::JobBoard, posted_by: [u8; 32]) -> Vec<[u8; 32]> {
        let directory = board.devices();
        let mut sources: Vec<[u8; 32]> = directory
            .master_of(&posted_by)
            .filter(|_| posted_by != self.me)
            .into_iter()
            .collect();
        sources.extend(
            directory
                .entries()
                .filter(|(author, _)| **author != posted_by && **author != self.me)
                .map(|(_, master)| *master),
        );
        sources
    }

    /// Spawn a run off the decision loop.
    fn start(&mut self, board: &mesh::JobBoard, job: JobId, now_ms: u64) -> Option<Step> {
        let record = board.job(job)?;
        // Bind the run to a lease only while this device actually holds it. A
        // phase can carry a lease id that belongs to somebody else (reclaimed,
        // lapsed, done), and tagging a run with one of those would author a
        // completion no peer would accept.
        let lease = match record.lease_at(now_ms, &self.config.lease) {
            mesh::LeasePhase::Held { lease, holder, .. } if holder == self.me => Some(lease),
            _ if record.lease_terms().is_some() => return None,
            _ => None,
        };
        let registry = self.config.registry.clone();
        let blobs = self.config.blobs.clone();
        let courier = self.config.courier.clone();
        let (handle, control) = JobControl::new();

        let task = match (&record.spec, record.kind) {
            (Some(spec), _) => {
                let spec = spec.as_ref().clone();
                let from = self.blob_sources(board, record.posted_by);
                tokio::spawn(async move {
                    // Grant first, then fetch under the lease: pulling before
                    // the grant spends bandwidth on races this device may lose.
                    deliver_inputs(&spec, &*blobs, &*courier, &from, &control).await;
                    run_job(
                        &registry,
                        &spec,
                        blobs.as_source(),
                        blobs.as_sink(),
                        &control,
                    )
                    .await
                    .map(|output| RunOutcome::Committed(Box::new(output)))
                })
            }
            (None, Some(kind)) => {
                let payload = record.payload.clone()?;
                tokio::spawn(async move {
                    run_legacy(&registry, kind, &payload, &control)
                        .await
                        .map(RunOutcome::Inline)
                })
            }
            (None, None) => return None,
        };

        self.inflight.insert(
            job,
            InFlight {
                handle,
                task,
                lease,
                reclaim: None,
            },
        );
        Some(Step::Started { job, lease })
    }

    /// Record the owner's demand and signal the run, according to what the job
    /// promised about interruption. Owner reclaim wins regardless of class; the
    /// class only decides how abrupt the handoff is.
    fn begin_reclaim(
        &mut self,
        job: JobId,
        reason: ReclaimReason,
        class: CheckpointClass,
        now_ms: u64,
    ) {
        let grace = self.config.policy.reclaim_grace_ms;
        let Some(flight) = self.inflight.get_mut(&job) else {
            return;
        };
        if flight.is_reclaiming() {
            return;
        }
        let signalled = match class {
            CheckpointClass::Restart => {
                flight.handle.cancel();
                true
            }
            CheckpointClass::Resumable => {
                // Stop at a boundary the run can name, rather than throwing the
                // work away outright.
                flight.handle.request_checkpoint();
                true
            }
            CheckpointClass::NonInterruptible => false,
        };
        flight.reclaim = Some(Reclaiming {
            reason,
            hard_cancel_at_ms: now_ms.saturating_add(if signalled { 0 } else { grace }),
            signalled,
        });
    }

    /// A job that promised not to be interrupted still loses the device when
    /// its grace window runs out.
    fn escalate_reclaims(&mut self, now_ms: u64) {
        for flight in self.inflight.values_mut() {
            if let Some(reclaim) = flight.reclaim.as_mut()
                && !reclaim.signalled
                && now_ms >= reclaim.hard_cancel_at_ms
            {
                flight.handle.cancel();
                reclaim.signalled = true;
            }
        }
    }

    /// Collect runs that have stopped and author what each one earned.
    async fn reap(&mut self, now_ms: u64) -> Result<Vec<Step>, HostError> {
        let finished: Vec<JobId> = self
            .inflight
            .iter()
            .filter(|(_, flight)| flight.task.is_finished())
            .map(|(job, _)| *job)
            .collect();

        let mut steps = Vec::new();
        for job in finished {
            let flight = self.inflight.remove(&job).expect("just listed");
            let stopped_at = flight.handle.progress();
            let outcome = flight.task.await;
            let lease = flight.lease;

            // A run that finished before the cancel landed has nothing left to
            // stop, and its result is worth more than the revoke would be.
            let succeeded = matches!(outcome, Ok(Ok(_)));
            if let (Some(reclaim), Some(lease), false) = (flight.reclaim, lease, succeeded) {
                self.author(&MeshEvent::LeaseRevokedByOwner {
                    job: job.0,
                    lease: lease.0,
                    reason: reclaim.reason,
                    at_ms: now_ms,
                })
                .await?;
                steps.push(Step::Reclaimed {
                    job,
                    lease,
                    reason: reclaim.reason,
                    stopped_at,
                });
                continue;
            }

            match outcome {
                Ok(Ok(RunOutcome::Committed(output))) => {
                    let event = match lease {
                        Some(lease) => MeshEvent::JobCompletedUnderLease {
                            job: job.0,
                            lease: lease.0,
                            output,
                            at_ms: now_ms,
                        },
                        None => MeshEvent::JobDoneV2 {
                            job: job.0,
                            output,
                            at_ms: now_ms,
                        },
                    };
                    self.author(&event).await?;
                    steps.push(Step::Completed { job, lease });
                }
                Ok(Ok(RunOutcome::Inline(result))) => {
                    self.author(&MeshEvent::JobDone {
                        job: job.0,
                        result,
                        at_ms: now_ms,
                    })
                    .await?;
                    steps.push(Step::Completed { job, lease: None });
                }
                Ok(Err(error)) => {
                    let reason = release_reason(&error);
                    steps.push(self.give_back(job, lease, reason, now_ms).await?);
                }
                // A panicked run is a failed run; the ring should not wait for it.
                Err(_) => {
                    steps.push(
                        self.give_back(job, lease, ReleaseReason::Failed, now_ms)
                            .await?,
                    );
                }
            }
        }
        Ok(steps)
    }

    /// Hand a lease back so the job reopens for another device.
    async fn give_back(
        &mut self,
        job: JobId,
        lease: Option<LeaseId>,
        reason: ReleaseReason,
        now_ms: u64,
    ) -> Result<Step, HostError> {
        let Some(lease) = lease else {
            return Ok(Step::Released {
                job,
                lease: LeaseId([0; 32]),
                reason,
            });
        };
        self.author(&MeshEvent::LeaseReleased {
            job: job.0,
            lease: lease.0,
            reason,
            at_ms: now_ms,
        })
        .await?;
        Ok(Step::Released { job, lease, reason })
    }

    async fn author(&self, event: &MeshEvent) -> Result<[u8; 32], HostError> {
        let op = self.synced.author(&self.keypair, event).await?;
        Ok(*op.hash.as_bytes())
    }
}
