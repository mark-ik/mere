//! Secret-free read model for Graphshell's resident Personae host.
//!
//! This is an application-layer projection. It may name public keys, public
//! carrier facts, vault posture, and grant summaries. It has no field capable
//! of carrying a seed, private key, vault root, cleartext signing payload, or
//! wrapped epoch material.

use std::io;
use std::path::Path;

use personae::signing::{PendingSigningRequest, SigningRecord};
use serde::{Deserialize, Serialize};
use session_runtime::{
    DeviceExposure, DeviceMode, RecoveryPolicy, load_device_roster, load_identity_wallet,
    load_signed_device_grant, verify_device_grant,
};

/// Storage protection selected for the resident vault.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaultProtectionView {
    OsProtected,
    Passphrase,
    Ephemeral,
}

/// Whether the resident vault may currently perform private operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaultLockView {
    Locked,
    Unlocked,
}

/// Whether Graphshell has taken ownership of an SSH agent endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentListenerView {
    /// H4 must retain the standalone agent while the replacement is unproved.
    StandaloneRetained,
    /// A nonstandard receipt endpoint is active, without changing user config.
    ReceiptEndpoint { endpoint: String },
    /// The standard endpoint is active after cutover proof.
    StandardEndpoint { endpoint: String },
}

/// Vault state safe to disclose to Graphshell clients.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultView {
    pub protection: VaultProtectionView,
    pub lock: VaultLockView,
    pub agent: AgentListenerView,
}

/// One public profile summary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileView {
    pub id: String,
    pub display_name: String,
    pub selected: bool,
    pub slot_count: usize,
    pub master_public_fingerprint: String,
}

/// Public half and recovery posture of one SSH vault slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshKeyView {
    pub profile: String,
    pub fingerprint: String,
    pub comment: String,
    pub public_openssh: String,
    pub lineage: String,
    pub device_loss_note: String,
    pub unlock_policy: String,
}

/// Public device roster entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceView {
    pub device_id: String,
    pub label: String,
    pub mode: String,
    pub exposure: String,
    pub public_key_fingerprint: String,
    pub revoked: bool,
    pub grant_ref: Option<String>,
}

/// Secret-free summary of one signed device grant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceGrantView {
    pub device_id: String,
    pub grant_ref: Option<String>,
    pub signature_valid: Option<bool>,
    pub issued_at_ms: u64,
    pub expires_at_ms: Option<u64>,
    pub personas: Vec<String>,
    pub scopes: Vec<String>,
    pub attenuations: Vec<String>,
    pub wrapped_epoch_count: usize,
}

/// Carry state loaded from session-runtime without importing secret bridges.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarryView {
    pub recovery_policy: Option<String>,
    pub personas: Vec<String>,
    pub devices: Vec<DeviceView>,
    pub grants: Vec<DeviceGrantView>,
    pub unavailable: Vec<String>,
}

/// Complete identity surface disclosed by the native host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentitySurfaceSnapshot {
    pub vault: VaultView,
    pub profiles: Vec<ProfileView>,
    pub ssh_keys: Vec<SshKeyView>,
    pub carry: CarryView,
    pub pending_signing: Vec<PendingSigningRequest>,
    pub signing_history: Vec<SigningRecord>,
}

impl IdentitySurfaceSnapshot {
    /// Serialize only the public read model.
    pub fn to_public_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}

