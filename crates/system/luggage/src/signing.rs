// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// Copyright 2023-2023 CrabNebula Ltd.
// Copyright 2026 Mark AB (markik) — luggage fork
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

//! Minisign verification, shared by the artifact and manifest paths.

use std::io::Read;

use base64::Engine;
use minisign_verify::{PublicKey, Signature};

use crate::error::{Error, Result};

/// Verify `data` against a base64-wrapped minisign signature and public key.
///
/// Both the signature and the key are the base64 of the usual minisign text
/// blocks, which is the shape `cargo packager` writes and the shape a
/// manifest carries.
pub(crate) fn verify_bytes(data: &[u8], release_signature: &str, pub_key: &str) -> Result<()> {
    let pub_key_decoded = base64_to_string(pub_key)?;
    let public_key = PublicKey::decode(&pub_key_decoded)?;
    let signature_base64_decoded = base64_to_string(release_signature)?;
    let signature = Signature::decode(&signature_base64_decoded)?;
    public_key.verify(data, &signature, true)?;
    Ok(())
}

/// [`verify_bytes`] over a reader. NOTE: the reader position is not reset.
pub(crate) fn verify_reader<R: Read>(
    reader: &mut R,
    release_signature: &str,
    pub_key: &str,
) -> Result<()> {
    let mut data = Vec::new();
    reader.read_to_end(&mut data)?;
    verify_bytes(&data, release_signature, pub_key)
}

pub(crate) fn base64_to_string(base64_string: &str) -> Result<String> {
    let decoded_string = &base64::engine::general_purpose::STANDARD.decode(base64_string)?;
    let result = std::str::from_utf8(decoded_string)
        .map_err(|_| Error::SignatureUtf8(base64_string.into()))?
        .to_string();
    Ok(result)
}

/// Normalise a `.sig` file's contents into the base64 form
/// [`verify_bytes`] wants, accepting either convention in the wild.
///
/// `cargo packager signer sign` writes the signature **already
/// base64-wrapped**; plain `minisign -S` writes the raw text block. Wrapping
/// unconditionally double-encodes the former, which fails verification with
/// a signature that is in fact perfectly good — found end to end on
/// 2026-07-24, after a unit test using the raw form passed happily.
pub(crate) fn normalize_signature_file(contents: &str) -> String {
    let trimmed = contents.trim();
    let already_wrapped = base64::engine::general_purpose::STANDARD
        .decode(trimmed)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .is_some_and(|text| text.trim_start().starts_with("untrusted comment:"));
    if already_wrapped {
        trimmed.to_string()
    } else {
        base64::engine::general_purpose::STANDARD.encode(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(data: &[u8]) -> (String, String) {
        let keypair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let signature =
            minisign::sign(None, &keypair.sk, std::io::Cursor::new(data), None, None).unwrap();
        let engine = base64::engine::general_purpose::STANDARD;
        (
            engine.encode(keypair.pk.to_box().unwrap().to_string()),
            engine.encode(signature.to_string()),
        )
    }

    #[test]
    fn a_good_signature_verifies() {
        let data = b"release manifest bytes";
        let (pubkey, signature) = sign(data);
        assert!(verify_bytes(data, &signature, &pubkey).is_ok());
    }

    #[test]
    fn a_signature_over_other_bytes_is_refused() {
        let (pubkey, signature) = sign(b"the real bytes");
        let err = verify_bytes(b"substituted bytes", &signature, &pubkey).unwrap_err();
        assert!(matches!(err, Error::Minisign(_)), "got: {err}");
    }

    #[test]
    fn another_keys_signature_is_refused() {
        let data = b"release manifest bytes";
        let (_, signature) = sign(data);
        let (other_pubkey, _) = sign(b"unrelated");
        assert!(verify_bytes(data, &signature, &other_pubkey).is_err());
    }

    /// Both `.sig` conventions must verify: `minisign -S` writes the raw
    /// text block, `cargo packager signer sign` writes it base64-wrapped.
    /// Wrapping unconditionally breaks the second, which is the one our own
    /// pipeline produces.
    #[test]
    fn both_sig_file_conventions_normalise_and_verify() {
        let data = b"release manifest bytes";
        let keypair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let signature =
            minisign::sign(None, &keypair.sk, std::io::Cursor::new(data), None, None).unwrap();
        let engine = base64::engine::general_purpose::STANDARD;
        let pubkey = engine.encode(keypair.pk.to_box().unwrap().to_string());

        // Raw text on disk, trailing newline and all.
        let raw = format!("{}\n", signature.to_string());
        assert!(
            verify_bytes(data, &normalize_signature_file(&raw), &pubkey).is_ok(),
            "raw minisign text must verify"
        );

        // Already base64-wrapped, as cargo-packager writes it.
        let wrapped = format!("{}\n", engine.encode(signature.to_string()));
        assert!(
            verify_bytes(data, &normalize_signature_file(&wrapped), &pubkey).is_ok(),
            "an already-wrapped signature must not be wrapped again"
        );
    }
}
