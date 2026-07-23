//! Pure AEAD sealing of arbitrary bytes under a 32-byte key.
//!
//! The reusable primitive beneath the sealed-record store: seal a payload under
//! a key with bound associated data, no filesystem and no key management. A
//! consumer that holds a persona-derived key (or any 32-byte key) uses this to
//! encrypt data at rest. XChaCha20-Poly1305 with a fresh random 24-byte nonce
//! prepended to the ciphertext.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand_core::{OsRng, RngCore};

use crate::IdentityError;

const NONCE_LEN: usize = 24;

/// Seal `plaintext` under `key`, binding `aad` (authenticated, not encrypted).
///
/// A fresh random nonce is generated and prepended to the ciphertext. Recover
/// with [`unseal_bytes`] using the same `key` and `aad`.
pub fn seal_bytes(key: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, IdentityError> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| IdentityError::Backend("seal_bytes encryption failed".to_string()))?;
    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Recover bytes sealed by [`seal_bytes`] with the same `key` and `aad`.
///
/// A wrong key, wrong `aad`, or tampered input all fail authentication.
pub fn unseal_bytes(key: &[u8; 32], aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, IdentityError> {
    if sealed.len() < NONCE_LEN {
        return Err(IdentityError::Backend(
            "sealed bytes are shorter than the nonce".to_string(),
        ));
    }
    let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| {
            IdentityError::Backend(
                "unseal_bytes failed: wrong key, wrong aad, or tampered".to_string(),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_then_unseal_round_trips() {
        let key = [7u8; 32];
        let sealed = seal_bytes(&key, b"ctx", b"a private payload").unwrap();
        assert_ne!(sealed.as_slice(), b"a private payload");
        assert_eq!(
            unseal_bytes(&key, b"ctx", &sealed).unwrap(),
            b"a private payload"
        );
    }

    #[test]
    fn wrong_key_fails() {
        let sealed = seal_bytes(&[1u8; 32], b"ctx", b"secret").unwrap();
        assert!(unseal_bytes(&[2u8; 32], b"ctx", &sealed).is_err());
    }

    #[test]
    fn wrong_aad_fails() {
        let key = [3u8; 32];
        let sealed = seal_bytes(&key, b"ctx-a", b"secret").unwrap();
        assert!(unseal_bytes(&key, b"ctx-b", &sealed).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = [4u8; 32];
        let mut sealed = seal_bytes(&key, b"ctx", b"secret").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xff;
        assert!(unseal_bytes(&key, b"ctx", &sealed).is_err());
    }

    #[test]
    fn a_fresh_nonce_each_time_so_ciphertexts_differ() {
        let key = [5u8; 32];
        let a = seal_bytes(&key, b"ctx", b"same").unwrap();
        let b = seal_bytes(&key, b"ctx", b"same").unwrap();
        assert_ne!(
            a, b,
            "random nonce -> distinct ciphertexts for identical input"
        );
    }
}
