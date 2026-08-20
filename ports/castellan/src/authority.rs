//! Castellan's authority half: the resident Personae host.
//!
//! Custody without ownership. The vault and its truth are `personae`'s; this
//! is the keeper that holds them for the resident, serves the SSH agent,
//! brokers signing approvals, and applies the typed intents the projection
//! offers. Applications (graphshell first) compose it; apps talk to a pipe
//! and never see the key.
//!
//! This host deliberately starts in `StandaloneRetained`: H4 may exercise the
//! shared vault and approval boundary without stealing the user's standard SSH
//! agent endpoint before restart and real-login proofs exist.

use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pandect::{DeviceId, PersonaId, WalletEpochSealer, revoke_remote_auth_device};
use personae::agent::VaultAgent;
use personae::signing::{ApprovalBroker, DecisionError, RememberApproval, SigningDecision};
use personae::ssh_slot;
use personae::{
    CredentialLineage, DerivedKeyAttestation, Ed25519Keypair, Ed25519PublicKey, IdentityError,
    IdentityProvider, IdentityStorage, IdentityVault, ProfileId, ProtocolKey, UnlockTier, roster,
};
use serde::{Deserialize, Serialize};
use ssh_key::{Algorithm, PrivateKey, PublicKey};
use uuid::Uuid;

use crate::projection::{
    CreateProfileIntentV1, DEVICE_REVOKE_INTENT, GenerateSshKeyIntentV1,
    ImportSshKeyNativeIntentV1, PROFILE_CREATE_INTENT, PROFILE_SWITCH_INTENT, RemoveSshKeyIntentV1,
    RevokeDeviceIntentV1, SIGNING_APPROVE_IDLE_INTENT, SIGNING_APPROVE_ONCE_INTENT,
    SIGNING_DENY_INTENT, SSH_GENERATE_INTENT, SSH_IMPORT_NATIVE_INTENT, SSH_REMOVE_INTENT,
    SigningDecisionIntentV1, SshUnlockPolicyIntentV1, SwitchProfileIntentV1,
};
use crate::view::{
    AgentListenerView, CarryView, IdentitySurfaceSnapshot, ProfileView, SshKeyView, VaultLockView,
    VaultProtectionView, VaultView, load_carry_view,
};

const MAX_SHORT_TTL_SECONDS: u32 = 24 * 60 * 60;
#[cfg(windows)]
pub const STANDARD_WINDOWS_AGENT_ENDPOINT: &str = r"\\.\pipe\openssh-ssh-agent";

/// Rejected Graphshell identity action.
#[derive(Debug, thiserror::Error)]
pub enum IdentityIntentError {
    #[error("unknown identity intent")]
    UnknownIntent,
    #[error("invalid signing decision payload: {0}")]
    InvalidPayload(#[from] serde_json::Error),
    #[error("signing decision rejected: {0}")]
    Decision(#[from] DecisionError),
    #[error("identity vault operation failed: {0}")]
    Identity(#[from] IdentityError),
    #[error("SSH key generation failed")]
    KeyGeneration,
    #[error("SSH public key encoding failed")]
    PublicEncoding,
    #[error("SSH key comment must be at most 256 printable characters")]
    InvalidComment,
    #[error(
        "a persona id must be 1-64 characters of letters, digits, hyphen or underscore, so it \
         survives becoming a filename unchanged"
    )]
    InvalidProfileId,
    #[error("a persona name must be 1-256 printable characters")]
    InvalidProfileName,
    #[error("short idle approval must be between 1 and 86400 seconds")]
    InvalidIdleWindow,
    #[error("SSH key removal requires explicit confirmation")]
    ConfirmationRequired,
    #[error("SSH key fingerprint is not present in the selected profile")]
    KeyNotFound,
    #[error("SSH import requires a native private-key handoff")]
    NativeHandoffRequired,
    #[error("device revocation requires explicit confirmation")]
    DeviceRevocationConfirmationRequired,
    #[error("carry authority is not configured")]
    CarryUnavailable,
    #[error("device revocation failed ({0:?})")]
    DeviceRevocation(std::io::ErrorKind),
}

/// Public result of a native SSH key mutation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshKeyMutationReceipt {
    pub operation: SshKeyMutationKind,
    pub fingerprint: String,
    pub comment: String,
    pub public_openssh: String,
    pub unlock_policy: String,
    pub replaced_existing: bool,
}

/// Mutation kind safe to retain in a local receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshKeyMutationKind {
    Generated,
    Imported,
    Removed,
}

