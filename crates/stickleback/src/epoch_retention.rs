//! Safe, authority-neutral proposals for retiring retained data epochs.
//!
//! A domain supplies checkpoint, authority, reachability, and offline-member
//! facts. Stickleback applies the mechanical profile floor and key chronology,
//! but never decides that a checkpoint or a member policy is authoritative.

use std::collections::{BTreeMap, BTreeSet};

use p2panda_encryption::data_scheme::GroupSecretId;
use proofs::Digest;
use serde::{Deserialize, Serialize};

use crate::{DataKeyring, GroupEncryptionMode, GroupEncryptionProfile};

/// A durable domain checkpoint on which an epoch-retention proposal relies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochCheckpointBasis {
    /// Stable identity of the checkpoint record.
    pub checkpoint: Digest,
    /// Authority revision named by the checkpoint when it was accepted.
    pub authority_revision: Digest,
    /// Authority revision converged by the domain at proposal time.
    pub current_authority_revision: Digest,
    /// Whether checkpoint plus retained tail preserves every author frontier
    /// needed to validate and continue writing.
    pub author_continuation_ready: bool,
}

/// Domain-owned reason that one retained epoch remains reachable.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EpochHoldReason {
    /// The checkpoint still needs ciphertext under this epoch to rebuild the
    /// current projection.
    DecryptionReachability,
    /// A retained fact may become effective after a later authority change.
    AuthorityReevaluation,
    /// A causally incomplete fact cannot yet be represented safely by the
    /// checkpoint.
    PendingCausality,
    /// The governed offline-member policy still promises recovery to this
    /// member.
    OfflineMember([u8; 32]),
}

/// One domain-provided hold on an exact epoch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochHold {
    pub epoch: GroupSecretId,
    pub reason: EpochHoldReason,
}

/// Facts supplied by the replicated domain to the neutral proposal engine.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochRetentionFacts {
    pub checkpoint: Option<EpochCheckpointBasis>,
    pub holds: Vec<EpochHold>,
}

/// Mechanical or domain gate that prevents every destructive candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EpochProposalBlocker {
    WrongEncryptionMode(GroupEncryptionMode),
    MissingCurrentEpoch,
    IncompleteEpochOrder,
    MissingCheckpoint,
    StaleCheckpointAuthority,
    MissingAuthorContinuation,
    HoldNamesUnknownEpoch(GroupSecretId),
}

/// Why the proposal retains one present epoch.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EpochRetentionReason {
    Current,
    ProfileFloor,
    Domain(EpochHoldReason),
    /// A global blocker keeps otherwise eligible epochs untouched.
    ProposalBlocked,
}

/// The complete decision for one retained epoch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetainedEpoch {
    pub epoch: GroupSecretId,
    pub reasons: Vec<EpochRetentionReason>,
}

/// Reviewable dry-run artifact. `forget` is populated only when every global
/// gate passes; execution and authorization remain domain-owned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochPruningProposal {
    pub checkpoint: Option<Digest>,
    pub retain: Vec<RetainedEpoch>,
    pub forget: Vec<GroupSecretId>,
    pub blockers: Vec<EpochProposalBlocker>,
}

impl EpochPruningProposal {
    pub fn is_executable(&self) -> bool {
        self.blockers.is_empty()
    }
}

/// Calculate an authority-neutral epoch-pruning proposal.
pub fn propose_epoch_pruning(
    profile: GroupEncryptionProfile,
    keyring: &DataKeyring,
    facts: &EpochRetentionFacts,
) -> EpochPruningProposal {
    let mut blockers = Vec::new();
    if profile.mode != GroupEncryptionMode::Data {
        blockers.push(EpochProposalBlocker::WrongEncryptionMode(profile.mode));
    }

    let current = keyring.current_epoch();
    if current.is_none() {
        blockers.push(EpochProposalBlocker::MissingCurrentEpoch);
    }

    let order = keyring.epochs_oldest_first();
    if order.is_none() {
        blockers.push(EpochProposalBlocker::IncompleteEpochOrder);
    }

    match &facts.checkpoint {
        None => blockers.push(EpochProposalBlocker::MissingCheckpoint),
        Some(checkpoint) => {
            if checkpoint.authority_revision != checkpoint.current_authority_revision {
                blockers.push(EpochProposalBlocker::StaleCheckpointAuthority);
            }
            if !checkpoint.author_continuation_ready {
                blockers.push(EpochProposalBlocker::MissingAuthorContinuation);
            }
        }
    }

    let present: BTreeSet<_> = keyring.epoch_ids().into_iter().collect();
    for hold in &facts.holds {
        if !present.contains(&hold.epoch) {
            let blocker = EpochProposalBlocker::HoldNamesUnknownEpoch(hold.epoch);
            if !blockers.contains(&blocker) {
                blockers.push(blocker);
            }
        }
    }

    let mut domain_holds = BTreeMap::<GroupSecretId, BTreeSet<EpochHoldReason>>::new();
    for hold in &facts.holds {
        if present.contains(&hold.epoch) {
            domain_holds
                .entry(hold.epoch)
                .or_default()
                .insert(hold.reason.clone());
        }
    }

    // With incomplete chronology, lexical ids are suitable only for a stable
    // blocked report. `ProposalBlocked` below ensures they can never become
    // destructive candidates.
    let lexical_fallback = keyring.epoch_ids();
    let ordered = order.unwrap_or(&lexical_fallback);
    let floor_start = ordered.len().saturating_sub(profile.retained_data_epochs);
    let blocked = !blockers.is_empty();
    let mut retain = Vec::new();
    let mut forget = Vec::new();

    for (index, epoch) in ordered.iter().copied().enumerate() {
        let mut reasons = BTreeSet::new();
        if current == Some(epoch) {
            reasons.insert(EpochRetentionReason::Current);
        }
        if index >= floor_start {
            reasons.insert(EpochRetentionReason::ProfileFloor);
        }
        if let Some(holds) = domain_holds.get(&epoch) {
            reasons.extend(holds.iter().cloned().map(EpochRetentionReason::Domain));
        }
        if blocked {
            reasons.insert(EpochRetentionReason::ProposalBlocked);
        }

        if reasons.is_empty() {
            forget.push(epoch);
        } else {
            retain.push(RetainedEpoch {
                epoch,
                reasons: reasons.into_iter().collect(),
            });
        }
    }

    EpochPruningProposal {
        checkpoint: facts
            .checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.checkpoint.clone()),
        retain,
        forget,
        blockers,
    }
}

