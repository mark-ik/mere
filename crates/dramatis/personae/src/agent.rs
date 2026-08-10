//! SSH agent backend serving the vault's `ssh` slots.
//!
//! V2 of the identity-vault-ssh-agent plan
//! ([`mere/design_docs/mere_docs/implementation_strategy/2026-07-22_identity-vault-ssh-agent_plan.md`](../../../../../../design_docs/mere_docs/implementation_strategy/2026-07-22_identity-vault-ssh-agent_plan.md)):
//! a [`ssh_agent_lib::agent::Session`] implementation over an
//! [`IdentityVault`], so stock `ssh` / `ssh-add` use vault-held keys through
//! the standard agent protocol. The `personae-agent` bin is the thin
//! listener around this module.
//!
//! ## Slot shape
//!
//! [`crate::ssh_slot`] owns it: a Direct `ssh` slot keyed by SHA256
//! fingerprint. `ssh-add <file>` is the import path — the OpenSSH client
//! reads the local file and hands the agent the private key, which lands
//! as a vault slot (encrypted at rest by whichever backend the vault
//! opened).
//!
//! ## Honest limits
//!
//! - Ed25519 only (the `ssh-key` dependency is built with only that
//!   algorithm; other key types are refused with a clear error).
//! - A standalone agent built with [`VaultAgent::new`] still refuses
//!   [`UnlockTier::PerUse`]. A resident host may provide an
//!   [`ApprovalBroker`] to enforce visible per-use decisions and bounded
//!   short-TTL reuse.
//! - `session-bind@openssh.com` signatures are verified and acknowledged,
//!   and the verified host/session facts scope approval caching. OpenSSH
//!   destination constraints are not yet stored as key policy.
//! - Any local process may connect to the agent endpoint, same as stock
//!   ssh-agent (plan §threat-model note).

use std::sync::{Arc, Mutex};

use signature::Signer;
use ssh_agent_lib::agent::Session;
use ssh_agent_lib::error::AgentError;
use ssh_agent_lib::proto::extension::{QueryResponse, SessionBind};
use ssh_agent_lib::proto::{
    AddIdentity, Extension, PrivateCredential, RemoveIdentity, SignRequest, message,
};
use ssh_key::HashAlg;
use ssh_key::Signature;
use ssh_key::private::PrivateKey;
use ssh_key::public::PublicKey;

use crate::signing::{
    ApprovalBroker, SigningFailureCode, SigningPolicy, SigningRecordResult, SigningRequest,
};
use crate::ssh_slot::{self, SshSlot};
use crate::vault::{IdentityStorage, IdentityVault, ProtocolKey, UnlockTier};

pub use crate::ssh_slot::{SSH_MOD_ID, protocol_key_for};

/// Agent session over a shared vault.
///
/// Cloning shares the vault (one mutation surface, N protocol sessions),
/// which is what `ssh_agent_lib::agent::listen` needs from a session-per-
/// connection agent.
pub struct VaultAgent<S: IdentityStorage> {
    vault: Arc<Mutex<IdentityVault<S>>>,
    approval: Option<ApprovalBroker>,
    adapter: String,
    binding: Option<VerifiedSshBinding>,
}

#[derive(Clone)]
struct VerifiedSshBinding {
    target: String,
    session: String,
}

impl<S: IdentityStorage> Clone for VaultAgent<S> {
    fn clone(&self) -> Self {
        Self {
            vault: Arc::clone(&self.vault),
            approval: self.approval.clone(),
            adapter: self.adapter.clone(),
            binding: None,
        }
    }
}

impl<S: IdentityStorage> VaultAgent<S> {
    /// Wrap a vault for agent serving.
    pub fn new(vault: IdentityVault<S>) -> Self {
        Self {
            vault: Arc::new(Mutex::new(vault)),
            approval: None,
            adapter: "ssh-agent.local".to_string(),
            binding: None,
        }
    }

    /// Wrap a vault and route tiered signing through a visible approval broker.
    pub fn with_approval_broker(
        vault: IdentityVault<S>,
        approval: ApprovalBroker,
        adapter: impl Into<String>,
    ) -> Self {
        Self {
            vault: Arc::new(Mutex::new(vault)),
            approval: Some(approval),
            adapter: adapter.into(),
            binding: None,
        }
    }

    /// Build an agent over a vault already shared with its resident host.
    pub fn from_shared_vault(
        vault: Arc<Mutex<IdentityVault<S>>>,
        approval: ApprovalBroker,
        adapter: impl Into<String>,
    ) -> Self {
        Self {
            vault,
            approval: Some(approval),
            adapter: adapter.into(),
            binding: None,
        }
    }

