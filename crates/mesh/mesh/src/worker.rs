// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The worker's decision function.
//!
//! [`next_action`] is pure — board, identity, and what this host is offering
//! *at one reading of its clock* in; one action out — so the loop is testable
//! without networking or sleeping. The host drives it: the `mesh-peer` bin now,
//! turnstone's compute actor later.
//!
//! The priority order is the whole social contract. Owner reclaim comes first
//! and is not negotiable; keeping a lease alive comes before doing more work;
//! finishing what you hold comes before taking on more. A device never claims
//! work it cannot run, and never keeps work its owner has asked for back.
//!
//! Execution itself lives in [`crate::registry`]: one route for every wire
//! generation.

use crate::board::{Job, JobBoard, JobId, JobState};
use crate::lease::{LeaseId, ReclaimReason};
use crate::policy::{DeviceConditions, DevicePolicy};
use crate::projection::{LeasePhase, LeasePolicy};
use crate::registry::ResourceRegistry;
use crate::resources::legacy_resource_id;
use crate::spec::HostFacts;

/// What this device is offering, as of one reading of its own clock.
///
/// The host rebuilds this each tick from fresh conditions and a fresh clock
/// reading; nothing below the host boundary asks the OS anything.
#[derive(Clone, Copy)]
pub struct HostOffer<'a> {
    pub registry: &'a ResourceRegistry,
    pub facts: HostFacts,
    pub policy: &'a DevicePolicy,
    pub conditions: DeviceConditions,
    /// This device's clock reading for the tick.
    pub now_ms: u64,
    /// How this observer treats lease time.
    pub lease: LeasePolicy,
    /// Jobs this device already has in flight.
    pub running: &'a [JobId],
}

impl<'a> HostOffer<'a> {
    /// A device that is idle, unconstrained, and holding nothing, at time zero.
    pub fn new(registry: &'a ResourceRegistry, facts: HostFacts, policy: &'a DevicePolicy) -> Self {
        Self {
            registry,
            facts,
            policy,
            conditions: DeviceConditions::spare(),
            now_ms: 0,
            lease: LeasePolicy::default(),
            running: &[],
        }
    }

    /// The clock reading this decision is made against.
    pub fn at(mut self, now_ms: u64) -> Self {
        self.now_ms = now_ms;
        self
    }

    pub fn observing(mut self, conditions: DeviceConditions) -> Self {
        self.conditions = conditions;
        self
    }

    pub fn running(mut self, running: &'a [JobId]) -> Self {
        self.running = running;
        self
    }

    pub fn with_lease_policy(mut self, lease: LeasePolicy) -> Self {
        self.lease = lease;
        self
    }

    /// Why the owner wants the device back, if they do. Applies to work already
    /// running, so being at capacity is deliberately not one of these.
    pub fn must_stop(&self) -> Option<ReclaimReason> {
        self.policy.withholding(&self.conditions)
    }

    /// Whether this device will take on anything new right now.
    pub fn accepting(&self) -> bool {
        self.must_stop().is_none() && !self.policy.at_capacity(&self.conditions)
    }

    /// Whether this device could run `job` at all: it has the adapter, the host
    /// fits the requirements, and the owner's policy allows the resource and
    /// its interruption promise.
    pub fn can_run(&self, job: &Job) -> bool {
        if let Some(spec) = &job.spec {
            return self.policy.accepts(spec) && self.registry.offers(spec, &self.facts);
        }
        let Some(kind) = job.kind else {
            return false;
        };
        job.payload.is_some()
            && self
                .registry
                .get(&legacy_resource_id(kind))
                .is_some_and(|adapter| adapter.descriptor().requires.satisfied_by(&self.facts))
    }

    fn is_running(&self, job: JobId) -> bool {
        self.running.contains(&job)
    }
}

/// What the worker loop should do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerAction {
    /// The owner wants this device back. Stop the run and author the revoke;
    /// nothing outranks this.
    Reclaim {
        job: JobId,
        lease: LeaseId,
        reason: ReclaimReason,
    },
    /// A held lease is due for a heartbeat. Silence is what other devices read
    /// as a lapse, so this comes before starting anything else.
    Heartbeat { job: JobId, lease: LeaseId },
    /// You won the claim race for `epoch`: bind yourself to the job inside the
    /// window the author signed.
    Grant {
        job: JobId,
        epoch: u32,
        granted_at_ms: u64,
        expires_at_ms: u64,
    },
    /// Run a job you hold (or, on an unleased job, one you won).
    Execute(JobId),
    /// Propose yourself. Racing another device's claim is fine — the board
    /// resolves the winner deterministically.
    Claim(JobId),
    /// Nothing to do.
    Idle,
}