/// Result of applying one local identity control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityIntentOutcome {
    SigningDecision,
    SshKeyMutation(SshKeyMutationReceipt),
    DeviceRevocation(DeviceRevocationReceipt),
    ProfileSwitch(ProfileSwitchReceipt),
    ProfileCreated(ProfileCreatedReceipt),
}

/// Public facts produced by minting a persona. No key material: the master
/// public key is reported as a fingerprint, the way the profile cards do.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileCreatedReceipt {
    pub id: String,
    pub display_name: String,
    pub master_public_fingerprint: String,
}

/// Public facts produced by a live profile switch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileSwitchReceipt {
    /// The persona the host now speaks as.
    pub profile: String,
    /// Whether the choice was written beside the vault for the rest of the
    /// family, or only applied to this host (an ephemeral vault has no
    /// beside). Shown, not guessed: "everyone follows" and "just here" are
    /// different promises.
    pub remembered: bool,
}

/// Public facts produced by a carry-authority revocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRevocationReceipt {
    pub device_id: String,
    pub already_revoked: bool,
    pub rotated_personas: Vec<String>,
    pub refreshed_devices: Vec<String>,
}

/// Native owner of one shared Personae vault and its SSH adapter.
pub struct PersonaeHost<S: IdentityStorage> {
    vault: Arc<Mutex<IdentityVault<S>>>,
    agent: VaultAgent<S>,
    approval: ApprovalBroker,
    data_root: Option<PathBuf>,
    /// Where the vault lives on disk, when it lives anywhere. A profile
    /// switch writes the family's remembered choice beside it
    /// ([`personae::roster::remember_profile`]); an ephemeral host has no
    /// beside, so `None` switches without remembering.
    vault_dir: Option<PathBuf>,
    protection: VaultProtectionView,
    lock: VaultLockView,
    listener: Arc<Mutex<AgentListenerView>>,
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
            listener: Arc::new(Mutex::new(AgentListenerView::StandaloneRetained)),
            vault_dir: None,
        }
    }

    /// Name where the vault lives, so a profile switch also remembers the
    /// choice for the rest of the family.
    pub fn with_vault_dir(mut self, dir: PathBuf) -> Self {
        self.vault_dir = Some(dir);
        self
    }

    /// A fresh per-connection SSH agent session over the resident vault.
    pub fn agent_session(&self) -> VaultAgent<S> {
        self.agent.clone()
    }

    /// Bind an isolated Windows named pipe for acceptance testing.
    ///
    /// The standard OpenSSH endpoint is rejected here. Taking it over requires
    /// a separate cutover path with restart and real-login receipts.
    #[cfg(windows)]
    pub fn bind_receipt_listener(
        &self,
        endpoint: &str,
    ) -> std::io::Result<ssh_agent_lib::agent::NamedPipeListener> {
        if endpoint.eq_ignore_ascii_case(STANDARD_WINDOWS_AGENT_ENDPOINT) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "receipt listener cannot bind the standard SSH agent endpoint",
            ));
        }
        let listener = ssh_agent_lib::agent::NamedPipeListener::bind(endpoint)?;
        *self.listener.lock().unwrap() = AgentListenerView::ReceiptEndpoint {
            endpoint: endpoint.to_string(),
        };
        Ok(listener)
    }

    /// Bind an isolated Unix socket for acceptance testing.
    #[cfg(not(windows))]
    pub fn bind_receipt_listener(
        &self,
        endpoint: &str,
    ) -> std::io::Result<tokio::net::UnixListener> {
        if endpoint.trim().is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "the receipt SSH agent socket path is empty",
            ));
        }
        let listener = tokio::net::UnixListener::bind(endpoint)?;
        *self.listener.lock().unwrap() = AgentListenerView::ReceiptEndpoint {
            endpoint: endpoint.to_string(),
        };
        Ok(listener)
    }

    /// Bind the standard Windows OpenSSH endpoint for the resident host.
    ///
    /// This is deliberately distinct from [`Self::bind_receipt_listener`]:
    /// only a lifecycle-managed device host should call it.
    #[cfg(windows)]
    pub fn bind_standard_listener(
        &self,
        endpoint: &str,
    ) -> std::io::Result<ssh_agent_lib::agent::NamedPipeListener> {
        if !endpoint.eq_ignore_ascii_case(STANDARD_WINDOWS_AGENT_ENDPOINT) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "the Windows standard listener must use the OpenSSH agent pipe",
            ));
        }
        let listener = ssh_agent_lib::agent::NamedPipeListener::bind(endpoint)?;
        *self.listener.lock().unwrap() = AgentListenerView::StandardEndpoint {
            endpoint: endpoint.to_string(),
        };
        Ok(listener)
    }

    /// Bind the configured Unix `SSH_AUTH_SOCK` for the resident host.
    #[cfg(not(windows))]
    pub fn bind_standard_listener(
        &self,
        endpoint: &str,
    ) -> std::io::Result<tokio::net::UnixListener> {
        if endpoint.trim().is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "the standard SSH agent socket path is empty",
            ));
        }
        let listener = tokio::net::UnixListener::bind(endpoint)?;
        *self.listener.lock().unwrap() = AgentListenerView::StandardEndpoint {
            endpoint: endpoint.to_string(),
        };
        Ok(listener)
    }

    /// Generate an Ed25519 key entirely inside the resident authority.
    pub fn generate_ssh_key(
        &self,
        request: GenerateSshKeyIntentV1,
    ) -> Result<SshKeyMutationReceipt, IdentityIntentError> {
        validate_comment(&request.comment)?;
        let tier = unlock_tier(request.unlock_policy)?;
        let mut private = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519)
            .map_err(|_| IdentityIntentError::KeyGeneration)?;
        private.set_comment(&request.comment);
        self.store_ssh_private(private, tier, SshKeyMutationKind::Generated)
    }

    /// Store a key received directly from a native file picker.
    ///
    /// Callers pass the parsed key object, not serialized key bytes. This API
    /// is intentionally separate from `apply_intent`.
    pub fn import_ssh_private(
        &self,
        private: PrivateKey,
        options: ImportSshKeyNativeIntentV1,
    ) -> Result<SshKeyMutationReceipt, IdentityIntentError> {
        validate_comment(private.comment())?;
        let tier = unlock_tier(options.unlock_policy)?;
        self.store_ssh_private(private, tier, SshKeyMutationKind::Imported)
    }

    fn store_ssh_private(
        &self,
        private: PrivateKey,
        tier: UnlockTier,
        operation: SshKeyMutationKind,
    ) -> Result<SshKeyMutationReceipt, IdentityIntentError> {
        let key = ssh_slot::protocol_key_for(&private);
        let slot = ssh_slot::slot_for(&private, tier)?;
        let public = PublicKey::from(&private);
        let fingerprint = public.fingerprint(ssh_key::HashAlg::Sha256).to_string();
        let public_openssh = public
            .to_openssh()
            .map_err(|_| IdentityIntentError::PublicEncoding)?;
        let comment = private.comment().to_string();
        let mut vault = self.vault.lock().unwrap();
        let replaced_existing = vault.current_profile().slots.contains_key(&key);
        vault.add_slot(key, slot)?;
        Ok(SshKeyMutationReceipt {
            operation,
            fingerprint,
            comment,
            public_openssh,
            unlock_policy: unlock_label(tier),
            replaced_existing,
        })
    }

    /// Remove one SSH slot only after the local UI confirms its fingerprint.
    pub fn remove_ssh_key(
        &self,
        request: RemoveSshKeyIntentV1,
    ) -> Result<SshKeyMutationReceipt, IdentityIntentError> {
        if !request.confirmed {
            return Err(IdentityIntentError::ConfirmationRequired);
        }
        let key = ProtocolKey::new(ssh_slot::SSH_MOD_ID, Some(request.fingerprint.clone()));
        let mut vault = self.vault.lock().unwrap();
        let Some(slot) = vault.current_profile().slots.get(&key) else {
            return Err(IdentityIntentError::KeyNotFound);
        };
        let private = ssh_slot::private_key_from_slot(slot)?;
        let public = PublicKey::from(&private);
        let receipt = SshKeyMutationReceipt {
            operation: SshKeyMutationKind::Removed,
            fingerprint: public.fingerprint(ssh_key::HashAlg::Sha256).to_string(),
            comment: private.comment().to_string(),
            public_openssh: public
                .to_openssh()
                .map_err(|_| IdentityIntentError::PublicEncoding)?,
            unlock_policy: unlock_label(slot.unlock_tier()),
            replaced_existing: false,
        };
        vault.remove_slot(&key)?;
        Ok(receipt)
    }

    /// The private-lane payload sealer for one persona.
    ///
    /// Eidetic sits below the wallet and owns no keys, so its seal seam stays
    /// inert until a host supplies a `PayloadSealer`; its own header says
    /// landing the seam "changes no runtime behavior" for exactly that reason.
    /// This is the supply point. The keeper already holds the carry root, which
    /// is the only thing beyond the persona that building one needs, so no
    /// other component has to learn where the wallet lives.
    ///
    /// `None` twice over, and deliberately not an error either time: a host
    /// with no carry root has no wallet to seal under, and a persona with no
    /// staged epoch has nothing to seal with. Eidetic's own posture is that "a
    /// keyless host stays in the cleartext lane, which is a host policy
    /// decision, not this seam's to enforce", so the decision is returned to
    /// the caller rather than taken here. A caller that requires sealing
    /// refuses on `None`; one that does not writes cleartext, which is what
    /// every caller does today.
    pub fn payload_sealer(&self, persona: PersonaId) -> io::Result<Option<WalletEpochSealer>> {
        let Some(data_root) = self.data_root.as_deref() else {
            return Ok(None);
        };
        WalletEpochSealer::for_persona(data_root, persona)
    }

    /// Revoke one delegated device through pandect's live authority.
    pub fn revoke_device(
        &self,
        request: RevokeDeviceIntentV1,
    ) -> Result<DeviceRevocationReceipt, IdentityIntentError> {
        if !request.confirmed {
            return Err(IdentityIntentError::DeviceRevocationConfirmationRequired);
        }
        let data_root = self
            .data_root
            .as_deref()
            .ok_or(IdentityIntentError::CarryUnavailable)?;
        let outcome = revoke_remote_auth_device(data_root, DeviceId::from_uuid(request.device_id))
            .map_err(|error| IdentityIntentError::DeviceRevocation(error.kind()))?;
        Ok(DeviceRevocationReceipt {
            device_id: outcome.device_id.as_uuid().to_string(),
            already_revoked: outcome.already_revoked,
            rotated_personas: outcome
                .rotated_personas
                .into_iter()
                .map(|persona| persona.as_uuid().to_string())
                .collect(),
            refreshed_devices: outcome
                .refreshed_devices
                .into_iter()
                .map(|device| device.as_uuid().to_string())
                .collect(),
        })
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
                agent: self.listener.lock().unwrap().clone(),
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

    /// Switch the resident vault to another persona, live.
    ///
    /// Everything sharing the vault follows from its next operation — the SSH
    /// agent holds the same `Arc<Mutex<IdentityVault>>`, so the next signing
    /// request is served from the new persona's slots with no restart
    /// anywhere. The choice is then remembered beside the vault so the rest
    /// of the family opens on it too; a host with no on-disk vault switches
    /// without remembering, and the receipt says which happened.
    pub fn switch_profile(
        &self,
        payload: SwitchProfileIntentV1,
    ) -> Result<ProfileSwitchReceipt, IdentityIntentError> {
        let id = ProfileId(payload.profile);
        self.vault.lock().unwrap().switch_profile(&id)?;
        let remembered = match &self.vault_dir {
            Some(dir) => match roster::remember_profile(dir, &id) {
                Ok(()) => true,
                // The switch itself succeeded; a choice that could not be
                // written is worth saying, not worth unwinding.
                Err(error) => {
                    tracing::warn!(%error, profile = %id.0, "switched without remembering");
                    false
                }
            },
            None => false,
        };
        Ok(ProfileSwitchReceipt {
            profile: id.0,
            remembered,
        })
    }

    /// Mint a persona in the vault.
    ///
    /// **Does not switch to it.** Creating an identity and becoming it are
    /// separate decisions: a persona minted for another device or another
    /// purpose is not one the user is necessarily adopting, and the new card
    /// carries the ordinary switch action for when they are.
    ///
    /// The id is constrained rather than sanitized. It reaches a filename in
    /// `owner_settings::settings_path`, which replaces anything unsafe with
    /// `_`; accepting `a/b` here would mean it and `a_b` silently share one
    /// settings file. Refusing at the point of creation is the only place that
    /// cannot be worked around later.
    pub fn create_profile(
        &self,
        payload: CreateProfileIntentV1,
    ) -> Result<ProfileCreatedReceipt, IdentityIntentError> {
        let id = payload.id.trim();
        if id.is_empty()
            || id.chars().count() > 64
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(IdentityIntentError::InvalidProfileId);
        }
        let display_name = payload.display_name.trim();
        if display_name.is_empty()
            || display_name.chars().count() > 256
            || display_name.chars().any(char::is_control)
        {
            return Err(IdentityIntentError::InvalidProfileName);
        }
        // `roster::create_profile` refuses a taken id, which is the guard that
        // matters: minting over a persona would replace its master key and
        // every certificate rooted on it.
        let vault = self.vault.lock().unwrap();
        let profile =
            roster::create_profile(vault.storage(), &ProfileId(id.to_string()), display_name)?;
        Ok(ProfileCreatedReceipt {
            id: profile.id.0,
            display_name: profile.display_name,
            master_public_fingerprint: format!(
                "blake3:{}",
                blake3::hash(&profile.master.public_key().to_bytes()).to_hex()
            ),
        })
    }

    /// Apply one typed action emitted by [`crate::projection`].
    pub fn apply_intent(
        &self,
        intent: &str,
        payload: &[u8],
    ) -> Result<IdentityIntentOutcome, IdentityIntentError> {
        match intent {
            SIGNING_APPROVE_ONCE_INTENT => {
                let payload: SigningDecisionIntentV1 = serde_json::from_slice(payload)?;
                self.approve_once(payload.request_id)?;
                Ok(IdentityIntentOutcome::SigningDecision)
            }
            SIGNING_APPROVE_IDLE_INTENT => {
                let payload: SigningDecisionIntentV1 = serde_json::from_slice(payload)?;
                self.approve_until_idle(payload.request_id)?;
                Ok(IdentityIntentOutcome::SigningDecision)
            }
            SIGNING_DENY_INTENT => {
                let payload: SigningDecisionIntentV1 = serde_json::from_slice(payload)?;
                self.deny(payload.request_id)?;
                Ok(IdentityIntentOutcome::SigningDecision)
            }
            SSH_GENERATE_INTENT => {
                let payload: GenerateSshKeyIntentV1 = serde_json::from_slice(payload)?;
                self.generate_ssh_key(payload)
                    .map(IdentityIntentOutcome::SshKeyMutation)
            }
            SSH_REMOVE_INTENT => {
                let payload: RemoveSshKeyIntentV1 = serde_json::from_slice(payload)?;
                self.remove_ssh_key(payload)
                    .map(IdentityIntentOutcome::SshKeyMutation)
            }
            SSH_IMPORT_NATIVE_INTENT => {
                let _: ImportSshKeyNativeIntentV1 = serde_json::from_slice(payload)?;
                Err(IdentityIntentError::NativeHandoffRequired)
            }
            DEVICE_REVOKE_INTENT => {
                let payload: RevokeDeviceIntentV1 = serde_json::from_slice(payload)?;
                self.revoke_device(payload)
                    .map(IdentityIntentOutcome::DeviceRevocation)
            }
            PROFILE_SWITCH_INTENT => {
                let payload: SwitchProfileIntentV1 = serde_json::from_slice(payload)?;
                self.switch_profile(payload)
                    .map(IdentityIntentOutcome::ProfileSwitch)
            }
            PROFILE_CREATE_INTENT => {
                let payload: CreateProfileIntentV1 = serde_json::from_slice(payload)?;
                self.create_profile(payload)
                    .map(IdentityIntentOutcome::ProfileCreated)
            }
            _ => Err(IdentityIntentError::UnknownIntent),
        }
    }
}

