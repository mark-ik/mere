// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Carrier-neutral group encryption state for replicated domains.
//!
//! Gemot decides membership and orders control facts. This module quarantines
//! the p2panda encryption engine, persists its data-scheme epochs, and seals
//! application bytes before a domain signs the resulting operation.

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_encryption::Rng;
use p2panda_encryption::crypto::xchacha20::XAeadNonce;
use p2panda_encryption::data_scheme::{
    GroupSecret, GroupSecretId, SecretBundle, SecretBundleState, decrypt_data, encrypt_data,
};
use serde::{Deserialize, Serialize};

/// Encryption semantics chosen by a shared-space profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupEncryptionMode {
    /// Retained epochs support durable shared documents and historical reads.
    Data,
    /// Per-message ratchets provide stronger forward secrecy for chat.
    Message,
}

/// Explicit product policy around the selected encryption engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupEncryptionProfile {
    pub mode: GroupEncryptionMode,
    /// Minimum number of data epochs retained before a domain-authorized prune
    /// may remove older secrets.
    pub retained_data_epochs: usize,
}

impl GroupEncryptionProfile {
    pub const fn durable_data(retained_data_epochs: usize) -> Self {
        Self {
            mode: GroupEncryptionMode::Data,
            retained_data_epochs,
        }
    }

    pub const fn forward_secure_messages() -> Self {
        Self {
            mode: GroupEncryptionMode::Message,
            retained_data_epochs: 0,
        }
    }
}

/// Data-scheme ciphertext carried as a domain operation body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupCiphertext {
    pub epoch: GroupSecretId,
    pub nonce: XAeadNonce,
    pub ciphertext: Vec<u8>,
}

/// Durable p2panda data-encryption epochs.
#[derive(Debug, Serialize, Deserialize)]
pub struct DataKeyring {
    version: u16,
    secrets: SecretBundleState,
    /// Rotation order is separate from the secret ids, whose lexical order has
    /// no temporal meaning. Version zero denotes a legacy persisted keyring
    /// whose chronology is unknown and therefore cannot drive pruning.
    #[serde(default)]
    epoch_order_version: u16,
    #[serde(default)]
    installed_epochs: Vec<GroupSecretId>,
}

impl Default for DataKeyring {
    fn default() -> Self {
        Self::new()
    }
}

impl DataKeyring {
    const VERSION: u16 = 1;

    pub fn new() -> Self {
        Self {
            version: Self::VERSION,
            secrets: SecretBundle::init(),
            epoch_order_version: 1,
            installed_epochs: Vec::new(),
        }
    }

    /// Generate and install the next epoch. The returned secret is handed to
    /// p2panda's DCGKA control-message engine for welcome/update distribution.
    pub fn rotate(&mut self, rng: &Rng) -> Result<GroupSecret, GroupCryptoError> {
        let secret = SecretBundle::generate(&self.secrets, rng)
            .map_err(|error| GroupCryptoError::Engine(error.to_string()))?;
        self.install(secret.clone());
        Ok(secret)
    }

    /// Rotate with the engine's operating-system-backed randomness source.
    pub fn rotate_random(&mut self) -> Result<GroupSecret, GroupCryptoError> {
        self.rotate(&Rng::default())
    }

    /// Install an epoch recovered from an authenticated welcome or control
    /// message.
    pub fn install(&mut self, secret: GroupSecret) {
        let epoch = secret.id();
        let was_present = self.secrets.contains(&epoch);
        let current = std::mem::replace(&mut self.secrets, SecretBundle::init());
        self.secrets = SecretBundle::insert(current, secret);
        if !was_present && self.epoch_order_version == 1 {
            self.installed_epochs.push(epoch);
        }
    }

    pub(crate) fn install_bundle(&mut self, bundle: SecretBundleState) -> Vec<GroupSecretId> {
        let mut secrets: Vec<_> = bundle.into_iter().map(|(_, secret)| secret).collect();
        secrets.sort_by_key(|secret| (secret.timestamp(), secret.id()));
        let mut installed = Vec::with_capacity(secrets.len());
        for secret in secrets {
            let epoch = secret.id();
            if !self.contains(&epoch) {
                installed.push(epoch);
            }
            self.install(secret);
        }
        installed
    }

    pub(crate) fn secret_bundle(&self) -> &SecretBundleState {
        &self.secrets
    }

