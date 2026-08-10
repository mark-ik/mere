//! Mesh-owned native-drop privacy and priority mapping.
//!
//! Replication owns selection mechanics. This module gives mesh events their
//! meaning for catch-up, archive, and radio profiles without teaching the
//! generic exporter about jobs or checkpoints.

use std::collections::BTreeMap;

use p2panda_core::Operation;
use stickleback::{DropExportDecision, DropExportSelector};

use crate::store::StoredCheckpoint;
use crate::wire::{MeshEvent, MeshExt, MeshLogId, from_operation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshDropProfile {
    CatchUp,
    Archive,
    Radio,
}

/// Privacy settings applied at the current mesh event-body granularity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshDropPrivacy {
    pub include_job_inputs: bool,
    pub include_job_results: bool,
}

/// Settings-derived ordering. Higher values are selected first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshDropPriorities {
    pub retention_checkpoint: u32,
    pub history_pruned: u32,
    pub job_done: u32,
    pub job_claimed: u32,
    pub job_posted: u32,
}

#[derive(Clone, Debug)]
enum MeshDropScope {
    CatchUp {
        checkpoint: Option<[u8; 32]>,
        frontier: BTreeMap<([u8; 32], MeshLogId), u64>,
    },
    Archive,
    Radio,
}

/// Concrete mesh policy for the shared native-drop exporter.
#[derive(Clone, Debug)]
pub struct MeshDropSelector {
    profile: MeshDropProfile,
    scope: MeshDropScope,
    privacy: MeshDropPrivacy,
    priorities: MeshDropPriorities,
}

impl MeshDropSelector {
    /// Select the latest accepted retention checkpoint plus its replay tail.
    pub fn catch_up(
        checkpoint: Option<&StoredCheckpoint>,
        privacy: MeshDropPrivacy,
        priorities: MeshDropPriorities,
    ) -> Self {
        let checkpoint_id = checkpoint.map(|stored| stored.operation);
        let frontier = checkpoint
            .into_iter()
            .flat_map(|stored| stored.checkpoint.frontier.iter())
            .map(|entry| ((entry.author, entry.log_id), entry.seq_num))
            .collect();
        Self {
            profile: MeshDropProfile::CatchUp,
            scope: MeshDropScope::CatchUp {
                checkpoint: checkpoint_id,
                frontier,
            },
            privacy,
            priorities,
        }
    }

    /// Select every locally retained, privacy-permitted mesh event.
    pub fn archive(privacy: MeshDropPrivacy, priorities: MeshDropPriorities) -> Self {
        Self {
            profile: MeshDropProfile::Archive,
            scope: MeshDropScope::Archive,
            privacy,
            priorities,
        }
    }

    /// Select privacy-permitted events in radio priority order. The caller
    /// supplies the radio byte budget to `stickleback` separately.
    pub fn radio(privacy: MeshDropPrivacy, priorities: MeshDropPriorities) -> Self {
        Self {
            profile: MeshDropProfile::Radio,
            scope: MeshDropScope::Radio,
            privacy,
            priorities,
        }
    }

    pub const fn profile(&self) -> MeshDropProfile {
        self.profile
    }

