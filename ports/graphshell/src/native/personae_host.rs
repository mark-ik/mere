//! Resident Personae authority composed by the native Graphshell application.
//!
//! This host deliberately starts in `StandaloneRetained`: H4 may exercise the
//! shared vault and approval boundary without stealing the user's standard SSH
//! agent endpoint before restart and real-login proofs exist.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use personae::agent::VaultAgent;
use personae::signing::{ApprovalBroker, DecisionError, RememberApproval, SigningDecision};
use personae::ssh_slot;
use personae::{CredentialLineage, IdentityStorage, IdentityVault, UnlockTier};
use uuid::Uuid;

use crate::identity::{
    AgentListenerView, CarryView, IdentitySurfaceSnapshot, ProfileView, SshKeyView, VaultLockView,
    VaultProtectionView, VaultView, load_carry_view,
};
use crate::identity_projection::{
    SIGNING_APPROVE_IDLE_INTENT, SIGNING_APPROVE_ONCE_INTENT, SIGNING_DENY_INTENT,
    SigningDecisionIntentV1,
};

/// Rejected Graphshell identity action.
#[derive(Debug, thiserror::Error)]
pub enum IdentityIntentError {
    #[error("unknown identity intent")]
    UnknownIntent,
    #[error("invalid signing decision payload: {0}")]
    InvalidPayload(#[from] serde_json::Error),
    #[error("signing decision rejected: {0}")]
    Decision(#[from] DecisionError),
}

/// Native owner of one shared Personae vault and its SSH adapter.
pub struct PersonaeHost<S: IdentityStorage> {
    vault: Arc<Mutex<IdentityVault<S>>>,
    agent: VaultAgent<S>,
    approval: ApprovalBroker,
    data_root: Option<PathBuf>,
    protection: VaultProtectionView,
    lock: VaultLockView,
    listener: AgentListenerView,
}

impl<S: IdentityStorage + 'static> PersonaeHost<S> {
    /// Compose a resident host without taking over the standard agent endpoint.
    pub fn new(
        vault: IdentityVault<S>,
        data_root: Option<PathBuf>,
        protection: VaultProtectionView,
    ) -> Self {
        Self::with_decision_timeout(vault, data_root, protection, Duration::from_secs(120))
    }

    /// Constructor with a bounded timeout for tests and configured hosts.
    pub fn with_decision_timeout(
        vault: IdentityVault<S>,
        data_root: Option<PathBuf>,
        protection: VaultProtectionView,
        decision_timeout: Duration,
    ) -> Self {
        let vault = Arc::new(Mutex::new(vault));
        let approval = ApprovalBroker::new(decision_timeout);
        let agent =
            VaultAgent::from_shared_vault(Arc::clone(&vault), approval.clone(), "graphshell.ssh");
        Self {
            vault,
            agent,
            approval,
            data_root,
            protection,
            lock: VaultLockView::Unlocked,
            listener: AgentListenerView::StandaloneRetained,
        }
    }

    /// A fresh per-connection SSH agent session over the resident vault.
    pub fn agent_session(&self) -> VaultAgent<S> {
        self.agent.clone()
    }

    /// Secret-free Graphshell read model.
    pub fn snapshot(&self) -> std::io::Result<IdentitySurfaceSnapshot> {
        let vault = self.vault.lock().unwrap();
        let profile = vault.current_profile();
        let current_id = profile.id.0.clone();
        let mut profiles: Vec<_> = vault
            .storage()
            .list_profiles()
            .unwrap_or_default()
            .into_iter()
            .map(|summary| ProfileView {
                selected: summary.id == profile.id,
                id: summary.id.0,
                display_name: summary.display_name,
                slot_count: summary.slot_count,
                master_public_fingerprint: "unknown until profile is selected".to_string(),
            })
            .collect();

        let master_fingerprint = format!(
            "blake3:{}",
            blake3::hash(&profile.master.public_key().to_bytes()).to_hex()
        );
        if let Some(current) = profiles.iter_mut().find(|entry| entry.selected) {
            current.master_public_fingerprint = master_fingerprint.clone();
        } else {
            profiles.push(ProfileView {
                id: current_id.clone(),
                display_name: profile.display_name.clone(),
                selected: true,
                slot_count: profile.slots.len(),
                master_public_fingerprint: master_fingerprint,
            });
        }
        profiles.sort_by(|left, right| left.id.cmp(&right.id));

        let mut ssh_keys = Vec::new();
        for slot in ssh_slot::ssh_slots(profile) {
            let source = profile
                .slots
                .get(&slot.key)
                .expect("ssh_slots only returns profile-owned slots");
            let lineage = source.lineage();
            ssh_keys.push(SshKeyView {
                profile: current_id.clone(),
                fingerprint: slot.fingerprint(),
                comment: slot.private.comment().to_string(),
                public_openssh: slot
                    .public()
                    .to_openssh()
                    .unwrap_or_else(|_| "public key encoding unavailable".to_string()),
                lineage: lineage_label(lineage).to_string(),
                device_loss_note: lineage.device_loss_note().to_string(),
                unlock_policy: unlock_label(source.unlock_tier()),
            });
        }
        ssh_keys.sort_by(|left, right| left.fingerprint.cmp(&right.fingerprint));
        drop(vault);

        let carry = match &self.data_root {
            Some(data_root) => load_carry_view(data_root)?,
            None => CarryView {
                unavailable: vec!["carry data root is not configured".to_string()],
                ..CarryView::default()
            },
        };

        Ok(IdentitySurfaceSnapshot {
            vault: VaultView {
                protection: self.protection,
                lock: self.lock,
                agent: self.listener.clone(),
            },
            profiles,
            ssh_keys,
            carry,
            pending_signing: self.approval.pending(),
            signing_history: self.approval.history(),
        })
    }

    /// Approve exactly one pending operation.
    pub fn approve_once(&self, request_id: Uuid) -> Result<(), DecisionError> {
        self.approval.decide(
            request_id,
            SigningDecision::Approve {
                remember: RememberApproval::Once,
            },
        )
    }

    /// Approve under the key's configured short idle window.
    pub fn approve_until_idle(&self, request_id: Uuid) -> Result<(), DecisionError> {
        self.approval.decide(
            request_id,
            SigningDecision::Approve {
                remember: RememberApproval::UntilIdle,
            },
        )
    }

    /// Deny one pending operation.
    pub fn deny(&self, request_id: Uuid) -> Result<(), DecisionError> {
        self.approval.decide(request_id, SigningDecision::Deny)
    }

    /// Apply one typed action emitted by [`crate::identity_projection`].
    pub fn apply_intent(&self, intent: &str, payload: &[u8]) -> Result<(), IdentityIntentError> {
        let payload: SigningDecisionIntentV1 = serde_json::from_slice(payload)?;
        match intent {
            SIGNING_APPROVE_ONCE_INTENT => self.approve_once(payload.request_id)?,
            SIGNING_APPROVE_IDLE_INTENT => self.approve_until_idle(payload.request_id)?,
            SIGNING_DENY_INTENT => self.deny(payload.request_id)?,
            _ => return Err(IdentityIntentError::UnknownIntent),
        }
        Ok(())
    }
}

fn lineage_label(lineage: CredentialLineage) -> &'static str {
    match lineage {
        CredentialLineage::LocallyDerived => "locally derived",
        CredentialLineage::LocallyGeneratedExternallyRegistered => {
            "locally generated, externally registered"
        }
        CredentialLineage::ExternallyIssued => "externally issued",
        CredentialLineage::ExternallyRootedLocallyHeld => "externally rooted, locally held",
    }
}