    pub fn contains(&self, epoch: &GroupSecretId) -> bool {
        self.secrets.contains(epoch)
    }

    pub fn epoch_count(&self) -> usize {
        self.secrets.len()
    }

    pub fn epoch_ids(&self) -> Vec<GroupSecretId> {
        let mut ids: Vec<_> = self.secrets.ids().copied().collect();
        ids.sort();
        ids
    }

    /// Current sealing epoch, if the keyring has been initialized.
    pub fn current_epoch(&self) -> Option<GroupSecretId> {
        self.secrets.latest().map(GroupSecret::id)
    }

    /// Exact installation order, oldest first. A legacy keyring remains
    /// decryptable but returns `None`, because lexical secret-id order is not a
    /// safe substitute for chronology.
    pub fn epochs_oldest_first(&self) -> Option<&[GroupSecretId]> {
        if self.epoch_order_version != 1
            || self.installed_epochs.len() != self.secrets.len()
            || self
                .installed_epochs
                .iter()
                .any(|epoch| !self.secrets.contains(epoch))
            || self.installed_epochs.last().copied() != self.current_epoch()
        {
            return None;
        }
        Some(&self.installed_epochs)
    }

    pub fn seal(&self, plaintext: &[u8], rng: &Rng) -> Result<GroupCiphertext, GroupCryptoError> {
        let secret = self
            .secrets
            .latest()
            .ok_or(GroupCryptoError::MissingCurrentEpoch)?;
        let nonce = rng
            .random_array()
            .map_err(|error| GroupCryptoError::Engine(error.to_string()))?;
        let ciphertext = encrypt_data(plaintext, secret, nonce)
            .map_err(|error| GroupCryptoError::Engine(error.to_string()))?;
        Ok(GroupCiphertext {
            epoch: secret.id(),
            nonce,
            ciphertext,
        })
    }

    /// Seal with the engine's operating-system-backed randomness source.
    ///
    /// Domain crates use this convenience when they do not otherwise need to
    /// depend on the quarantined p2panda encryption engine.
    pub fn seal_random(&self, plaintext: &[u8]) -> Result<GroupCiphertext, GroupCryptoError> {
        self.seal(plaintext, &Rng::default())
    }

    pub fn open(&self, envelope: &GroupCiphertext) -> Result<Vec<u8>, GroupCryptoError> {
        let secret = self
            .secrets
            .get(&envelope.epoch)
            .ok_or(GroupCryptoError::UnknownEpoch(envelope.epoch))?;
        decrypt_data(&envelope.ciphertext, secret, envelope.nonce)
            .map_err(|error| GroupCryptoError::Engine(error.to_string()))
    }

