// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Wallet-backed engram sealer — the concrete [`PayloadSealer`] for the private
//! lane (persona-wallet gap #2, host side).
//!
//! `eidetic::seal` defines the boundary but owns no keys; this is the host
//! implementation that seals eidetic private-lane payloads under a persona's
//! wallet private-epoch key. It is the muniment/signet joint from the trust-plane
//! plan, made concrete: the store holds bytes, the wallet owns the epoch key, and
//! this sealer is what binds them.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use eidetic::{Hash, PayloadSealer, SealEpochId, SealedBlobRef};
use rand_core::{OsRng, RngCore};

use crate::manifest::PersonaId;
use crate::wallet_store::{KeyEpochId, load_current_private_epoch};

/// At-rest seal format for engram payloads. Same AEAD family as the wallet's
/// wrapped-epoch format so one primitive covers key wrapping and payload sealing.
const ENGRAM_SEAL_FORMAT_V1: &str = "xchacha20poly1305-v1";
/// BLAKE3 derive-key context turning a variable-length epoch secret into the
/// 32-byte payload-sealing key. Distinct from any wallet wrapping context.
const ENGRAM_SEAL_KEY_CONTEXT: &str = "mere.session_runtime.engram_seal.key.v1";

fn derive_engram_key(epoch_secret: &[u8]) -> [u8; 32] {
    blake3::derive_key(ENGRAM_SEAL_KEY_CONTEXT, epoch_secret)
}

fn epoch_to_seal_id(epoch: KeyEpochId) -> SealEpochId {
    SealEpochId(*epoch.0.as_bytes())
}

/// A persona-scoped [`PayloadSealer`] over the wallet's private-epoch keys.
///
/// Seals under the persona's current epoch and unseals any epoch whose key it
/// holds, so pre-rotation content stays readable once its epoch is loaded. The
/// sealing key is BLAKE3-derived from the epoch secret (variable-length secrets
/// all yield a 32-byte key), and the content hash, persona, and epoch are bound
/// as AEAD associated data so a sealed payload cannot be transplanted to another
/// manifest, persona, or epoch.
pub struct WalletEpochSealer {
    persona: PersonaId,
    current: SealEpochId,
    keys: HashMap<SealEpochId, [u8; 32]>,
}

impl WalletEpochSealer {
    /// Build a sealer directly from an epoch id and its secret. The primitive a
    /// caller uses when it already holds the epoch; also the seam tests use.
    pub fn from_epoch(persona: PersonaId, epoch_id: KeyEpochId, epoch_secret: &[u8]) -> Self {
        let current = epoch_to_seal_id(epoch_id);
        let mut keys = HashMap::new();
        keys.insert(current, derive_engram_key(epoch_secret));
        Self {
            persona,
            current,
            keys,
        }
    }

    /// Build a sealer from a persona's current private epoch, or `None` when the
    /// persona has no staged epoch yet (nothing to seal under, so writes stay
    /// cleartext — the host's degraded-but-honest posture).
    pub fn for_persona(data_root: &Path, persona: PersonaId) -> io::Result<Option<Self>> {
        let Some(epoch) = load_current_private_epoch(data_root, persona)? else {
            return Ok(None);
        };
        Ok(Some(Self::from_epoch(persona, epoch.epoch_id, &epoch.epoch_secret)))
    }

    /// Add another epoch's key (e.g. a pre-rotation epoch, for historical reads).
    pub fn with_epoch(mut self, epoch_id: KeyEpochId, epoch_secret: &[u8]) -> Self {
        self.keys
            .insert(epoch_to_seal_id(epoch_id), derive_engram_key(epoch_secret));
        self
    }

    fn aad(&self, content_hash: &Hash, epoch: &SealEpochId) -> Vec<u8> {
        let mut aad = Vec::with_capacity(27 + 64 + 16 + 16);
        aad.extend_from_slice(b"mere.eidetic.engram-seal.v1");
        aad.extend_from_slice(content_hash.to_hex().as_bytes());
        aad.extend_from_slice(self.persona.as_uuid().as_bytes());
        aad.extend_from_slice(&epoch.0);
        aad
    }
}

