// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The worker's decision function.
//!
//! [`next_action`] is pure (board + identity + what this host offers in, action
//! out) so the loop is testable without networking; the *host* drives it — the
//! `mesh-peer` bin now, turnstone's compute actor later. Owner-priority and
//! preemption are the lease-scheduler milestone; an M2 worker simply works when
//! asked, and only on work it can actually run.
//!
//! Execution itself lives in [`crate::registry`]: one route for both wire
//! generations.

use crate::board::{Job, JobBoard, JobId, JobState};
use crate::registry::ResourceRegistry;
use crate::resources::legacy_resource_id;
use crate::spec::HostFacts;

/// What this device advertises: the resources it has registered and the facts
/// they must fit inside. The host owns both — the mesh never inspects the OS.
#[derive(Clone, Copy)]
pub struct HostOffer<'a> {
    pub registry: &'a ResourceRegistry,
    pub facts: HostFacts,
}

impl<'a> HostOffer<'a> {
    pub fn new(registry: &'a ResourceRegistry, facts: HostFacts) -> Self {
        Self { registry, facts }
    }

    /// Whether this device could run `job` right now.
    pub fn can_run(&self, job: &Job) -> bool {
        if let Some(spec) = &job.spec {
            return self.registry.offers(spec, &self.facts);
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
}

/// What the worker loop should do next, in priority order: finish what you
/// won before claiming more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerAction {
    /// You are the claim winner of this job and no result exists: run it and
    /// post the result.
    Execute(JobId),
    /// This job has no claim from you yet (and no result): post `JobClaimed`.
    /// Racing another device's claim is fine — the board resolves the winner
    /// deterministically.
    Claim(JobId),
    /// Nothing to do.
    Idle,
}

/// The next thing `me` (an author/verifying key) should do against `board`.
///
/// A job this host cannot run is skipped rather than claimed: claiming work you
/// cannot finish is how a trusted ring stalls itself.
pub fn next_action(board: &JobBoard, me: &[u8; 32], offer: &HostOffer<'_>) -> WorkerAction {
    // 1) A job I won and haven't finished.
    for job in board.jobs() {
        if let JobState::Claimed { winner } = &job.state
            && winner == me
            && offer.can_run(job)
        {
            return WorkerAction::Execute(job.id);
        }
    }
    // 2) An unclaimed job I can run. (M1 allowed self-execution and M2 keeps
    //    it: a one-device mesh still round-trips, which keeps the smallest
    //    setup useful and the demo honest.)
    for job in board.jobs() {
        if matches!(job.state, JobState::Posted) && offer.can_run(job) {
            return WorkerAction::Claim(job.id);
        }
    }
    WorkerAction::Idle
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ident::ResourceId;
    use crate::registry::run_legacy;
    use crate::resource::JobControl;
    use crate::spec::{ComputeClass, DeterminismClass, JobSpec, ResourceRequirements};
    use crate::wire::{JobKind, MeshEvent, MeshExt, to_operation};
    use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};
    use p2panda_core::Operation;
    use proofs::BlobRef;

    const MESH: [u8; 32] = [0x4d; 32];

    fn keypair(seed: u8) -> Ed25519Keypair {
        InMemoryProvider::from_seed([seed; 32])
            .derive_keypair(b"mesh-worker")
            .unwrap()
    }

    fn registry() -> ResourceRegistry {
        ResourceRegistry::builtin()
    }

