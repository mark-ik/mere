//! Deterministic Reticulum identity derivation from a Mere master seed.

use std::fmt::Write as _;

use hkdf::Hkdf;
use sha2::Sha256;

use identity::Ed25519Keypair;
use reticulum::identity::PrivateIdentity as ReticulumPrivateIdentity;

/// HKDF-SHA256 context string for the dual-key derivation. Changing this changes
/// every derived Reticulum identity, so treat it as a wire-format version.
const IDENTITY_HKDF_INFO: &[u8] = b"mere-reticulum-identity-v1";

/// Derive a deterministic Reticulum identity from a Mere master keypair.
///
/// Reticulum needs a dual-key identity (X25519 for ECDH + Ed25519 for signing),
/// 64 secret bytes in total. Mere's master key is a single 32-byte Ed25519 seed,
/// so we stretch it with HKDF-SHA256 into 64 bytes and hand them to
/// [`PrivateIdentity::new_from_hex_string`](ReticulumPrivateIdentity::new_from_hex_string),
/// which parses them as X25519 secret (first 32) ++ Ed25519 signing seed
/// (last 32) — exactly our split. Going through the hex constructor avoids naming
/// the ed25519-dalek / x25519-dalek types (and pinning their versions) here.
///
/// The derivation is pure, so the same master seed always produces the same
/// Reticulum destination across restarts.
pub(super) fn derive_identity(master: &Ed25519Keypair) -> ReticulumPrivateIdentity {
    let seed = master.to_seed();
    let hk = Hkdf::<Sha256>::new(None, &seed);
    let mut okm = [0u8; 64];
    hk.expand(IDENTITY_HKDF_INFO, &mut okm)
        .expect("64 bytes is a valid HKDF-SHA256 output length");

    let mut hex = String::with_capacity(okm.len() * 2);
    for byte in &okm {
        let _ = write!(hex, "{byte:02x}");
    }

    ReticulumPrivateIdentity::new_from_hex_string(&hex)
        .expect("128 hex chars is a valid PrivateIdentity encoding")
}