    fn privacy_permits(&self, event: &MeshEvent) -> bool {
        match event {
            // A V2 spec carries addresses rather than bytes, but naming a blob
            // is enough to fetch it. It counts as an input for privacy.
            MeshEvent::JobPosted { .. } | MeshEvent::JobPostedV2 { .. } => {
                self.privacy.include_job_inputs
            }
            MeshEvent::JobDone { .. }
            | MeshEvent::JobDoneV2 { .. }
            | MeshEvent::JobCompletedUnderLease { .. } => self.privacy.include_job_results,
            // Lease lifecycle names devices and times, not inputs or results.
            // It travels with the claims it belongs to.
            MeshEvent::LeaseGranted { .. }
            | MeshEvent::LeaseHeartbeat { .. }
            | MeshEvent::LeaseReleased { .. }
            | MeshEvent::LeaseRevokedByOwner { .. } => true,
            MeshEvent::RetentionCheckpoint { checkpoint } => {
                let contains_inputs = checkpoint
                    .snapshot
                    .jobs
                    .iter()
                    .any(|job| job.payload.is_some() || job.spec.is_some());
                let contains_results = checkpoint
                    .snapshot
                    .jobs
                    .iter()
                    .any(|job| job.state.is_terminal());
                (self.privacy.include_job_inputs || !contains_inputs)
                    && (self.privacy.include_job_results || !contains_results)
            }
            // A device saying which master key it answers to carries no job
            // content at all — and a catch-up without it cannot resolve who to
            // fetch blobs from, so it always travels.
            MeshEvent::DeviceAttested { .. }
            | MeshEvent::JobClaimed { .. }
            | MeshEvent::HistoryPruned { .. } => true,
        }
    }

    fn priority(&self, event: &MeshEvent) -> u32 {
        match event {
            // Nothing else can be acted on without knowing who the authors are.
            MeshEvent::DeviceAttested { .. } => self.priorities.retention_checkpoint,
            MeshEvent::RetentionCheckpoint { .. } => self.priorities.retention_checkpoint,
            MeshEvent::HistoryPruned { .. } => self.priorities.history_pruned,
            MeshEvent::JobDone { .. }
            | MeshEvent::JobDoneV2 { .. }
            | MeshEvent::JobCompletedUnderLease { .. } => self.priorities.job_done,
            // A lease fact is only useful next to the claim it settles.
            MeshEvent::JobClaimed { .. }
            | MeshEvent::LeaseGranted { .. }
            | MeshEvent::LeaseHeartbeat { .. }
            | MeshEvent::LeaseReleased { .. }
            | MeshEvent::LeaseRevokedByOwner { .. } => self.priorities.job_claimed,
            MeshEvent::JobPosted { .. } | MeshEvent::JobPostedV2 { .. } => {
                self.priorities.job_posted
            }
        }
    }

    fn in_scope(&self, operation: &Operation<MeshExt>, event: &MeshEvent) -> bool {
        let MeshDropScope::CatchUp {
            checkpoint,
            frontier,
        } = &self.scope
        else {
            return true;
        };
        if matches!(event, MeshEvent::RetentionCheckpoint { .. }) {
            return checkpoint.is_none_or(|expected| operation.hash.as_bytes() == &expected);
        }
        frontier
            .get(&(*operation.header.verifying_key.as_bytes(), event.log_id()))
            .is_none_or(|seq_num| u64::from(operation.header.seq_num) > *seq_num)
    }
}