    fn offer<'a>(registry: &'a ResourceRegistry) -> HostOffer<'a> {
        HostOffer::new(registry, HostFacts::cpu(4096))
    }

    fn post(kp: &Ed25519Keypair) -> Operation<MeshExt> {
        to_operation(
            kp,
            MESH,
            &MeshEvent::JobPosted {
                kind: JobKind::Echo,
                payload: b"x".to_vec(),
                nonce: 0,
                at_ms: 1,
            },
            0,
            None,
        )
    }

    #[tokio::test]
    async fn the_legacy_kinds_still_produce_their_m1_results() {
        let registry = registry();
        let (_handle, control) = JobControl::new();
        assert_eq!(
            run_legacy(&registry, JobKind::Echo, b"abc", &control)
                .await
                .unwrap(),
            b"abc".to_vec()
        );
        let h1 = run_legacy(&registry, JobKind::Blake3, b"abc", &control)
            .await
            .unwrap();
        assert_eq!(h1.len(), 32);
        assert_eq!(h1, p2panda_core::Hash::digest(b"abc").as_bytes().to_vec());
    }

    #[test]
    fn worker_claims_then_executes_then_idles() {
        let asker = keypair(1);
        let worker = keypair(2);
        let me = worker.public_key().to_bytes();
        let registry = registry();
        let offer = offer(&registry);

        let posted = post(&asker);
        let id = JobId(*posted.hash.as_bytes());
        let board = JobBoard::fold(MESH, [&posted]);
        assert_eq!(next_action(&board, &me, &offer), WorkerAction::Claim(id));

        let claim = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 2,
            },
            0,
            None,
        );
        let board = JobBoard::fold(MESH, [&posted, &claim]);
        assert_eq!(next_action(&board, &me, &offer), WorkerAction::Execute(id));

        let done = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobDone {
                job: id.0,
                result: b"x".to_vec(),
                at_ms: 3,
            },
            1,
            Some(*claim.hash.as_bytes()),
        );
        let board = JobBoard::fold(MESH, [&posted, &claim, &done]);
        assert_eq!(next_action(&board, &me, &offer), WorkerAction::Idle);
    }

    #[test]
    fn a_claim_loser_does_not_execute() {
        let asker = keypair(1);
        let w1 = keypair(2);
        let w2 = keypair(3);
        let registry = registry();
        let offer = offer(&registry);
        let posted = post(&asker);
        let id = JobId(*posted.hash.as_bytes());
        let claims: Vec<_> = [&w1, &w2]
            .iter()
            .map(|kp| {
                to_operation(
                    kp,
                    MESH,
                    &MeshEvent::JobClaimed {
                        job: id.0,
                        at_ms: 2,
                    },
                    0,
                    None,
                )
            })
            .collect();
        let board = JobBoard::fold(MESH, [&posted, &claims[0], &claims[1]]);
        let winner_is_w1 = claims[0].hash.as_bytes() < claims[1].hash.as_bytes();
        let (winner_me, loser_me) = if winner_is_w1 {
            (w1.public_key().to_bytes(), w2.public_key().to_bytes())
        } else {
            (w2.public_key().to_bytes(), w1.public_key().to_bytes())
        };
        assert_eq!(
            next_action(&board, &winner_me, &offer),
            WorkerAction::Execute(id)
        );
        assert_eq!(
            next_action(&board, &loser_me, &offer),
            WorkerAction::Idle,
            "the loser neither executes nor re-claims a claimed job"
        );
    }

    #[test]
    fn work_this_host_cannot_run_is_left_for_a_device_that_can() {
        let asker = keypair(1);
        let me = keypair(2).public_key().to_bytes();
        let registry = registry();
        let offer = offer(&registry);

        let mut spec = JobSpec::simple(
            ResourceId::parse("esp.embed.lexical/v1").unwrap(),
            "texts",
            BlobRef::blake3(b"batch"),
            "vectors",
            4096,
            DeterminismClass::Exact,
        );
        spec.requirements = ResourceRequirements {
            memory_mib: 0,
            compute: ComputeClass::Gpu,
        };
        let gpu_job = to_operation(
            &asker,
            MESH,
            &MeshEvent::JobPostedV2 {
                spec: Box::new(spec),
                nonce: 0,
                at_ms: 1,
            },
            0,
            None,
        );
        let unknown = to_operation(
            &asker,
            MESH,
            &MeshEvent::JobPostedV2 {
                spec: Box::new(JobSpec::simple(
                    ResourceId::parse("nobody.has-this/v1").unwrap(),
                    "in",
                    BlobRef::blake3(b"x"),
                    "out",
                    64,
                    DeterminismClass::Exact,
                )),
                nonce: 1,
                at_ms: 1,
            },
            1,
            Some(*gpu_job.hash.as_bytes()),
        );

        let board = JobBoard::fold(MESH, [&gpu_job, &unknown]);
        assert_eq!(board.len(), 2, "both jobs are on the board");
        assert_eq!(
            next_action(&board, &me, &offer),
            WorkerAction::Idle,
            "a CPU-only device claims neither the GPU job nor the unknown resource"
        );
    }

    #[test]
    fn a_v2_job_this_host_offers_is_claimed_and_executed() {
        let asker = keypair(1);
        let worker = keypair(2);
        let me = worker.public_key().to_bytes();
        let registry = registry();
        let offer = offer(&registry);

        let posted = to_operation(
            &asker,
            MESH,
            &MeshEvent::JobPostedV2 {
                spec: Box::new(JobSpec::simple(
                    ResourceId::parse("esp.embed.lexical/v1").unwrap(),
                    "texts",
                    BlobRef::blake3(b"batch"),
                    "vectors",
                    4096,
                    DeterminismClass::Exact,
                )),
                nonce: 0,
                at_ms: 1,
            },
            0,
            None,
        );
        let id = JobId(*posted.hash.as_bytes());
        let board = JobBoard::fold(MESH, [&posted]);
        assert_eq!(next_action(&board, &me, &offer), WorkerAction::Claim(id));

        let claim = to_operation(
            &worker,
            MESH,
            &MeshEvent::JobClaimed {
                job: id.0,
                at_ms: 2,
            },
            0,
            None,
        );
        let board = JobBoard::fold(MESH, [&posted, &claim]);
        assert_eq!(next_action(&board, &me, &offer), WorkerAction::Execute(id));
    }
}