    /// Remove an epoch only after the domain's checkpoint, authority, and
    /// retention gates have approved the exact id.
    pub fn forget_authorized(&mut self, epoch: &GroupSecretId) -> bool {
        let current = std::mem::replace(&mut self.secrets, SecretBundle::init());
        let (next, removed) = SecretBundle::remove(current, epoch);
        self.secrets = next;
        if removed.is_some() && self.epoch_order_version == 1 {
            self.installed_epochs.retain(|candidate| candidate != epoch);
        }
        removed.is_some()
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, GroupCryptoError> {
        encode_cbor(self).map_err(|error| GroupCryptoError::Encode(error.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GroupCryptoError> {
        let state: Self =
            decode_cbor(bytes).map_err(|error| GroupCryptoError::Decode(error.to_string()))?;
        state.validate()?;
        Ok(state)
    }

    pub(crate) fn validate(&self) -> Result<(), GroupCryptoError> {
        let state = self;
        if state.version != Self::VERSION {
            return Err(GroupCryptoError::UnsupportedVersion(state.version));
        }
        if state.epoch_order_version > 1 {
            return Err(GroupCryptoError::UnsupportedEpochOrderVersion(
                state.epoch_order_version,
            ));
        }
        if state.epoch_order_version == 0 && !state.installed_epochs.is_empty()
            || state.epoch_order_version == 1 && state.epochs_oldest_first().is_none()
        {
            return Err(GroupCryptoError::InvalidEpochOrder);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GroupCryptoError {
    #[error("group has no current data-encryption epoch")]
    MissingCurrentEpoch,
    #[error("group ciphertext names an unavailable epoch")]
    UnknownEpoch(GroupSecretId),
    #[error("group encryption engine: {0}")]
    Engine(String),
    #[error("encode group encryption state: {0}")]
    Encode(String),
    #[error("decode group encryption state: {0}")]
    Decode(String),
    #[error("unsupported group encryption state version {0}")]
    UnsupportedVersion(u16),
    #[error("unsupported group epoch-order version {0}")]
    UnsupportedEpochOrderVersion(u16),
    #[error("group epoch order does not match the retained secrets")]
    InvalidEpochOrder,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_epochs_reopen_and_removed_members_miss_the_rotated_key() {
        let rng = Rng::default();
        let mut alice = DataKeyring::new();
        let first = alice.rotate(&rng).unwrap();
        let mut bob = DataKeyring::new();
        bob.install(first);

        let old = alice.seal(b"history", &rng).unwrap();
        assert_eq!(bob.open(&old).unwrap(), b"history");

        let next = alice.rotate(&rng).unwrap();
        let new = alice.seal(b"after removal", &rng).unwrap();
        assert_eq!(new.epoch, next.id());
        assert!(matches!(
            bob.open(&new),
            Err(GroupCryptoError::UnknownEpoch(_))
        ));
        assert_eq!(bob.open(&old).unwrap(), b"history");

        let bytes = alice.to_bytes().unwrap();
        let reopened = DataKeyring::from_bytes(&bytes).unwrap();
        assert_eq!(reopened.epoch_count(), 2);
        assert_eq!(reopened.epochs_oldest_first(), alice.epochs_oldest_first());
        assert_eq!(reopened.open(&old).unwrap(), b"history");
        assert_eq!(reopened.open(&new).unwrap(), b"after removal");
    }

    #[test]
    fn a_legacy_keyring_stays_decryptable_but_cannot_invent_epoch_order() {
        #[derive(Serialize)]
        struct LegacyKeyring<'a> {
            version: u16,
            secrets: &'a SecretBundleState,
        }

        let rng = Rng::default();
        let mut current = DataKeyring::new();
        current.rotate(&rng).unwrap();
        current.rotate(&rng).unwrap();
        let sealed = current.seal(b"legacy history", &rng).unwrap();
        let bytes = encode_cbor(&LegacyKeyring {
            version: DataKeyring::VERSION,
            secrets: &current.secrets,
        })
        .unwrap();

        let legacy = DataKeyring::from_bytes(&bytes).unwrap();
        assert_eq!(legacy.open(&sealed).unwrap(), b"legacy history");
        assert_eq!(legacy.epoch_count(), 2);
        assert!(legacy.epochs_oldest_first().is_none());

        let revision = proofs::Digest::blake3(b"authority");
        let proposal = crate::propose_epoch_pruning(
            GroupEncryptionProfile::durable_data(1),
            &legacy,
            &crate::EpochRetentionFacts {
                checkpoint: Some(crate::EpochCheckpointBasis {
                    checkpoint: proofs::Digest::blake3(b"checkpoint"),
                    authority_revision: revision.clone(),
                    current_authority_revision: revision,
                    author_continuation_ready: true,
                }),
                holds: Vec::new(),
            },
        );
        assert_eq!(
            proposal.blockers,
            vec![crate::EpochProposalBlocker::IncompleteEpochOrder]
        );
        assert!(proposal.forget.is_empty());
        assert_eq!(proposal.retain.len(), 2);
    }

    #[test]
    fn authorized_forgetting_updates_the_persisted_order() {
        let rng = Rng::default();
        let mut ring = DataKeyring::new();
        let first = ring.rotate(&rng).unwrap().id();
        let second = ring.rotate(&rng).unwrap().id();
        let third = ring.rotate(&rng).unwrap().id();

        assert!(ring.forget_authorized(&second));
        assert_eq!(ring.epochs_oldest_first(), Some([first, third].as_slice()));

        let reopened = DataKeyring::from_bytes(&ring.to_bytes().unwrap()).unwrap();
        assert_eq!(
            reopened.epochs_oldest_first(),
            Some([first, third].as_slice())
        );
    }

    #[test]
    fn a_profile_makes_history_semantics_explicit() {
        assert_eq!(
            GroupEncryptionProfile::durable_data(3),
            GroupEncryptionProfile {
                mode: GroupEncryptionMode::Data,
                retained_data_epochs: 3,
            }
        );
        assert_eq!(
            GroupEncryptionProfile::forward_secure_messages().mode,
            GroupEncryptionMode::Message
        );
    }
}