impl DropExportSelector<MeshExt> for MeshDropSelector {
    fn select(&self, operation: &Operation<MeshExt>) -> DropExportDecision {
        let Ok((_, event)) = from_operation(operation) else {
            // Mesh admission needs the event body. A bodyless or malformed
            // operation is not an importable archive record.
            return DropExportDecision::Omit;
        };
        if !self.privacy_permits(&event) || !self.in_scope(operation, &event) {
            return DropExportDecision::Omit;
        }
        DropExportDecision::Full {
            priority: self.priority(&event),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use identity::{Ed25519Keypair, IdentityProvider, InMemoryProvider};
    use p2panda_core::Topic;
    use proofs::Digest;
    use stickleback::{DropExportBudget, DropRecord, export_topic_operations_selected};

    use crate::retention::{JobBoardSnapshot, LogFrontier, PolicyRevision, RetentionCheckpoint};
    use crate::store::MeshStore;
    use crate::wire::{JobKind, to_operation};

    const MESH: [u8; 32] = [0x4d; 32];

    fn keypair() -> Ed25519Keypair {
        InMemoryProvider::from_seed([7; 32])
            .derive_keypair(b"mesh-drop-export")
            .unwrap()
    }

    fn priorities() -> MeshDropPriorities {
        MeshDropPriorities {
            retention_checkpoint: 500,
            history_pruned: 400,
            job_done: 300,
            job_claimed: 200,
            job_posted: 100,
        }
    }

    fn events(key: &Ed25519Keypair) -> Vec<Operation<MeshExt>> {
        let posted = to_operation(
            key,
            MESH,
            &MeshEvent::JobPosted {
                kind: JobKind::Blake3,
                payload: b"private input".to_vec(),
                nonce: 1,
                at_ms: 10,
            },
            0,
            None,
        );
        let claimed = to_operation(
            key,
            MESH,
            &MeshEvent::JobClaimed {
                job: *posted.hash.as_bytes(),
                at_ms: 20,
            },
            1,
            Some(*posted.hash.as_bytes()),
        );
        let done = to_operation(
            key,
            MESH,
            &MeshEvent::JobDone {
                job: *posted.hash.as_bytes(),
                result: b"compact result".to_vec(),
                at_ms: 30,
            },
            2,
            Some(*claimed.hash.as_bytes()),
        );
        vec![posted, claimed, done]
    }

    #[tokio::test]
    async fn radio_mapping_drives_the_shared_exporter() {
        let key = keypair();
        let operations = events(&key);
        let store = MeshStore::in_memory();
        for operation in &operations {
            store.insert(operation).await.unwrap();
        }
        let selector = MeshDropSelector::radio(
            MeshDropPrivacy {
                include_job_inputs: false,
                include_job_results: true,
            },
            priorities(),
        );
        let (records, stats) = export_topic_operations_selected::<_, _, MeshLogId, _>(
            &store.sync_store(),
            &Topic::from(MESH),
            &selector,
            DropExportBudget::default(),
        )
        .await
        .unwrap();
        assert_eq!(stats.policy_omitted, 1);
        assert_eq!(stats.selected_records, 2);
        let DropRecord::Operation { inline_body, .. } = &records[0] else {
            panic!("mesh exports operation records")
        };
        let body = inline_body.as_ref().expect("mesh events require bodies");
        let event: MeshEvent = p2panda_core::cbor::decode_cbor(&body[..]).unwrap();
        assert!(matches!(event, MeshEvent::JobDone { .. }));
    }

    #[test]
    fn catch_up_keeps_current_checkpoint_and_tail_only() {
        let key = keypair();
        let operations = events(&key);
        let checkpoint = RetentionCheckpoint::new(
            MESH,
            PolicyRevision([3; 32]),
            Digest::blake3(b"mesh authority"),
            vec![LogFrontier {
                author: key.public_key().to_bytes(),
                log_id: MeshLogId::Events,
                seq_num: 1,
                operation: *operations[1].hash.as_bytes(),
            }],
            JobBoardSnapshot::default(),
            25,
        );
        let checkpoint_operation = to_operation(
            &key,
            MESH,
            &MeshEvent::RetentionCheckpoint {
                checkpoint: Box::new(checkpoint.clone()),
            },
            0,
            None,
        );
        let stored = StoredCheckpoint {
            operation: *checkpoint_operation.hash.as_bytes(),
            checkpoint,
        };
        let selector = MeshDropSelector::catch_up(
            Some(&stored),
            MeshDropPrivacy {
                include_job_inputs: true,
                include_job_results: true,
            },
            priorities(),
        );
        assert_eq!(selector.profile(), MeshDropProfile::CatchUp);
        assert_eq!(selector.select(&operations[0]), DropExportDecision::Omit);
        assert_eq!(selector.select(&operations[1]), DropExportDecision::Omit);
        assert!(matches!(
            selector.select(&operations[2]),
            DropExportDecision::Full { priority: 300 }
        ));
        assert!(matches!(
            selector.select(&checkpoint_operation),
            DropExportDecision::Full { priority: 500 }
        ));
    }
}
