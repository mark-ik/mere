//! Shared plaintext wire shape for stored profiles.
//!
//! Both on-disk backends ([`crate::PassphraseEncryptedStorage`] and
//! [`crate::SealedProfileStorage`]) serialize a [`crate::vault::Profile`]
//! to this serde shape before encrypting. One definition so the two
//! at-rest formats cannot drift structurally; the encryption envelopes
//! around it differ per backend and are versioned there.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::vault::{CredentialLineage, IdentitySlot, ProtocolKey, SecretBytes, UnlockTier};

/// Plaintext inner shape — what a backend serializes then encrypts.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PlaintextProfile {
    pub(crate) display_name: String,
    /// 32-byte master signing-key seed.
    pub(crate) master_seed: [u8; 32],
    /// Slots, encoded.
    pub(crate) slots: Vec<PlaintextSlot>,
}

/// Plaintext slot — same structural shape as [`IdentitySlot`] but with
/// `Vec<u8>` for the secret payload (since `SecretBytes` doesn't impl
/// serde).
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PlaintextSlot {
    pub(crate) mod_id: String,
    pub(crate) instance: Option<String>,
    pub(crate) kind: String,
    pub(crate) payload: Vec<u8>,
    /// Set iff Bootstrap-category slot. None for Direct.
    pub(crate) state_dir: Option<PathBuf>,
    pub(crate) is_bootstrap: bool,
    pub(crate) lineage: CredentialLineage,
    pub(crate) unlock_tier: UnlockTier,
}

pub(crate) fn slot_to_plaintext(key: &ProtocolKey, slot: &IdentitySlot) -> PlaintextSlot {
    match slot {
        IdentitySlot::Direct {
            kind,
            payload,
            lineage,
            unlock_tier,
        } => PlaintextSlot {
            mod_id: key.mod_id.clone(),
            instance: key.instance.clone(),
            kind: kind.clone(),
            payload: payload.as_slice().to_vec(),
            state_dir: None,
            is_bootstrap: false,
            lineage: *lineage,
            unlock_tier: *unlock_tier,
        },
        IdentitySlot::Bootstrap {
            kind,
            bootstrap,
            state_dir,
            lineage,
            unlock_tier,
        } => PlaintextSlot {
            mod_id: key.mod_id.clone(),
            instance: key.instance.clone(),
            kind: kind.clone(),
            payload: bootstrap.as_slice().to_vec(),
            state_dir: Some(state_dir.clone()),
            is_bootstrap: true,
            lineage: *lineage,
            unlock_tier: *unlock_tier,
        },
    }
}

pub(crate) fn plaintext_to_slot(p: &PlaintextSlot) -> (ProtocolKey, IdentitySlot) {
    let key = ProtocolKey {
        mod_id: p.mod_id.clone(),
        instance: p.instance.clone(),
    };
    let slot = if p.is_bootstrap {
        IdentitySlot::Bootstrap {
            kind: p.kind.clone(),
            bootstrap: SecretBytes::new(p.payload.clone()),
            state_dir: p.state_dir.clone().unwrap_or_else(|| PathBuf::from(".")),
            lineage: p.lineage,
            unlock_tier: p.unlock_tier,
        }
    } else {
        IdentitySlot::Direct {
            kind: p.kind.clone(),
            payload: SecretBytes::new(p.payload.clone()),
            lineage: p.lineage,
            unlock_tier: p.unlock_tier,
        }
    };
    (key, slot)
}
