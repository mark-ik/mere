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

    /// Install an epoch recovered from an authenticated welcome or control
    /// message.
    pub fn install(&mut self, secret: GroupSecret) {
        let current = std::mem::replace(&mut self.secrets, SecretBundle::init());
        self.secrets = SecretBundle::insert(current, secret);
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
        removed.is_some()
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, GroupCryptoError> {
        encode_cbor(self).map_err(|error| GroupCryptoError::Encode(error.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GroupCryptoError> {
        let state: Self =
            decode_cbor(bytes).map_err(|error| GroupCryptoError::Decode(error.to_string()))?;
        if state.version != Self::VERSION {
            return Err(GroupCryptoError::UnsupportedVersion(state.version));
        }
        Ok(state)
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
        assert_eq!(reopened.open(&old).unwrap(), b"history");
        assert_eq!(reopened.open(&new).unwrap(), b"after removal");
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