    /// Share the vault with the resident host's public projection adapter.
    pub fn shared_vault(&self) -> Arc<Mutex<IdentityVault<S>>> {
        Arc::clone(&self.vault)
    }

    /// Approval broker used by this resident agent, when configured.
    pub fn approval_broker(&self) -> Option<&ApprovalBroker> {
        self.approval.as_ref()
    }

    fn ssh_identities(&self) -> Vec<SshSlot> {
        ssh_slot::ssh_slots(self.vault.lock().unwrap().current_profile())
    }

    fn find_by_public(&self, wanted: &PublicKey) -> Option<SshSlot> {
        ssh_slot::find_by_public(self.vault.lock().unwrap().current_profile(), wanted)
    }
}

#[ssh_agent_lib::async_trait]
impl<S: IdentityStorage + 'static> Session for VaultAgent<S> {
    async fn request_identities(&mut self) -> Result<Vec<message::Identity>, AgentError> {
        Ok(self
            .ssh_identities()
            .into_iter()
            .map(|identity| {
                let public = PublicKey::from(&identity.private);
                message::Identity {
                    credential: public.key_data().clone().into(),
                    comment: identity.private.comment().to_string(),
                }
            })
            .collect())
    }

    async fn sign(&mut self, request: SignRequest) -> Result<Signature, AgentError> {
        let wanted: PublicKey = request.credential.key_data().clone().into();
        let Some(identity) = self.find_by_public(&wanted) else {
            return Err(std::io::Error::other("identity not found in vault").into());
        };
        let authorization = if let Some(approval) = self.approval.clone() {
            let profile = self.vault.lock().unwrap().current_profile().id.0.clone();
            let mut signing_request = SigningRequest::new(
                profile,
                identity.fingerprint(),
                "ssh.sign",
                request.data.as_slice(),
                self.adapter.clone(),
            );
            if let Some(binding) = &self.binding {
                signing_request = signing_request
                    .with_authenticated_target(binding.target.clone())
                    .with_session_binding(binding.session.clone());
            }
            Some(
                approval
                    .authorize(signing_request, SigningPolicy::from(identity.tier))
                    .await
                    .map_err(|error| std::io::Error::other(error.to_string()))?,
            )
        } else if identity.tier == UnlockTier::PerUse {
            tracing::warn!(key = ?identity.key, "per-use slot refused: no confirmation UI yet");
            return Err(std::io::Error::other(
                "per-use slot refused: the agent has no confirmation UI yet",
            )
            .into());
        } else {
            None
        };
        tracing::info!(key = ?identity.key, "signing request");
        let signed = identity
            .private
            .try_sign(request.data.as_slice())
            .map_err(AgentError::other);
        if let (Some(approval), Some(authorization)) = (&self.approval, authorization) {
            let result = match &signed {
                Ok(signature) => SigningRecordResult::Signed {
                    signature_ref: format!(
                        "blake3:{}",
                        blake3::hash(signature.as_bytes()).to_hex()
                    ),
                },
                Err(_) => SigningRecordResult::Failed {
                    code: SigningFailureCode::AdapterFailure,
                },
            };
            approval.complete(authorization, result);
        }
        signed
    }

    async fn add_identity(&mut self, identity: AddIdentity) -> Result<(), AgentError> {
        let PrivateCredential::Key { privkey, comment } = identity.credential else {
            return Err(std::io::Error::other("unsupported credential type").into());
        };
        let mut private = PrivateKey::try_from(privkey).map_err(AgentError::other)?;
        if private.comment().is_empty() && !comment.is_empty() {
            private.set_comment(&comment);
        }
        let key = ssh_slot::protocol_key_for(&private);
        let slot = ssh_slot::slot_for(&private, UnlockTier::Session).map_err(AgentError::other)?;
        tracing::info!(?key, "adding ssh identity to vault");
        self.vault
            .lock()
            .unwrap()
            .add_slot(key, slot)
            .map_err(AgentError::other)
    }

    async fn remove_identity(&mut self, identity: RemoveIdentity) -> Result<(), AgentError> {
        let wanted: PublicKey = identity.credential.key_data().clone().into();
        let Some(found) = self.find_by_public(&wanted) else {
            return Err(std::io::Error::other("identity not found in vault").into());
        };
        tracing::info!(key = ?found.key, "removing ssh identity from vault");
        self.vault
            .lock()
            .unwrap()
            .remove_slot(&found.key)
            .map_err(AgentError::other)?;
        Ok(())
    }

    async fn extension(&mut self, extension: Extension) -> Result<Option<Extension>, AgentError> {
        match extension.name.as_str() {
            "query" => {
                let response = Extension::new_message(QueryResponse {
                    extensions: vec!["query".into(), "session-bind@openssh.com".into()],
                })?;
                Ok(Some(response))
            }
            // Modern OpenSSH binds each connection to a host session. v1
            // verifies the binding signature and acknowledges; per-session
            // key restrictions (the constraint half) are not enforced yet.
            "session-bind@openssh.com" => match extension.parse_message::<SessionBind>()? {
                Some(bind) => {
                    bind.verify_signature()
                        .map_err(|_| AgentError::ExtensionFailure)?;
                    self.binding = Some(VerifiedSshBinding {
                        target: format!(
                            "ssh-host-key:{}",
                            bind.host_key.fingerprint(HashAlg::Sha256)
                        ),
                        session: format!(
                            "blake3:{};forwarding={}",
                            blake3::hash(&bind.session_id).to_hex(),
                            bind.is_forwarding
                        ),
                    });
                    tracing::debug!("session-bind acknowledged");
                    Ok(None)
                }
                None => Err(AgentError::Failure),
            },
            other => {
                tracing::debug!(extension = other, "unsupported extension");
                Err(AgentError::ExtensionFailure)
            }
        }
    }

    async fn remove_all_identities(&mut self) -> Result<(), AgentError> {
        let keys: Vec<ProtocolKey> = self
            .ssh_identities()
            .into_iter()
            .map(|identity| identity.key)
            .collect();
        tracing::info!(count = keys.len(), "removing all ssh identities from vault");
        let mut vault = self.vault.lock().unwrap();
        for key in keys {
            vault.remove_slot(&key).map_err(AgentError::other)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ed25519Keypair;
    use crate::signing::{ApprovalSource, RememberApproval, SigningDecision, SigningRecordResult};
    use crate::vault::{InMemoryStorage, Profile, ProfileId};
    use signature::Verifier;
    use ssh_key::Algorithm;
    use std::time::Duration;

    fn test_agent() -> VaultAgent<InMemoryStorage> {
        let profile = Profile::new(
            ProfileId("test".into()),
            "test",
            Ed25519Keypair::from_seed([7; 32]),
        );
        VaultAgent::new(IdentityVault::with_profile(InMemoryStorage::new(), profile))
    }

    fn random_key(comment: &str) -> PrivateKey {
        let mut key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        key.set_comment(comment);
        key
    }

    fn vault_slot_count(agent: &VaultAgent<InMemoryStorage>) -> usize {
        agent.vault.lock().unwrap().current_profile().slots.len()
    }

    #[tokio::test]
    async fn added_identity_is_listed_and_stored() {
        let mut agent = test_agent();
        let key = random_key("laptop");
        let stored_key = protocol_key_for(&key);
        agent
            .add_identity(AddIdentity {
                credential: PrivateCredential::Key {
                    privkey: key.key_data().clone(),
                    comment: "laptop".into(),
                },
            })
            .await
            .unwrap();

        let listed = agent.request_identities().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].comment, "laptop");
        assert!(agent.vault.lock().unwrap().slot(&stored_key).is_some());
    }

    #[tokio::test]
    async fn sign_round_trips_and_verifies() {
        let mut agent = test_agent();
        let key = random_key("signer");
        let public = PublicKey::from(&key);
        agent
            .add_identity(AddIdentity {
                credential: PrivateCredential::Key {
                    privkey: key.key_data().clone(),
                    comment: String::new(),
                },
            })
            .await
            .unwrap();

        let data = b"session-blob-to-sign".to_vec();
        let sig = agent
            .sign(SignRequest {
                credential: public.key_data().clone().into(),
                data: data.clone(),
                flags: 0,
            })
            .await
            .unwrap();
        public.key_data().verify(&data, &sig).unwrap();
    }

    #[tokio::test]
    async fn per_use_slot_refuses_to_sign() {
        let mut agent = test_agent();
        let key = random_key("guarded");
        let public = PublicKey::from(&key);
        agent
            .vault
            .lock()
            .unwrap()
            .add_slot(
                ssh_slot::protocol_key_for(&key),
                ssh_slot::slot_for(&key, UnlockTier::PerUse).unwrap(),
            )
            .unwrap();

        let err = agent
            .sign(SignRequest {
                credential: public.key_data().clone().into(),
                data: b"blob".to_vec(),
                flags: 0,
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("per-use"), "got: {err}");
    }

    #[tokio::test]
    async fn per_use_slot_signs_only_after_broker_approval() {
        let key = random_key("guarded");
        let public = PublicKey::from(&key);
        let mut profile = Profile::new(
            ProfileId("research".into()),
            "Research",
            Ed25519Keypair::from_seed([7; 32]),
        );
        profile.slots.insert(
            ssh_slot::protocol_key_for(&key),
            ssh_slot::slot_for(&key, UnlockTier::PerUse).unwrap(),
        );
        let broker = ApprovalBroker::new(Duration::from_secs(1));
        let mut agent = VaultAgent::with_approval_broker(
            IdentityVault::with_profile(InMemoryStorage::new(), profile),
            broker.clone(),
            "ssh-agent.test",
        );
        let data = b"approval-bound-payload".to_vec();
        let verify_data = data.clone();
        let signing = tokio::spawn(async move {
            agent
                .sign(SignRequest {
                    credential: public.key_data().clone().into(),
                    data,
                    flags: 0,
                })
                .await
        });

        let pending = loop {
            if let Some(pending) = broker.pending().into_iter().next() {
                break pending;
            }
            tokio::task::yield_now().await;
        };
        assert_eq!(pending.request.profile, "research");
        assert_eq!(pending.request.operation, "ssh.sign");
        broker
            .decide(
                pending.request.request_id,
                SigningDecision::Approve {
                    remember: RememberApproval::Once,
                },
            )
            .unwrap();

        let signature = signing.await.unwrap().unwrap();
        PublicKey::from(&key)
            .key_data()
            .verify(&verify_data, &signature)
            .unwrap();
        let history = broker.history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].approval_source, Some(ApprovalSource::UserOnce));
        assert!(matches!(
            history[0].result,
            SigningRecordResult::Signed { .. }
        ));
    }

    #[tokio::test]
    async fn per_use_slot_denial_returns_to_the_ssh_adapter() {
        let key = random_key("guarded");
        let public = PublicKey::from(&key);
        let mut profile = Profile::new(
            ProfileId("research".into()),
            "Research",
            Ed25519Keypair::from_seed([7; 32]),
        );
        profile.slots.insert(
            ssh_slot::protocol_key_for(&key),
            ssh_slot::slot_for(&key, UnlockTier::PerUse).unwrap(),
        );
        let broker = ApprovalBroker::new(Duration::from_secs(1));
        let mut agent = VaultAgent::with_approval_broker(
            IdentityVault::with_profile(InMemoryStorage::new(), profile),
            broker.clone(),
            "ssh-agent.test",
        );
        let signing = tokio::spawn(async move {
            agent
                .sign(SignRequest {
                    credential: public.key_data().clone().into(),
                    data: b"denied-payload".to_vec(),
                    flags: 0,
                })
                .await
        });

        let pending = loop {
            if let Some(pending) = broker.pending().into_iter().next() {
                break pending;
            }
            tokio::task::yield_now().await;
        };
        broker
            .decide(pending.request.request_id, SigningDecision::Deny)
            .unwrap();
        let error = signing.await.unwrap().unwrap_err();
        assert!(error.to_string().contains("denied"), "got: {error}");
        assert_eq!(broker.history().len(), 1);
        assert_eq!(broker.history()[0].result, SigningRecordResult::Denied);
    }

    #[tokio::test]
    async fn unknown_key_is_refused() {
        let mut agent = test_agent();
        let stranger = PublicKey::from(&random_key("stranger"));
        let err = agent
            .sign(SignRequest {
                credential: stranger.key_data().clone().into(),
                data: b"blob".to_vec(),
                flags: 0,
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");
    }

    #[tokio::test]
    async fn remove_and_remove_all_clear_slots() {
        let mut agent = test_agent();
        let a = random_key("a");
        let b = random_key("b");
        for key in [&a, &b] {
            agent
                .add_identity(AddIdentity {
                    credential: PrivateCredential::Key {
                        privkey: key.key_data().clone(),
                        comment: String::new(),
                    },
                })
                .await
                .unwrap();
        }
        assert_eq!(vault_slot_count(&agent), 2);

        agent
            .remove_identity(RemoveIdentity {
                credential: PublicKey::from(&a).key_data().clone().into(),
            })
            .await
            .unwrap();
        assert_eq!(vault_slot_count(&agent), 1);

        agent.remove_all_identities().await.unwrap();
        assert_eq!(vault_slot_count(&agent), 0);
    }

    #[tokio::test]
    async fn readding_the_same_key_is_idempotent() {
        let mut agent = test_agent();
        let key = random_key("dup");
        for _ in 0..2 {
            agent
                .add_identity(AddIdentity {
                    credential: PrivateCredential::Key {
                        privkey: key.key_data().clone(),
                        comment: "dup".into(),
                    },
                })
                .await
                .unwrap();
        }
        assert_eq!(vault_slot_count(&agent), 1);
    }
}
