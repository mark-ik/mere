// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! SSH keys as vault slots.
//!
//! The slot shape and its encode/decode helpers, shared by the agent
//! ([`crate::agent`], which serves these over the SSH agent protocol) and
//! the `personae-vault` CLI (which imports and inspects them). Feature
//! `ssh`; the `agent` feature implies it.
//!
//! An SSH key is a Direct slot with `kind: "ssh"` whose payload is the
//! OpenSSH-encoded private key (the same bytes an `id_ed25519` file
//! holds, comment included). The [`ProtocolKey`] instance is the key's
//! SHA256 fingerprint, so multiple keys coexist and re-importing the same
//! key is idempotent.

use ssh_key::private::PrivateKey;
use ssh_key::public::PublicKey;
use ssh_key::{HashAlg, LineEnding};

use crate::IdentityError;
use crate::vault::{
    CredentialLineage, IdentitySlot, Profile, ProtocolKey, SecretBytes, UnlockTier,
};

/// The `mod_id` under which SSH keys are stored.
pub const SSH_MOD_ID: &str = "ssh";

/// One decoded SSH slot.
pub struct SshSlot {
    /// Where it lives in the profile.
    pub key: ProtocolKey,
    /// The decoded private key.
    pub private: PrivateKey,
    /// The slot's unlock tier.
    pub tier: UnlockTier,
}

impl SshSlot {
    /// The public half.
    pub fn public(&self) -> PublicKey {
        PublicKey::from(&self.private)
    }

    /// SHA256 fingerprint, in the `SHA256:...` form ssh tools print.
    pub fn fingerprint(&self) -> String {
        self.public().fingerprint(HashAlg::Sha256).to_string()
    }
}

/// The [`ProtocolKey`] an SSH private key stores under.
pub fn protocol_key_for(private: &PrivateKey) -> ProtocolKey {
    let fingerprint = PublicKey::from(private).fingerprint(HashAlg::Sha256);
    ProtocolKey::new(SSH_MOD_ID, Some(fingerprint.to_string()))
}

/// Build a vault slot for an SSH private key.
///
/// Lineage is always `LocallyGeneratedExternallyRegistered`: an SSH
/// keypair is generated locally and registered with servers
/// (`authorized_keys`), so losing it means registering a replacement, not
/// re-deriving it.
pub fn slot_for(private: &PrivateKey, tier: UnlockTier) -> Result<IdentitySlot, IdentityError> {
    if private.is_encrypted() {
        return Err(IdentityError::Backend(
            "this private key is passphrase-encrypted; decrypt it first (ssh-keygen -p -f <file>) \
             — the vault provides its own encryption at rest"
                .to_string(),
        ));
    }
    let encoded = private
        .to_openssh(LineEnding::LF)
        .map_err(|err| IdentityError::Backend(format!("encode openssh private key: {err}")))?;
    Ok(IdentitySlot::Direct {
        kind: SSH_MOD_ID.to_string(),
        payload: SecretBytes::new(encoded.as_bytes().to_vec()),
        lineage: CredentialLineage::LocallyGeneratedExternallyRegistered,
        unlock_tier: tier,
    })
}

/// Decode an `ssh`-kind slot's payload back into a private key.
pub fn private_key_from_slot(slot: &IdentitySlot) -> Result<PrivateKey, IdentityError> {
    let IdentitySlot::Direct { payload, .. } = slot else {
        return Err(IdentityError::Backend(
            "not a Direct slot; ssh keys are stored Direct".to_string(),
        ));
    };
    PrivateKey::from_openssh(payload.as_slice())
        .map_err(|err| IdentityError::Backend(format!("decode openssh private key: {err}")))
}