/// The next thing `me` (an author/verifying key) should do against `board`.
pub fn next_action(board: &JobBoard, me: &[u8; 32], offer: &HostOffer<'_>) -> WorkerAction {
    let phases: Vec<(&Job, LeasePhase)> = board
        .jobs()
        .map(|job| (job, job.lease_at(offer.now_ms, &offer.lease)))
        .collect();

    // 1) The owner's hardware, back to the owner. Before anything else, and
    //    regardless of the job's checkpoint class: a grace window changes how
    //    the handoff happens, not who has authority.
    if let Some(reason) = offer.must_stop() {
        for (job, phase) in &phases {
            if phase.held_by(me)
                && offer.is_running(job.id)
                && let Some(lease) = phase.lease()
            {
                return WorkerAction::Reclaim {
                    job: job.id,
                    lease,
                    reason,
                };
            }
        }
    }

    // 2) Keep held leases alive before doing anything that takes longer.
    for (job, phase) in &phases {
        if let LeasePhase::Held {
            lease,
            last_seen_ms,
            ..
        } = phase
            && phase.held_by(me)
            && let Some(terms) = job.lease_terms()
            && offer.now_ms >= last_seen_ms.saturating_add(terms.heartbeat_ms)
        {
            return WorkerAction::Heartbeat {
                job: job.id,
                lease: *lease,
            };
        }
    }

    // 3) Finish what you hold. A leased job needs a live lease; an unleased one
    //    keeps M2's claim-winner rule.
    for (job, phase) in &phases {
        let mine = match phase {
            LeasePhase::Unleased => {
                matches!(&job.state, JobState::Claimed { winner } if winner == me)
            }
            _ => phase.held_by(me),
        };
        if mine && !offer.is_running(job.id) && offer.can_run(job) {
            return WorkerAction::Execute(job.id);
        }
    }

    if !offer.accepting() {
        return WorkerAction::Idle;
    }

    // 4) Bind yourself to a job whose claim race you have already won.
    for (job, phase) in &phases {
        let Some(epoch) = phase.open_epoch() else {
            continue;
        };
        let Some(terms) = job.lease_terms() else {
            continue;
        };
        if job.next_holder(offer.now_ms).as_ref() == Some(me) && offer.can_run(job) {
            return WorkerAction::Grant {
                job: job.id,
                epoch,
                granted_at_ms: offer.now_ms,
                expires_at_ms: offer.now_ms.saturating_add(terms.max_duration_ms),
            };
        }
    }

    // 5) Propose yourself for anything open you have not already claimed for
    //    this epoch. (A one-device mesh still round-trips: self-execution stays
    //    allowed, which keeps the smallest setup useful and the demo honest.)
    for (job, phase) in &phases {
        if !offer.can_run(job) {
            continue;
        }
        let open = match phase {
            LeasePhase::Unleased => matches!(job.state, JobState::Posted),
            _ => phase.open_epoch().is_some() && !job.has_claimed(me),
        };
        if open {
            return WorkerAction::Claim(job.id);
        }
    }
    WorkerAction::Idle
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ident::ResourceId;
    use crate::lease::{LeaseActivity, LeaseProgress, LeaseTerms};
    use crate::policy::NetworkClass;
    use crate::spec::{
        CheckpointClass, ComputeClass, DeterminismClass, JobSpec, ResourceRequirements,
    };
    use crate::wire::{JobKind, MeshEvent, MeshExt, to_operation};
    use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};
    use p2panda_core::Operation;
    use proofs::BlobRef;
    use std::collections::BTreeSet;

    const MESH: [u8; 32] = [0x4d; 32];

    /// One author's chained log, so tests read as a sequence of events rather
    /// than a pile of seq/backlink bookkeeping.
    struct Log {
        keypair: Ed25519Keypair,
        seq: u32,
        backlink: Option<[u8; 32]>,
    }

    impl Log {
        fn new(seed: u8) -> Self {
            Self {
                keypair: InMemoryProvider::from_seed([seed; 32])
                    .derive_keypair(b"mesh-worker")
                    .unwrap(),
                seq: 0,
                backlink: None,
            }
        }

        fn me(&self) -> [u8; 32] {
            self.keypair.public_key().to_bytes()
        }

        fn author(&mut self, event: &MeshEvent) -> Operation<MeshExt> {
            let op = to_operation(&self.keypair, MESH, event, self.seq, self.backlink);
            self.seq += 1;
            self.backlink = Some(*op.hash.as_bytes());
            op
        }
    }

    fn registry() -> ResourceRegistry {
        ResourceRegistry::builtin()
    }

    fn spec(leased: bool) -> JobSpec {
        let spec = JobSpec::simple(
            ResourceId::parse("mesh.echo/v1").unwrap(),
            "payload",
            BlobRef::blake3(b"x"),
            "result",
            64,
            DeterminismClass::Exact,
        );
        if leased {
            spec.leased(LeaseTerms::new(60_000, 10_000))
        } else {
            spec
        }
    }

    fn posted_v2(log: &mut Log, spec: JobSpec) -> Operation<MeshExt> {
        log.author(&MeshEvent::JobPostedV2 {
            spec: Box::new(spec),
            nonce: 0,
            at_ms: 0,
        })
    }

    fn m1_post(log: &mut Log) -> Operation<MeshExt> {
        log.author(&MeshEvent::JobPosted {
            kind: JobKind::Echo,
            payload: b"x".to_vec(),
            nonce: 0,
            at_ms: 1,
        })
    }

    fn permissive() -> DevicePolicy {
        DevicePolicy::permissive()
    }

    #[test]
    fn an_unleased_job_still_follows_the_m2_path() {
        let mut asker = Log::new(1);
        let mut worker = Log::new(2);
        let me = worker.me();
        let registry = registry();
        let policy = permissive();
        let offer = HostOffer::new(&registry, HostFacts::cpu(4096), &policy);

        let post = m1_post(&mut asker);
        let id = JobId(*post.hash.as_bytes());
        let board = JobBoard::fold(MESH, [&post]);
        assert_eq!(next_action(&board, &me, &offer), WorkerAction::Claim(id));

        let claim = worker.author(&MeshEvent::JobClaimed {
            job: id.0,
            at_ms: 2,
        });
        let board = JobBoard::fold(MESH, [&post, &claim]);
        assert_eq!(next_action(&board, &me, &offer), WorkerAction::Execute(id));

        let done = worker.author(&MeshEvent::JobDone {
            job: id.0,
            result: b"x".to_vec(),
            at_ms: 3,
        });
        let board = JobBoard::fold(MESH, [&post, &claim, &done]);
        assert_eq!(next_action(&board, &me, &offer), WorkerAction::Idle);
    }

    #[test]
    fn a_claim_loser_neither_grants_nor_executes() {
        let mut asker = Log::new(1);
        let mut w1 = Log::new(2);
        let mut w2 = Log::new(3);
        let registry = registry();
        let policy = permissive();
        let offer = HostOffer::new(&registry, HostFacts::cpu(4096), &policy).at(1_000);

        let post = posted_v2(&mut asker, spec(true));
        let id = JobId(*post.hash.as_bytes());
        let c1 = w1.author(&MeshEvent::JobClaimed {
            job: id.0,
            at_ms: 100,
        });
        let c2 = w2.author(&MeshEvent::JobClaimed {
            job: id.0,
            at_ms: 100,
        });
        let board = JobBoard::fold(MESH, [&post, &c1, &c2]);

        let winner_is_w1 = c1.hash.as_bytes() < c2.hash.as_bytes();
        let (winner, loser) = if winner_is_w1 {
            (w1.me(), w2.me())
        } else {
            (w2.me(), w1.me())
        };
        assert_eq!(
            next_action(&board, &winner, &offer),
            WorkerAction::Grant {
                job: id,
                epoch: 0,
                granted_at_ms: 1_000,
                expires_at_ms: 61_000
            }
        );
        assert_eq!(
            next_action(&board, &loser, &offer),
            WorkerAction::Idle,
            "the loser has already claimed and did not win; it waits"
        );
    }

    /// Claim → grant → execute, then the same board a moment later wants a
    /// heartbeat instead.
    #[test]
    fn a_leased_job_is_claimed_granted_executed_and_kept_alive() {
        let mut asker = Log::new(1);
        let mut worker = Log::new(2);
        let me = worker.me();
        let registry = registry();
        let policy = permissive();

        let post = posted_v2(&mut asker, spec(true));
        let id = JobId(*post.hash.as_bytes());
        let offer = HostOffer::new(&registry, HostFacts::cpu(4096), &policy);

        let board = JobBoard::fold(MESH, [&post]);
        assert_eq!(
            next_action(&board, &me, &offer.at(500)),
            WorkerAction::Claim(id)
        );

        let claim = worker.author(&MeshEvent::JobClaimed {
            job: id.0,
            at_ms: 1_000,
        });
        let board = JobBoard::fold(MESH, [&post, &claim]);
        assert_eq!(
            next_action(&board, &me, &offer.at(1_000)),
            WorkerAction::Grant {
                job: id,
                epoch: 0,
                granted_at_ms: 1_000,
                expires_at_ms: 61_000
            }
        );

        let grant = worker.author(&MeshEvent::LeaseGranted {
            job: id.0,
            epoch: 0,
            granted_at_ms: 1_000,
            expires_at_ms: 61_000,
        });
        let lease = LeaseId(*grant.hash.as_bytes());
        let board = JobBoard::fold(MESH, [&post, &claim, &grant]);
        assert_eq!(
            next_action(&board, &me, &offer.at(1_500)),
            WorkerAction::Execute(id)
        );

        // Once it is running, the lease needs feeding, not restarting.
        let running = [id];
        assert_eq!(
            next_action(&board, &me, &offer.at(1_500).running(&running)),
            WorkerAction::Idle,
            "not due yet: the grant itself counts as a sign of life"
        );
        assert_eq!(
            next_action(&board, &me, &offer.at(11_000).running(&running)),
            WorkerAction::Heartbeat { job: id, lease }
        );
    }

    #[test]
    fn owner_reclaim_outranks_heartbeats_and_new_work() {
        let mut asker = Log::new(1);
        let mut worker = Log::new(2);
        let me = worker.me();
        let registry = registry();
        let policy = DevicePolicy::conservative();

        let post = posted_v2(&mut asker, spec(true));
        let id = JobId(*post.hash.as_bytes());
        let claim = worker.author(&MeshEvent::JobClaimed {
            job: id.0,
            at_ms: 1_000,
        });
        let grant = worker.author(&MeshEvent::LeaseGranted {
            job: id.0,
            epoch: 0,
            granted_at_ms: 1_000,
            expires_at_ms: 61_000,
        });
        let lease = LeaseId(*grant.hash.as_bytes());
        let board = JobBoard::fold(MESH, [&post, &claim, &grant]);
        let running = [id];

        // Heartbeat is overdue, so without the owner this would be a heartbeat.
        let spare = HostOffer::new(&registry, HostFacts::cpu(4096), &policy)
            .at(30_000)
            .running(&running);
        assert_eq!(
            next_action(&board, &me, &spare),
            WorkerAction::Heartbeat { job: id, lease }
        );

        // The human comes back to the keyboard.
        let reclaimed = spare.observing(DeviceConditions::spare().in_use());
        assert_eq!(
            next_action(&board, &me, &reclaimed),
            WorkerAction::Reclaim {
                job: id,
                lease,
                reason: ReclaimReason::ForegroundActivity
            }
        );
    }

    #[test]
    fn a_lapsed_lease_reopens_the_job_for_another_device() {
        let mut asker = Log::new(1);
        let mut first = Log::new(2);
        let mut second = Log::new(3);
        let registry = registry();
        let policy = permissive();

        let post = posted_v2(&mut asker, spec(true));
        let id = JobId(*post.hash.as_bytes());
        let claim = first.author(&MeshEvent::JobClaimed {
            job: id.0,
            at_ms: 1_000,
        });
        let grant = first.author(&MeshEvent::LeaseGranted {
            job: id.0,
            epoch: 0,
            granted_at_ms: 1_000,
            expires_at_ms: 61_000,
        });
        // The second device claims only after the first lease's window ends.
        let late_claim = second.author(&MeshEvent::JobClaimed {
            job: id.0,
            at_ms: 61_000,
        });
        let board = JobBoard::fold(MESH, [&post, &claim, &grant, &late_claim]);
        let offer = HostOffer::new(&registry, HostFacts::cpu(4096), &policy)
            .with_lease_policy(LeasePolicy { max_skew_ms: 0 });

        // While the first lease is live, the late claimant does nothing.
        assert_eq!(
            next_action(&board, &second.me(), &offer.at(20_000)),
            WorkerAction::Idle
        );
        // Once it has expired, the second device is the eligible winner.
        assert_eq!(
            next_action(&board, &second.me(), &offer.at(61_000)),
            WorkerAction::Grant {
                job: id,
                epoch: 1,
                granted_at_ms: 61_000,
                expires_at_ms: 121_000
            }
        );
        // The first device's original claim is not eligible for epoch 1.
        assert_eq!(
            next_action(&board, &first.me(), &offer.at(61_000)),
            WorkerAction::Claim(id),
            "the old holder must re-claim like anyone else"
        );
    }

    #[test]
    fn a_full_device_keeps_its_job_and_takes_no_more() {
        let mut asker = Log::new(1);
        let mut worker = Log::new(2);
        let me = worker.me();
        let registry = registry();
        let mut policy = DevicePolicy::permissive();
        policy.max_concurrent_jobs = 1;

        let held = posted_v2(&mut asker, spec(true));
        let held_id = JobId(*held.hash.as_bytes());
        let claim = worker.author(&MeshEvent::JobClaimed {
            job: held_id.0,
            at_ms: 1_000,
        });
        let grant = worker.author(&MeshEvent::LeaseGranted {
            job: held_id.0,
            epoch: 0,
            granted_at_ms: 1_000,
            expires_at_ms: 61_000,
        });
        let spare_job = posted_v2(&mut asker, spec(true));
        let board = JobBoard::fold(MESH, [&held, &claim, &grant, &spare_job]);

        let running = [held_id];
        let offer = HostOffer::new(&registry, HostFacts::cpu(4096), &policy)
            .at(2_000)
            .running(&running)
            .observing(DeviceConditions {
                running_jobs: 1,
                ..DeviceConditions::spare()
            });
        assert_eq!(
            next_action(&board, &me, &offer),
            WorkerAction::Idle,
            "full: no claim on the second job, and no reclaim of the first"
        );
        assert_eq!(
            offer.must_stop(),
            None,
            "being full is not a reason to hand work back"
        );
    }

    #[test]
    fn the_owners_policy_gates_resources_and_checkpoint_classes() {
        let mut asker = Log::new(1);
        let me = Log::new(2).me();
        let registry = registry();

        let mut uninterruptible = spec(true);
        uninterruptible.checkpoint = CheckpointClass::NonInterruptible;
        let post = posted_v2(&mut asker, uninterruptible);
        let board = JobBoard::fold(MESH, [&post]);

        let mut policy = DevicePolicy::permissive();
        policy.accepted_checkpoints = BTreeSet::from([CheckpointClass::Restart]);
        let offer = HostOffer::new(&registry, HostFacts::cpu(4096), &policy);
        assert_eq!(next_action(&board, &me, &offer), WorkerAction::Idle);

        let mut blocked = DevicePolicy::permissive();
        blocked.allowed_resources = BTreeSet::from([ResourceId::parse("mesh.blake3/v1").unwrap()]);
        let offer = HostOffer::new(&registry, HostFacts::cpu(4096), &blocked);
        assert_eq!(next_action(&board, &me, &offer), WorkerAction::Idle);
    }

    #[test]
    fn work_this_host_cannot_run_is_left_for_a_device_that_can() {
        let mut asker = Log::new(1);
        let me = Log::new(2).me();
        let registry = registry();
        let policy = permissive();
        let offer = HostOffer::new(&registry, HostFacts::cpu(4096), &policy);

        let mut gpu = spec(true);
        gpu.requirements = ResourceRequirements {
            memory_mib: 0,
            compute: ComputeClass::Gpu,
        };
        let gpu_job = posted_v2(&mut asker, gpu);
        let mut unknown = spec(true);
        unknown.resource = ResourceId::parse("nobody.has-this/v1").unwrap();
        let unknown_job = posted_v2(&mut asker, unknown);

        let board = JobBoard::fold(MESH, [&gpu_job, &unknown_job]);
        assert_eq!(board.len(), 2, "both jobs are on the board");
        assert_eq!(next_action(&board, &me, &offer), WorkerAction::Idle);
    }

    #[test]
    fn an_offline_device_offers_nothing_and_a_wired_one_offers_everything() {
        let registry = registry();
        let policy = DevicePolicy::conservative();
        let offline =
            HostOffer::new(&registry, HostFacts::cpu(4096), &policy).observing(DeviceConditions {
                network: NetworkClass::Offline,
                ..DeviceConditions::spare()
            });
        assert!(!offline.accepting());
        assert_eq!(offline.must_stop(), Some(ReclaimReason::Network));

        let wired = HostOffer::new(&registry, HostFacts::cpu(4096), &policy);
        assert!(wired.accepting());
        assert_eq!(wired.must_stop(), None);
    }

    #[test]
    fn heartbeat_progress_travels_from_the_control_handle() {
        // The worker decides *when* to heartbeat; the payload comes from the
        // run itself, so a heartbeat cannot claim progress that did not happen.
        let (handle, control) = crate::resource::JobControl::new();
        control.report(4, 10);
        control.hold_checkpoint(true);
        assert_eq!(
            handle.progress(),
            LeaseProgress {
                done: 4,
                total: 10,
                checkpoint_held: true,
                activity: LeaseActivity::Running,
            }
        );
    }
}