impl<S: IdentityStorage + 'static> IdentityProvider for PersonaeHost<S> {
    fn master_public_key(&self) -> Ed25519PublicKey {
        IdentityProvider::master_public_key(&*self.vault.lock().unwrap())
    }

    fn derive_keypair(&self, salt: &[u8]) -> Result<Ed25519Keypair, IdentityError> {
        self.vault.lock().unwrap().derive_keypair(salt)
    }

    fn attest_derived_key(&self, salt: &[u8]) -> Result<DerivedKeyAttestation, IdentityError> {
        self.vault.lock().unwrap().attest_derived_key(salt)
    }
}

fn validate_comment(comment: &str) -> Result<(), IdentityIntentError> {
    if comment.chars().count() > 256 || comment.chars().any(char::is_control) {
        return Err(IdentityIntentError::InvalidComment);
    }
    Ok(())
}

fn unlock_tier(policy: SshUnlockPolicyIntentV1) -> Result<UnlockTier, IdentityIntentError> {
    match policy {
        SshUnlockPolicyIntentV1::Session => Ok(UnlockTier::Session),
        SshUnlockPolicyIntentV1::ShortTtl { idle_seconds }
            if (1..=MAX_SHORT_TTL_SECONDS).contains(&idle_seconds) =>
        {
            Ok(UnlockTier::ShortTtl { idle_seconds })
        }
        SshUnlockPolicyIntentV1::ShortTtl { .. } => Err(IdentityIntentError::InvalidIdleWindow),
        SshUnlockPolicyIntentV1::PerUse => Ok(UnlockTier::PerUse),
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
    use signature::Verifier;
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

    #[test]
    fn a_created_persona_joins_the_vault_and_the_host_stays_where_it_was() {
        // Creating and becoming are separate decisions: a persona minted for
        // another device is not one the user is adopting.
        let storage = InMemoryStorage::new();
        let work = Profile::new(
            ProfileId("work".into()),
            "Work",
            Ed25519Keypair::from_seed([0x31; 32]),
        );
        storage.save_profile(&work).unwrap();
        let host = PersonaeHost::new(
            IdentityVault::with_profile(storage, work),
            None,
            VaultProtectionView::Ephemeral,
        );
        let before = IdentityProvider::master_public_key(&host).to_bytes();

        let outcome = host
            .apply_intent(
                PROFILE_CREATE_INTENT,
                &serde_json::to_vec(&CreateProfileIntentV1 {
                    id: "alt".into(),
                    display_name: "Late Night Alt".into(),
                })
                .unwrap(),
            )
            .unwrap();

        match outcome {
            IdentityIntentOutcome::ProfileCreated(receipt) => {
                assert_eq!(receipt.id, "alt");
                assert_eq!(receipt.display_name, "Late Night Alt");
                assert!(receipt.master_public_fingerprint.starts_with("blake3:"));
            }
            other => panic!("expected a creation receipt, got {other:?}"),
        }

        let snapshot = host.snapshot().unwrap();
        let mut ids: Vec<&str> = snapshot.profiles.iter().map(|p| p.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["alt", "work"], "the new persona is in the roster");
        assert_eq!(
            snapshot
                .profiles
                .iter()
                .filter(|p| p.selected)
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            ["work"],
            "creating does not switch"
        );
        assert_eq!(
            IdentityProvider::master_public_key(&host).to_bytes(),
            before,
            "and the host still speaks as the persona it had"
        );
    }

    #[test]
    fn an_id_that_would_not_survive_becoming_a_filename_is_refused() {
        // `owner_settings::settings_path` replaces anything unsafe with `_`,
        // so accepting `a/b` here would mean it and `a_b` silently share one
        // settings file. Refusing at creation is the only place that cannot be
        // worked around later.
        let host = PersonaeHost::new(
            IdentityVault::with_profile(
                InMemoryStorage::new(),
                Profile::new(
                    ProfileId("work".into()),
                    "Work",
                    Ed25519Keypair::from_seed([0x32; 32]),
                ),
            ),
            None,
            VaultProtectionView::Ephemeral,
        );
        for bad in ["../../evil", "a/b", "with space", "", "   "] {
            assert!(
                matches!(
                    host.create_profile(CreateProfileIntentV1 {
                        id: bad.into(),
                        display_name: "Whatever".into(),
                    }),
                    Err(IdentityIntentError::InvalidProfileId)
                ),
                "{bad:?} must be refused"
            );
        }
        assert!(
            host.create_profile(CreateProfileIntentV1 {
                id: "a".repeat(65),
                display_name: "Long".into(),
            })
            .is_err()
        );
        // And a name that is only whitespace, or carries control characters.
        for bad in ["", "   ", "two\nlines"] {
            assert!(
                matches!(
                    host.create_profile(CreateProfileIntentV1 {
                        id: "fine".into(),
                        display_name: bad.into(),
                    }),
                    Err(IdentityIntentError::InvalidProfileName)
                ),
                "{bad:?} must be refused as a name"
            );
        }
        assert_eq!(
            host.snapshot().unwrap().profiles.len(),
            1,
            "nothing was minted by any of the refusals"
        );
    }

    #[test]
    fn creating_over_an_existing_persona_is_refused() {
        // The guard that matters: minting over a persona replaces its master
        // key and every certificate rooted on it.
        let storage = InMemoryStorage::new();
        let work = Profile::new(
            ProfileId("work".into()),
            "Work",
            Ed25519Keypair::from_seed([0x33; 32]),
        );
        storage.save_profile(&work).unwrap();
        let before = work.master.public_key().to_bytes();
        let host = PersonaeHost::new(
            IdentityVault::with_profile(storage, work),
            None,
            VaultProtectionView::Ephemeral,
        );

        assert!(
            host.create_profile(CreateProfileIntentV1 {
                id: "work".into(),
                display_name: "Impostor".into(),
            })
            .is_err()
        );
        assert_eq!(
            IdentityProvider::master_public_key(&host).to_bytes(),
            before,
            "the existing persona keeps its key"
        );
    }

    #[test]
    fn a_projected_switch_is_live_and_remembered_for_the_family() {
        // The gap this closes: the projection could show which persona the
        // host speaks as, and nothing could change it.
        let storage = InMemoryStorage::new();
        let work = Profile::new(
            ProfileId("work".into()),
            "Work",
            Ed25519Keypair::from_seed([0x11; 32]),
        );
        let personal = Profile::new(
            ProfileId("personal".into()),
            "Personal",
            Ed25519Keypair::from_seed([0x12; 32]),
        );
        storage.save_profile(&work).unwrap();
        storage.save_profile(&personal).unwrap();
        let vault_dir =
            std::env::temp_dir().join(format!("graphshell-switch-receipt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&vault_dir);
        let host = PersonaeHost::new(
            IdentityVault::with_profile(storage, work),
            None,
            VaultProtectionView::Ephemeral,
        )
        .with_vault_dir(vault_dir.clone());

        let before = IdentityProvider::master_public_key(&host).to_bytes();
        let outcome = host
            .apply_intent(
                PROFILE_SWITCH_INTENT,
                &serde_json::to_vec(&SwitchProfileIntentV1 {
                    profile: "personal".into(),
                })
                .unwrap(),
            )
            .unwrap();

        // Live: the host's own identity — the one the shared SSH agent
        // serves — is the new persona, and the snapshot marks it selected.
        assert_ne!(
            IdentityProvider::master_public_key(&host).to_bytes(),
            before
        );
        let snapshot = host.snapshot().unwrap();
        let selected: Vec<&str> = snapshot
            .profiles
            .iter()
            .filter(|profile| profile.selected)
            .map(|profile| profile.id.as_str())
            .collect();
        assert_eq!(selected, ["personal"]);

        // Remembered: the family's choice file now names the new persona.
        match outcome {
            IdentityIntentOutcome::ProfileSwitch(receipt) => {
                assert_eq!(receipt.profile, "personal");
                assert!(receipt.remembered);
            }
            other => panic!("expected a profile switch receipt, got {other:?}"),
        }
        assert_eq!(
            roster::chosen_profile(&vault_dir),
            Some(ProfileId("personal".into()))
        );

        // A persona that does not exist is an error, and the host still
        // speaks as the one it had.
        assert!(
            host.switch_profile(SwitchProfileIntentV1 {
                profile: "absent".into(),
            })
            .is_err()
        );
        assert_eq!(
            host.snapshot()
                .unwrap()
                .profiles
                .iter()
                .find(|p| p.selected)
                .unwrap()
                .id,
            "personal"
        );
        let _ = std::fs::remove_dir_all(&vault_dir);
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

    #[cfg(windows)]
    #[tokio::test]
    async fn isolated_named_pipe_lists_and_signs_through_the_ssh_wire_protocol() {
        use ssh_agent_lib::client::Client;
        use tokio::net::windows::named_pipe::ClientOptions;

        let mut private =
            ssh_key::PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        private.set_comment("wire-receipt");
        let public = ssh_key::PublicKey::from(&private);
        let mut profile = Profile::new(
            ProfileId("research".to_string()),
            "Research",
            Ed25519Keypair::from_seed([0x3a; 32]),
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
        assert_eq!(
            host.bind_receipt_listener(r"\\.\pipe\openssh-ssh-agent")
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            host.bind_standard_listener(r"\\.\pipe\graphshell-not-standard")
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidInput
        );
        let endpoint = format!(r"\\.\pipe\graphshell-h4-receipt-{}", Uuid::new_v4());
        let listener = host.bind_receipt_listener(&endpoint).unwrap();
        let server = tokio::spawn(ssh_agent_lib::agent::listen(listener, host.agent_session()));

        let pipe = ClientOptions::new().open(&endpoint).unwrap();
        let mut client = Client::new(pipe);
        // One slot, two offers. A certifiable key is offered as its
        // certificate first, so a host that trusts the authority needs no
        // per-key enrollment, and then bare, so a host that has only ever seen
        // the key still works. Both resolve to the same slot when signing,
        // because a credential's key data is the key either way.
        let identities = client.request_identities().await.unwrap();
        assert_eq!(
            identities
                .iter()
                .map(|identity| identity.comment.as_str())
                .collect::<Vec<_>>(),
            ["wire-receipt (personae certificate)", "wire-receipt"],
            "the wire offers the certificate before the bare key"
        );
        assert_eq!(
            host.snapshot().unwrap().vault.agent,
            AgentListenerView::ReceiptEndpoint {
                endpoint: endpoint.clone()
            }
        );

        let data = b"graphshell-isolated-wire-receipt".to_vec();
        let verify_data = data.clone();
        let credential = identities[0].credential.clone();
        let signing = tokio::spawn(async move {
            client
                .sign(SignRequest {
                    credential,
                    data,
                    flags: 0,
                })
                .await
        });

        let pending = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Some(pending) = host.snapshot().unwrap().pending_signing.into_iter().next() {
                    break pending;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        host.approve_once(pending.request.request_id).unwrap();

        let signature = signing.await.unwrap().unwrap();
        public.key_data().verify(&verify_data, &signature).unwrap();
        assert!(matches!(
            host.snapshot().unwrap().signing_history[0].result,
            personae::signing::SigningRecordResult::Signed { .. }
        ));

        server.abort();
        let _ = server.await;
    }

    #[test]
    fn typed_generation_and_confirmed_removal_mutate_the_shared_vault() {
        let profile = Profile::new(
            ProfileId("research".to_string()),
            "Research",
            Ed25519Keypair::from_seed([0x19; 32]),
        );
        let host = PersonaeHost::new(
            IdentityVault::with_profile(InMemoryStorage::new(), profile),
            None,
            VaultProtectionView::Ephemeral,
        );
        let generate = serde_json::to_vec(&GenerateSshKeyIntentV1 {
            comment: "generated in Graphshell".to_string(),
            unlock_policy: SshUnlockPolicyIntentV1::ShortTtl { idle_seconds: 45 },
        })
        .unwrap();
        let generated = host.apply_intent(SSH_GENERATE_INTENT, &generate).unwrap();
        let IdentityIntentOutcome::SshKeyMutation(generated) = generated else {
            panic!("expected SSH key mutation");
        };
        assert_eq!(generated.operation, SshKeyMutationKind::Generated);
        assert_eq!(generated.unlock_policy, "45s idle");
        assert!(generated.public_openssh.starts_with("ssh-ed25519 "));
        assert_eq!(host.snapshot().unwrap().ssh_keys.len(), 1);

        let unconfirmed = serde_json::to_vec(&RemoveSshKeyIntentV1 {
            fingerprint: generated.fingerprint.clone(),
            confirmed: false,
        })
        .unwrap();
        assert!(matches!(
            host.apply_intent(SSH_REMOVE_INTENT, &unconfirmed),
            Err(IdentityIntentError::ConfirmationRequired)
        ));
        assert_eq!(host.snapshot().unwrap().ssh_keys.len(), 1);

        let confirmed = serde_json::to_vec(&RemoveSshKeyIntentV1 {
            fingerprint: generated.fingerprint,
            confirmed: true,
        })
        .unwrap();
        let removed = host.apply_intent(SSH_REMOVE_INTENT, &confirmed).unwrap();
        assert!(matches!(
            removed,
            IdentityIntentOutcome::SshKeyMutation(SshKeyMutationReceipt {
                operation: SshKeyMutationKind::Removed,
                ..
            })
        ));
        assert!(host.snapshot().unwrap().ssh_keys.is_empty());
    }

    #[test]
    fn native_import_never_accepts_private_key_bytes_as_an_intent() {
        let profile = Profile::new(
            ProfileId("research".to_string()),
            "Research",
            Ed25519Keypair::from_seed([0x29; 32]),
        );
        let host = PersonaeHost::new(
            IdentityVault::with_profile(InMemoryStorage::new(), profile),
            None,
            VaultProtectionView::Ephemeral,
        );
        let mut private =
            ssh_key::PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        private.set_comment("native import");
        let private_openssh = private.to_openssh(LineEnding::LF).unwrap().to_string();
        let receipt = host
            .import_ssh_private(
                private,
                ImportSshKeyNativeIntentV1 {
                    unlock_policy: SshUnlockPolicyIntentV1::PerUse,
                },
            )
            .unwrap();
        let public_json = serde_json::to_string(&receipt).unwrap();
        assert_eq!(receipt.operation, SshKeyMutationKind::Imported);
        assert!(!public_json.contains(&private_openssh));
        assert!(!public_json.contains("BEGIN OPENSSH PRIVATE KEY"));

        let options = serde_json::to_vec(&ImportSshKeyNativeIntentV1 {
            unlock_policy: SshUnlockPolicyIntentV1::Session,
        })
        .unwrap();
        assert!(matches!(
            host.apply_intent(SSH_IMPORT_NATIVE_INTENT, &options),
            Err(IdentityIntentError::NativeHandoffRequired)
        ));
        assert_eq!(host.snapshot().unwrap().ssh_keys.len(), 1);
    }
}