/// Load the public carry lane. Secret seed and epoch bridge APIs are not used.
pub fn load_carry_view(data_root: &Path) -> io::Result<CarryView> {
    let mut view = CarryView::default();
    let Some(wallet) = load_identity_wallet(data_root)? else {
        view.unavailable
            .push("identity wallet manifest is absent".to_string());
        return Ok(view);
    };

    view.recovery_policy = Some(recovery_policy_label(wallet.recovery_policy).to_string());
    view.personas = wallet
        .personas
        .iter()
        .map(|persona| persona.persona_id.as_uuid().to_string())
        .collect();
    view.personas.sort();

    let roster = load_device_roster(data_root)?;
    if let Some(roster) = roster {
        for device in roster.devices {
            let device_id = device.device_id.as_uuid().to_string();
            let grant_ref = device.grant_ref.map(|reference| reference.to_string());
            view.devices.push(DeviceView {
                device_id: device_id.clone(),
                label: device.label,
                mode: device_mode_label(device.mode).to_string(),
                exposure: exposure_label(device.exposure).to_string(),
                public_key_fingerprint: format!(
                    "blake3:{}",
                    blake3::hash(&device.device_pubkey.0).to_hex()
                ),
                revoked: roster.revoked.contains(&device.device_id),
                grant_ref: grant_ref.clone(),
            });

            match load_signed_device_grant(data_root, device.device_id)? {
                Some(grant) => {
                    let signature_valid = verify_device_grant(&grant).ok();
                    view.grants.push(DeviceGrantView {
                        device_id,
                        grant_ref,
                        signature_valid,
                        issued_at_ms: grant.payload.issued_at_ms,
                        expires_at_ms: grant.payload.expires_at_ms,
                        personas: grant
                            .payload
                            .personas
                            .iter()
                            .map(|persona| persona.as_uuid().to_string())
                            .collect(),
                        scopes: grant.payload.scopes,
                        attenuations: grant.payload.attenuations,
                        wrapped_epoch_count: grant.payload.wrapped_private_epochs.len(),
                    });
                }
                None if device.grant_ref.is_some() => view
                    .unavailable
                    .push(format!("grant bytes unavailable for device {device_id}")),
                None => {}
            }
        }
    } else {
        view.unavailable.push("device roster is absent".to_string());
    }

    view.devices
        .sort_by(|left, right| left.device_id.cmp(&right.device_id));
    view.grants
        .sort_by(|left, right| left.device_id.cmp(&right.device_id));
    Ok(view)
}

fn device_mode_label(mode: DeviceMode) -> &'static str {
    match mode {
        DeviceMode::Copy => "seed copy",
        DeviceMode::RemoteAuth => "delegated",
    }
}

fn exposure_label(exposure: DeviceExposure) -> &'static str {
    match exposure {
        DeviceExposure::HiddenClient => "hidden client",
        DeviceExposure::ExposedEgress => "exposed egress",
    }
}

fn recovery_policy_label(policy: RecoveryPolicy) -> &'static str {
    match policy {
        RecoveryPolicy::SeedAndDeviceHandover => "seed and device handover",
        RecoveryPolicy::SeedPhraseOnly => "seed phrase only",
    }
}

#[cfg(test)]
mod tests {
    use session_runtime::{
        DeviceId, DevicePublicKey, DeviceRecord, DeviceRoster, IdentityWalletManifest, PersonaId,
        PersonaWalletRef, save_device_roster, save_identity_wallet,
    };

    use super::*;

    #[test]
    fn carry_adapter_projects_public_roster_facts_without_secret_bridges() {
        let root = std::env::temp_dir().join(format!(
            "graphshell-h4-carry-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let device_id = DeviceId::new();
        let mut wallet = IdentityWalletManifest::default();
        wallet.personas.push(PersonaWalletRef {
            persona_id: PersonaId::default_persona(),
        });
        let roster = DeviceRoster {
            schema_version: session_runtime::WALLET_SCHEMA_VERSION,
            devices: vec![DeviceRecord {
                device_id,
                device_pubkey: DevicePublicKey([0x33; 32]),
                label: "Travel laptop".to_string(),
                mode: DeviceMode::RemoteAuth,
                exposure: DeviceExposure::HiddenClient,
                grant_ref: None,
            }],
            revoked: vec![device_id],
        };
        save_identity_wallet(&root, &wallet).unwrap();
        save_device_roster(&root, &roster).unwrap();

        let projected = load_carry_view(&root).unwrap();
        assert_eq!(projected.personas.len(), 1);
        assert_eq!(projected.devices.len(), 1);
        assert_eq!(projected.devices[0].label, "Travel laptop");
        assert!(projected.devices[0].revoked);
        let json = serde_json::to_string(&projected).unwrap();
        assert!(!json.contains("device_seed"));
        assert!(!json.contains("epoch_secret"));
        assert!(!json.contains("private_epoch"));

        std::fs::remove_dir_all(root).unwrap();
    }
}
