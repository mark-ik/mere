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
//! An SSH key is a Direct slot with `kind: "ssh"` whose payload is the
//! OpenSSH-encoded private key (the same bytes an `id_ed25519` file holds,
//! comment included). The [`ProtocolKey`] instance is the key's SHA256
//! fingerprint, so multiple keys coexist and re-adding the same key is
//! idempotent.
//!
//! `ssh-add <file>` is the import path: the OpenSSH client reads the local
//! file and hands the agent the private key, which lands here as a vault
//! slot (encrypted at rest by whichever storage backend the vault opened).
//!
//! ## Honest v1 limits
//!
//! - Ed25519 only (the `ssh-key` dependency is built with only that
//!   algorithm; other key types are refused with a clear error).
//! - [`UnlockTier::PerUse`] slots refuse to sign — the agent has no
//!   confirmation UI yet, and silently signing would make the tier a lie.
//!   [`UnlockTier::ShortTtl`] is treated as Session until relock lands.
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
use ssh_key::private::PrivateKey;
use ssh_key::public::PublicKey;
use ssh_key::{HashAlg, LineEnding, Signature};

use crate::vault::{
    CredentialLineage, IdentitySlot, IdentityStorage, IdentityVault, ProtocolKey, SecretBytes,
    UnlockTier,
};

/// The `mod_id` under which the agent stores SSH keys.
pub const SSH_MOD_ID: &str = "ssh";

/// One vault-held SSH identity, decoded for agent use.
struct SshIdentity {
    key: ProtocolKey,
    private: PrivateKey,
    tier: UnlockTier,
}

/// Agent session over a shared vault.
///
/// Cloning shares the vault (one mutation surface, N protocol sessions),
/// which is what `ssh_agent_lib::agent::listen` needs from a session-per-
/// connection agent.
pub struct VaultAgent<S: IdentityStorage> {
    vault: Arc<Mutex<IdentityVault<S>>>,
}

impl<S: IdentityStorage> Clone for VaultAgent<S> {
    fn clone(&self) -> Self {
        Self {
            vault: Arc::clone(&self.vault),
        }
    }
}

impl<S: IdentityStorage> VaultAgent<S> {
    /// Wrap a vault for agent serving.
    pub fn new(vault: IdentityVault<S>) -> Self {
        Self {
            vault: Arc::new(Mutex::new(vault)),
        }
    }

    /// Decode all `ssh`-kind slots. Slots that fail to parse are skipped
    /// with a warning rather than poisoning the whole listing.
    fn ssh_identities(&self) -> Vec<SshIdentity> {
        let vault = self.vault.lock().unwrap();
        let mut out = Vec::new();
        for (key, slot) in &vault.current_profile().slots {
            if key.mod_id != SSH_MOD_ID {
                continue;
            }
            let IdentitySlot::Direct {
                payload,
                unlock_tier,
                ..
            } = slot
            else {
                continue;
            };
            match PrivateKey::from_openssh(payload.as_slice()) {
                Ok(private) => out.push(SshIdentity {
                    key: key.clone(),
                    private,
                    tier: *unlock_tier,
                }),
                Err(err) => {
                    tracing::warn!(?key, %err, "ssh slot payload failed to parse; skipping");
                }
            }
        }
        out
    }

    fn find_by_public(&self, wanted: &PublicKey) -> Option<SshIdentity> {
        self.ssh_identities()
            .into_iter()
            // Compare key data, not `PublicKey`, because `PublicKey`
            // equality includes the comment and the requesting side sends
            // no comment.
            .find(|identity| {
                PublicKey::from(&identity.private).key_data() == wanted.key_data()
            })
    }
}

/// The [`ProtocolKey`] an SSH private key stores under.
pub fn protocol_key_for(private: &PrivateKey) -> ProtocolKey {
    let fingerprint = PublicKey::from(private).fingerprint(HashAlg::Sha256);
    ProtocolKey::new(SSH_MOD_ID, Some(fingerprint.to_string()))
}

/// Build the vault slot for an SSH private key.
pub fn slot_for(private: &PrivateKey) -> Result<IdentitySlot, AgentError> {
    let encoded = private.to_openssh(LineEnding::LF).map_err(AgentError::other)?;
    Ok(IdentitySlot::Direct {
        kind: SSH_MOD_ID.to_string(),
        payload: SecretBytes::new(encoded.as_bytes().to_vec()),
        // An SSH keypair is generated locally and registered with servers
        // (authorized_keys); losing it means registering a replacement.
        lineage: CredentialLineage::LocallyGeneratedExternallyRegistered,
        unlock_tier: UnlockTier::Session,
    })
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
        if identity.tier == UnlockTier::PerUse {
            tracing::warn!(key = ?identity.key, "per-use slot refused: no confirmation UI yet");
            return Err(std::io::Error::other(
                "per-use slot refused: the agent has no confirmation UI yet",
            )
            .into());
        }
        tracing::info!(key = ?identity.key, "signing request");
        identity
            .private
            .try_sign(request.data.as_slice())
            .map_err(AgentError::other)
    }

    async fn add_identity(&mut self, identity: AddIdentity) -> Result<(), AgentError> {
        let PrivateCredential::Key { privkey, comment } = identity.credential else {
            return Err(std::io::Error::other("unsupported credential type").into());
        };
        let mut private = PrivateKey::try_from(privkey).map_err(AgentError::other)?;
        if private.comment().is_empty() && !comment.is_empty() {
            private.set_comment(&comment);
        }
        let key = protocol_key_for(&private);
        let slot = slot_for(&private)?;
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
    use crate::vault::{InMemoryStorage, Profile, ProfileId};
    use crate::Ed25519Keypair;
    use signature::Verifier;
    use ssh_key::Algorithm;

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
        let protocol_key = protocol_key_for(&key);
        let IdentitySlot::Direct {
            kind,
            payload,
            lineage,
            ..
        } = slot_for(&key).unwrap()
        else {
            unreachable!()
        };
        agent
            .vault
            .lock()
            .unwrap()
            .add_slot(
                protocol_key,
                IdentitySlot::Direct {
                    kind,
                    payload,
                    lineage,
                    unlock_tier: UnlockTier::PerUse,
                },
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