#[cfg(test)]
mod tests {
    use p2panda_encryption::Rng;

    use super::*;

    fn checkpoint() -> EpochCheckpointBasis {
        let revision = Digest::blake3(b"authority");
        EpochCheckpointBasis {
            checkpoint: Digest::blake3(b"checkpoint"),
            authority_revision: revision.clone(),
            current_authority_revision: revision,
            author_continuation_ready: true,
        }
    }

    fn ring(count: usize) -> DataKeyring {
        let rng = Rng::default();
        let mut ring = DataKeyring::new();
        for _ in 0..count {
            ring.rotate(&rng).unwrap();
        }
        ring
    }

    #[test]
    fn keeps_current_profile_suffix_and_domain_holds() {
        let ring = ring(5);
        let order = ring.epochs_oldest_first().unwrap();
        let held = order[1];
        let proposal = propose_epoch_pruning(
            GroupEncryptionProfile::durable_data(2),
            &ring,
            &EpochRetentionFacts {
                checkpoint: Some(checkpoint()),
                holds: vec![EpochHold {
                    epoch: held,
                    reason: EpochHoldReason::OfflineMember([7; 32]),
                }],
            },
        );

        assert!(proposal.is_executable());
        assert_eq!(proposal.forget, vec![order[0], order[2]]);
        assert!(
            proposal
                .retain
                .iter()
                .find(|decision| decision.epoch == held)
                .unwrap()
                .reasons
                .contains(&EpochRetentionReason::Domain(
                    EpochHoldReason::OfflineMember([7; 32])
                ))
        );
        assert!(
            proposal
                .retain
                .iter()
                .find(|decision| decision.epoch == *order.last().unwrap())
                .unwrap()
                .reasons
                .contains(&EpochRetentionReason::Current)
        );
    }

    #[test]
    fn every_global_gate_blocks_every_candidate() {
        let ring = ring(3);
        let order = ring.epochs_oldest_first().unwrap();
        let proposal = propose_epoch_pruning(
            GroupEncryptionProfile::durable_data(1),
            &ring,
            &EpochRetentionFacts::default(),
        );

        assert!(!proposal.is_executable());
        assert!(proposal.forget.is_empty());
        assert_eq!(
            proposal.blockers,
            vec![EpochProposalBlocker::MissingCheckpoint]
        );
        assert_eq!(
            proposal
                .retain
                .iter()
                .map(|decision| decision.epoch)
                .collect::<Vec<_>>(),
            order
        );
    }

    #[test]
    fn stale_authority_incomplete_continuation_and_unknown_holds_are_visible() {
        let ring = ring(2);
        let mut basis = checkpoint();
        basis.current_authority_revision = Digest::blake3(b"new authority");
        basis.author_continuation_ready = false;
        let proposal = propose_epoch_pruning(
            GroupEncryptionProfile::durable_data(1),
            &ring,
            &EpochRetentionFacts {
                checkpoint: Some(basis),
                holds: vec![EpochHold {
                    epoch: GroupSecretId::from([0xff; 32]),
                    reason: EpochHoldReason::DecryptionReachability,
                }],
            },
        );

        assert!(proposal.forget.is_empty());
        assert!(
            proposal
                .blockers
                .contains(&EpochProposalBlocker::StaleCheckpointAuthority)
        );
        assert!(
            proposal
                .blockers
                .contains(&EpochProposalBlocker::MissingAuthorContinuation)
        );
        assert!(
            proposal
                .blockers
                .iter()
                .any(|blocker| matches!(blocker, EpochProposalBlocker::HoldNamesUnknownEpoch(_)))
        );
    }
}