fn unlock_label(tier: UnlockTier) -> String {
    match tier {
        UnlockTier::Session => "session".to_string(),
        UnlockTier::ShortTtl { idle_seconds } => format!("{idle_seconds}s idle"),
        UnlockTier::PerUse => "every use".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use personae::ssh_slot::{protocol_key_for, slot_for};
    use personae::{Ed25519Keypair, InMemoryStorage, Profile, ProfileId};
    use ssh_agent_lib::agent::Session;
    use ssh_agent_lib::proto::SignRequest;
    use ssh_key::{Algorithm, LineEnding};

    use super::*;

    #[test]
    fn snapshot_discloses_public_ssh_material_but_not_the_private_slot() {
        let mut private =
            ssh_key::PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        private.set_comment("workstation");
        let private_openssh = private.to_openssh(LineEnding::LF).unwrap().to_string();
        let mut profile = Profile::new(
            ProfileId("research".to_string()),
            "Research",
            Ed25519Keypair::from_seed([0x5a; 32]),
        );
        profile.slots.insert(
            protocol_key_for(&private),
            slot_for(&private, UnlockTier::PerUse).unwrap(),
        );
        let storage = InMemoryStorage::new();
        storage.save_profile(&profile).unwrap();
        let host = PersonaeHost::new(
            IdentityVault::with_profile(storage, profile),
            None,
            VaultProtectionView::Ephemeral,
        );

        let snapshot = host.snapshot().unwrap();
        let json = snapshot.to_public_json().unwrap();
        assert_eq!(snapshot.profiles.len(), 1);
        assert_eq!(snapshot.ssh_keys.len(), 1);
        assert_eq!(snapshot.ssh_keys[0].comment, "workstation");
        assert_eq!(snapshot.ssh_keys[0].unlock_policy, "every use");
        assert!(
            snapshot.ssh_keys[0]
                .public_openssh
                .starts_with("ssh-ed25519 ")
        );
        assert!(!json.contains(&private_openssh));
        assert!(!json.contains("BEGIN OPENSSH PRIVATE KEY"));
        assert!(!json.contains("5a5a5a5a5a5a5a5a"));
        assert_eq!(snapshot.vault.agent, AgentListenerView::StandaloneRetained);
    }

    #[tokio::test]
    async fn projected_approval_intent_releases_the_real_ssh_adapter() {
        let mut private =
            ssh_key::PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        private.set_comment("approval-key");
        let public = ssh_key::PublicKey::from(&private);
        let mut profile = Profile::new(
            ProfileId("research".to_string()),
            "Research",
            Ed25519Keypair::from_seed([0x2a; 32]),
        );
        profile.slots.insert(
            protocol_key_for(&private),
            slot_for(&private, UnlockTier::PerUse).unwrap(),
        );
        let host = PersonaeHost::with_decision_timeout(
            IdentityVault::with_profile(InMemoryStorage::new(), profile),
            None,
            VaultProtectionView::Ephemeral,
            Duration::from_secs(2),
        );
        let mut agent = host.agent_session();
        let signing = tokio::spawn(async move {
            agent
                .sign(SignRequest {
                    credential: public.key_data().clone().into(),
                    data: b"native-host-approval".to_vec(),
                    flags: 0,
                })
                .await
        });

        let pending = loop {
            if let Some(pending) = host.snapshot().unwrap().pending_signing.into_iter().next() {
                break pending;
            }
            tokio::task::yield_now().await;
        };
        let payload = serde_json::to_vec(&SigningDecisionIntentV1 {
            request_id: pending.request.request_id,
        })
        .unwrap();
        host.apply_intent(SIGNING_APPROVE_ONCE_INTENT, &payload)
            .unwrap();

        let signature = signing.await.unwrap().unwrap();
        assert!(!signature.as_bytes().is_empty());
        let completed = host.snapshot().unwrap();
        assert!(completed.pending_signing.is_empty());
        assert_eq!(completed.signing_history.len(), 1);
        assert!(matches!(
            completed.signing_history[0].result,
            personae::signing::SigningRecordResult::Signed { .. }
        ));
    }
}
