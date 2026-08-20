//! Pure AEAD sealing of arbitrary bytes under a 32-byte key.
//!
//! The reusable primitive beneath the sealed-record store: seal a payload under
//! a key with bound associated data, no filesystem and no key management. A
//! consumer that holds a persona-derived key (or any 32-byte key) uses this to
//! encrypt data at rest. XChaCha20-Poly1305 with a fresh random 24-byte nonce
//! prepended to the ciphertext.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};

use crate::IdentityError;

const NONCE_LEN: usize = 24;

/// Seal `plaintext` under `key`, binding `aad` (authenticated, not encrypted).
///
/// A fresh random nonce is generated and prepended to the ciphertext. Recover
/// with [`unseal_bytes`] using the same `key` and `aad`.
pub fn seal_bytes(key: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, IdentityError> {
    let cipher = XChaCha20Poly1305::new(&Key::try_from(&key[..]).expect("fixed-length key material"));
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).expect("OS randomness available");
    let ciphertext = cipher
        .encrypt(
            &XNonce::try_from(&nonce[..]).expect("fixed-length key material"),
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
    let cipher = XChaCha20Poly1305::new(&Key::try_from(&key[..]).expect("fixed-length key material"));
    cipher
        .decrypt(
            &XNonce::try_from(&nonce[..]).expect("fixed-length key material"),
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

    /// The sealed-record format, pinned byte for byte.
    ///
    /// This guards real data. Vaults, wallets, and woodshed sessions on disk
    /// right now were sealed by this function, so the day its output changes
    /// is the day they stop opening. A crypto-crate version bump is exactly
    /// how that would happen quietly, which is why the assertion is on bytes
    /// rather than on a round trip: a round trip passes happily while writing
    /// and reading a *new* format.
    ///
    /// Recorded 2026-08-10 during the digest 0.10 to 0.11 unification, and
    /// confirmed identical under both generations. XChaCha20-Poly1305 is
    /// standardized, so this is the algorithm speaking, not the crate.
    ///
    /// The nonce is supplied rather than generated, since `seal_bytes` draws a
    /// fresh random one and its output is deliberately not reproducible.
    #[test]
    fn the_sealed_format_is_pinned_across_crate_generations() {
        use chacha20poly1305::aead::{Aead, KeyInit, Payload};
        use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};

        let key = [7u8; 32];
        let nonce = [9u8; 24];
        let aad = b"ctx";
        let plaintext = b"a private payload";

        let cipher = XChaCha20Poly1305::new(&Key::try_from(&key[..]).expect("fixed-length key material"));
        let ciphertext = cipher
            .encrypt(
                &XNonce::try_from(&nonce[..]).expect("fixed-length key material"),
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .unwrap();

        let mut sealed = Vec::new();
        sealed.extend_from_slice(&nonce);
        sealed.extend_from_slice(&ciphertext);

        assert_eq!(
            hex::encode(&sealed),
            concat!(
                // the 24-byte nonce, prepended verbatim
                "090909090909090909090909090909090909090909090909",
                // XChaCha20-Poly1305 ciphertext plus its 16-byte tag
                "df5292f815dc18169132785d111f688a4e",
                "78010cef9d49354ca255964add266317",
            ),
            "the sealed byte format changed"
        );

        // And the public path still opens it, so the pin is on the real format
        // rather than on a construction that only resembles it.
        assert_eq!(unseal_bytes(&key, aad, &sealed).unwrap(), plaintext);
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