/// Every `ssh`-kind slot in a profile, decoded.
///
/// Slots whose payload fails to parse are skipped with a warning rather
/// than failing the whole listing: one bad slot must not make the agent
/// or the CLI useless.
pub fn ssh_slots(profile: &Profile) -> Vec<SshSlot> {
    let mut out = Vec::new();
    for (key, slot) in &profile.slots {
        if key.mod_id != SSH_MOD_ID {
            continue;
        }
        match private_key_from_slot(slot) {
            Ok(private) => out.push(SshSlot {
                key: key.clone(),
                private,
                tier: slot.unlock_tier(),
            }),
            Err(err) => tracing::warn!(?key, %err, "ssh slot failed to decode; skipping"),
        }
    }
    out
}

/// Find a profile's slot for a given public key.
///
/// Compares key *data*, not [`PublicKey`], because `PublicKey` equality
/// includes the comment and requesters send none.
pub fn find_by_public(profile: &Profile, wanted: &PublicKey) -> Option<SshSlot> {
    ssh_slots(profile)
        .into_iter()
        .find(|slot| slot.public().key_data() == wanted.key_data())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ed25519Keypair;
    use crate::vault::{ProfileId, UnlockTier};
    use ssh_key::Algorithm;

    fn random_key(comment: &str) -> PrivateKey {
        let mut key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        key.set_comment(comment);
        key
    }

    fn profile_with(keys: &[&PrivateKey]) -> Profile {
        let mut profile = Profile::new(
            ProfileId("t".into()),
            "t",
            Ed25519Keypair::from_seed([3; 32]),
        );
        for key in keys {
            profile.slots.insert(
                protocol_key_for(key),
                slot_for(key, UnlockTier::Session).unwrap(),
            );
        }
        profile
    }

    #[test]
    fn slot_round_trips_the_private_key_and_comment() {
        let key = random_key("laptop");
        let slot = slot_for(&key, UnlockTier::Session).unwrap();
        let decoded = private_key_from_slot(&slot).unwrap();
        assert_eq!(decoded.comment(), "laptop");
        assert_eq!(
            PublicKey::from(&decoded).key_data(),
            PublicKey::from(&key).key_data()
        );
    }

    #[test]
    fn protocol_key_is_the_fingerprint_so_reimport_is_idempotent() {
        let key = random_key("dup");
        let profile = profile_with(&[&key, &key]);
        assert_eq!(profile.slots.len(), 1);
        assert_eq!(protocol_key_for(&key).mod_id, SSH_MOD_ID);
    }

    #[test]
    fn ssh_slots_skips_undecodable_payloads() {
        let good = random_key("good");
        let mut profile = profile_with(&[&good]);
        profile.slots.insert(
            ProtocolKey::new(SSH_MOD_ID, Some("SHA256:garbage".into())),
            IdentitySlot::Direct {
                kind: SSH_MOD_ID.into(),
                payload: SecretBytes::new(b"not an openssh key".to_vec()),
                lineage: CredentialLineage::LocallyGeneratedExternallyRegistered,
                unlock_tier: UnlockTier::Session,
            },
        );
        let listed = ssh_slots(&profile);
        assert_eq!(listed.len(), 1, "the good slot still lists");
        assert_eq!(listed[0].private.comment(), "good");
    }

    #[test]
    fn find_by_public_ignores_comments() {
        let key = random_key("with-comment");
        let profile = profile_with(&[&key]);
        let mut bare = PublicKey::from(&key);
        bare.set_comment("");
        assert!(find_by_public(&profile, &bare).is_some());
        assert!(find_by_public(&profile, &PublicKey::from(&random_key("other"))).is_none());
    }

    #[test]
    fn non_ssh_slots_are_ignored() {
        let mut profile = profile_with(&[&random_key("k")]);
        profile.slots.insert(
            ProtocolKey::new("nostr", None),
            IdentitySlot::Direct {
                kind: "nostr".into(),
                payload: SecretBytes::new(vec![1; 32]),
                lineage: CredentialLineage::LocallyGeneratedExternallyRegistered,
                unlock_tier: UnlockTier::Session,
            },
        );
        assert_eq!(ssh_slots(&profile).len(), 1);
    }
}