impl PayloadSealer for WalletEpochSealer {
    fn seal(
        &self,
        content_hash: &Hash,
        cleartext: &[u8],
    ) -> eidetic::Result<(Vec<u8>, SealedBlobRef)> {
        let key = self.keys.get(&self.current).ok_or_else(|| {
            eidetic::Error::new("wallet epoch sealer is missing its current epoch key")
        })?;
        let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
        let mut nonce = [0u8; 24];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: cleartext,
                    aad: &self.aad(content_hash, &self.current),
                },
            )
            .map_err(|_| eidetic::Error::new("engram seal encryption failed"))?;
        let mut sealed = nonce.to_vec();
        sealed.extend_from_slice(&ciphertext);
        Ok((
            sealed,
            SealedBlobRef {
                epoch: self.current,
                format: ENGRAM_SEAL_FORMAT_V1.to_string(),
            },
        ))
    }

    fn unseal(
        &self,
        content_hash: &Hash,
        marker: &SealedBlobRef,
        sealed: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        if marker.format != ENGRAM_SEAL_FORMAT_V1 {
            return Err(eidetic::Error::new(format!(
                "unsupported engram seal format: {}",
                marker.format
            )));
        }
        if sealed.len() < 24 {
            return Err(eidetic::Error::new(
                "sealed engram is shorter than the XChaCha20 nonce",
            ));
        }
        let key = self.keys.get(&marker.epoch).ok_or_else(|| {
            eidetic::Error::new(format!("no epoch key for {}", marker.epoch.to_hex()))
        })?;
        let (nonce, ciphertext) = sealed.split_at(24);
        let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
        cipher
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: &self.aad(content_hash, &marker.epoch),
                },
            )
            .map_err(|_| eidetic::Error::new("engram unseal decryption failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn epoch(byte: u8) -> KeyEpochId {
        KeyEpochId(uuid::Uuid::from_bytes([byte; 16]))
    }

    #[test]
    fn seal_then_unseal_round_trips() {
        let persona = PersonaId::new();
        let sealer = WalletEpochSealer::from_epoch(persona, epoch(1), b"epoch-secret-material");
        let cleartext = b"a private engram payload";
        let hash = Hash::of(cleartext);

        let (sealed, marker) = sealer.seal(&hash, cleartext).unwrap();
        assert_ne!(sealed.as_slice(), cleartext);
        assert_eq!(marker.format, ENGRAM_SEAL_FORMAT_V1);
        let back = sealer.unseal(&hash, &marker, &sealed).unwrap();
        assert_eq!(back, cleartext);
    }

    #[test]
    fn another_persona_cannot_unseal_even_with_the_same_epoch_secret() {
        // Same epoch id + secret, different persona: the persona is bound into
        // the AEAD associated data, so the seal does not transfer.
        let cleartext = b"persona-bound";
        let hash = Hash::of(cleartext);
        let a = WalletEpochSealer::from_epoch(PersonaId::new(), epoch(2), b"shared-secret");
        let b = WalletEpochSealer::from_epoch(PersonaId::new(), epoch(2), b"shared-secret");
        let (sealed, marker) = a.seal(&hash, cleartext).unwrap();
        assert!(b.unseal(&hash, &marker, &sealed).is_err());
    }

    #[test]
    fn a_different_epoch_key_fails_to_unseal() {
        let persona = PersonaId::new();
        let cleartext = b"epoch-keyed";
        let hash = Hash::of(cleartext);
        let sealer = WalletEpochSealer::from_epoch(persona, epoch(3), b"secret-A");
        let (sealed, marker) = sealer.seal(&hash, cleartext).unwrap();
        // Same persona + epoch id, different secret -> different key.
        let wrong = WalletEpochSealer::from_epoch(persona, epoch(3), b"secret-B");
        assert!(wrong.unseal(&hash, &marker, &sealed).is_err());
    }

    #[test]
    fn reads_a_pre_rotation_epoch_via_with_epoch() {
        let persona = PersonaId::new();
        let cleartext = b"sealed before rotation";
        let hash = Hash::of(cleartext);
        let old = WalletEpochSealer::from_epoch(persona, epoch(0xAA), b"old-secret");
        let (sealed, marker) = old.seal(&hash, cleartext).unwrap();

        let rotated = WalletEpochSealer::from_epoch(persona, epoch(0xBB), b"new-secret")
            .with_epoch(epoch(0xAA), b"old-secret");
        let back = rotated.unseal(&hash, &marker, &sealed).unwrap();
        assert_eq!(back, cleartext);
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let persona = PersonaId::new();
        let cleartext = b"integrity";
        let hash = Hash::of(cleartext);
        let sealer = WalletEpochSealer::from_epoch(persona, epoch(4), b"secret");
        let (mut sealed, marker) = sealer.seal(&hash, cleartext).unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xff;
        assert!(sealer.unseal(&hash, &marker, &sealed).is_err());
    }

    #[test]
    fn for_persona_is_none_without_a_staged_epoch() {
        // No wallet state on this path, so there is no epoch to seal under and
        // the host stays in the cleartext lane rather than erroring.
        let dir = std::env::temp_dir().join("mere-engram-seal-none-probe");
        let _ = std::fs::remove_dir_all(&dir);
        let sealer = WalletEpochSealer::for_persona(&dir, PersonaId::new()).unwrap();
        assert!(sealer.is_none());
    }

    #[test]
    fn for_persona_builds_a_working_sealer_from_a_staged_epoch() {
        // The path the meerkat wiring relies on: real wallet state -> for_persona
        // -> a sealer that actually seals and unseals.
        use crate::wallet_store::{
            ensure_wallet_state, load_persona_wallet, stage_persona_private_epoch,
        };

        let dir = std::env::temp_dir().join(format!(
            "mere-engram-seal-forpersona-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let persona = PersonaId::new();
        ensure_wallet_state(&dir, persona, "Test PC").unwrap();
        let head = load_persona_wallet(&dir, persona)
            .unwrap()
            .unwrap()
            .private_epoch_head;
        stage_persona_private_epoch(&dir, persona, head, b"staged-epoch-secret").unwrap();

        let sealer = WalletEpochSealer::for_persona(&dir, persona)
            .unwrap()
            .expect("a staged epoch yields a sealer");
        let cleartext = b"round-trip via for_persona";
        let hash = Hash::of(cleartext);
        let (sealed, marker) = sealer.seal(&hash, cleartext).unwrap();
        assert_ne!(sealed.as_slice(), cleartext);
        assert_eq!(sealer.unseal(&hash, &marker, &sealed).unwrap(), cleartext);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
